use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyList, PyString, PyTuple, PyType};
use std::collections::HashMap;
use std::sync::Arc;

use crate::binding::core::{
    Constraints, FieldDef, SCHEMA_ATTR, SCHEMA_CACHE, Schema, SchemaConfig, StructConfig,
    StructDef, StructMetaData, TypeExpr, UnionCache, WireType, is_nodefault, nodefault_singleton,
};
use crate::binding::parse::{ConstraintsIR, TypeInfoIR, introspect_struct_fields};

fn schema_to_python(py: Python<'_>, def: Arc<StructDef>) -> PyResult<Py<Schema>> {
    Py::new(py, Schema { def })
}

pub fn compile_schema_from_info<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
    info: &Bound<'py, PyAny>,
    config: SchemaConfig,
) -> PyResult<Option<Arc<StructDef>>> {
    if let Ok(params) = cls.getattr("__parameters__")
        && let Ok(tuple) = params.cast::<PyTuple>()
        && !tuple.is_empty()
    {
        return Ok(None);
    }
    if info.is_none() {
        return Ok(None);
    }

    let fields_any = info.getattr("fields")?;
    let fields = fields_any.cast::<PyTuple>()?;
    let mut fields_def: Vec<FieldDef> = Vec::new();
    let mut tags_seen = HashMap::new();

    for field_any in fields.iter() {
        let name: String = field_any.getattr("name")?.extract()?;
        let name_py = PyString::intern(py, name.as_str()).unbind();
        let tag: u8 = field_any.getattr("tag")?.extract()?;

        if let Some(existing) = tags_seen.get(&tag) {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "Duplicate tag {} in '{}' and '{}'",
                tag, existing, name
            )));
        }
        tags_seen.insert(tag, name.clone());

        let type_any = field_any.getattr("type")?;
        let type_expr = parse_type_info(&type_any)?;

        let is_optional: bool = field_any.getattr("optional")?.extract()?;
        let has_default: bool = field_any.getattr("has_default")?.extract()?;
        let default_any = field_any.getattr("default")?;
        let default_factory_any = field_any.getattr("default_factory").ok();
        let mut default_value = if has_default && !is_nodefault(&default_any)? {
            Some(default_any.unbind())
        } else {
            None
        };
        let default_factory = if has_default {
            if let Some(factory_obj) = default_factory_any.as_ref() {
                if !is_nodefault(factory_obj)? {
                    Some(factory_obj.clone().unbind())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };
        if default_value.is_none() && is_optional {
            default_value = Some(py.None());
        }
        let is_required = !is_optional && default_value.is_none() && default_factory.is_none();

        let constraints_any = field_any.getattr("constraints")?;
        let constraints = parse_constraints(&constraints_any, name.as_str())?;

        fields_def.push(FieldDef {
            name,
            name_py,
            tag,
            ty: type_expr,
            default_value,
            default_factory,
            is_optional,
            is_required,
            init: true,
            constraints,
        });
    }

    compile_schema_from_fields(py, cls, fields_def, config)
}

pub fn compile_schema_from_class<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
    config: SchemaConfig,
) -> PyResult<Option<Arc<StructDef>>> {
    let Some(fields_ir) = introspect_struct_fields(py, cls)? else {
        return Ok(None);
    };

    let mut fields_def: Vec<FieldDef> = Vec::with_capacity(fields_ir.len());
    for field in fields_ir {
        let name = field.name;
        let name_py = PyString::intern(py, name.as_str()).unbind();
        let type_expr = type_info_ir_to_type_expr(py, &field.typ)?;
        let constraints =
            constraints_ir_to_constraints(py, field.constraints.as_ref(), name.as_str())?;

        let default_value = if field.has_default {
            field.default_value.as_ref().map(|v| v.clone_ref(py))
        } else {
            None
        };
        // if default_value.is_none() && field.is_optional {
        //     default_value = Some(py.None());
        // }

        fields_def.push(FieldDef {
            name,
            name_py,
            tag: field.tag,
            ty: type_expr,
            default_value,
            default_factory: field.default_factory,
            is_optional: field.is_optional,
            is_required: field.is_required,
            init: field.init,
            constraints,
        });
    }

    compile_schema_from_fields(py, cls, fields_def, config)
}

fn compile_schema_from_fields<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
    mut fields_def: Vec<FieldDef>,
    config: SchemaConfig,
) -> PyResult<Option<Arc<StructDef>>> {
    if fields_def.is_empty() {
        return Ok(None);
    }

    fields_def.sort_by_key(|f| f.tag);

    let mut name_to_index = HashMap::with_capacity(fields_def.len());
    let mut name_ptr_to_index = HashMap::with_capacity(fields_def.len());
    let mut max_tag = 0;

    for (idx, f) in fields_def.iter().enumerate() {
        name_to_index.insert(f.name.clone(), idx);
        name_ptr_to_index.insert(f.name_py.as_ptr() as usize, idx);
        if f.tag > max_tag {
            max_tag = f.tag;
        }
    }

    let meta = Arc::new(StructMetaData {
        name_to_index,
        name_ptr_to_index,
    });

    let mut tag_lookup_vec = vec![None; (max_tag as usize) + 1];
    for (idx, f) in fields_def.iter().enumerate() {
        tag_lookup_vec[f.tag as usize] = Some(idx);
    }

    let def = StructDef {
        class_ptr: cls.as_ptr() as usize,
        name: cls.name()?.to_string(),
        fields_sorted: fields_def,
        tag_lookup_vec,
        meta,
        frozen: config.frozen,
        order: config.order,
        forbid_unknown_tags: config.forbid_unknown_tags,
        eq: config.eq,
        omit_defaults: config.omit_defaults,
        repr_omit_defaults: config.repr_omit_defaults,
        kw_only: config.kw_only,
        dict: config.dict,
        weakref: config.weakref,
        simplelist: config.simplelist,
    };

    let def = Arc::new(def);
    let capsule = schema_to_python(py, Arc::clone(&def))?;
    cls.setattr(SCHEMA_ATTR, capsule)?;
    SCHEMA_CACHE.with(|cache| {
        cache
            .borrow_mut()
            .insert(cls.as_ptr() as usize, Arc::downgrade(&def));
    });

    let mut field_names = Vec::with_capacity(def.fields_sorted.len());
    for field in &def.fields_sorted {
        field_names.push(field.name_py.bind(py).to_owned().into_any().unbind());
    }
    let fields_tuple = PyTuple::new(py, field_names)?;
    cls.setattr("__struct_fields__", fields_tuple)?;

    let struct_config = Py::new(py, StructConfig::from_schema_config(&config))?;
    cls.setattr("__struct_config__", struct_config)?;

    let signature = build_signature(py, &def, &config)?;
    cls.setattr("__signature__", signature)?;
    Ok(Some(def))
}

fn type_info_ir_to_type_expr(py: Python<'_>, typ: &TypeInfoIR) -> PyResult<TypeExpr> {
    match typ {
        TypeInfoIR::Int => Ok(TypeExpr::Primitive(WireType::Int)),
        TypeInfoIR::Str => Ok(TypeExpr::Primitive(WireType::String)),
        TypeInfoIR::Float => Ok(TypeExpr::Primitive(WireType::Double)),
        TypeInfoIR::Bool => Ok(TypeExpr::Primitive(WireType::Bool)),
        TypeInfoIR::Bytes => Ok(TypeExpr::List(Box::new(TypeExpr::Primitive(WireType::Int)))),
        TypeInfoIR::Any => Ok(TypeExpr::Any),
        TypeInfoIR::NoneType => Ok(TypeExpr::NoneType),
        TypeInfoIR::TypedDict => Ok(TypeExpr::Map(
            Box::new(TypeExpr::Primitive(WireType::String)),
            Box::new(TypeExpr::Any),
        )),
        TypeInfoIR::Dataclass(cls) => Ok(TypeExpr::Dataclass(cls.clone_ref(py))),
        TypeInfoIR::NamedTuple(cls, items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(type_info_ir_to_type_expr(py, item)?);
            }
            Ok(TypeExpr::NamedTuple(cls.clone_ref(py), out))
        }
        TypeInfoIR::TarsDict => Ok(TypeExpr::TarsDict),
        TypeInfoIR::Set(inner) => Ok(TypeExpr::Set(Box::new(type_info_ir_to_type_expr(
            py, inner,
        )?))),
        TypeInfoIR::Enum(cls, inner) => Ok(TypeExpr::Enum(
            cls.clone_ref(py),
            Box::new(type_info_ir_to_type_expr(py, inner)?),
        )),
        TypeInfoIR::Union(items) => {
            let mut variants = Vec::with_capacity(items.len());
            for item in items {
                variants.push(type_info_ir_to_type_expr(py, item)?);
            }
            Ok(TypeExpr::Union(variants, UnionCache::default()))
        }
        TypeInfoIR::List(inner) => Ok(TypeExpr::List(Box::new(type_info_ir_to_type_expr(
            py, inner,
        )?))),
        TypeInfoIR::Tuple(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(type_info_ir_to_type_expr(py, item)?);
            }
            Ok(TypeExpr::Tuple(out))
        }
        TypeInfoIR::VarTuple(inner) => Ok(TypeExpr::VarTuple(Box::new(type_info_ir_to_type_expr(
            py, inner,
        )?))),
        TypeInfoIR::Map(k, v) => Ok(TypeExpr::Map(
            Box::new(type_info_ir_to_type_expr(py, k)?),
            Box::new(type_info_ir_to_type_expr(py, v)?),
        )),
        TypeInfoIR::Optional(inner) => Ok(TypeExpr::Optional(Box::new(type_info_ir_to_type_expr(
            py, inner,
        )?))),
        TypeInfoIR::Struct(cls) => Ok(TypeExpr::Struct(cls.as_ptr() as usize)),
    }
}

fn constraints_ir_to_constraints(
    py: Python<'_>,
    constraints: Option<&ConstraintsIR>,
    field_name: &str,
) -> PyResult<Option<Box<Constraints>>> {
    let Some(c) = constraints else {
        return Ok(None);
    };

    if !has_any_constraints(
        c.gt.is_some(),
        c.lt.is_some(),
        c.ge.is_some(),
        c.le.is_some(),
        c.min_len.is_some(),
        c.max_len.is_some(),
        c.pattern.is_some(),
    ) {
        return Ok(None);
    }

    let pattern = if let Some(p) = c.pattern.as_deref() {
        let re = py.import("re")?;
        let compile = re.getattr("compile")?;
        let pattern_obj = compile.call1((p,)).map_err(|e| {
            pyo3::exceptions::PyTypeError::new_err(format!(
                "Invalid regex pattern for field '{}': {}",
                field_name, e
            ))
        })?;
        Some(pattern_obj.unbind())
    } else {
        None
    };

    Ok(Some(Box::new(Constraints {
        gt: c.gt,
        lt: c.lt,
        ge: c.ge,
        le: c.le,
        min_len: c.min_len,
        max_len: c.max_len,
        pattern,
    })))
}

fn build_signature(py: Python<'_>, def: &StructDef, config: &SchemaConfig) -> PyResult<Py<PyAny>> {
    let inspect = py.import("inspect")?;
    let param_cls = inspect.getattr("Parameter")?;
    let sig_cls = inspect.getattr("Signature")?;
    let params = PyList::empty(py);
    let nodefault = nodefault_singleton(py)?;
    let mut seen_default = false;

    for field in &def.fields_sorted {
        let kwargs = PyDict::new(py);
        let mut has_default = false;

        if let Some(default_val) = field.default_value.as_ref() {
            kwargs.set_item("default", default_val.bind(py))?;
            has_default = true;
        } else if field.default_factory.is_some() {
            kwargs.set_item("default", nodefault.bind(py))?;
            has_default = true;
        } else if field.is_optional {
            kwargs.set_item("default", py.None())?;
            has_default = true;
        }

        let kind = if config.kw_only || (seen_default && !has_default) {
            param_cls.getattr("KEYWORD_ONLY")?
        } else {
            param_cls.getattr("POSITIONAL_OR_KEYWORD")?
        };

        if has_default {
            seen_default = true;
        }

        let param = if kwargs.is_empty() {
            param_cls.call1((field.name_py.bind(py), kind))?
        } else {
            param_cls.call((field.name_py.bind(py), kind), Some(&kwargs))?
        };
        params.append(param)?;
    }

    let sig = sig_cls.call1((params,))?;
    Ok(sig.unbind())
}

fn parse_type_info(obj: &Bound<'_, PyAny>) -> PyResult<TypeExpr> {
    let kind: String = obj.getattr("kind")?.extract()?;
    match kind.as_str() {
        "int" => Ok(TypeExpr::Primitive(WireType::Int)),
        "str" => Ok(TypeExpr::Primitive(WireType::String)),
        "float" => Ok(TypeExpr::Primitive(WireType::Double)),
        "bool" => Ok(TypeExpr::Primitive(WireType::Bool)),
        "bytes" => Ok(TypeExpr::List(Box::new(TypeExpr::Primitive(WireType::Int)))),
        "any" => Ok(TypeExpr::Any),
        "none" => Ok(TypeExpr::NoneType),
        "tarsdict" => Ok(TypeExpr::TarsDict),
        "typeddict" | "dataclass" => Ok(TypeExpr::Map(
            Box::new(TypeExpr::Primitive(WireType::String)),
            Box::new(TypeExpr::Any),
        )),
        "set" => {
            let inner_any = obj.getattr("item_type")?;
            let inner = parse_type_info(&inner_any)?;
            Ok(TypeExpr::Set(Box::new(inner)))
        }
        "list" => {
            let inner_any = obj.getattr("item_type")?;
            let inner = parse_type_info(&inner_any)?;
            Ok(TypeExpr::List(Box::new(inner)))
        }
        "tuple" => {
            let items_any = obj.getattr("items")?;
            let items = items_any.cast::<PyTuple>()?;
            let mut out = Vec::with_capacity(items.len());
            for item in items.iter() {
                out.push(parse_type_info(&item)?);
            }
            Ok(TypeExpr::Tuple(out))
        }
        "namedtuple" => {
            let items_any = obj.getattr("items")?;
            let items = items_any.cast::<PyTuple>()?;
            let mut out = Vec::with_capacity(items.len());
            for item in items.iter() {
                out.push(parse_type_info(&item)?);
            }
            Ok(TypeExpr::Tuple(out))
        }
        "var_tuple" => {
            let inner_any = obj.getattr("item_type")?;
            let inner = parse_type_info(&inner_any)?;
            Ok(TypeExpr::VarTuple(Box::new(inner)))
        }
        "map" => {
            let key_any = obj.getattr("key_type")?;
            let value_any = obj.getattr("value_type")?;
            let key = parse_type_info(&key_any)?;
            let value = parse_type_info(&value_any)?;
            Ok(TypeExpr::Map(Box::new(key), Box::new(value)))
        }
        "optional" => {
            let inner_any = obj.getattr("inner_type")?;
            let inner = parse_type_info(&inner_any)?;
            Ok(TypeExpr::Optional(Box::new(inner)))
        }
        "enum" => {
            let cls_any = obj.getattr("cls")?;
            let cls = cls_any.cast::<PyType>()?;
            let inner_any = obj.getattr("value_type")?;
            let inner = parse_type_info(&inner_any)?;
            Ok(TypeExpr::Enum(cls.clone().unbind(), Box::new(inner)))
        }
        "union" => {
            let variants_any = obj.getattr("variants")?;
            let variants = variants_any.cast::<PyTuple>()?;
            let mut items = Vec::with_capacity(variants.len());
            for v in variants.iter() {
                items.push(parse_type_info(&v)?);
            }
            Ok(TypeExpr::Union(items, UnionCache::default()))
        }
        "struct" => {
            let cls_any = obj.getattr("cls")?;
            let cls = cls_any.cast::<PyType>()?;
            Ok(TypeExpr::Struct(cls.as_ptr() as usize))
        }
        _ => Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "Unsupported Tars type: {}",
            kind
        ))),
    }
}

fn parse_constraints(
    obj: &Bound<'_, PyAny>,
    field_name: &str,
) -> PyResult<Option<Box<Constraints>>> {
    if obj.is_none() {
        return Ok(None);
    }

    let gt: Option<f64> = obj.getattr("gt")?.extract()?;
    let lt: Option<f64> = obj.getattr("lt")?.extract()?;
    let ge: Option<f64> = obj.getattr("ge")?.extract()?;
    let le: Option<f64> = obj.getattr("le")?.extract()?;
    let min_len: Option<usize> = obj.getattr("min_len")?.extract()?;
    let max_len: Option<usize> = obj.getattr("max_len")?.extract()?;
    let pattern_str: Option<String> = obj.getattr("pattern")?.extract()?;

    if !has_any_constraints(
        gt.is_some(),
        lt.is_some(),
        ge.is_some(),
        le.is_some(),
        min_len.is_some(),
        max_len.is_some(),
        pattern_str.is_some(),
    ) {
        return Ok(None);
    }

    let pattern = match pattern_str.as_deref() {
        Some(p) => {
            let py = obj.py();
            let re_module = py.import("re")?;
            let pattern_obj = re_module.call_method1("compile", (p,)).map_err(|e| {
                pyo3::exceptions::PyTypeError::new_err(format!(
                    "Invalid regex pattern for field '{}': {}",
                    field_name, e
                ))
            })?;
            Some(pattern_obj.unbind())
        }
        None => None,
    };

    Ok(Some(Box::new(Constraints {
        gt,
        lt,
        ge,
        le,
        min_len,
        max_len,
        pattern,
    })))
}

#[inline]
fn has_any_constraints(
    gt: bool,
    lt: bool,
    ge: bool,
    le: bool,
    min_len: bool,
    max_len: bool,
    pattern: bool,
) -> bool {
    gt || lt || ge || le || min_len || max_len || pattern
}
