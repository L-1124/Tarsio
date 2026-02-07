use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyString, PyTuple, PyType};
use std::collections::{HashMap, HashSet};

use crate::binding::meta::Meta;
use crate::binding::schema::Struct;

#[derive(Debug)]
pub struct ConstraintsIR {
    pub gt: Option<f64>,
    pub lt: Option<f64>,
    pub ge: Option<f64>,
    pub le: Option<f64>,
    pub min_len: Option<usize>,
    pub max_len: Option<usize>,
    pub pattern: Option<String>,
}

#[derive(Debug)]
pub enum TypeInfoIR {
    Int,
    Str,
    Float,
    Bool,
    Bytes,
    Any,
    NoneType,
    DateTime,
    Date,
    Time,
    Timedelta,
    Uuid,
    Decimal,
    Enum(Py<PyType>, Box<TypeInfoIR>),
    Union(Vec<TypeInfoIR>),
    List(Box<TypeInfoIR>),
    Tuple(Box<TypeInfoIR>),
    Map(Box<TypeInfoIR>, Box<TypeInfoIR>),
    Optional(Box<TypeInfoIR>),
    Struct(Py<PyType>),
}

#[derive(Debug)]
pub struct FieldInfoIR {
    pub name: String,
    pub tag: u8,
    pub typ: TypeInfoIR,
    pub default_value: Option<Py<PyAny>>,
    pub default_factory: Option<Py<PyAny>>,
    pub has_default: bool,
    pub is_optional: bool,
    pub is_required: bool,
    pub init: bool,
    pub constraints: Option<ConstraintsIR>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructKind {
    TarsStruct,
    Dataclass,
    NamedTuple,
    TypedDict,
}

pub fn introspect_struct_fields<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
) -> PyResult<Option<Vec<FieldInfoIR>>> {
    if is_generic_template(cls)? {
        return Ok(None);
    }

    let Some(kind) = detect_struct_kind(py, cls)? else {
        return Ok(None);
    };

    match kind {
        StructKind::TarsStruct => introspect_tars_struct_fields(py, cls),
        StructKind::Dataclass => introspect_dataclass_fields(py, cls),
        StructKind::NamedTuple => introspect_namedtuple_fields(py, cls),
        StructKind::TypedDict => introspect_typeddict_fields(py, cls),
    }
}

pub fn introspect_type_info_ir<'py>(
    py: Python<'py>,
    tp: &Bound<'py, PyAny>,
) -> PyResult<(TypeInfoIR, Option<ConstraintsIR>)> {
    let typing = py.import("typing")?;
    let origin = typing.call_method1("get_origin", (tp,))?;
    let annotated = typing.getattr("Annotated")?;
    if !origin.is_none() && origin.is(&annotated) {
        let args_any = typing.call_method1("get_args", (tp,))?;
        let args = args_any.cast::<PyTuple>()?;
        let (real_type, _tag, constraints) = parse_annotated_args("_", args)?;
        let typevar_map = HashMap::new();
        let (typ, _is_optional) = translate_type_info_ir(py, &real_type, &typevar_map)?;
        return Ok((typ, constraints));
    }

    let typevar_map = HashMap::new();
    let (typ, _is_optional) = translate_type_info_ir(py, tp, &typevar_map)?;
    Ok((typ, None))
}

type GenericOrigin<'py> = (Option<Bound<'py, PyAny>>, Option<Bound<'py, PyTuple>>);

fn is_generic_template(cls: &Bound<'_, PyType>) -> PyResult<bool> {
    if let Ok(params) = cls.getattr("__parameters__")
        && let Ok(tuple) = params.cast::<PyTuple>()
    {
        return Ok(!tuple.is_empty());
    }
    Ok(false)
}

fn resolve_generic_origin<'py>(cls: &Bound<'py, PyType>) -> PyResult<GenericOrigin<'py>> {
    if let (Ok(origin), Ok(args)) = (cls.getattr("__origin__"), cls.getattr("__args__"))
        && !origin.is_none()
        && !args.is_none()
    {
        if let Ok(tup) = args.clone().cast_into::<PyTuple>() {
            return Ok((Some(origin), Some(tup)));
        }
        if let Ok(seq) = args.try_iter() {
            let collected: Vec<Py<PyAny>> = seq
                .map(|item| item.map(|v| v.unbind()))
                .collect::<Result<_, _>>()?;
            let tup = PyTuple::new(cls.py(), collected)?;
            return Ok((Some(origin), Some(tup)));
        }
    }

    if let Ok(orig_bases) = cls.getattr("__orig_bases__")
        && let Ok(bases) = orig_bases.cast::<PyTuple>()
    {
        for base in bases.iter() {
            if let (Ok(base_origin), Ok(base_args)) =
                (base.getattr("__origin__"), base.getattr("__args__"))
                && !base_origin.is_none()
                && !base_args.is_none()
            {
                if let Ok(tup) = base_args.clone().cast_into::<PyTuple>() {
                    return Ok((Some(base_origin), Some(tup)));
                }
                if let Ok(seq) = base_args.try_iter() {
                    let collected: Vec<Py<PyAny>> = seq
                        .map(|item| item.map(|v| v.unbind()))
                        .collect::<Result<_, _>>()?;
                    let tup = PyTuple::new(cls.py(), collected)?;
                    return Ok((Some(base_origin), Some(tup)));
                }
            }
        }
    }
    Ok((None, None))
}

fn build_typevar_map<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
) -> PyResult<HashMap<usize, Bound<'py, PyAny>>> {
    let mut map: HashMap<usize, Bound<'py, PyAny>> = HashMap::new();

    let (origin, args) = resolve_generic_origin(cls)?;
    if let (Some(origin), Some(args)) = (origin, args)
        && let Ok(params_any) = origin.getattr("__parameters__")
        && let Ok(params) = params_any.cast::<PyTuple>()
    {
        for (param, arg) in params.iter().zip(args.iter()) {
            map.insert(param.as_ptr() as usize, arg);
        }
        return Ok(map);
    }

    let typing = py.import("typing")?;
    let typevar_cls = typing.getattr("TypeVar")?;

    if let Ok(params_any) = cls.getattr("__parameters__")
        && let Ok(params) = params_any.cast::<PyTuple>()
    {
        for param in params.iter() {
            if param.is_instance(&typevar_cls)? {
                map.insert(param.as_ptr() as usize, param);
            }
        }
    }

    Ok(map)
}

fn get_type_hints_with_fallback<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
) -> PyResult<Bound<'py, PyDict>> {
    let typing = py.import("typing")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("include_extras", true)?;

    let localns = PyDict::new(py);
    let cls_name: String = cls.getattr("__name__")?.extract()?;
    localns.set_item(cls_name.as_str(), cls)?;
    kwargs.set_item("localns", &localns)?;

    let hints_any = typing.call_method("get_type_hints", (cls,), Some(&kwargs))?;
    let mut hints = hints_any.cast::<PyDict>()?.clone();
    if !hints.is_empty() {
        return Ok(hints);
    }

    let (origin, _args) = resolve_generic_origin(cls)?;
    if let Some(origin) = origin
        && let Ok(origin_type) = origin.cast::<PyType>()
    {
        let origin_name: String = origin_type.getattr("__name__")?.extract()?;
        localns.set_item(origin_name.as_str(), origin_type)?;
        let origin_hints_any =
            typing.call_method("get_type_hints", (origin_type,), Some(&kwargs))?;
        hints = origin_hints_any.cast::<PyDict>()?.clone();
    }

    Ok(hints)
}

pub fn detect_struct_kind<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
) -> PyResult<Option<StructKind>> {
    if cls.is_subclass_of::<Struct>()? {
        return Ok(Some(StructKind::TarsStruct));
    }
    if is_dataclass_class(py, cls)? {
        return Ok(Some(StructKind::Dataclass));
    }
    if is_namedtuple_class(py, cls)? {
        return Ok(Some(StructKind::NamedTuple));
    }
    if is_typed_dict_class(py, cls)? {
        return Ok(Some(StructKind::TypedDict));
    }
    Ok(None)
}

fn is_dataclass_class<'py>(py: Python<'py>, cls: &Bound<'py, PyType>) -> PyResult<bool> {
    let dataclasses = py.import("dataclasses")?;
    let is_dc = dataclasses
        .getattr("is_dataclass")?
        .call1((cls,))?
        .is_truthy()?;
    Ok(is_dc)
}

fn is_namedtuple_class<'py>(_py: Python<'py>, cls: &Bound<'py, PyType>) -> PyResult<bool> {
    if !cls.is_subclass_of::<PyTuple>()? {
        return Ok(false);
    }
    let Ok(fields_any) = cls.getattr("_fields") else {
        return Ok(false);
    };
    Ok(fields_any.cast::<PyTuple>().is_ok())
}

fn is_typed_dict_class<'py>(py: Python<'py>, cls: &Bound<'py, PyType>) -> PyResult<bool> {
    let typing = py.import("typing")?;
    if let Ok(meta) = typing.getattr("_TypedDictMeta")
        && cls.is_instance(&meta)?
    {
        return Ok(true);
    }
    Ok(cls.getattr("__total__").is_ok() && cls.getattr("__annotations__").is_ok())
}

fn is_subclass<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
    base: &Bound<'py, PyAny>,
) -> PyResult<bool> {
    let builtins = py.import("builtins")?;
    let issubclass = builtins.getattr("issubclass")?;
    issubclass.call1((cls, base))?.is_truthy()
}

fn introspect_tars_struct_fields<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
) -> PyResult<Option<Vec<FieldInfoIR>>> {
    let typevar_map = build_typevar_map(py, cls)?;
    let hints = get_type_hints_with_fallback(py, cls)?;
    if hints.is_empty() {
        return Ok(None);
    }

    let typing = py.import("typing")?;
    let annotated = typing.getattr("Annotated")?;

    let mut fields = Vec::new();
    let mut tags_seen: HashMap<u8, String> = HashMap::new();

    for (name_obj, type_hint) in hints.iter() {
        let name: String = name_obj.extract()?;
        if name.starts_with("__") {
            continue;
        }

        let origin = typing.call_method1("get_origin", (&type_hint,))?;
        if origin.is_none() || !origin.is(&annotated) {
            continue;
        }

        let args_any = typing.call_method1("get_args", (&type_hint,))?;
        let args = args_any.cast::<PyTuple>()?;
        let (real_type, tag, constraints) = parse_annotated_args(name.as_str(), args)?;

        if let Some(existing) = tags_seen.get(&tag) {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "Duplicate tag {} in '{}' and '{}'",
                tag, existing, name
            )));
        }
        tags_seen.insert(tag, name.clone());

        let (typ, is_optional) = translate_type_info_ir(py, &real_type, &typevar_map)?;

        let (has_default, default_val) = lookup_default_value(py, cls, name.as_str())?;
        let (has_default, default_value) = if !has_default && is_optional {
            (true, Some(py.None()))
        } else if has_default {
            (true, default_val)
        } else {
            (false, None)
        };

        let is_required = !is_optional && default_value.is_none();

        fields.push(FieldInfoIR {
            name,
            tag,
            typ,
            default_value,
            default_factory: None,
            has_default,
            is_optional,
            is_required,
            init: true,
            constraints,
        });
    }

    if fields.is_empty() {
        return Ok(None);
    }
    fields.sort_by_key(|f| f.tag);
    Ok(Some(fields))
}

fn introspect_dataclass_fields<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
) -> PyResult<Option<Vec<FieldInfoIR>>> {
    let dataclasses = py.import("dataclasses")?;
    let fields_any = dataclasses.call_method1("fields", (cls,))?;
    let missing = dataclasses.getattr("MISSING")?;
    let typevar_map = build_typevar_map(py, cls)?;
    let hints = get_type_hints_with_fallback(py, cls)?;
    if hints.is_empty() {
        return Ok(None);
    }

    let typing = py.import("typing")?;
    let annotated = typing.getattr("Annotated")?;

    let mut fields = Vec::new();
    let mut tags_seen: HashMap<u8, String> = HashMap::new();

    for (idx, field_any) in fields_any.try_iter()?.enumerate() {
        let field_any = field_any?;
        let name: String = field_any.getattr("name")?.extract()?;
        let init: bool = field_any.getattr("init")?.extract()?;
        let type_hint = if let Some(hint) = hints.get_item(&name)? {
            hint
        } else {
            field_any.getattr("type")?
        };

        let origin = typing.call_method1("get_origin", (&type_hint,))?;
        let (real_type, tag_opt, constraints) = if !origin.is_none() && origin.is(&annotated) {
            let args_any = typing.call_method1("get_args", (&type_hint,))?;
            let args = args_any.cast::<PyTuple>()?;
            parse_annotated_args_loose(name.as_str(), args)?
        } else {
            (type_hint, None, None)
        };

        let tag = match tag_opt {
            Some(tag) => tag,
            None => {
                if idx > 255 {
                    return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                        "Tag exceeds 255 for field '{}'",
                        name
                    )));
                }
                idx as u8
            }
        };

        if let Some(existing) = tags_seen.get(&tag) {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "Duplicate tag {} in '{}' and '{}'",
                tag, existing, name
            )));
        }
        tags_seen.insert(tag, name.clone());

        let (typ, is_optional) = translate_type_info_ir(py, &real_type, &typevar_map)?;

        let default_any = field_any.getattr("default")?;
        let default_factory_any = field_any.getattr("default_factory")?;

        let mut default_value = if default_any.is(&missing) {
            None
        } else {
            Some(default_any.unbind())
        };
        let default_factory = if default_factory_any.is(&missing) {
            None
        } else {
            Some(default_factory_any.unbind())
        };

        let mut has_default = default_value.is_some() || default_factory.is_some();
        if !has_default && is_optional {
            default_value = Some(py.None());
            has_default = true;
        }

        let is_required = !is_optional && default_value.is_none() && default_factory.is_none();

        fields.push(FieldInfoIR {
            name,
            tag,
            typ,
            default_value,
            default_factory,
            has_default,
            is_optional,
            is_required,
            init,
            constraints,
        });
    }

    if fields.is_empty() {
        return Ok(None);
    }
    fields.sort_by_key(|f| f.tag);
    Ok(Some(fields))
}

fn introspect_namedtuple_fields<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
) -> PyResult<Option<Vec<FieldInfoIR>>> {
    let fields_any = cls.getattr("_fields")?;
    let fields_tuple = fields_any.cast::<PyTuple>()?;
    let typevar_map = build_typevar_map(py, cls)?;
    let hints = get_type_hints_with_fallback(py, cls)?;
    if hints.is_empty() {
        return Ok(None);
    }

    let defaults = match cls.getattr("_field_defaults") {
        Ok(value) => match value.cast::<PyDict>() {
            Ok(dict) => Some(dict.clone().unbind()),
            Err(_) => None,
        },
        Err(_) => None,
    };

    let typing = py.import("typing")?;
    let annotated = typing.getattr("Annotated")?;

    let mut fields = Vec::new();
    let mut tags_seen: HashMap<u8, String> = HashMap::new();

    for (idx, name_any) in fields_tuple.iter().enumerate() {
        let name: String = name_any.extract()?;
        let Some(type_hint) = hints.get_item(&name)? else {
            continue;
        };

        let origin = typing.call_method1("get_origin", (&type_hint,))?;
        let (real_type, tag_opt, constraints) = if !origin.is_none() && origin.is(&annotated) {
            let args_any = typing.call_method1("get_args", (&type_hint,))?;
            let args = args_any.cast::<PyTuple>()?;
            parse_annotated_args_loose(name.as_str(), args)?
        } else {
            (type_hint, None, None)
        };

        let tag = match tag_opt {
            Some(tag) => tag,
            None => {
                if idx > 255 {
                    return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                        "Tag exceeds 255 for field '{}'",
                        name
                    )));
                }
                idx as u8
            }
        };

        if let Some(existing) = tags_seen.get(&tag) {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "Duplicate tag {} in '{}' and '{}'",
                tag, existing, name
            )));
        }
        tags_seen.insert(tag, name.clone());

        let (typ, is_optional) = translate_type_info_ir(py, &real_type, &typevar_map)?;

        let mut default_value = defaults
            .as_ref()
            .and_then(|d| d.bind(py).get_item(&name).ok().flatten())
            .map(|v| v.unbind());
        let default_factory = None;

        let mut has_default = default_value.is_some();
        if !has_default && is_optional {
            default_value = Some(py.None());
            has_default = true;
        }

        let is_required = !is_optional && default_value.is_none();

        fields.push(FieldInfoIR {
            name,
            tag,
            typ,
            default_value,
            default_factory,
            has_default,
            is_optional,
            is_required,
            init: true,
            constraints,
        });
    }

    if fields.is_empty() {
        return Ok(None);
    }
    fields.sort_by_key(|f| f.tag);
    Ok(Some(fields))
}

fn introspect_typeddict_fields<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
) -> PyResult<Option<Vec<FieldInfoIR>>> {
    let annotations_any = cls.getattr("__annotations__")?;
    let annotations = annotations_any.cast::<PyDict>()?;
    if annotations.is_empty() {
        return Ok(None);
    }

    let typevar_map = build_typevar_map(py, cls)?;
    let hints = get_type_hints_with_fallback(py, cls)?;
    let total: bool = cls.getattr("__total__")?.extract().unwrap_or(true);

    let required_keys = cls
        .getattr("__required_keys__")
        .ok()
        .and_then(|v| extract_str_set(&v).ok().flatten());
    let optional_keys = cls
        .getattr("__optional_keys__")
        .ok()
        .and_then(|v| extract_str_set(&v).ok().flatten());

    let typing = py.import("typing")?;
    let annotated = typing.getattr("Annotated")?;

    let mut fields = Vec::new();
    let mut tags_seen: HashMap<u8, String> = HashMap::new();

    for (idx, (name_any, _)) in annotations.iter().enumerate() {
        let name: String = name_any.extract()?;
        let type_hint = hints
            .get_item(&name)?
            .or_else(|| annotations.get_item(&name).ok().flatten());
        let Some(type_hint) = type_hint else {
            continue;
        };

        let origin = typing.call_method1("get_origin", (&type_hint,))?;
        let (real_type, tag_opt, constraints) = if !origin.is_none() && origin.is(&annotated) {
            let args_any = typing.call_method1("get_args", (&type_hint,))?;
            let args = args_any.cast::<PyTuple>()?;
            parse_annotated_args_loose(name.as_str(), args)?
        } else {
            (type_hint, None, None)
        };

        let tag = match tag_opt {
            Some(tag) => tag,
            None => {
                if idx > 255 {
                    return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                        "Tag exceeds 255 for field '{}'",
                        name
                    )));
                }
                idx as u8
            }
        };

        if let Some(existing) = tags_seen.get(&tag) {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "Duplicate tag {} in '{}' and '{}'",
                tag, existing, name
            )));
        }
        tags_seen.insert(tag, name.clone());

        let (typ, is_optional_by_type) = translate_type_info_ir(py, &real_type, &typevar_map)?;

        let mut is_optional = is_optional_by_type;
        if let Some(required) = required_keys.as_ref() {
            is_optional = !required.contains(&name);
        } else if let Some(optional) = optional_keys.as_ref() {
            if optional.contains(&name) {
                is_optional = true;
            }
        } else if !total {
            is_optional = true;
        }

        let mut default_value = None;
        let default_factory = None;
        let mut has_default = false;
        if !has_default && is_optional {
            default_value = Some(py.None());
            has_default = true;
        }

        let is_required = !is_optional && default_value.is_none();

        fields.push(FieldInfoIR {
            name,
            tag,
            typ,
            default_value,
            default_factory,
            has_default,
            is_optional,
            is_required,
            init: true,
            constraints,
        });
    }

    if fields.is_empty() {
        return Ok(None);
    }
    fields.sort_by_key(|f| f.tag);
    Ok(Some(fields))
}

fn extract_str_set<'py>(obj: &Bound<'py, PyAny>) -> PyResult<Option<HashSet<String>>> {
    if obj.is_none() {
        return Ok(None);
    }
    let mut set = HashSet::new();
    if let Ok(iter) = obj.try_iter() {
        for item in iter {
            let item = item?;
            set.insert(item.extract()?);
        }
        return Ok(Some(set));
    }
    Ok(None)
}

fn parse_annotated_args<'py>(
    field_name: &str,
    args: &Bound<'py, PyTuple>,
) -> PyResult<(Bound<'py, PyAny>, u8, Option<ConstraintsIR>)> {
    if args.len() < 2 {
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "Missing tag for field '{}'",
            field_name
        )));
    }

    let real_type = args.get_item(0)?;
    let mut found_int_tag: Option<u8> = None;
    let mut found_meta: Option<PyRef<'py, Meta>> = None;

    for item in args.iter().skip(1) {
        if let Ok(int_tag) = item.extract::<i64>() {
            if !(0..=255).contains(&int_tag) {
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "Tag must be in range 0..=255 for field '{}'",
                    field_name
                )));
            }
            if found_int_tag.is_some() {
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "Multiple integer tags are not allowed for field '{}'",
                    field_name
                )));
            }
            found_int_tag = Some(int_tag as u8);
            continue;
        }

        if let Ok(meta) = item.extract::<PyRef<'py, Meta>>() {
            if found_meta.is_some() {
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "Multiple Meta objects are not allowed for field '{}'",
                    field_name
                )));
            }
            found_meta = Some(meta);
        }
    }

    if found_int_tag.is_some() && found_meta.is_some() {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "Do not mix integer tag and Meta object",
        ));
    }

    if let Some(meta) = found_meta {
        let Some(tag) = meta.tag else {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "Meta object must include 'tag' for field '{}'",
                field_name
            )));
        };

        let constraints = ConstraintsIR {
            gt: meta.gt,
            lt: meta.lt,
            ge: meta.ge,
            le: meta.le,
            min_len: meta.min_len,
            max_len: meta.max_len,
            pattern: meta.pattern.clone(),
        };
        return Ok((real_type, tag, Some(constraints)));
    }

    if let Some(tag) = found_int_tag {
        return Ok((real_type, tag, None));
    }

    Err(pyo3::exceptions::PyTypeError::new_err(format!(
        "Missing tag for field '{}'",
        field_name
    )))
}

fn parse_annotated_args_loose<'py>(
    field_name: &str,
    args: &Bound<'py, PyTuple>,
) -> PyResult<(Bound<'py, PyAny>, Option<u8>, Option<ConstraintsIR>)> {
    let real_type = args.get_item(0)?;
    let mut found_int_tag: Option<u8> = None;
    let mut found_meta: Option<PyRef<'py, Meta>> = None;

    for item in args.iter().skip(1) {
        if let Ok(int_tag) = item.extract::<i64>() {
            if !(0..=255).contains(&int_tag) {
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "Tag must be in range 0..=255 for field '{}'",
                    field_name
                )));
            }
            if found_int_tag.is_some() {
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "Multiple integer tags are not allowed for field '{}'",
                    field_name
                )));
            }
            found_int_tag = Some(int_tag as u8);
            continue;
        }

        if let Ok(meta) = item.extract::<PyRef<'py, Meta>>() {
            if found_meta.is_some() {
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "Multiple Meta objects are not allowed for field '{}'",
                    field_name
                )));
            }
            found_meta = Some(meta);
        }
    }

    if found_int_tag.is_some() && found_meta.is_some() {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "Do not mix integer tag and Meta object",
        ));
    }

    if let Some(meta) = found_meta {
        let Some(tag) = meta.tag else {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "Meta object must include 'tag' for field '{}'",
                field_name
            )));
        };

        let constraints = ConstraintsIR {
            gt: meta.gt,
            lt: meta.lt,
            ge: meta.ge,
            le: meta.le,
            min_len: meta.min_len,
            max_len: meta.max_len,
            pattern: meta.pattern.clone(),
        };
        return Ok((real_type, Some(tag), Some(constraints)));
    }

    Ok((real_type, found_int_tag, None))
}

fn translate_type_info_ir<'py>(
    py: Python<'py>,
    tp: &Bound<'py, PyAny>,
    typevar_map: &HashMap<usize, Bound<'py, PyAny>>,
) -> PyResult<(TypeInfoIR, bool)> {
    let mut resolved = resolve_typevar(py, tp, typevar_map)?;

    if resolved.is_instance_of::<PyString>() {
        let s: String = resolved.extract()?;
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "Forward references not supported yet: {}",
            s
        )));
    }

    let typing = py.import("typing")?;
    let forward_ref = typing.getattr("ForwardRef")?;
    if resolved.is_instance(&forward_ref)? {
        let repr: String = resolved.repr()?.extract()?;
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "Forward references not supported yet: {}",
            repr
        )));
    }

    let annotated = typing.getattr("Annotated")?;
    let union_origin = typing.getattr("Union")?;
    let types_mod = py.import("types")?;
    let union_type = types_mod.getattr("UnionType").ok();
    let builtins = py.import("builtins")?;
    let none_type = py.None().bind(py).get_type();

    let mut forced_optional = false;

    loop {
        if let Ok(super_type) = resolved.getattr("__supertype__") {
            resolved = super_type;
            continue;
        }

        let origin = typing.call_method1("get_origin", (&resolved,))?;
        if origin.is_none() {
            break;
        }

        if origin.is(&annotated) {
            let args_any = typing.call_method1("get_args", (&resolved,))?;
            let args = args_any.cast::<PyTuple>()?;
            let (real_type, _tag, _constraints) = parse_annotated_args_loose("_", args)?;
            resolved = real_type;
            continue;
        }

        if let Ok(final_cls) = typing.getattr("Final")
            && origin.is(&final_cls)
        {
            let args_any = typing.call_method1("get_args", (&resolved,))?;
            let args = args_any.cast::<PyTuple>()?;
            if args.is_empty() {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "Final requires an inner type",
                ));
            }
            resolved = args.get_item(0)?;
            continue;
        }

        if let Ok(type_alias) = typing.getattr("TypeAlias")
            && origin.is(&type_alias)
        {
            let args_any = typing.call_method1("get_args", (&resolved,))?;
            let args = args_any.cast::<PyTuple>()?;
            if args.is_empty() {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "TypeAlias requires an inner type",
                ));
            }
            resolved = args.get_item(0)?;
            continue;
        }

        if let Ok(type_alias_type) = typing.getattr("TypeAliasType")
            && origin.is(&type_alias_type)
        {
            let args_any = typing.call_method1("get_args", (&resolved,))?;
            let args = args_any.cast::<PyTuple>()?;
            if args.is_empty() {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "TypeAliasType requires an inner type",
                ));
            }
            resolved = args.get_item(0)?;
            continue;
        }

        if let Ok(required_cls) = typing.getattr("Required")
            && origin.is(&required_cls)
        {
            let args_any = typing.call_method1("get_args", (&resolved,))?;
            let args = args_any.cast::<PyTuple>()?;
            if args.is_empty() {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "Required requires an inner type",
                ));
            }
            resolved = args.get_item(0)?;
            continue;
        }

        if let Ok(not_required_cls) = typing.getattr("NotRequired")
            && origin.is(&not_required_cls)
        {
            let args_any = typing.call_method1("get_args", (&resolved,))?;
            let args = args_any.cast::<PyTuple>()?;
            if args.is_empty() {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "NotRequired requires an inner type",
                ));
            }
            forced_optional = true;
            resolved = args.get_item(0)?;
            continue;
        }

        if let Ok(literal_cls) = typing.getattr("Literal")
            && origin.is(&literal_cls)
        {
            let args_any = typing.call_method1("get_args", (&resolved,))?;
            let args = args_any.cast::<PyTuple>()?;
            if args.is_empty() {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "Literal requires at least one value",
                ));
            }

            let mut variants = Vec::new();
            let mut seen = HashSet::new();
            let mut has_none = false;

            for val in args.iter() {
                if val.is_none() || val.is(&none_type) {
                    has_none = true;
                    continue;
                }
                let val_type = val.get_type();
                let (typ, _opt) = translate_type_info_ir(py, val_type.as_any(), typevar_map)?;
                let key = format!("{:?}", typ);
                if seen.insert(key) {
                    variants.push(typ);
                }
            }

            if variants.is_empty() {
                return Ok((TypeInfoIR::NoneType, true));
            }
            if has_none {
                forced_optional = true;
            }
            if variants.len() == 1 {
                return Ok((variants.remove(0), forced_optional));
            }
            return Ok((TypeInfoIR::Union(variants), forced_optional));
        }

        let is_union =
            origin.is(&union_origin) || union_type.as_ref().is_some_and(|u| origin.is(u));
        if is_union {
            let args_any = typing.call_method1("get_args", (&resolved,))?;
            let args = args_any.cast::<PyTuple>()?;
            let mut variants = Vec::new();
            let mut has_none = false;
            for a in args.iter() {
                if a.is_none() || a.is(&none_type) {
                    has_none = true;
                } else {
                    let (inner, _opt_inner) = translate_type_info_ir(py, &a, typevar_map)?;
                    variants.push(inner);
                }
            }
            if variants.is_empty() {
                return Ok((TypeInfoIR::NoneType, true));
            }
            if has_none && variants.len() == 1 {
                return Ok((TypeInfoIR::Optional(Box::new(variants.remove(0))), true));
            }
            return Ok((TypeInfoIR::Union(variants), has_none || forced_optional));
        }

        if origin.is(&builtins.getattr("list")?) || origin.is(&builtins.getattr("tuple")?) {
            let args_any = typing.call_method1("get_args", (&resolved,))?;
            let args = args_any.cast::<PyTuple>()?;
            if args.is_empty() {
                let repr: String = resolved.repr()?.extract()?;
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "Unsupported Tars type: {}",
                    repr
                )));
            }
            if origin.is(&builtins.getattr("tuple")?) {
                let ellipsis = py.Ellipsis();
                let inner_any =
                    if args.len() == 1 || (args.len() == 2 && args.get_item(1)?.is(ellipsis)) {
                        args.get_item(0)?
                    } else {
                        let repr: String = resolved.repr()?.extract()?;
                        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                            "Unsupported tuple type: {}",
                            repr
                        )));
                    };
                let (inner, _opt) = translate_type_info_ir(py, &inner_any, typevar_map)?;
                return Ok((TypeInfoIR::Tuple(Box::new(inner)), forced_optional));
            }

            let (inner, _opt) = translate_type_info_ir(py, &args.get_item(0)?, typevar_map)?;
            return Ok((TypeInfoIR::List(Box::new(inner)), forced_optional));
        }

        if origin.is(&builtins.getattr("dict")?) {
            let args_any = typing.call_method1("get_args", (&resolved,))?;
            let args = args_any.cast::<PyTuple>()?;
            if args.len() < 2 {
                let repr: String = resolved.repr()?.extract()?;
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "Unsupported Tars type: {}",
                    repr
                )));
            }
            let (k, _opt_k) = translate_type_info_ir(py, &args.get_item(0)?, typevar_map)?;
            let (v, _opt_v) = translate_type_info_ir(py, &args.get_item(1)?, typevar_map)?;
            return Ok((TypeInfoIR::Map(Box::new(k), Box::new(v)), forced_optional));
        }

        break;
    }

    if let Ok(any_type) = typing.getattr("Any")
        && resolved.is(&any_type)
    {
        return Ok((TypeInfoIR::Any, forced_optional));
    }

    if resolved.is(&none_type) || resolved.is_none() {
        return Ok((TypeInfoIR::NoneType, true));
    }

    if resolved.is(&builtins.getattr("int")?) {
        return Ok((TypeInfoIR::Int, forced_optional));
    }
    if resolved.is(&builtins.getattr("str")?) {
        return Ok((TypeInfoIR::Str, forced_optional));
    }
    if resolved.is(&builtins.getattr("float")?) {
        return Ok((TypeInfoIR::Float, forced_optional));
    }
    if resolved.is(&builtins.getattr("bool")?) {
        return Ok((TypeInfoIR::Bool, forced_optional));
    }
    if resolved.is(&builtins.getattr("bytes")?) {
        return Ok((TypeInfoIR::Bytes, forced_optional));
    }

    let datetime_mod = py.import("datetime")?;
    if resolved.is(&datetime_mod.getattr("datetime")?) {
        return Ok((TypeInfoIR::DateTime, forced_optional));
    }
    if resolved.is(&datetime_mod.getattr("date")?) {
        return Ok((TypeInfoIR::Date, forced_optional));
    }
    if resolved.is(&datetime_mod.getattr("time")?) {
        return Ok((TypeInfoIR::Time, forced_optional));
    }
    if resolved.is(&datetime_mod.getattr("timedelta")?) {
        return Ok((TypeInfoIR::Timedelta, forced_optional));
    }

    let uuid_mod = py.import("uuid")?;
    if resolved.is(&uuid_mod.getattr("UUID")?) {
        return Ok((TypeInfoIR::Uuid, forced_optional));
    }

    let decimal_mod = py.import("decimal")?;
    if resolved.is(&decimal_mod.getattr("Decimal")?) {
        return Ok((TypeInfoIR::Decimal, forced_optional));
    }

    let enum_mod = py.import("enum")?;
    if let Ok(resolved_type) = resolved.clone().cast_into::<PyType>() {
        let enum_base = enum_mod.getattr("Enum")?;
        if is_subclass(py, &resolved_type, &enum_base)? {
            let members_any = resolved_type.getattr("__members__")?;
            let values_any = members_any.call_method0("values")?;
            let mut variants = Vec::new();
            let mut seen = HashSet::new();
            let mut has_member = false;
            for member in values_any.try_iter()? {
                let member = member?;
                has_member = true;
                let value = member.getattr("value")?;
                let value_type = value.get_type();
                let (typ, _opt) = translate_type_info_ir(py, value_type.as_any(), typevar_map)?;
                let key = format!("{:?}", typ);
                if seen.insert(key) {
                    variants.push(typ);
                }
            }
            if !has_member {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "Enum must define at least one member",
                ));
            }
            let inner = if variants.len() == 1 {
                variants.remove(0)
            } else {
                TypeInfoIR::Union(variants)
            };
            return Ok((
                TypeInfoIR::Enum(resolved_type.unbind(), Box::new(inner)),
                forced_optional,
            ));
        }
    }

    if let Ok(resolved_type) = resolved.clone().cast_into::<PyType>()
        && detect_struct_kind(py, &resolved_type)?.is_some()
    {
        return Ok((TypeInfoIR::Struct(resolved_type.unbind()), forced_optional));
    }

    let repr: String = resolved.repr()?.extract()?;
    Err(pyo3::exceptions::PyTypeError::new_err(format!(
        "Unsupported Tars type: {}",
        repr
    )))
}

fn resolve_typevar<'py>(
    py: Python<'py>,
    tp: &Bound<'py, PyAny>,
    typevar_map: &HashMap<usize, Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    let typing = py.import("typing")?;
    let typevar_cls = typing.getattr("TypeVar")?;
    if tp.is_instance(&typevar_cls)?
        && let Some(mapped) = typevar_map.get(&(tp.as_ptr() as usize))
    {
        return Ok(mapped.clone());
    }
    Ok(tp.clone())
}

fn lookup_default_value<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
    field_name: &str,
) -> PyResult<(bool, Option<Py<PyAny>>)> {
    let types_mod = py.import("types")?;
    let member_descriptor = types_mod.getattr("MemberDescriptorType")?;
    let getset_descriptor = types_mod.getattr("GetSetDescriptorType")?;

    let mro_any = cls.getattr("__mro__")?;
    let mro = mro_any.cast::<PyTuple>()?;

    for base in mro.iter() {
        if let Ok(defaults_any) = base.getattr("__tarsio_defaults__")
            && let Ok(defaults) = defaults_any.cast::<PyDict>()
            && let Some(v) = defaults.get_item(field_name)?
        {
            return Ok((true, Some(v.unbind())));
        }

        if let Ok(base_dict_any) = base.getattr("__dict__")
            && let Ok(base_dict) = base_dict_any.cast::<PyDict>()
            && let Some(v) = base_dict.get_item(field_name)?
        {
            if v.is_instance(&member_descriptor)? || v.is_instance(&getset_descriptor)? {
                continue;
            }
            return Ok((true, Some(v.unbind())));
        }
    }

    Ok((false, None))
}
