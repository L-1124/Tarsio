use bytes::BufMut;
use pyo3::exceptions::{PyRuntimeError, PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{
    PyAny, PyBool, PyBytes, PyDict, PyFloat, PyFrozenSet, PyList, PySequence, PySet, PyString,
    PyType,
};
use simdutf8::basic::from_utf8;
use std::cell::RefCell;

use smallvec::SmallVec;

use crate::binding::codec::ser;
use crate::binding::error::{DeError, DeResult, PathItem};
use crate::binding::schema::{Struct, StructDef, TarsDict, TypeExpr, ensure_schema_for_class};
use crate::binding::utils::{
    BUFFER_DEFAULT_CAPACITY, BUFFER_SHRINK_THRESHOLD, PySequenceFast, check_depth,
    check_exact_sequence_type, dataclass_fields, with_stdlib_cache,
};
use crate::codec::consts::TarsType;
use crate::codec::reader::TarsReader;
use crate::codec::writer::TarsWriter;

thread_local! {
    static RAW_ENCODE_BUFFER: RefCell<Vec<u8>> = RefCell::new(Vec::with_capacity(128));
}

pub(crate) fn serialize_struct_fields<W, F>(
    writer: &mut TarsWriter<W>,
    obj: &Bound<'_, PyAny>,
    def: &StructDef,
    depth: usize,
    serialize_typed: &F,
) -> PyResult<()>
where
    W: BufMut,
    F: Fn(&mut TarsWriter<W>, u8, &TypeExpr, &Bound<'_, PyAny>, usize) -> PyResult<()>,
{
    check_depth(depth)?;

    for field in &def.fields_sorted {
        let value = obj.getattr(field.name_py.bind(obj.py())).ok();

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
                if def.simplelist
                    && let TypeExpr::Struct(ptr) = &field.ty
                {
                    let nested_cls = struct_class_from_ptr(obj.py(), *ptr)?;
                    let nested_def = ensure_schema_for_class(obj.py(), &nested_cls)?;
                    let mut nested = Vec::with_capacity(256);
                    {
                        let mut nested_writer = TarsWriter::with_buffer(&mut nested);
                        serialize_struct_fields(
                            &mut nested_writer,
                            &val,
                            &nested_def,
                            depth + 1,
                            &ser::serialize_impl,
                        )?;
                    }
                    writer.write_bytes(field.tag, &nested);
                    continue;
                }
                serialize_typed(writer, field.tag, &field.ty, &val, depth + 1)?;
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

fn struct_class_from_ptr<'py>(py: Python<'py>, ptr: usize) -> PyResult<Bound<'py, PyType>> {
    let obj_ptr = ptr as *mut pyo3::ffi::PyObject;
    if obj_ptr.is_null() {
        return Err(PyTypeError::new_err("Invalid struct pointer"));
    }
    // SAFETY: 指针 ptr 来自 Schema 系统，生命周期受控。
    let any = unsafe { Bound::from_borrowed_ptr(py, obj_ptr) };
    let cls = any.cast::<PyType>()?;
    Ok(cls.clone())
}

pub(crate) fn write_tarsdict_fields<W, F>(
    writer: &mut TarsWriter<W>,
    dict: &Bound<'_, PyDict>,
    depth: usize,
    serialize_typed: &F,
) -> PyResult<()>
where
    W: BufMut,
    F: Fn(&mut TarsWriter<W>, u8, &TypeExpr, &Bound<'_, PyAny>, usize) -> PyResult<()>,
{
    check_depth(depth)?;

    let mut items: SmallVec<[(u8, Bound<'_, PyAny>); 16]> = SmallVec::with_capacity(dict.len());
    for (key, value) in dict.iter() {
        if value.is_none() {
            continue;
        }
        let tag = key
            .extract::<u8>()
            .map_err(|_| PyTypeError::new_err("Struct tag must be int in range 0-255"))?;
        items.push((tag, value));
    }

    items.sort_by_key(|(tag, _)| *tag);
    for (tag, value) in items {
        serialize_any(writer, tag, &value, depth + 1, serialize_typed)?;
    }
    Ok(())
}

pub(crate) fn serialize_any<W, F>(
    writer: &mut TarsWriter<W>,
    tag: u8,
    value: &Bound<'_, PyAny>,
    depth: usize,
    serialize_typed: &F,
) -> PyResult<()>
where
    W: BufMut,
    F: Fn(&mut TarsWriter<W>, u8, &TypeExpr, &Bound<'_, PyAny>, usize) -> PyResult<()>,
{
    check_depth(depth)?;
    if value.is_none() {
        return Err(PyTypeError::new_err("Unsupported class type: NoneType"));
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
            serialize_any(writer, tag, &inner, depth + 1, serialize_typed)?;
            return Ok(true);
        }
        Ok(false)
    })?;

    if is_enum {
        return Ok(());
    }

    if value.is_instance_of::<TarsDict>() {
        let dict = value.cast::<PyDict>()?;
        writer.write_tag(tag, TarsType::StructBegin);
        write_tarsdict_fields(writer, dict, depth + 1, serialize_typed)?;
        writer.write_tag(0, TarsType::StructEnd);
        return Ok(());
    }

    let cls = value.get_type();
    if let Ok(def) = ensure_schema_for_class(value.py(), &cls) {
        writer.write_tag(tag, TarsType::StructBegin);
        serialize_struct_fields(writer, value, &def, depth + 1, serialize_typed)?;
        writer.write_tag(0, TarsType::StructEnd);
        return Ok(());
    }

    if let Some(fields) = dataclass_fields(value)? {
        writer.write_tag(tag, TarsType::Map);
        let len = fields.len();
        writer.write_int(0, len as i64);
        for (name_any, _field) in fields {
            let field_value = value.getattr(name_any.cast::<PyString>()?)?;
            serialize_any(writer, 0, &name_any, depth + 1, serialize_typed)?;
            serialize_any(writer, 1, &field_value, depth + 1, serialize_typed)?;
        }
        return Ok(());
    }

    if value.is_instance_of::<PyDict>() {
        let dict = value.cast::<PyDict>()?;
        writer.write_tag(tag, TarsType::Map);
        writer.write_int(0, dict.len() as i64);
        for (k, v) in dict {
            serialize_any(writer, 0, &k, depth + 1, serialize_typed)?;
            serialize_any(writer, 1, &v, depth + 1, serialize_typed)?;
        }
        return Ok(());
    }

    if value.is_instance_of::<PySet>() {
        let set = value.cast::<PySet>()?;
        writer.write_tag(tag, TarsType::List);
        let len = set.len() as i64;
        writer.write_int(0, len);
        for item in set.iter() {
            serialize_any(writer, 0, &item, depth + 1, serialize_typed)?;
        }
        return Ok(());
    }
    if value.is_instance_of::<PyFrozenSet>() {
        let set = value.cast::<PyFrozenSet>()?;
        writer.write_tag(tag, TarsType::List);
        let len = set.len() as i64;
        writer.write_int(0, len);
        for item in set.iter() {
            serialize_any(writer, 0, &item, depth + 1, serialize_typed)?;
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
                serialize_any(writer, 0, &item, depth + 1, serialize_typed)?;
            }
        } else {
            let seq = value.extract::<Bound<'_, PySequence>>()?;
            let len = seq.len()?;
            writer.write_int(0, len as i64);
            for i in 0..len {
                let item = seq.get_item(i)?;
                serialize_any(writer, 0, &item, depth + 1, serialize_typed)?;
            }
        }
        return Ok(());
    }

    Err(PyTypeError::new_err("Unsupported Any value type"))
}

#[inline]
pub(crate) fn read_size_non_negative(reader: &mut TarsReader, context: &str) -> DeResult<usize> {
    let len = reader
        .read_size()
        .map_err(|e| DeError::new(format!("Failed to read {} size: {}", context, e)))?;
    if len < 0 {
        return Err(DeError::new(format!("Invalid {} size", context)));
    }
    Ok(len as usize)
}

fn read_simple_list_bytes<'a>(reader: &'a mut TarsReader) -> DeResult<&'a [u8]> {
    let subtype = reader
        .read_u8()
        .map_err(|e| DeError::new(format!("Failed to read SimpleList subtype: {e}")))?;
    if subtype != 0 {
        return Err(DeError::new("SimpleList must contain Byte (0)".into()));
    }
    let len = read_size_non_negative(reader, "SimpleList")?;
    let bytes = reader
        .read_bytes(len)
        .map_err(|e| DeError::new(format!("Failed to read SimpleList bytes: {e}")))?;
    Ok(bytes)
}

pub(crate) fn decode_any_value<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    type_id: TarsType,
    depth: usize,
) -> DeResult<Bound<'py, PyAny>> {
    check_depth(depth).map_err(DeError::wrap)?;
    match type_id {
        TarsType::ZeroTag | TarsType::Int1 | TarsType::Int2 | TarsType::Int4 | TarsType::Int8 => {
            let v = reader
                .read_int(type_id)
                .map_err(|e| DeError::new(format!("Failed to read int: {e}")))?;
            Ok(v.into_pyobject(py)
                .map_err(|e| DeError::new(e.to_string()))?
                .into_any())
        }
        TarsType::Float => {
            let v = reader
                .read_float(type_id)
                .map_err(|e| DeError::new(format!("Failed to read float: {e}")))?;
            Ok(v.into_pyobject(py)
                .map_err(|e| DeError::new(e.to_string()))?
                .into_any())
        }
        TarsType::Double => {
            let v = reader
                .read_double(type_id)
                .map_err(|e| DeError::new(format!("Failed to read double: {e}")))?;
            Ok(v.into_pyobject(py)
                .map_err(|e| DeError::new(e.to_string()))?
                .into_any())
        }
        TarsType::String1 | TarsType::String4 => {
            let bytes = reader
                .read_string(type_id)
                .map_err(|e| DeError::new(format!("Failed to read string bytes: {e}")))?;
            let s = from_utf8(bytes).map_err(|_| DeError::new("Invalid UTF-8 string".into()))?;
            Ok(s.into_pyobject(py)
                .map_err(|e| DeError::new(e.to_string()))?
                .into_any())
        }
        TarsType::StructBegin => {
            decode_any_struct_fields(py, reader, depth + 1).map(|d| d.into_any())
        }
        TarsType::List => decode_any_list(py, reader, depth + 1),
        TarsType::SimpleList => decode_any_simple_list(py, reader),
        TarsType::Map => decode_any_map(py, reader, depth + 1),
        TarsType::StructEnd => Err(DeError::new("Unexpected StructEnd".into())),
    }
}

pub(crate) fn decode_any_struct_fields<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    depth: usize,
) -> DeResult<Bound<'py, PyDict>> {
    check_depth(depth).map_err(DeError::wrap)?;
    let dict = PyDict::new(py);
    while !reader.is_end() {
        let (tag, type_id) = reader
            .read_head()
            .map_err(|e| DeError::new(format!("Read head error: {e}")))?;
        if type_id == TarsType::StructEnd {
            return Ok(dict);
        }
        if dict.contains(tag).map_err(DeError::wrap)? {
            return Err(DeError::new(format!("Duplicate tag {tag} in struct")));
        }
        let value = decode_any_value(py, reader, type_id, depth + 1)
            .map_err(|e| e.prepend(PathItem::Tag(tag)))?;
        dict.set_item(tag, value).map_err(DeError::wrap)?;
    }
    Ok(dict)
}

fn decode_any_list<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    depth: usize,
) -> DeResult<Bound<'py, PyAny>> {
    check_depth(depth).map_err(DeError::wrap)?;
    let len = read_size_non_negative(reader, "list")?;
    let list_any = unsafe {
        // SAFETY: PyList_New 返回新引用并预留 len 个槽位。若返回空指针则抛错。
        let ptr = ffi::PyList_New(len as isize);
        if ptr.is_null() {
            return Err(DeError::wrap(PyErr::fetch(py)));
        }
        Bound::from_owned_ptr(py, ptr)
    };
    for idx in 0..len {
        let (_, item_type) = reader
            .read_head()
            .map_err(|e| DeError::new(format!("Failed to read list item head: {e}")))?;
        let item = decode_any_value(py, reader, item_type, depth + 1)
            .map_err(|e| e.prepend(PathItem::Index(idx)))?;
        let set_res = unsafe {
            // SAFETY: PyList_SetItem 会“偷”引用, item.into_ptr 转移所有权。
            // 每个索引只写入一次,与 PyList_New 的预分配长度一致。
            ffi::PyList_SetItem(list_any.as_ptr(), idx as isize, item.into_ptr())
        };
        if set_res != 0 {
            return Err(DeError::wrap(PyErr::fetch(py)));
        }
    }
    Ok(list_any)
}

fn decode_any_map<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    depth: usize,
) -> DeResult<Bound<'py, PyAny>> {
    check_depth(depth).map_err(DeError::wrap)?;
    let len = read_size_non_negative(reader, "map")?;
    let dict = PyDict::new(py);
    for _ in 0..len {
        let (_, kt) = reader
            .read_head()
            .map_err(|e| DeError::new(format!("Failed to read map key head: {e}")))?;
        let key = decode_any_value(py, reader, kt, depth + 1)
            .map_err(|e| e.prepend(PathItem::Key("<key>".into())))?; // Key 通常没有明确路径，先用占位符

        let (_, vt) = reader
            .read_head()
            .map_err(|e| DeError::new(format!("Failed to read map value head: {e}")))?;

        let val = decode_any_value(py, reader, vt, depth + 1)
            .map_err(|e| e.prepend(PathItem::Key(key.to_string())))?;

        if key.hash().is_err() {
            return Err(DeError::new("Map key must be hashable".into()));
        }
        dict.set_item(key, val).map_err(DeError::wrap)?;
    }
    Ok(dict.into_any())
}

fn decode_any_simple_list<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
) -> DeResult<Bound<'py, PyAny>> {
    let bytes = read_simple_list_bytes(reader)?;
    Ok(PyBytes::new(py, bytes).into_any())
}

/// 将 TarsDict 编码为 Tars 二进制数据.
///
/// Args:
///     obj: dict[int, TarsValue],tag 范围为 0-255.
///
/// Returns:
///     编码后的 bytes.
///
/// Raises:
///     TypeError: obj 不是 dict,或 tag 超出 0-255,或值类型不受支持.
///     ValueError: 递归深度超过 MAX_DEPTH.
#[pyfunction]
pub fn encode_raw(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<Py<PyBytes>> {
    if let Ok(dict) = obj.cast::<PyDict>()
        && obj.is_instance_of::<TarsDict>()
    {
        if dict.is_empty() {
            return Ok(PyBytes::new(py, &[]).unbind());
        }
        return encode_raw_dict_to_pybytes(py, dict, 0);
    }

    encode_raw_value_to_pybytes(py, obj)
}

fn encode_raw_value_to_pybytes(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<Py<PyBytes>> {
    RAW_ENCODE_BUFFER.with(|cell| {
        let mut buffer = cell
            .try_borrow_mut()
            .map_err(|_| PyRuntimeError::new_err("Re-entrant encode_raw detected"))?;
        buffer.clear();

        {
            let mut writer = TarsWriter::with_buffer(&mut *buffer);
            encode_value(&mut writer, 0, obj, 0)?;
        }

        let result = PyBytes::new(py, &buffer[..]).unbind();

        let used = buffer.len();
        if buffer.capacity() > BUFFER_SHRINK_THRESHOLD && used < (BUFFER_SHRINK_THRESHOLD / 4) {
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

/// 将 Tars 二进制数据解码为 TarsDict.
///
/// Args:
///     data: 待解码的 bytes.
///
/// Returns:
///     解码后的 dict[int, TarsValue] (实际返回 TarsDict 实例).
///
/// Raises:
///     ValueError: 数据格式不正确、存在 trailing bytes、或递归深度超过 MAX_DEPTH.
#[pyfunction]
pub fn decode_raw<'py>(py: Python<'py>, data: &[u8]) -> PyResult<Bound<'py, PyDict>> {
    let mut reader = TarsReader::new(data);
    let dict = decode_struct_fields(py, &mut reader, true, 0)?;

    if !reader.is_end() {
        return Err(PyValueError::new_err("Trailing bytes after decode_raw"));
    }

    Ok(dict)
}

fn encode_raw_dict_to_pybytes(
    py: Python<'_>,
    dict: &Bound<'_, PyDict>,
    depth: usize,
) -> PyResult<Py<PyBytes>> {
    check_depth(depth)?;

    RAW_ENCODE_BUFFER.with(|cell| {
        let mut buffer = cell.try_borrow_mut().map_err(|_| {
            PyRuntimeError::new_err("Re-entrant encode_raw detected: thread-local buffer is already borrowed. Possible cause: __repr__/__str__/__eq__ (e.g. debug printing, exception formatting) triggered encode_raw during an ongoing encode_raw.")
        })?;
        buffer.clear();

        {
            let mut writer = TarsWriter::with_buffer(&mut *buffer);
            // Top-level object for encode_raw must be a Struct (dict[int, TarsValue])
            let mut fields: SmallVec<[(u8, Bound<'_, PyAny>); 16]> = SmallVec::with_capacity(dict.len());
            for (key, value) in dict.iter() {
                if value.is_none() {
                    continue;
                }
                let tag = key
                    .extract::<u8>()
                    .map_err(|_| PyTypeError::new_err("Struct tag must be int in range 0-255"))?;
                fields.push((tag, value));
            }
            write_struct_fields_from_vec(&mut writer, fields, depth)?;
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

fn write_struct_fields_from_vec(
    writer: &mut TarsWriter<impl BufMut>,
    items: SmallVec<[(u8, Bound<'_, PyAny>); 16]>,
    depth: usize,
) -> PyResult<()> {
    check_depth(depth)?;

    let mut sorted_items = items;
    sorted_items.sort_by_key(|(tag, _)| *tag);

    for (tag, value) in sorted_items {
        encode_value(writer, tag, &value, depth + 1)?;
    }

    Ok(())
}

fn encode_value(
    writer: &mut TarsWriter<impl BufMut>,
    tag: u8,
    value: &Bound<'_, PyAny>,
    depth: usize,
) -> PyResult<()> {
    check_depth(depth)?;
    if value.is_instance_of::<Struct>() {
        return Err(PyTypeError::new_err("Unsupported raw value type"));
    }

    serialize_any(writer, tag, value, depth, &ser::serialize_impl)
}

fn decode_struct_fields<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    allow_end: bool,
    depth: usize,
) -> PyResult<Bound<'py, PyDict>> {
    check_depth(depth)?;

    // 使用 TarsDict (继承自 PyDict)
    let dict = Bound::new(py, TarsDict)?
        .into_any()
        .cast::<PyDict>()?
        .to_owned();

    while !reader.is_end() {
        let (tag, type_id) = reader
            .read_head()
            .map_err(|e| PyValueError::new_err(format!("Read head error: {e}")))?;

        if type_id == TarsType::StructEnd {
            if allow_end {
                return Ok(dict);
            }
            return Err(PyValueError::new_err("Unexpected StructEnd in decode_raw"));
        }

        if dict.contains(tag)? {
            return Err(PyValueError::new_err(format!(
                "Duplicate tag {tag} in struct"
            )));
        }

        let value = decode_value(py, reader, type_id, depth + 1)?;
        dict.set_item(tag, value)?;
    }

    Ok(dict)
}

fn decode_value<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    type_id: TarsType,
    depth: usize,
) -> PyResult<Bound<'py, PyAny>> {
    check_depth(depth)?;
    if type_id == TarsType::StructBegin {
        return decode_struct_fields(py, reader, true, depth + 1).map(|d| d.into_any());
    }
    decode_any_value(py, reader, type_id, depth).map_err(|e| e.to_pyerr(py))
}

/// 启发式探测字节数据是否为一个有效的 Tars Struct.
///
/// Args:
///     data: 可能包含 Tars Struct 的 bytes.
///
/// Returns:
///     若解析成功且完全消费输入,返回 TarsDict;否则返回 None.
#[pyfunction]
pub fn probe_struct<'py>(py: Python<'py>, data: &[u8]) -> Option<Bound<'py, PyDict>> {
    if data.is_empty() {
        return None;
    }

    // Level 1: Fail Fast
    let type_id = data[0] & 0x0F;
    if type_id > 13 {
        return None;
    }

    // Level 2: Speculative Decoding
    let mut reader = TarsReader::new(data);
    if let Ok(dict) = decode_struct_fields(py, &mut reader, true, 0) {
        // Level 3: Final Validation
        if reader.is_end() && !dict.is_empty() {
            return Some(dict);
        }
    }

    None
}
