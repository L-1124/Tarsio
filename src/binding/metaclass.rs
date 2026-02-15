use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyModule, PyTuple, PyType};

use crate::binding::compiler::compile_schema_from_class;
use crate::binding::schema::SchemaConfig;

#[pyfunction]
#[pyo3(signature = (mcls, name, bases, namespace, **kwargs))]
pub fn _tarsio_structmeta_new<'py>(
    py: Python<'py>,
    mcls: &Bound<'py, PyType>,
    name: &Bound<'py, PyAny>,
    bases: &Bound<'py, PyTuple>,
    namespace: &Bound<'py, PyDict>,
    kwargs: Option<&Bound<'py, PyDict>>,
) -> PyResult<Bound<'py, PyAny>> {
    let mut frozen = false;
    let mut order = false;
    let mut forbid_unknown_tags = false;
    let mut eq = true;
    let mut omit_defaults = false;
    let mut repr_omit_defaults = false;
    let mut kw_only = false;
    let mut dict = false;
    let mut weakref = false;

    if let Some(k) = kwargs {
        if let Some(v) = k.get_item("frozen")? {
            frozen = v.extract::<bool>()?;
            k.del_item("frozen")?;
        }
        if let Some(v) = k.get_item("forbid_unknown_tags")? {
            forbid_unknown_tags = v.extract::<bool>()?;
            k.del_item("forbid_unknown_tags")?;
        }
        if let Some(v) = k.get_item("eq")? {
            eq = v.extract::<bool>()?;
            k.del_item("eq")?;
        }
        if let Some(v) = k.get_item("order")? {
            order = v.extract::<bool>()?;
            k.del_item("order")?;
        }
        if let Some(v) = k.get_item("omit_defaults")? {
            omit_defaults = v.extract::<bool>()?;
            k.del_item("omit_defaults")?;
        }
        if let Some(v) = k.get_item("repr_omit_defaults")? {
            repr_omit_defaults = v.extract::<bool>()?;
            k.del_item("repr_omit_defaults")?;
        }
        if let Some(v) = k.get_item("kw_only")? {
            kw_only = v.extract::<bool>()?;
            k.del_item("kw_only")?;
        }
        if let Some(v) = k.get_item("dict")? {
            dict = v.extract::<bool>()?;
            k.del_item("dict")?;
        }
        if let Some(v) = k.get_item("weakref")? {
            weakref = v.extract::<bool>()?;
            k.del_item("weakref")?;
        }
    }

    let mut field_names: Vec<String> = Vec::new();
    if let Some(ann_any) = namespace.get_item("__annotations__")?
        && let Ok(ann) = ann_any.cast::<PyDict>()
    {
        for k in ann.keys() {
            let s = k.extract::<String>()?;
            if s.starts_with("__") {
                continue;
            }
            field_names.push(s);
        }
    }

    if !field_names.is_empty() {
        let defaults = PyDict::new(py);
        for name in &field_names {
            if let Some(v) = namespace.get_item(name.as_str())? {
                namespace.del_item(name.as_str())?;
                defaults.set_item(name.as_str(), v)?;
            }
        }
        if !defaults.is_empty() {
            namespace.set_item("__tarsio_defaults__", defaults)?;
        }
    }

    if namespace.get_item("__slots__")?.is_none() && !field_names.is_empty() {
        let mut slots: Vec<Py<PyAny>> = Vec::new();
        for name in &field_names {
            slots.push(name.as_str().into_pyobject(py)?.into_any().unbind());
        }
        if dict {
            slots.push("__dict__".into_pyobject(py)?.into_any().unbind());
        }
        if weakref {
            slots.push("__weakref__".into_pyobject(py)?.into_any().unbind());
        }
        let slots_tuple = PyTuple::new(py, slots)?;
        namespace.set_item("__slots__", slots_tuple)?;
    }

    let builtins = py.import("builtins")?;
    let type_obj = builtins.getattr("type")?;
    let new_cls_any = type_obj.call_method("__new__", (mcls, name, bases, namespace), None)?;
    let new_cls = new_cls_any.cast::<PyType>()?.clone();

    let _ = compile_schema_from_class(
        py,
        &new_cls,
        SchemaConfig {
            frozen,
            order,
            forbid_unknown_tags,
            eq,
            omit_defaults,
            repr_omit_defaults,
            kw_only,
            dict,
            weakref,
        },
    )?;

    Ok(new_cls.into_any())
}

pub fn add_struct_meta(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    let builtins = py.import("builtins")?;
    let type_obj = builtins.getattr("type")?;

    let ns = PyDict::new(py);
    ns.set_item("__module__", "tarsio._core")?;
    let new_fn = wrap_pyfunction!(_tarsio_structmeta_new, m)?;
    ns.set_item("__new__", new_fn)?;

    let bases = PyTuple::new(py, vec![type_obj.clone()])?;
    let meta = type_obj.call1(("StructMeta", bases, ns))?;
    let typing_ext = py.import("typing_extensions")?;
    let decorator = typing_ext.getattr("dataclass_transform")?.call0()?;
    let _ = decorator.call1((meta.clone(),))?;
    m.add("StructMeta", meta)?;
    Ok(())
}
