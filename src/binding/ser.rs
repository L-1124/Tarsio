use pyo3::exceptions::{PyRuntimeError, PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{
    PyAny, PyBool, PyBytes, PyDict, PyFloat, PyFrozenSet, PyList, PySequence, PySet, PyString,
    PyType,
};
use std::cell::RefCell;

use bytes::BufMut;

use crate::binding::introspect::StructKind;
use crate::binding::schema::{StructDef, TypeExpr, WireType, ensure_schema_for_class};
use crate::codec::consts::TarsType;
use crate::codec::writer::TarsWriter;

const MAX_DEPTH: usize = 100;
// Capacity threshold (1MB). If buffer exceeds this, we shrink it back.
const BUFFER_SHRINK_THRESHOLD: usize = 1024 * 1024;
// Default initial capacity (128 bytes).
const BUFFER_DEFAULT_CAPACITY: usize = 128;

thread_local! {
    static ENCODE_BUFFER: RefCell<Vec<u8>> = RefCell::new(Vec::with_capacity(128));
    static STDLIB_CACHE: RefCell<Option<StdlibCache>> = const { RefCell::new(None) };
}

struct StdlibCache {
    enum_type: Py<PyAny>,
}

fn with_stdlib_cache<F, R>(py: Python<'_>, f: F) -> PyResult<R>
where
    F: FnOnce(&StdlibCache) -> PyResult<R>,
{
    STDLIB_CACHE.with(|cell| {
        let mut cache_opt = cell.borrow_mut();
        if cache_opt.is_none() {
            let enum_mod = py.import("enum")?;
            let enum_type = enum_mod.getattr("Enum")?.unbind();

            *cache_opt = Some(StdlibCache { enum_type });
        }
        f(cache_opt.as_ref().unwrap())
    })
}

struct PySequenceFast {
    ptr: *mut ffi::PyObject,
    len: isize,
    is_list: bool,
}

impl PySequenceFast {
    fn new_exact(obj: &Bound<'_, PyAny>, is_list: bool) -> PyResult<Self> {
        // SAFETY:
        // 1. ptr 是一个有效的 Python 对象指针，已知是 list 或 tuple。
        // 2. 我们刚刚增加了引用计数，确保它保持存活。
        let ptr = obj.as_ptr();
        unsafe { ffi::Py_INCREF(ptr) };

        let len = unsafe {
            if is_list {
                let list_ptr = ptr as *mut ffi::PyListObject;
                (*list_ptr).ob_base.ob_size
            } else {
                let tuple_ptr = ptr as *mut ffi::PyTupleObject;
                (*tuple_ptr).ob_base.ob_size
            }
        };
        Ok(Self { ptr, len, is_list })
    }

    fn len(&self) -> usize {
        self.len as usize
    }

    fn get_item<'py>(&self, py: Python<'py>, idx: usize) -> PyResult<Bound<'py, PyAny>> {
        if idx as isize >= self.len {
            return Err(PyValueError::new_err("Index out of bounds"));
        }
        // SAFETY:
        // 1. ptr 保持强引用存活。
        // 2. GetItem 返回借用引用(Borrowed Reference)。
        // 3. 不缓存 items 指针，避免列表扩容导致的 UAF。
        unsafe {
            let item_ptr = if self.is_list {
                ffi::PyList_GetItem(self.ptr, idx as isize)
            } else {
                ffi::PyTuple_GetItem(self.ptr, idx as isize)
            };
            if item_ptr.is_null() {
                return Err(PyErr::fetch(py));
            }
            Ok(Bound::from_borrowed_ptr(py, item_ptr))
        }
    }
}

impl Drop for PySequenceFast {
    fn drop(&mut self) {
        // SAFETY:
        // self.ptr 是一个拥有的引用（强引用）。
        // 当此包装器被删除时，我们需要减少它的引用计数。
        unsafe {
            ffi::Py_DECREF(self.ptr);
        }
    }
}

fn check_exact_sequence_type(obj: &Bound<'_, PyAny>) -> Option<bool> {
    // SAFETY:
    // 在有效的 Python 对象指针上调用标准类型检查宏是安全的。
    unsafe {
        if ffi::PyList_CheckExact(obj.as_ptr()) != 0 {
            Some(true)
        } else if ffi::PyTuple_CheckExact(obj.as_ptr()) != 0 {
            Some(false)
        } else {
            None
        }
    }
}

/// 将一个已注册的 Struct 实例编码为 Tars 二进制数据(Schema API).
///
/// Args:
///     obj: Struct 实例.
///
/// Returns:
///     编码后的 bytes.
///
/// Raises:
///     TypeError: obj 不是已注册的 Struct.
///     ValueError: 缺少必填字段、类型不匹配、或递归深度超过限制.
#[pyfunction]
pub fn encode(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<Py<PyBytes>> {
    encode_object_to_pybytes(py, obj)
}

pub fn encode_object_to_pybytes(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<Py<PyBytes>> {
    let cls = obj.get_type();
    let def = ensure_schema_for_class(py, &cls)?;

    ENCODE_BUFFER.with(|cell| {
        let mut buffer = cell.try_borrow_mut().map_err(|_| {
            PyRuntimeError::new_err("Re-entrant encode detected: thread-local buffer is already borrowed. Possible cause: __repr__/__str__/__eq__ (e.g. debug printing, exception formatting) triggered encode during an ongoing encode.")
        })?;
        buffer.clear();

        {
            let mut writer = TarsWriter::with_buffer(&mut *buffer);
            serialize_struct_fields(&mut writer, obj, &def, 0)?;
        }

        let result = PyBytes::new(py, &buffer[..]).unbind();

        // Capacity management: 仅当使用量明显变小才缩容，避免频繁抖动。
        let used = buffer.len();
        if buffer.capacity() > BUFFER_SHRINK_THRESHOLD
            && used < (BUFFER_SHRINK_THRESHOLD / 4)
        {
            let target = if used == 0 {
                BUFFER_DEFAULT_CAPACITY
            } else {
                used.next_power_of_two().max(BUFFER_DEFAULT_CAPACITY)
            };
            buffer.shrink_to(target);
        }

        Ok(result)
    })
}

fn serialize_struct_fields(
    writer: &mut TarsWriter<impl BufMut>,
    obj: &Bound<'_, PyAny>,
    def: &StructDef,
    depth: usize,
) -> PyResult<()> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded or circular reference detected",
        ));
    }

    for field in &def.fields_sorted {
        let value = match def.kind {
            StructKind::TypedDict => {
                let dict = obj
                    .cast::<PyDict>()
                    .map_err(|_| PyTypeError::new_err("TypedDict value must be a dict instance"))?;
                dict.get_item(field.name_py.bind(obj.py()))?
            }
            _ => obj.getattr(field.name_py.bind(obj.py())).ok(),
        };

        match value {
            Some(val) => {
                if val.is_none() {
                    // 可选字段为 None 时跳过
                    continue;
                }
                if def.omit_defaults {
                    if let Some(default_val) = &field.default_value {
                        if val.eq(default_val.bind(obj.py()))? {
                            continue;
                        }
                    } else if field.is_optional && val.is_none() {
                        continue;
                    }
                }
                serialize_impl(writer, field.tag, &field.ty, &val, depth + 1)?;
            }
            None => {
                if field.is_required {
                    return Err(PyValueError::new_err(format!(
                        "Missing required field '{}'",
                        field.name
                    )));
                }
            }
        }
    }
    Ok(())
}

fn serialize_impl(
    writer: &mut TarsWriter<impl BufMut>,
    tag: u8,
    type_expr: &TypeExpr,
    val: &Bound<'_, PyAny>,
    depth: usize,
) -> PyResult<()> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded or circular reference detected",
        ));
    }

    match type_expr {
        TypeExpr::Primitive(wire_type) => match wire_type {
            WireType::Int => {
                let v: i64 = val.extract()?;
                writer.write_int(tag, v);
            }
            WireType::Bool => {
                let v: bool = val.extract()?;
                writer.write_int(tag, i64::from(v));
            }
            WireType::Long => {
                let v: i64 = val.extract()?;
                writer.write_int(tag, v);
            }
            WireType::Float => {
                let v: f32 = val.extract()?;
                writer.write_float(tag, v);
            }
            WireType::Double => {
                let v: f64 = val.extract()?;
                writer.write_double(tag, v);
            }
            WireType::String => {
                let v: &str = val.extract()?;
                writer.write_string(tag, v);
            }
            _ => {
                return Err(PyTypeError::new_err(
                    "Unsupported primitive wire type in serialization",
                ));
            }
        },
        TypeExpr::Any => {
            serialize_any(writer, tag, val, depth + 1)?;
        }
        TypeExpr::NoneType => {
            return Err(PyTypeError::new_err(
                "NoneType must be encoded via Optional or Union",
            ));
        }
        TypeExpr::Enum(enum_cls, inner) => {
            let enum_type = enum_cls.bind(val.py());
            if !val.is_instance(enum_type.as_any())? {
                return Err(PyTypeError::new_err("Enum value type mismatch"));
            }
            let value = val.getattr("value")?;
            serialize_impl(writer, tag, inner, &value, depth + 1)?;
        }
        TypeExpr::Union(variants) => {
            let variant = select_union_variant(val.py(), variants, val)?;
            serialize_impl(writer, tag, variant, val, depth + 1)?;
        }
        TypeExpr::Struct(ptr) => {
            writer.write_tag(tag, TarsType::StructBegin);
            let cls = class_from_ptr(val.py(), *ptr)?;
            let def = ensure_schema_for_class(val.py(), &cls)?;
            serialize_struct_fields(writer, val, &def, depth + 1)?;
            writer.write_tag(0, TarsType::StructEnd);
        }
        TypeExpr::List(inner) | TypeExpr::VarTuple(inner) => {
            if matches!(**inner, TypeExpr::Primitive(WireType::Int))
                && val.is_instance_of::<PyBytes>()
                && let Ok(bytes) = val.extract::<&[u8]>()
            {
                writer.write_bytes(tag, bytes);
                return Ok(());
            }

            writer.write_tag(tag, TarsType::List);
            if let Some(is_list) = check_exact_sequence_type(val) {
                let seq_fast = PySequenceFast::new_exact(val, is_list)?;
                let len = seq_fast.len();
                writer.write_int(0, len as i64);
                for i in 0..len {
                    let item = seq_fast.get_item(val.py(), i)?;
                    serialize_impl(writer, 0, inner, &item, depth + 1)?;
                }
            } else {
                let seq = val.extract::<Bound<'_, PySequence>>()?;
                let len = seq.len()?;
                writer.write_int(0, len as i64);
                for i in 0..len {
                    let item = seq.get_item(i)?;
                    serialize_impl(writer, 0, inner, &item, depth + 1)?;
                }
            }
        }
        TypeExpr::Tuple(items) => {
            writer.write_tag(tag, TarsType::List);
            let expected = items.len();
            if let Some(is_list) = check_exact_sequence_type(val) {
                let seq_fast = PySequenceFast::new_exact(val, is_list)?;
                let len = seq_fast.len();
                if len != expected {
                    return Err(PyTypeError::new_err(
                        "Tuple value length does not match annotation",
                    ));
                }
                writer.write_int(0, len as i64);
                for (idx, item_type) in items.iter().enumerate() {
                    let item = seq_fast.get_item(val.py(), idx)?;
                    serialize_impl(writer, 0, item_type, &item, depth + 1)?;
                }
            } else {
                let seq = val.extract::<Bound<'_, PySequence>>()?;
                let len = seq.len()?;
                if len != expected {
                    return Err(PyTypeError::new_err(
                        "Tuple value length does not match annotation",
                    ));
                }
                writer.write_int(0, len as i64);
                for (idx, item_type) in items.iter().enumerate() {
                    let item = seq.get_item(idx)?;
                    serialize_impl(writer, 0, item_type, &item, depth + 1)?;
                }
            }
        }
        TypeExpr::Set(inner) => {
            writer.write_tag(tag, TarsType::List);
            if val.is_instance_of::<PySet>() {
                let set = val.cast::<PySet>()?;
                let len = set.len() as i64;
                writer.write_int(0, len);
                for item in set.iter() {
                    serialize_impl(writer, 0, inner, &item, depth + 1)?;
                }
                return Ok(());
            }
            if val.is_instance_of::<PyFrozenSet>() {
                let set = val.cast::<PyFrozenSet>()?;
                let len = set.len() as i64;
                writer.write_int(0, len);
                for item in set.iter() {
                    serialize_impl(writer, 0, inner, &item, depth + 1)?;
                }
                return Ok(());
            }
            return Err(PyTypeError::new_err("Set value must be set or frozenset"));
        }
        TypeExpr::Map(k_type, v_type) => {
            writer.write_tag(tag, TarsType::Map);
            let dict = val.extract::<Bound<'_, PyDict>>()?;
            let len = dict.len();
            writer.write_int(0, len as i64);

            for (k, v) in dict {
                serialize_impl(writer, 0, k_type, &k, depth + 1)?;
                serialize_impl(writer, 1, v_type, &v, depth + 1)?;
            }
        }
        TypeExpr::Optional(inner) => {
            if val.is_none() {
                return Ok(());
            }
            serialize_impl(writer, tag, inner, val, depth + 1)?;
        }
    }
    Ok(())
}

fn class_from_ptr<'py>(py: Python<'py>, ptr: usize) -> PyResult<Bound<'py, PyType>> {
    let obj_ptr = ptr as *mut ffi::PyObject;
    if obj_ptr.is_null() {
        return Err(PyTypeError::new_err("Invalid struct pointer"));
    }
    // SAFETY:
    // 指针 ptr 来自我们需要自己的模式/结构定义系统。
    // 我们假设它指向一个有效的、存活的 PyObject（具体来说是 PyTypeObject）。
    // from_borrowed_ptr 创建一个借用该对象的 Bound 包装器。
    let any = unsafe { Bound::from_borrowed_ptr(py, obj_ptr) };
    let cls = any.cast::<PyType>()?;
    Ok(cls.clone())
}

fn select_union_variant<'py>(
    py: Python<'py>,
    variants: &'py [TypeExpr],
    value: &Bound<'py, PyAny>,
) -> PyResult<&'py TypeExpr> {
    if value.is_none() {
        for (idx, variant) in variants.iter().enumerate() {
            if matches!(variant, TypeExpr::Optional(_) | TypeExpr::NoneType) {
                let _ = idx;
                return Ok(variant);
            }
        }
        return Err(PyTypeError::new_err("Union does not accept None"));
    }

    for (idx, variant) in variants.iter().enumerate() {
        if value_matches_type(py, variant, value)? {
            let _ = idx;
            return Ok(variant);
        }
    }
    Err(PyTypeError::new_err(
        "Value does not match any union variant",
    ))
}

fn value_matches_type<'py>(
    py: Python<'py>,
    typ: &TypeExpr,
    value: &Bound<'py, PyAny>,
) -> PyResult<bool> {
    match typ {
        TypeExpr::Any => Ok(true),
        TypeExpr::NoneType => Ok(value.is_none()),
        TypeExpr::Primitive(wire_type) => match wire_type {
            WireType::Int => {
                if value.is_instance_of::<PyBool>() {
                    Ok(false)
                } else {
                    Ok(value.extract::<i64>().is_ok())
                }
            }
            WireType::Bool => Ok(value.is_instance_of::<PyBool>()),
            WireType::Long => Ok(value.extract::<i64>().is_ok()),
            WireType::Float | WireType::Double => Ok(value.is_instance_of::<PyFloat>()),
            WireType::String => Ok(value.is_instance_of::<PyString>()),
            _ => Ok(false),
        },
        TypeExpr::Enum(enum_cls, _) => Ok(value.is_instance(enum_cls.bind(py).as_any())?),
        TypeExpr::Struct(ptr) => {
            let cls = class_from_ptr(py, *ptr)?;
            let def = ensure_schema_for_class(py, &cls)?;
            match def.kind {
                StructKind::TypedDict => Ok(value.is_instance_of::<PyDict>()),
                _ => Ok(value.is_instance(cls.as_any())?),
            }
        }
        TypeExpr::List(_) | TypeExpr::VarTuple(_) => Ok(value.is_instance_of::<PySequence>()
            && !value.is_instance_of::<PyString>()
            && !value.is_instance_of::<PyBytes>()
            && !value.is_instance_of::<PyDict>()),
        TypeExpr::Tuple(items) => {
            if value.is_instance_of::<PyString>()
                || value.is_instance_of::<PyBytes>()
                || value.is_instance_of::<PyDict>()
            {
                return Ok(false);
            }
            let seq = match value.extract::<Bound<'_, PySequence>>() {
                Ok(v) => v,
                Err(_) => return Ok(false),
            };
            let len = seq.len()?;
            if len != items.len() {
                return Ok(false);
            }
            for (idx, item_type) in items.iter().enumerate() {
                let item = seq.get_item(idx)?;
                if !value_matches_type(py, item_type, &item)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        TypeExpr::Set(_) => {
            Ok(value.is_instance_of::<PySet>() || value.is_instance_of::<PyFrozenSet>())
        }
        TypeExpr::Map(_, _) => Ok(value.is_instance_of::<PyDict>()),
        TypeExpr::Optional(inner) => {
            if value.is_none() {
                Ok(true)
            } else {
                value_matches_type(py, inner, value)
            }
        }
        TypeExpr::Union(variants) => {
            for variant in variants {
                if value_matches_type(py, variant, value)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
    }
}

fn serialize_any(
    writer: &mut TarsWriter<impl BufMut>,
    tag: u8,
    value: &Bound<'_, PyAny>,
    depth: usize,
) -> PyResult<()> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded or circular reference detected",
        ));
    }
    if value.is_none() {
        return Err(PyTypeError::new_err("Any cannot encode None directly"));
    }
    if value.is_instance_of::<PyBool>() {
        let v: bool = value.extract()?;
        writer.write_int(tag, i64::from(v));
        return Ok(());
    }
    if value.is_instance_of::<PyFloat>() {
        let v: f64 = value.extract()?;
        writer.write_double(tag, v);
        return Ok(());
    }
    if value.is_instance_of::<PyString>() {
        let v = value.cast::<PyString>()?.to_str()?;
        writer.write_string(tag, v);
        return Ok(());
    }
    if value.is_instance_of::<PyBytes>() {
        let v: &[u8] = value.extract()?;
        writer.write_bytes(tag, v);
        return Ok(());
    }
    if let Ok(v) = value.extract::<i64>() {
        writer.write_int(tag, v);
        return Ok(());
    }

    let is_enum = with_stdlib_cache(value.py(), |cache| {
        let py = value.py();
        if value.is_instance(cache.enum_type.bind(py).as_any())? {
            let inner = value.getattr("value")?;
            serialize_any(writer, tag, &inner, depth + 1)?;
            return Ok(true);
        }
        Ok(false)
    })?;

    if is_enum {
        return Ok(());
    }

    let cls = value.get_type();
    if let Ok(def) = ensure_schema_for_class(value.py(), &cls) {
        writer.write_tag(tag, TarsType::StructBegin);
        serialize_struct_fields(writer, value, &def, depth + 1)?;
        writer.write_tag(0, TarsType::StructEnd);
        return Ok(());
    }

    if value.is_instance_of::<PyDict>() {
        let dict = value.cast::<PyDict>()?;
        writer.write_tag(tag, TarsType::Map);
        writer.write_int(0, dict.len() as i64);
        for (k, v) in dict {
            serialize_any(writer, 0, &k, depth + 1)?;
            serialize_any(writer, 1, &v, depth + 1)?;
        }
        return Ok(());
    }

    if value.is_instance_of::<PySet>() {
        let set = value.cast::<PySet>()?;
        writer.write_tag(tag, TarsType::List);
        let len = set.len() as i64;
        writer.write_int(0, len);
        for item in set.iter() {
            serialize_any(writer, 0, &item, depth + 1)?;
        }
        return Ok(());
    }
    if value.is_instance_of::<PyFrozenSet>() {
        let set = value.cast::<PyFrozenSet>()?;
        writer.write_tag(tag, TarsType::List);
        let len = set.len() as i64;
        writer.write_int(0, len);
        for item in set.iter() {
            serialize_any(writer, 0, &item, depth + 1)?;
        }
        return Ok(());
    }

    if value.is_instance_of::<PyList>() || value.is_instance_of::<PySequence>() {
        writer.write_tag(tag, TarsType::List);
        if let Some(is_list) = check_exact_sequence_type(value) {
            let seq_fast = PySequenceFast::new_exact(value, is_list)?;
            let len = seq_fast.len();
            writer.write_int(0, len as i64);
            for i in 0..len {
                let item = seq_fast.get_item(value.py(), i)?;
                serialize_any(writer, 0, &item, depth + 1)?;
            }
        } else {
            let seq = value.extract::<Bound<'_, PySequence>>()?;
            let len = seq.len()?;
            writer.write_int(0, len as i64);
            for i in 0..len {
                let item = seq.get_item(i)?;
                serialize_any(writer, 0, &item, depth + 1)?;
            }
        }
        return Ok(());
    }

    Err(PyTypeError::new_err("Unsupported Any value type"))
}
