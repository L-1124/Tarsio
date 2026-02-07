use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyString, PyTuple, PyType};
use std::collections::HashMap;

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
    pub has_default: bool,
    pub is_optional: bool,
    pub is_required: bool,
    pub constraints: Option<ConstraintsIR>,
}

pub fn introspect_struct_fields<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
) -> PyResult<Option<Vec<FieldInfoIR>>> {
    if is_generic_template(cls)? {
        return Ok(None);
    }

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
            has_default,
            is_optional,
            is_required,
            constraints,
        });
    }

    if fields.is_empty() {
        return Ok(None);
    }
    fields.sort_by_key(|f| f.tag);
    Ok(Some(fields))
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

fn translate_type_info_ir<'py>(
    py: Python<'py>,
    tp: &Bound<'py, PyAny>,
    typevar_map: &HashMap<usize, Bound<'py, PyAny>>,
) -> PyResult<(TypeInfoIR, bool)> {
    let resolved = resolve_typevar(py, tp, typevar_map)?;

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

    let builtins = py.import("builtins")?;
    if resolved.is(&builtins.getattr("int")?) {
        return Ok((TypeInfoIR::Int, false));
    }
    if resolved.is(&builtins.getattr("str")?) {
        return Ok((TypeInfoIR::Str, false));
    }
    if resolved.is(&builtins.getattr("float")?) {
        return Ok((TypeInfoIR::Float, false));
    }
    if resolved.is(&builtins.getattr("bool")?) {
        return Ok((TypeInfoIR::Bool, false));
    }
    if resolved.is(&builtins.getattr("bytes")?) {
        return Ok((TypeInfoIR::Bytes, false));
    }

    let origin = typing.call_method1("get_origin", (&resolved,))?;
    if !origin.is_none() {
        let args_any = typing.call_method1("get_args", (&resolved,))?;
        let args = args_any.cast::<PyTuple>()?;

        if origin.is(&builtins.getattr("list")?) || origin.is(&builtins.getattr("tuple")?) {
            if args.is_empty() {
                let repr: String = resolved.repr()?.extract()?;
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "Unsupported Tars type: {}",
                    repr
                )));
            }
            let (inner, _opt) = translate_type_info_ir(py, &args.get_item(0)?, typevar_map)?;
            if origin.is(&builtins.getattr("list")?) {
                return Ok((TypeInfoIR::List(Box::new(inner)), false));
            }
            return Ok((TypeInfoIR::Tuple(Box::new(inner)), false));
        }

        if origin.is(&builtins.getattr("dict")?) {
            if args.len() < 2 {
                let repr: String = resolved.repr()?.extract()?;
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "Unsupported Tars type: {}",
                    repr
                )));
            }
            let (k, _opt_k) = translate_type_info_ir(py, &args.get_item(0)?, typevar_map)?;
            let (v, _opt_v) = translate_type_info_ir(py, &args.get_item(1)?, typevar_map)?;
            return Ok((TypeInfoIR::Map(Box::new(k), Box::new(v)), false));
        }

        let union_origin = typing.getattr("Union")?;
        let types_mod = py.import("types")?;
        let union_type = types_mod.getattr("UnionType").ok();
        let is_union =
            origin.is(&union_origin) || union_type.as_ref().is_some_and(|u| origin.is(u));
        if is_union {
            let none_type = py.None().bind(py).get_type();
            let mut non_none: Vec<Bound<'py, PyAny>> = Vec::new();
            let mut has_none = false;
            for a in args.iter() {
                if a.is_none() || a.is(&none_type) {
                    has_none = true;
                } else {
                    non_none.push(a);
                }
            }
            if has_none && non_none.len() == 1 {
                let (inner, _opt_inner) = translate_type_info_ir(py, &non_none[0], typevar_map)?;
                return Ok((TypeInfoIR::Optional(Box::new(inner)), true));
            }
            let repr: String = resolved.repr()?.extract()?;
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "Unsupported Tars type: {}",
                repr
            )));
        }
    }

    if let Ok(resolved_type) = resolved.clone().cast_into::<PyType>()
        && resolved_type.is_subclass_of::<Struct>()?
    {
        return Ok((TypeInfoIR::Struct(resolved_type.unbind()), false));
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
