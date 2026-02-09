use pyo3::exceptions::{PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyDict};
use std::cell::RefCell;

use bytes::BufMut;

use crate::binding::any_codec::{decode_any_value, serialize_any};
use crate::binding::schema::{Struct, TarsDict};
use crate::binding::ser;
use crate::codec::consts::TarsType;
use crate::codec::reader::TarsReader;
use crate::codec::writer::TarsWriter;

const MAX_DEPTH: usize = 100;
// Capacity threshold (1MB). If buffer exceeds this, we shrink it back.
const BUFFER_SHRINK_THRESHOLD: usize = 1024 * 1024;
// Default initial capacity (128 bytes).
const BUFFER_DEFAULT_CAPACITY: usize = 128;

thread_local! {
    static RAW_ENCODE_BUFFER: RefCell<Vec<u8>> = RefCell::new(Vec::with_capacity(128));
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
///     auto_simplelist: 是否自动解析 SimpleList 的 bytes.
///         为 True 时: 若内容看起来像 Tars Struct 则保持 bytes,
///         否则在 UTF-8 完整可解码时返回 str, 失败回退为 bytes.
///
/// Returns:
///     解码后的 dict[int, TarsValue] (实际返回 TarsDict 实例).
///
/// Raises:
///     ValueError: 数据格式不正确、存在 trailing bytes、或递归深度超过 MAX_DEPTH.
#[pyfunction]
#[pyo3(signature = (data, auto_simplelist = false))]
pub fn decode_raw<'py>(
    py: Python<'py>,
    data: &[u8],
    auto_simplelist: bool,
) -> PyResult<Bound<'py, PyDict>> {
    let mut reader = TarsReader::new(data);
    let dict = decode_struct_fields(py, &mut reader, true, 0, auto_simplelist)?;

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
    auto_simplelist: bool,
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

        let value = decode_value(py, reader, type_id, depth + 1, auto_simplelist)?;
        dict.set_item(tag, value)?;
    }

    Ok(dict)
}

fn decode_value<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    type_id: TarsType,
    depth: usize,
    auto_simplelist: bool,
) -> PyResult<Bound<'py, PyAny>> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded during raw deserialization",
        ));
    }
    if type_id == TarsType::StructBegin {
        return decode_struct_fields(py, reader, true, depth + 1, auto_simplelist)
            .map(|d| d.into_any());
    }
    decode_any_value(py, reader, type_id, depth, auto_simplelist)
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
    if let Ok(dict) = decode_struct_fields(py, &mut reader, true, 0, false) {
        // Level 3: Final Validation
        if reader.is_end() && !dict.is_empty() {
            return Some(dict);
        }
    }

    None
}
