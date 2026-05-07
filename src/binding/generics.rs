//! Struct 泛型参数化辅助逻辑。

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyTuple, PyType};

fn normalize_class_getitem_args<'py>(
    py: Python<'py>,
    params: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyTuple>> {
    if let Ok(items) = params.cast::<PyTuple>() {
        return Ok(items.clone());
    }
    PyTuple::new(py, [params.clone().unbind()])
}

fn contains_unresolved_typevar(py: Python<'_>, item: &Bound<'_, PyAny>) -> PyResult<bool> {
    let typing = py.import("typing")?;
    let typevar_cls = typing.getattr("TypeVar")?;
    if item.is_instance(&typevar_cls)? {
        return Ok(true);
    }

    if let Ok(params_any) = item.getattr("__parameters__")
        && let Ok(params) = params_any.cast::<PyTuple>()
        && !params.is_empty()
    {
        return Ok(true);
    }
    Ok(false)
}

fn get_generic_alias<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
    args: &Bound<'py, PyTuple>,
) -> PyResult<Bound<'py, PyAny>> {
    let types_mod = py.import("types")?;
    let generic_alias = types_mod.getattr("GenericAlias")?;
    generic_alias.call1((cls, args))
}

fn build_parametrized_struct_name(
    _py: Python<'_>,
    cls: &Bound<'_, PyType>,
    args: &Bound<'_, PyTuple>,
) -> PyResult<String> {
    let base_name = cls.name()?.to_string();
    let mut parts = Vec::with_capacity(args.len());
    for item in args.iter() {
        let repr_obj = item.repr()?;
        parts.push(repr_obj.to_str()?.to_string());
    }
    Ok(format!("{}[{}]", base_name, parts.join(", ")))
}

pub(crate) fn handle_class_getitem<'py>(
    cls: &Bound<'py, PyType>,
    params: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let py = cls.py();
    let args = normalize_class_getitem_args(py, params)?;

    let expected_any = cls
        .getattr("__parameters__")
        .unwrap_or_else(|_| PyTuple::empty(py).into_any());
    let expected = expected_any.cast::<PyTuple>()?;
    if expected.is_empty() {
        let class_name = cls
            .name()
            .map(|n| n.to_string())
            .unwrap_or_else(|_| "Unknown".to_string());
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "{} is not a generic class",
            class_name
        )));
    }

    if args.len() != expected.len() {
        let class_name = cls
            .name()
            .map(|n| n.to_string())
            .unwrap_or_else(|_| "Unknown".to_string());
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "Too {} arguments for {}. Expected {}, got {}",
            if args.len() < expected.len() {
                "few"
            } else {
                "many"
            },
            class_name,
            expected.len(),
            args.len()
        )));
    }

    for item in args.iter() {
        if contains_unresolved_typevar(py, &item)? {
            return get_generic_alias(py, cls, &args);
        }
    }

    let cache_any = cls.getattr("__tarsio_generic_cache__").ok();
    let cache = if let Some(obj) = cache_any {
        if let Ok(dict) = obj.cast::<PyDict>() {
            dict.clone()
        } else {
            let d = PyDict::new(py);
            cls.setattr("__tarsio_generic_cache__", &d)?;
            d
        }
    } else {
        let d = PyDict::new(py);
        cls.setattr("__tarsio_generic_cache__", &d)?;
        d
    };

    if let Some(existing) = cache.get_item(&args)? {
        return Ok(existing);
    }

    let name = build_parametrized_struct_name(py, cls, &args)?;
    let bases = PyTuple::new(py, [cls.clone().unbind()])?;
    let namespace = PyDict::new(py);
    namespace.set_item("__module__", cls.getattr("__module__")?)?;
    namespace.set_item("__origin__", cls)?;
    namespace.set_item("__args__", &args)?;
    namespace.set_item("__parameters__", PyTuple::empty(py))?;

    let struct_cfg = cls.getattr("__struct_config__")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("frozen", struct_cfg.getattr("frozen")?)?;
    kwargs.set_item("order", struct_cfg.getattr("order")?)?;
    kwargs.set_item("forbid_unknown_tags", false)?;
    kwargs.set_item("eq", struct_cfg.getattr("eq")?)?;
    kwargs.set_item("omit_defaults", struct_cfg.getattr("omit_defaults")?)?;
    kwargs.set_item(
        "repr_omit_defaults",
        struct_cfg.getattr("repr_omit_defaults")?,
    )?;
    kwargs.set_item("kw_only", struct_cfg.getattr("kw_only")?)?;
    kwargs.set_item("dict", struct_cfg.getattr("dict")?)?;
    kwargs.set_item("weakref", struct_cfg.getattr("weakref")?)?;

    let mcls = cls.get_type();
    let new_cls_any = mcls.call((name, bases, namespace), Some(&kwargs))?;
    let new_cls = new_cls_any.cast::<PyType>()?;
    cache.set_item(&args, new_cls)?;
    Ok(new_cls.clone().into_any())
}
