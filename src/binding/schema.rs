#![allow(
    clippy::collapsible_if,
    clippy::needless_borrows_for_generic_args,
    clippy::needless_match,
    clippy::manual_map
)]
use crate::codec::consts::TarsType;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyString, PyTuple, PyType};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};

// ==========================================
// [L2] Wire IR: 物理层 (面向 Codec)
// ==========================================

#[derive(Debug, Clone, PartialEq)]
pub enum WireType {
    Int,
    Long,
    Float,
    Double,
    String,
    Struct(usize),
    List(Box<WireType>),
    Map(Box<WireType>, Box<WireType>),
}

impl WireType {
    pub fn to_tars_type(&self) -> TarsType {
        match self {
            WireType::Int => TarsType::Int4,
            WireType::Long => TarsType::Int8,
            WireType::Float => TarsType::Float,
            WireType::Double => TarsType::Double,
            WireType::String => TarsType::String1,
            WireType::Struct(_) => TarsType::StructBegin,
            WireType::List(_) => TarsType::List,
            WireType::Map(_, _) => TarsType::Map,
        }
    }
}

// ==========================================
// [L1] Semantic IR: 语义层 (面向 Schema)
// ==========================================

#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpr {
    Primitive(WireType),
    Struct(usize),
    List(Box<TypeExpr>),
    Map(Box<TypeExpr>, Box<TypeExpr>),
    Optional(Box<TypeExpr>),
}

impl TypeExpr {
    pub fn lower(&self) -> WireType {
        match self {
            TypeExpr::Primitive(w) => w.clone(),
            TypeExpr::Struct(ptr) => WireType::Struct(*ptr),
            TypeExpr::List(inner) => WireType::List(Box::new(inner.lower())),
            TypeExpr::Map(k, v) => WireType::Map(Box::new(k.lower()), Box::new(v.lower())),
            TypeExpr::Optional(inner) => inner.lower(),
        }
    }

    pub fn is_optional(&self) -> bool {
        matches!(self, TypeExpr::Optional(_))
    }
}

// ==========================================
// Schema Definitions
// ==========================================

#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: String,
    pub tag: u8,
    pub ty: TypeExpr,
    pub wire_type: WireType,
    pub is_optional: bool,
    pub is_required: bool,
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub class: Arc<Py<PyType>>,
    pub fields_sorted: Vec<FieldDef>,
    pub tag_index: HashMap<u8, usize>,
    pub name_to_tag: HashMap<String, u8>,
}

impl StructDef {
    /// Bind class to Python interpreter and return Bound reference.
    pub fn bind_class<'py>(&self, py: Python<'py>) -> Bound<'py, PyType> {
        self.class.bind(py).clone()
    }
}

type SchemaRegistry = HashMap<usize, StructDef>;

static REGISTRY: LazyLock<RwLock<SchemaRegistry>> = LazyLock::new(|| RwLock::new(HashMap::new()));

pub fn get_schema(type_ptr: usize) -> Option<StructDef> {
    REGISTRY.read().unwrap().get(&type_ptr).cloned()
}

// ==========================================
// Python Class Binding
// ==========================================

#[pyclass(subclass, module = "tarsio")]
pub struct Struct;

#[pymethods]
impl Struct {
    #[new]
    #[pyo3(signature = (*_args, **_kwargs))]
    fn new(_args: &Bound<'_, PyTuple>, _kwargs: Option<&Bound<'_, PyDict>>) -> Self {
        Struct
    }

    #[pyo3(signature = (*args, **kwargs))]
    fn __init__(
        slf: &Bound<'_, Struct>,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        let cls = slf.get_type();
        let type_ptr = cls.as_ptr() as usize;

        // Runtime Schema Lookup
        let def = if let Some(d) = get_schema(type_ptr) {
            d
        } else {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "Cannot instantiate abstract schema class '{}'",
                cls.name()?
            )));
        };

        construct_instance(&def, slf.as_any(), args, kwargs)
    }

    /// Encode struct to bytes.
    fn encode(slf: &Bound<'_, Struct>) -> PyResult<Py<pyo3::types::PyBytes>> {
        let py = slf.py();
        let result = crate::binding::ser::encode_object(slf.as_any())?;
        Ok(pyo3::types::PyBytes::new(py, &result).unbind())
    }

    /// Decode bytes into a struct instance.
    #[classmethod]
    fn decode<'py>(cls: &Bound<'py, PyType>, data: &[u8]) -> PyResult<Bound<'py, PyAny>> {
        let py = cls.py();
        crate::binding::de::decode_object(py, cls, data)
    }

    #[classmethod]
    fn __init_subclass__(cls: &Bound<'_, PyType>) -> PyResult<()> {
        let py = cls.py();

        let builtins = py.import("builtins")?;
        let super_fn = builtins.getattr("super")?;
        let struct_type = py.get_type::<Struct>();
        let super_obj = super_fn.call1((struct_type, cls))?;
        super_obj.call_method0("__init_subclass__")?;

        // 0. Generic Template Check
        if let Ok(params) = cls.getattr("__parameters__") {
            if let Ok(tuple) = params.cast::<PyTuple>() {
                if !tuple.is_empty() {
                    return Ok(());
                }
            }
        }

        // 1. Context Setup
        let typing = py.import("typing")?;
        let get_origin = typing.getattr("get_origin")?;
        let get_args = typing.getattr("get_args")?;

        let mut generic_origin: Option<Bound<'_, PyAny>> = None;
        let mut generic_args: Option<Bound<'_, PyTuple>> = None;
        if let Ok(origin) = cls.getattr("__origin__") {
            if let Ok(args) = cls.getattr("__args__") {
                if let Ok(args_tuple) = args.cast::<PyTuple>() {
                    generic_origin = Some(origin);
                    generic_args = Some(args_tuple.clone());
                }
            }
        }
        if generic_origin.is_none() {
            if let Ok(orig_bases) = cls.getattr("__orig_bases__") {
                if let Ok(bases) = orig_bases.cast::<PyTuple>() {
                    for base in bases.iter() {
                        if let Ok(origin) = base.getattr("__origin__") {
                            if let Ok(args) = base.getattr("__args__") {
                                if let Ok(args_tuple) = args.cast::<PyTuple>() {
                                    generic_origin = Some(origin);
                                    generic_args = Some(args_tuple.clone());
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut typevar_map = HashMap::new();
        if let (Some(origin), Some(args_tuple)) = (generic_origin.as_ref(), generic_args.as_ref()) {
            if let Ok(params) = origin.getattr("__parameters__") {
                let params_tuple = params.cast::<PyTuple>()?;
                for (param, arg) in params_tuple.iter().zip(args_tuple.iter()) {
                    typevar_map.insert(param.as_ptr() as usize, arg);
                }
            }
        }

        // 2. Resolve Type Hints
        let get_type_hints = typing.getattr("get_type_hints")?;

        let localns = PyDict::new(py);
        localns.set_item(cls.getattr("__name__")?, cls)?;

        let kwargs = PyDict::new(py);
        kwargs.set_item("include_extras", true)?;
        kwargs.set_item("localns", &localns)?;

        // Forward Ref Resolution via typing.get_type_hints
        let mut hints = match get_type_hints.call((cls,), Some(&kwargs)) {
            Ok(h) => h.cast::<PyDict>()?.clone(),
            Err(e) => return Err(e),
        };

        if hints.is_empty() {
            if let Some(origin) = generic_origin.as_ref() {
                if let Ok(origin_name) = origin.getattr("__name__") {
                    localns.set_item(origin_name, &origin)?;
                }
                hints = match get_type_hints.call((origin,), Some(&kwargs)) {
                    Ok(h) => h.cast::<PyDict>()?.clone(),
                    Err(e) => return Err(e),
                };
            }
        }

        if hints.is_empty() {
            return Ok(());
        }

        // 3. Lowering Phase
        let mut fields: Vec<FieldDef> = Vec::new();
        let mut tags_seen = HashMap::new();

        for (name, type_hint) in hints.iter() {
            let name = name.extract::<String>()?;
            if name.starts_with("__") {
                continue;
            }

            let origin = get_origin.call1((&type_hint,))?;

            // Check Annotated compatibility
            let is_annotated = if let Ok(annotated) = typing.getattr("Annotated") {
                origin.is(&annotated)
            } else {
                false
            };

            if !is_annotated {
                continue;
            }

            let args = get_args.call1((&type_hint,))?;
            if args.len()? < 2 {
                continue;
            }

            let real_type = args.get_item(0)?;
            let tag_obj = args.get_item(1)?;

            let tag = match tag_obj.extract::<u8>() {
                Ok(t) => t,
                Err(_) => continue,
            };

            if let Some(existing) = tags_seen.get(&tag) {
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "Duplicate tag {} in '{}' and '{}'",
                    tag, existing, name
                )));
            }
            tags_seen.insert(tag, name.clone());

            // Dual IR Construction
            let type_expr = parse_type_expr(&real_type, &get_origin, &get_args, &typevar_map)?;

            let wire_type = type_expr.lower();
            let is_optional = type_expr.is_optional();
            // Default required if not optional. Future: check for default values.
            let has_default = false;
            let is_required = !is_optional && !has_default;

            fields.push(FieldDef {
                name: name.clone(),
                tag,
                ty: type_expr,
                wire_type,
                is_optional,
                is_required,
            });
        }

        // 4. Register Schema
        if !fields.is_empty() {
            fields.sort_by_key(|f| f.tag);

            let mut tag_index = HashMap::new();
            let mut name_to_tag = HashMap::new();

            for (idx, f) in fields.iter().enumerate() {
                tag_index.insert(f.tag, idx);
                name_to_tag.insert(f.name.clone(), f.tag);
            }

            let def = StructDef {
                class: Arc::new(cls.clone().unbind()),
                fields_sorted: fields,
                tag_index,
                name_to_tag,
            };

            // Register global schema (class is now part of StructDef)
            let type_ptr = cls.as_ptr() as usize;
            REGISTRY.write().unwrap().insert(type_ptr, def);
        }

        Ok(())
    }
}

// ==========================================
// Constructor Logic
// ==========================================

fn construct_instance(
    def: &StructDef,
    self_obj: &Bound<'_, PyAny>,
    args: &Bound<'_, PyTuple>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    // Positional args for fields map directly to args tuple
    let num_positional = args.len();
    let given_args = args;

    if num_positional > def.fields_sorted.len() {
        let expected = def.fields_sorted.len() + 1;
        let given = num_positional + 1;
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "__init__() takes {} positional arguments but {} were given",
            expected, given
        )));
    }

    for (i, field) in def.fields_sorted.iter().enumerate() {
        let val = if i < num_positional {
            // It is provided positionally
            // Check Collision with kwargs
            if let Some(k) = kwargs {
                if k.contains(&field.name)? {
                    return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                        "__init__() got multiple values for argument '{}'",
                        field.name
                    )));
                }
            }
            Some(given_args.get_item(i)?)
        } else {
            // Look in keyword args
            if let Some(k) = kwargs {
                match k.get_item(&field.name)? {
                    Some(v) => Some(v),
                    None => None,
                }
            } else {
                None
            }
        };

        match val {
            Some(v) => self_obj.setattr(&field.name, v)?,
            None => {
                if field.is_optional {
                    // Default to None for optional fields if missing
                    self_obj.setattr(&field.name, pyo3::types::PyNone::get(self_obj.py()))?;
                } else if field.is_required {
                    return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                        "__init__() missing 1 required positional argument: '{}'",
                        field.name
                    )));
                } else {
                    // Should be covered by required check, but safe fallback
                    return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                        "__init__() missing 1 required argument: '{}'",
                        field.name
                    )));
                }
            }
        }
    }

    // Check for unexpected keyword args
    if let Some(k) = kwargs {
        for key in k.keys() {
            let key_str = key.extract::<String>()?;
            if !def.name_to_tag.contains_key(&key_str) {
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "__init__() got an unexpected keyword argument '{}'",
                    key_str
                )));
            }
        }
    }

    Ok(())
}

// ==========================================
// AST Parser
// ==========================================

fn parse_type_expr(
    ty: &Bound<'_, PyAny>,
    get_origin: &Bound<'_, PyAny>,
    get_args: &Bound<'_, PyAny>,
    typevar_map: &HashMap<usize, Bound<'_, PyAny>>,
) -> PyResult<TypeExpr> {
    let ptr = ty.as_ptr() as usize;
    let resolved_ty = typevar_map.get(&ptr).unwrap_or(ty);

    if resolved_ty.is_instance_of::<PyString>() {
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "Forward references not supported yet: {}",
            resolved_ty
        )));
    }

    if let Ok(name) = resolved_ty.getattr("__name__") {
        if let Ok(name_str) = name.extract::<String>() {
            match name_str.as_str() {
                "int" => return Ok(TypeExpr::Primitive(WireType::Int)),
                "str" => return Ok(TypeExpr::Primitive(WireType::String)),
                "float" => return Ok(TypeExpr::Primitive(WireType::Double)),
                "bool" => return Ok(TypeExpr::Primitive(WireType::Int)),
                "bytes" => return Ok(TypeExpr::List(Box::new(TypeExpr::Primitive(WireType::Int)))),
                _ => {}
            }
        }
    }

    let origin = get_origin.call1((resolved_ty,))?;
    if !origin.is_none() {
        let original_name: String = origin.getattr("__name__")?.extract()?;
        let args = get_args.call1((resolved_ty,))?;

        if original_name == "list" && args.len()? > 0 {
            let inner = args.get_item(0)?;
            let inner_expr = parse_type_expr(&inner, get_origin, get_args, typevar_map)?;
            return Ok(TypeExpr::List(Box::new(inner_expr)));
        }

        if original_name == "dict" && args.len()? >= 2 {
            let k = args.get_item(0)?;
            let v = args.get_item(1)?;
            let k_expr = parse_type_expr(&k, get_origin, get_args, typevar_map)?;
            let v_expr = parse_type_expr(&v, get_origin, get_args, typevar_map)?;
            return Ok(TypeExpr::Map(Box::new(k_expr), Box::new(v_expr)));
        }

        if original_name == "Union" || original_name == "UnionType" {
            let len = args.len()?;
            let mut has_none = false;
            let mut inner_types = Vec::new();

            for i in 0..len {
                let arg = args.get_item(i)?;
                let is_none = arg.is_none() || arg.repr()?.to_string().contains("NoneType");
                if is_none {
                    has_none = true;
                } else {
                    inner_types.push(arg);
                }
            }

            if has_none && inner_types.len() == 1 {
                let inner = &inner_types[0];
                let inner_expr = parse_type_expr(inner, get_origin, get_args, typevar_map)?;
                return Ok(TypeExpr::Optional(Box::new(inner_expr)));
            }
        }
    }

    if resolved_ty.hasattr("__module__")? {
        let ptr = resolved_ty.as_ptr() as usize;
        return Ok(TypeExpr::Struct(ptr));
    }

    Err(pyo3::exceptions::PyTypeError::new_err(format!(
        "Unsupported Tars type: {}",
        resolved_ty
    )))
}
