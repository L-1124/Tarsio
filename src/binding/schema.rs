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
    // 1. Check attribute directly
    if let Ok(schema_attr) = cls.getattr(SCHEMA_ATTR)
        && let Ok(schema) = schema_attr.extract::<Py<Schema>>()
    {
        return Ok(Some(schema.borrow(py).def.clone()));
    }

    // 2. Check weak cache
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

    // 1. Check cache first
    let cached =
        SCHEMA_CACHE.with(|cache| cache.borrow().get(&cls_key).and_then(|weak| weak.upgrade()));

    if let Some(def) = cached {
        return Ok(def);
    }

    // 2. Parameters check for generic classes
    if let Ok(params) = cls.getattr("__parameters__")
        && let Ok(tuple) = params.cast::<PyTuple>()
        && !tuple.is_empty()
    {
        let class_name = cls
            .name()
            .map(|n| n.to_string())
            .unwrap_or_else(|_| "Unknown".to_string());
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "Unsupported class type: {}",
            class_name
        )));
    }

    // 3. Try normal schema lookup
    if let Some(def) = schema_from_class(py, cls)? {
        SCHEMA_CACHE.with(|cache| {
            cache.borrow_mut().insert(cls_key, Arc::downgrade(&def));
        });
        return Ok(def);
    }

    // 4. Internal introspection for automated schema building
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

    // 5. Fallback: not a struct
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
/// 继承该类会在类创建时编译并注册 Schema, 字段使用 `typing.Annotated[T, tag]` 声明.
///
/// Examples:
///     ```python
///     from typing import Annotated
///     from tarsio import Struct
///
///     class User(Struct):
///         uid: Annotated[int, 0]
///         name: Annotated[str, 1]
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

    fn __copy__(slf: &Bound<'_, Struct>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let cls = slf.get_type();
        let def = schema_from_class(py, &cls)?.ok_or_else(|| {
            pyo3::exceptions::PyTypeError::new_err("Schema not found during copy")
        })?;

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

    fn __repr__(slf: &Bound<'_, Struct>) -> PyResult<String> {
        let py = slf.py();
        let cls = slf.get_type();
        let class_name = cls.name()?.extract::<String>()?;

        // 尝试获取 Schema
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

            // 使用 Python 的 repr() 获取值的字符串表示
            if def.repr_omit_defaults
                && let Some(default_val) = &field.default_value
                && val.eq(default_val)?
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

// ==========================================
// 构造器逻辑
// ==========================================

fn set_field_value(
    self_obj: &Bound<'_, PyAny>,
    field: &FieldDef,
    value: &Bound<'_, PyAny>,
) -> PyResult<()> {
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
    // 位置参数按字段顺序映射到 args
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

    // Fast Path: 全位置参数，无 kwargs，无需中间缓冲区
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

    Ok(())
}
