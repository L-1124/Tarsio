use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::pyclass::CompareOp;
use pyo3::types::{PyAny, PyDict, PyTuple, PyType};
use smallvec::SmallVec;
use std::fmt::Write;
use std::sync::Arc;

use crate::binding::compiler::compile_schema_from_class;
pub use crate::binding::core::*;
use crate::binding::generics::handle_class_getitem;
use crate::binding::instantiate::construct_instance;
use crate::binding::parse::detect_struct_kind;

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
        handle_class_getitem(cls, params)
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
