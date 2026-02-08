use pyo3::exceptions::{PyRuntimeError, PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{
    PyAny, PyBool, PyBytes, PyDict, PyFloat, PyFrozenSet, PyList, PySequence, PySet, PyString,
};
use std::cell::RefCell;

use bytes::BufMut;

use crate::binding::schema::TarsDict;
use crate::codec::consts::TarsType;
use crate::codec::reader::TarsReader;
use crate::codec::writer::TarsWriter;
use simdutf8::basic::from_utf8;

const MAX_DEPTH: usize = 100;
// Capacity threshold (1MB). If buffer exceeds this, we shrink it back.
const BUFFER_SHRINK_THRESHOLD: usize = 1024 * 1024;
// Default initial capacity (128 bytes).
const BUFFER_DEFAULT_CAPACITY: usize = 128;

thread_local! {
    static RAW_ENCODE_BUFFER: RefCell<Vec<u8>> = RefCell::new(Vec::with_capacity(128));
}

struct PySequenceFast {
    ptr: *mut ffi::PyObject,
    is_list: bool,
}

impl PySequenceFast {
    fn new(obj: &Bound<'_, PyAny>) -> PyResult<Self> {
        let ptr = obj.as_ptr();
        // Check types first without creating new references if possible,
        // but PySequence_Fast logic is specific.
        // Here we just use what we have since we know it's a sequence.
        // But to be consistent with `ser.rs` logic of owning the reference:
        unsafe { ffi::Py_INCREF(ptr) };

        let is_list = unsafe { ffi::PyList_Check(ptr) != 0 };
        // We assume valid tuple/list input here from caller checks, or check again.
        Ok(Self { ptr, is_list })
    }

    fn len(&self, py: Python<'_>) -> PyResult<usize> {
        let len = unsafe {
            if self.is_list {
                let list_ptr = self.ptr as *mut ffi::PyListObject;
                (*list_ptr).ob_base.ob_size
            } else {
                let tuple_ptr = self.ptr as *mut ffi::PyTupleObject;
                (*tuple_ptr).ob_base.ob_size
            }
        };
        if len < 0 {
            return Err(PyErr::fetch(py));
        }
        Ok(len as usize)
    }

    fn get_item<'py>(&self, py: Python<'py>, idx: usize) -> PyResult<Bound<'py, PyAny>> {
        let item_ptr = unsafe {
            if self.is_list {
                ffi::PyList_GetItem(self.ptr, idx as isize)
            } else {
                ffi::PyTuple_GetItem(self.ptr, idx as isize)
            }
        };
        if item_ptr.is_null() {
            return Err(PyErr::fetch(py));
        }
        Ok(unsafe { Bound::from_borrowed_ptr(py, item_ptr) })
    }
}

impl Drop for PySequenceFast {
    fn drop(&mut self) {
        unsafe {
            ffi::Py_DECREF(self.ptr);
        }
    }
}

fn is_exact_tuple(obj: &Bound<'_, PyAny>) -> bool {
    // 仅对精确 tuple 走 PySequence_Fast，避免子类被物化
    unsafe { ffi::PyTuple_CheckExact(obj.as_ptr()) != 0 }
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
    if let Ok(dict) = obj.cast::<PyDict>() {
        if obj.is_instance_of::<TarsDict>() {
            if dict.is_empty() {
                return Ok(PyBytes::new(py, &[]).unbind());
            }
            return encode_raw_dict_to_pybytes(py, dict, 0);
        }
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
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded during raw serialization",
        ));
    }

    RAW_ENCODE_BUFFER.with(|cell| {
        let mut buffer = cell.try_borrow_mut().map_err(|_| {
            PyRuntimeError::new_err("Re-entrant encode_raw detected: thread-local buffer is already borrowed. Possible cause: __repr__/__str__/__eq__ (e.g. debug printing, exception formatting) triggered encode_raw during an ongoing encode_raw.")
        })?;
        buffer.clear();

        {
            let mut writer = TarsWriter::with_buffer(&mut *buffer);
            // Top-level object for encode_raw must be a Struct (dict[int, TarsValue])
            let mut fields: Vec<(u8, Bound<'_, PyAny>)> = Vec::with_capacity(dict.len());
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
    items: Vec<(u8, Bound<'_, PyAny>)>,
    depth: usize,
) -> PyResult<()> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded during raw serialization",
        ));
    }

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
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded during raw serialization",
        ));
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
        let v: String = value.extract()?;
        writer.write_string(tag, &v);
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

    if value.is_instance_of::<PyDict>() {
        let dict = value.cast::<PyDict>()?;
        if value.is_instance_of::<TarsDict>() {
            writer.write_tag(tag, TarsType::StructBegin);

            // Filter None values and extract tags for struct encoding
            let mut struct_items = Vec::with_capacity(dict.len());
            for (k, v) in dict.iter() {
                if v.is_none() {
                    continue;
                }
                let tag_u8 = k
                    .extract::<u8>()
                    .map_err(|_| PyTypeError::new_err("Struct tag must be int in range 0-255"))?;
                struct_items.push((tag_u8, v));
            }
            write_struct_fields_from_vec(writer, struct_items, depth + 1)?;
            writer.write_tag(0, TarsType::StructEnd);
        } else {
            let len = dict.len();
            writer.write_tag(tag, TarsType::Map);
            writer.write_int(0, len as i64);
            for (k, v) in dict.iter() {
                if k.hash().is_err() {
                    return Err(PyTypeError::new_err("Map key must be hashable"));
                }
                encode_value(writer, 0, &k, depth + 1)?;
                encode_value(writer, 1, &v, depth + 1)?;
            }
        }
        return Ok(());
    }

    if let Ok(set) = value.cast::<PySet>() {
        writer.write_tag(tag, TarsType::List);
        writer.write_int(0, set.len() as i64);

        for item in set.iter() {
            encode_value(writer, 0, &item, depth + 1)?;
        }
        return Ok(());
    }

    if let Ok(set) = value.cast::<PyFrozenSet>() {
        writer.write_tag(tag, TarsType::List);
        writer.write_int(0, set.len() as i64);

        for item in set.iter() {
            encode_value(writer, 0, &item, depth + 1)?;
        }
        return Ok(());
    }

    if value.is_instance_of::<PySequence>() && !value.is_instance_of::<PyList>() {
        writer.write_tag(tag, TarsType::List);
        if is_exact_tuple(value) {
            let seq_fast = PySequenceFast::new(value)?;
            let len = seq_fast.len(value.py())?;
            writer.write_int(0, len as i64);
            for i in 0..len {
                let item = seq_fast.get_item(value.py(), i)?;
                encode_value(writer, 0, &item, depth + 1)?;
            }
        } else {
            let seq = value.extract::<Bound<'_, PySequence>>()?;
            let len = seq.len()?;
            writer.write_int(0, len as i64);
            for i in 0..len {
                let item = seq.get_item(i)?;
                encode_value(writer, 0, &item, depth + 1)?;
            }
        }
        return Ok(());
    }

    if value.is_instance_of::<PyList>() {
        let list = value.cast::<PyList>()?;
        writer.write_tag(tag, TarsType::List);
        writer.write_int(0, list.len() as i64);

        for item in list.iter() {
            encode_value(writer, 0, &item, depth + 1)?;
        }
        return Ok(());
    }

    Err(PyTypeError::new_err("Unsupported raw value type"))
}

fn decode_struct_fields<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    allow_end: bool,
    depth: usize,
) -> PyResult<Bound<'py, PyDict>> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded during raw deserialization",
        ));
    }

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
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded during raw deserialization",
        ));
    }
    match type_id {
        TarsType::ZeroTag | TarsType::Int1 | TarsType::Int2 | TarsType::Int4 | TarsType::Int8 => {
            let v = reader
                .read_int(type_id)
                .map_err(|e| PyValueError::new_err(format!("Failed to read int: {e}")))?;
            Ok(v.into_pyobject(py)?.into_any())
        }
        TarsType::Float => {
            let v = reader
                .read_float(type_id)
                .map_err(|e| PyValueError::new_err(format!("Failed to read float: {e}")))?;
            Ok(v.into_pyobject(py)?.into_any())
        }
        TarsType::Double => {
            let v = reader
                .read_double(type_id)
                .map_err(|e| PyValueError::new_err(format!("Failed to read double: {e}")))?;
            Ok(v.into_pyobject(py)?.into_any())
        }
        TarsType::String1 | TarsType::String4 => {
            let bytes = reader
                .read_string(type_id)
                .map_err(|e| PyValueError::new_err(format!("Failed to read string bytes: {e}")))?;
            let s = from_utf8(bytes).map_err(|_| PyValueError::new_err("Invalid UTF-8 string"))?;
            Ok(s.into_pyobject(py)?.into_any())
        }
        TarsType::StructBegin => {
            decode_struct_fields(py, reader, true, depth + 1).map(|d| d.into_any())
        }
        TarsType::List => decode_list_value(py, reader, depth),
        TarsType::SimpleList => decode_simple_list(py, reader),
        TarsType::Map => decode_map_value(py, reader, depth),
        TarsType::StructEnd => Err(PyValueError::new_err("Unexpected StructEnd")),
    }
}

fn decode_list_value<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    depth: usize,
) -> PyResult<Bound<'py, PyAny>> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded during raw deserialization",
        ));
    }
    let len = reader
        .read_size()
        .map_err(|e| PyValueError::new_err(format!("Failed to read list size: {e}")))?;
    if len < 0 {
        return Err(PyValueError::new_err("Invalid list size"));
    }
    let len = len as usize;

    let list = unsafe {
        // SAFETY: PyList_New 返回新引用并预留 len 个槽位。若返回空指针则抛错。
        let ptr = ffi::PyList_New(len as isize);
        if ptr.is_null() {
            return Err(PyErr::fetch(py));
        }
        Bound::from_owned_ptr(py, ptr)
    };

    // Optimization: Peek first head to see if we should even try to collect bytes.
    let mut is_bytes = true;
    let mut bytes_candidate: Option<Vec<u8>> = if len > 0 {
        if let Ok((_, t)) = reader.peek_head() {
            if matches!(t, TarsType::ZeroTag | TarsType::Int1) {
                Some(Vec::with_capacity(len))
            } else {
                is_bytes = false;
                None
            }
        } else {
            is_bytes = false;
            None
        }
    } else {
        // Empty list -> b""
        Some(Vec::new())
    };

    for idx in 0..len {
        let (_, item_type) = reader
            .read_head()
            .map_err(|e| PyValueError::new_err(format!("Failed to read list item head: {e}")))?;

        let item = match item_type {
            TarsType::ZeroTag
            | TarsType::Int1
            | TarsType::Int2
            | TarsType::Int4
            | TarsType::Int8 => {
                let v = reader
                    .read_int(item_type)
                    .map_err(|e| PyValueError::new_err(format!("Failed to read int: {e}")))?;
                if is_bytes {
                    if (item_type == TarsType::Int1 || item_type == TarsType::ZeroTag)
                        && (0..=255).contains(&v)
                    {
                        if let Some(buf) = &mut bytes_candidate {
                            buf.push(v as u8);
                        }
                    } else {
                        is_bytes = false;
                        bytes_candidate = None; // Drop the buffer early
                    }
                }
                v.into_pyobject(py)?.into_any()
            }
            _ => {
                is_bytes = false;
                bytes_candidate = None;
                decode_value(py, reader, item_type, depth + 1)?
            }
        };
        let set_res = unsafe {
            // SAFETY: PyList_SetItem 会“偷”引用, item.into_ptr 转移所有权。
            ffi::PyList_SetItem(list.as_ptr(), idx as isize, item.into_ptr())
        };
        if set_res != 0 {
            return Err(PyErr::fetch(py));
        }
    }

    if is_bytes && let Some(buf) = bytes_candidate {
        return Ok(PyBytes::new(py, &buf).into_any());
    }

    Ok(list.into_any())
}

fn decode_simple_list<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
) -> PyResult<Bound<'py, PyAny>> {
    let subtype = reader
        .read_u8()
        .map_err(|e| PyValueError::new_err(format!("Failed to read SimpleList subtype: {e}")))?;

    if subtype != 0 {
        return Err(PyValueError::new_err("SimpleList must contain Byte (0)"));
    }

    let len = reader
        .read_size()
        .map_err(|e| PyValueError::new_err(format!("Failed to read SimpleList size: {e}")))?;
    if len < 0 {
        return Err(PyValueError::new_err("Invalid SimpleList size"));
    }
    let len = len as usize;

    let bytes = reader
        .read_bytes(len)
        .map_err(|e| PyValueError::new_err(format!("Failed to read SimpleList bytes: {e}")))?;

    if let Ok(s) = from_utf8(bytes) {
        return Ok(s.into_pyobject(py)?.into_any());
    }

    Ok(PyBytes::new(py, bytes).into_any())
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

fn decode_map_value<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    depth: usize,
) -> PyResult<Bound<'py, PyAny>> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded during raw deserialization",
        ));
    }
    let len = reader
        .read_size()
        .map_err(|e| PyValueError::new_err(format!("Failed to read map size: {e}")))?;
    if len < 0 {
        return Err(PyValueError::new_err("Invalid map size"));
    }
    let len = len as usize;
    let dict = PyDict::new(py);

    for _ in 0..len {
        let (_, kt) = reader
            .read_head()
            .map_err(|e| PyValueError::new_err(format!("Failed to read map key head: {e}")))?;
        let key = decode_value(py, reader, kt, depth + 1)?;

        let (_, vt) = reader
            .read_head()
            .map_err(|e| PyValueError::new_err(format!("Failed to read map value head: {e}")))?;
        let val = decode_value(py, reader, vt, depth + 1)?;

        if key.hash().is_err() {
            return Err(PyTypeError::new_err("Map key must be hashable"));
        }
        dict.set_item(key, val)?;
    }

    Ok(dict.into_any())
}
