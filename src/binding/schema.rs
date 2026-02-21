use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::pyclass::CompareOp;
use pyo3::types::{PyAny, PyDict, PyString, PyTuple, PyType};
use smallvec::SmallVec;
use std::fmt::Write;
use std::sync::Arc;

use crate::binding::compiler::compile_schema_from_class;
pub use crate::binding::core::*;
use crate::binding::parse::detect_struct_kind;
use crate::binding::validation::validate_type_and_constraints;

pub(crate) fn schema_from_class(
    py: Python<'_>,
    cls: &Bound<'_, PyType>,
) -> PyResult<Option<Arc<StructDef>>> {
    if let Ok(schema_attr) = cls.getattr(SCHEMA_ATTR)
        && let Ok(schema) = schema_attr.extract::<Py<Schema>>()
    {
        return Ok(Some(schema.borrow(py).def.clone()));
    }

    let cls_key = cls.as_ptr() as usize;
    let cached =
        SCHEMA_CACHE.with(|cache| cache.borrow().get(&cls_key).and_then(|weak| weak.upgrade()));

    if cached.is_some() {
        return Ok(cached);
    }

    Ok(None)
}

pub fn ensure_schema_for_class(
    py: Python<'_>,
    cls: &Bound<'_, PyType>,
) -> PyResult<Arc<StructDef>> {
    let cls_key = cls.as_ptr() as usize;

    if let Some(def) = schema_from_class(py, cls)? {
        SCHEMA_CACHE.with(|cache| {
            cache.borrow_mut().insert(cls_key, Arc::downgrade(&def));
        });
        return Ok(def);
    }

    if detect_struct_kind(py, cls)? {
        let default_config = SchemaConfig {
            frozen: false,
            order: false,
            forbid_unknown_tags: false,
            eq: true,
            omit_defaults: false,
            repr_omit_defaults: false,
            kw_only: false,
            dict: false,
            weakref: false,
        };

        if let Some(def) = compile_schema_from_class(py, cls, default_config)? {
            return Ok(def);
        }
    }

    let class_name = cls
        .name()
        .map(|n| n.to_string())
        .unwrap_or_else(|_| "Unknown".to_string());
    Err(pyo3::exceptions::PyTypeError::new_err(format!(
        "Unsupported class type: {}",
        class_name
    )))
}

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

#[pymethods]
impl TarsDict {
    #[new]
    #[pyo3(signature = (*_args, **_kwargs))]
    fn new(_args: &Bound<'_, PyTuple>, _kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        Ok(TarsDict)
    }

    fn __traverse__(&self, _visit: pyo3::PyVisit<'_>) -> Result<(), pyo3::PyTraverseError> {
        Ok(())
    }

    fn __clear__(&mut self) {}
}

/// Tarsio 的 Struct 基类.
///
/// 继承该类会在类创建时编译并注册 Schema.
/// 字段 tag 可通过 `field(tag=...)` 显式声明，未声明时按定义顺序自动分配。
///
/// Examples:
///     ```python
///     from typing import Annotated
///     from tarsio import Struct, field
///
///     class User(Struct):
///         uid: int = field(tag=0)
///         name: Annotated[str, "doc"]  # 自动分配 tag
///     ```
///
/// Notes:
///     解码时, wire 缺失字段会使用模型默认值; Optional 字段未显式赋默认值时视为 None.
#[pymethods]
impl Struct {
    #[new]
    #[pyo3(signature = (*_args, **_kwargs))]
    fn new(_args: &Bound<'_, PyTuple>, _kwargs: Option<&Bound<'_, PyDict>>) -> Self {
        Struct
    }

    fn __traverse__(&self, _visit: pyo3::PyVisit<'_>) -> Result<(), pyo3::PyTraverseError> {
        Ok(())
    }

    fn __clear__(&mut self) {}

    #[pyo3(signature = (*args, **kwargs))]
    fn __init__(
        slf: &Bound<'_, Struct>,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        let py = slf.py();
        let cls = slf.get_type();
        let def = schema_from_class(py, &cls)?.ok_or_else(|| {
            let class_name = cls
                .name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|_| "Unknown".to_string());
            pyo3::exceptions::PyTypeError::new_err(format!(
                "Cannot instantiate abstract schema class '{}'",
                class_name
            ))
        })?;

        construct_instance(&def, slf.as_any(), args, kwargs)
    }

    /// 将当前实例编码为 Tars 二进制数据.
    ///
    /// Returns:
    ///     编码后的 bytes.
    ///
    /// Raises:
    ///     ValueError: 缺少必填字段、类型不匹配、或递归深度超过限制.
    fn encode(slf: &Bound<'_, Struct>) -> PyResult<Py<pyo3::types::PyBytes>> {
        let py = slf.py();
        crate::binding::codec::ser::encode_object_to_pybytes(py, slf.as_any())
    }

    /// 将 Tars 二进制数据解码为当前类的实例.
    ///
    /// Args:
    ///     data: 待解码的 bytes.
    ///
    /// Returns:
    ///     解码得到的实例.
    ///
    /// Raises:
    ///     TypeError: 目标类未注册 Schema.
    ///     ValueError: 数据格式不正确、缺少必填字段、或递归深度超过限制.
    #[classmethod]
    fn decode<'py>(cls: &Bound<'py, PyType>, data: &[u8]) -> PyResult<Bound<'py, PyAny>> {
        let py = cls.py();
        crate::binding::codec::de::decode_object(py, cls, data)
    }

    #[classmethod]
    fn __class_getitem__<'py>(
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

    fn __copy__(slf: &Bound<'_, Struct>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let cls = slf.get_type();
        let def = schema_from_class(py, &cls)?.ok_or_else(|| {
            pyo3::exceptions::PyTypeError::new_err("Schema not found during copy")
        })?;

        // SAFETY:
        // 1. `cls` 是有效的 Python 类型对象，来自 `slf.get_type()`。
        // 2. `PyType_GenericAlloc` 返回新引用；空指针时立即通过 `PyErr::fetch` 返回错误。
        // 3. `Bound::from_owned_ptr` 正确接管该新引用所有权。
        let instance = unsafe {
            let type_ptr = cls.as_ptr() as *mut ffi::PyTypeObject;
            let obj_ptr = ffi::PyType_GenericAlloc(type_ptr, 0);
            if obj_ptr.is_null() {
                return Err(PyErr::fetch(py));
            }
            Bound::from_owned_ptr(py, obj_ptr)
        };

        for field in &def.fields_sorted {
            let val = match slf.getattr(field.name_py.bind(py)) {
                Ok(v) => v,
                Err(_) => {
                    if let Some(default_value) = field.default_value.as_ref() {
                        default_value.bind(py).clone()
                    } else if let Some(factory) = field.default_factory.as_ref() {
                        factory.bind(py).call0()?
                    } else if field.is_optional {
                        py.None().into_bound(py)
                    } else if field.is_required {
                        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                            "Missing required field '{}' during copy",
                            field.name
                        )));
                    } else {
                        continue;
                    }
                }
            };

            // SAFETY:
            // 1. `instance` 与 `name_py` 均为当前 GIL 下的有效 Python 对象。
            // 2. `val` 在调用期间保持存活，`PyObject_GenericSetAttr` 仅借用引用。
            // 3. 返回非 0 表示 Python 异常已设置，立即 `PyErr::fetch` 传播。
            unsafe {
                let name_py = field.name_py.bind(py);
                let res =
                    ffi::PyObject_GenericSetAttr(instance.as_ptr(), name_py.as_ptr(), val.as_ptr());
                if res != 0 {
                    return Err(PyErr::fetch(py));
                }
            }
        }

        Ok(instance.unbind())
    }

    #[pyo3(signature = (**changes))]
    fn __replace__(
        slf: &Bound<'_, Struct>,
        changes: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let cls = slf.get_type();
        let def = schema_from_class(py, &cls)?.ok_or_else(|| {
            pyo3::exceptions::PyTypeError::new_err("Schema not found during replace")
        })?;

        // SAFETY:
        // 1. `cls` 是有效的 Python 类型对象，来自 `slf.get_type()`。
        // 2. `PyType_GenericAlloc` 返回新引用；空指针时立即通过 `PyErr::fetch` 返回错误。
        // 3. `Bound::from_owned_ptr` 正确接管该新引用所有权。
        let instance = unsafe {
            let type_ptr = cls.as_ptr() as *mut ffi::PyTypeObject;
            let obj_ptr = ffi::PyType_GenericAlloc(type_ptr, 0);
            if obj_ptr.is_null() {
                return Err(PyErr::fetch(py));
            }
            Bound::from_owned_ptr(py, obj_ptr)
        };

        let kwargs = PyDict::new(py);
        for field in &def.fields_sorted {
            let val = match slf.getattr(field.name_py.bind(py)) {
                Ok(v) => v,
                Err(_) => {
                    if let Some(default_value) = field.default_value.as_ref() {
                        default_value.bind(py).clone()
                    } else if let Some(factory) = field.default_factory.as_ref() {
                        factory.bind(py).call0()?
                    } else if field.is_optional {
                        py.None().into_bound(py)
                    } else if field.is_required {
                        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                            "Missing required field '{}' during replace",
                            field.name
                        )));
                    } else {
                        continue;
                    }
                }
            };
            kwargs.set_item(field.name_py.bind(py), val)?;
        }

        if let Some(items) = changes {
            for (key, value) in items.iter() {
                kwargs.set_item(key, value)?;
            }
        }

        let empty_args = PyTuple::empty(py);
        construct_instance(&def, instance.as_any(), &empty_args, Some(&kwargs))?;
        Ok(instance.unbind())
    }

    fn __repr__(slf: &Bound<'_, Struct>) -> PyResult<String> {
        let py = slf.py();
        let cls = slf.get_type();
        let class_name = cls.name()?.extract::<String>()?;

        let def = match schema_from_class(py, &cls)? {
            Some(d) => d,
            None => return Ok(format!("{}()", class_name)),
        };

        let mut result = String::with_capacity(class_name.len() + 2 + def.fields_sorted.len() * 24);
        result.push_str(&class_name);
        result.push('(');
        let mut first = true;
        for field in &def.fields_sorted {
            let val = match slf.getattr(field.name_py.bind(py)) {
                Ok(v) => v,
                Err(_) => continue, // Skip missing fields
            };

            if def.repr_omit_defaults
                && let Some(default_val) = &field.default_value
                && val.eq(default_val.bind(py))?
            {
                continue;
            }

            if !first {
                result.push_str(", ");
            }
            first = false;

            let val_repr = val.repr()?;
            let val_repr_str = val_repr.to_str()?;
            write!(result, "{}={}", field.name, val_repr_str)
                .map_err(|_| pyo3::exceptions::PyRuntimeError::new_err("failed to build repr"))?;
        }
        result.push(')');

        Ok(result)
    }

    fn __rich_repr__(slf: &Bound<'_, Struct>) -> PyResult<Vec<(String, Py<PyAny>)>> {
        let py = slf.py();
        let cls = slf.get_type();
        let def = match schema_from_class(py, &cls)? {
            Some(d) => d,
            None => return Ok(Vec::new()),
        };

        let mut items = Vec::with_capacity(def.fields_sorted.len());
        for field in &def.fields_sorted {
            let val = match slf.getattr(field.name_py.bind(py)) {
                Ok(v) => v,
                Err(_) => continue,
            };

            if def.repr_omit_defaults
                && let Some(default_val) = &field.default_value
                && val.eq(default_val.bind(py))?
            {
                continue;
            }
            items.push((field.name.clone(), val.unbind()));
        }
        Ok(items)
    }

    fn __richcmp__(
        slf: &Bound<'_, Struct>,
        other: &Bound<'_, PyAny>,
        op: CompareOp,
    ) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        match op {
            CompareOp::Eq => {
                if !other.is_instance_of::<Struct>() {
                    return Ok(false.into_pyobject(py)?.to_owned().into_any().unbind());
                }

                let cls1 = slf.get_type();
                let cls2 = other.get_type();
                let def = match schema_from_class(py, &cls1)? {
                    Some(d) => d,
                    None => return Ok(false.into_pyobject(py)?.to_owned().into_any().unbind()),
                };

                if !def.eq {
                    return Ok(py.NotImplemented());
                }

                if !cls1.is(&cls2) {
                    return Ok(false.into_pyobject(py)?.to_owned().into_any().unbind());
                }

                for field in &def.fields_sorted {
                    let v1 = slf.getattr(field.name_py.bind(py))?;
                    let v2 = other.getattr(field.name_py.bind(py))?;
                    if !v1.eq(v2)? {
                        return Ok(false.into_pyobject(py)?.to_owned().into_any().unbind());
                    }
                }
                Ok(true.into_pyobject(py)?.to_owned().into_any().unbind())
            }
            CompareOp::Ne => {
                let eq = Self::__richcmp__(slf, other, CompareOp::Eq)?;
                if eq.bind(py).is(py.NotImplemented()) {
                    return Ok(py.NotImplemented());
                }
                let is_eq: bool = eq.bind(py).extract()?;
                Ok((!is_eq).into_pyobject(py)?.to_owned().into_any().unbind())
            }
            CompareOp::Lt | CompareOp::Le | CompareOp::Gt | CompareOp::Ge => {
                if !other.is_instance_of::<Struct>() {
                    return Ok(py.NotImplemented());
                }

                let cls1 = slf.get_type();
                let cls2 = other.get_type();
                let def = match schema_from_class(py, &cls1)? {
                    Some(d) => d,
                    None => return Ok(py.NotImplemented()),
                };

                if !def.order {
                    return Ok(py.NotImplemented());
                }

                if !cls1.is(&cls2) {
                    return Ok(py.NotImplemented());
                }

                let mut vals1: SmallVec<[_; 16]> = SmallVec::with_capacity(def.fields_sorted.len());
                let mut vals2: SmallVec<[_; 16]> = SmallVec::with_capacity(def.fields_sorted.len());
                for field in &def.fields_sorted {
                    vals1.push(slf.getattr(field.name_py.bind(py))?);
                    vals2.push(other.getattr(field.name_py.bind(py))?);
                }
                let t1 = PyTuple::new(py, vals1)?;
                let t2 = PyTuple::new(py, vals2)?;
                t1.rich_compare(t2, op)
                    .map(|v| v.to_owned().into_any().unbind())
            }
        }
    }

    fn __hash__(slf: &Bound<'_, Struct>) -> PyResult<isize> {
        let py = slf.py();
        let cls = slf.get_type();
        let def = match schema_from_class(py, &cls)? {
            Some(d) => d,
            None => {
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "unhashable type: '{}'",
                    cls.name()?
                )));
            }
        };

        if !def.frozen {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "unhashable type: '{}' (not frozen)",
                cls.name()?
            )));
        }

        let mut vals: SmallVec<[_; 16]> = SmallVec::with_capacity(def.fields_sorted.len());
        for field in &def.fields_sorted {
            vals.push(slf.getattr(field.name_py.bind(py))?);
        }
        let tuple = PyTuple::new(py, vals)?;
        tuple.hash()
    }

    fn __setattr__(
        slf: &Bound<'_, Struct>,
        name: Bound<'_, PyAny>,
        value: Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let cls = slf.get_type();
        if let Some(def) = schema_from_class(slf.py(), &cls)?
            && def.frozen
        {
            return Err(pyo3::exceptions::PyAttributeError::new_err(format!(
                "can't set attributes of frozen instance '{}'",
                cls.name()?
            )));
        }
        // SAFETY:
        // 1. `slf`/`name`/`value` 均是有效 Python 对象引用。
        // 2. `PyObject_GenericSetAttr` 仅在对象上执行属性写入，不转移引用所有权。
        // 3. 若写入失败，Python 异常已设置并通过 `PyErr::fetch` 传播。
        unsafe {
            let res =
                pyo3::ffi::PyObject_GenericSetAttr(slf.as_ptr(), name.as_ptr(), value.as_ptr());
            if res != 0 {
                return Err(PyErr::fetch(slf.py()));
            }
        }
        Ok(())
    }
}

fn set_field_value(
    self_obj: &Bound<'_, PyAny>,
    field: &FieldDef,
    value: &Bound<'_, PyAny>,
) -> PyResult<()> {
    // SAFETY:
    // 1. `self_obj` 与 `field.name_py` 是同一解释器内的有效对象。
    // 2. `value` 在调用期间保持存活，C API 不会窃取其引用。
    // 3. 失败时 Python 异常已设置，立即抓取并返回。
    unsafe {
        let name_py = field.name_py.bind(self_obj.py());
        let res =
            pyo3::ffi::PyObject_GenericSetAttr(self_obj.as_ptr(), name_py.as_ptr(), value.as_ptr());
        if res != 0 {
            return Err(PyErr::fetch(self_obj.py()));
        }
    }
    Ok(())
}

fn missing_required_argument_error(field: &FieldDef) -> PyErr {
    pyo3::exceptions::PyTypeError::new_err(format!(
        "__init__() missing 1 required positional argument: '{}'",
        field.name
    ))
}

pub(crate) fn run_post_init(self_obj: &Bound<'_, PyAny>) -> PyResult<()> {
    let py = self_obj.py();
    match self_obj.getattr("__post_init__") {
        Ok(post_init) => {
            post_init.call0()?;
            Ok(())
        }
        Err(err) => {
            if err.is_instance_of::<pyo3::exceptions::PyAttributeError>(py) {
                Ok(())
            } else {
                Err(err)
            }
        }
    }
}

#[inline]
fn lookup_keyword_index(def: &StructDef, key: &Bound<'_, PyAny>) -> PyResult<Option<usize>> {
    if let Ok(key_str_obj) = key.cast::<PyString>() {
        let key_ptr = key_str_obj.as_ptr() as usize;
        if let Some(idx) = def.meta.name_ptr_to_index.get(&key_ptr) {
            return Ok(Some(*idx));
        }

        let key_str = key_str_obj.to_str()?;
        return Ok(def.meta.name_to_index.get(key_str).copied());
    }
    Ok(None)
}

pub fn construct_instance(
    def: &StructDef,
    self_obj: &Bound<'_, PyAny>,
    args: &Bound<'_, PyTuple>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    let py = self_obj.py();
    let num_positional = args.len();
    let num_fields = def.fields_sorted.len();

    if def.kw_only && num_positional > 0 {
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "__init__() takes 0 positional arguments but {} were given",
            num_positional
        )));
    }

    if num_positional > num_fields {
        let expected = num_fields + 1;
        let given = num_positional + 1;
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "__init__() takes {} positional arguments but {} were given",
            expected, given
        )));
    }

    let no_kwargs = kwargs.is_none_or(|k| k.is_empty());
    if no_kwargs && num_positional == num_fields {
        for (idx, field) in def.fields_sorted.iter().enumerate() {
            let val = args.get_item(idx)?;
            if !(field.is_optional && val.is_none()) {
                validate_type_and_constraints(
                    py,
                    &val,
                    &field.ty,
                    field.constraints.as_deref(),
                    field.name.as_str(),
                )?;
            }
            set_field_value(self_obj, field, &val)?;
        }
        run_post_init(self_obj)?;
        return Ok(());
    }

    let mut mapped_values: SmallVec<[Option<Py<PyAny>>; 16]> =
        std::iter::repeat_with(|| None).take(num_fields).collect();

    for (idx, slot) in mapped_values.iter_mut().enumerate().take(num_positional) {
        *slot = Some(args.get_item(idx)?.unbind());
    }

    if let Some(k) = kwargs {
        const KWARGS_DIRECT_ITER_THRESHOLD: usize = 8;
        let kw_len = k.len();
        let use_kwargs_iteration =
            kw_len <= KWARGS_DIRECT_ITER_THRESHOLD || kw_len.saturating_mul(2) < num_fields;

        if use_kwargs_iteration {
            for (key, value) in k.iter() {
                let idx = lookup_keyword_index(def, &key)?.ok_or_else(|| {
                    pyo3::exceptions::PyTypeError::new_err(format!(
                        "__init__() got an unexpected keyword argument '{}'",
                        key.extract::<String>()
                            .unwrap_or_else(|_| "<non-string-key>".to_string())
                    ))
                })?;

                // 位置参数已占用，或 kwargs 冲突（防御性检查）
                if idx < num_positional || mapped_values[idx].is_some() {
                    return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                        "__init__() got multiple values for argument '{}'",
                        def.fields_sorted[idx].name
                    )));
                }
                mapped_values[idx] = Some(value.unbind());
            }
        } else {
            let mut matched = 0usize;
            for (idx, field) in def.fields_sorted.iter().enumerate() {
                if let Some(value) = k.get_item(field.name_py.bind(py))? {
                    matched += 1;
                    if idx < num_positional {
                        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                            "__init__() got multiple values for argument '{}'",
                            field.name
                        )));
                    }
                    mapped_values[idx] = Some(value.unbind());
                }
            }

            if matched != kw_len {
                for key in k.keys() {
                    if lookup_keyword_index(def, &key)?.is_none() {
                        let key_str = key
                            .extract::<String>()
                            .unwrap_or_else(|_| "<non-string-key>".to_string());
                        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                            "__init__() got an unexpected keyword argument '{}'",
                            key_str
                        )));
                    }
                }
            }
        }
    }

    for (idx, field) in def.fields_sorted.iter().enumerate() {
        let val_to_set = match mapped_values[idx].as_ref() {
            Some(v) => v.bind(py).clone(),
            None => {
                if let Some(default_value) = field.default_value.as_ref() {
                    default_value.bind(py).clone()
                } else if let Some(factory) = field.default_factory.as_ref() {
                    factory.bind(py).call0()?
                } else if field.is_optional || field.is_required {
                    return Err(missing_required_argument_error(field));
                } else {
                    // 理论上被 required 校验覆盖,这里作为安全兜底
                    return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                        "__init__() missing 1 required argument: '{}'",
                        field.name
                    )));
                }
            }
        };

        if !(field.is_optional && val_to_set.is_none()) {
            validate_type_and_constraints(
                py,
                &val_to_set,
                &field.ty,
                field.constraints.as_deref(),
                field.name.as_str(),
            )?;
        }
        set_field_value(self_obj, field, &val_to_set)?;
    }

    run_post_init(self_obj)?;
    Ok(())
}
