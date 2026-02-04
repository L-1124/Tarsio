use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBool, PyBytes, PyDict, PyFloat, PyList, PySequence, PyString};

use crate::codec::consts::TarsType;
use crate::codec::reader::TarsReader;
use crate::codec::writer::TarsWriter;
use simdutf8::basic::from_utf8;

const MAX_DEPTH: usize = 100;

/// 将 TarsDict 编码为 Tars 二进制数据。
///
/// Args:
///     obj: dict[int, TarsValue]，tag 范围为 0-255。
///
/// Returns:
///     编码后的 bytes。
///
/// Raises:
///     TypeError: obj 不是 dict，或 tag 超出 0-255，或值类型不受支持。
///     ValueError: 递归深度超过 MAX_DEPTH。
#[pyfunction]
pub fn encode_raw(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<Py<PyBytes>> {
    let dict = obj
        .cast::<PyDict>()
        .map_err(|_| PyTypeError::new_err("encode_raw expects a dict[int, TarsValue]"))?;

    let bytes = encode_raw_dict(dict, 0)?;
    Ok(PyBytes::new(py, &bytes).unbind())
}

/// 将 Tars 二进制数据解码为 TarsDict。
///
/// Args:
///     data: 待解码的 bytes。
///
/// Returns:
///     解码后的 dict[int, TarsValue]。
///
/// Raises:
///     ValueError: 数据格式不正确、存在 trailing bytes、或递归深度超过 MAX_DEPTH。
#[pyfunction]
pub fn decode_raw<'py>(py: Python<'py>, data: &[u8]) -> PyResult<Bound<'py, PyDict>> {
    let mut reader = TarsReader::new(data);
    let dict = decode_struct_fields(py, &mut reader, true, 0)?;

    if !reader.is_end() {
        return Err(PyValueError::new_err("Trailing bytes after decode_raw"));
    }

    Ok(dict)
}

fn encode_raw_dict(dict: &Bound<'_, PyDict>, depth: usize) -> PyResult<Vec<u8>> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded during raw serialization",
        ));
    }
    let mut writer = TarsWriter::new();
    write_struct_fields(&mut writer, dict, depth)?;
    Ok(writer.into_inner())
}

fn write_struct_fields(
    writer: &mut TarsWriter,
    dict: &Bound<'_, PyDict>,
    depth: usize,
) -> PyResult<()> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded during raw serialization",
        ));
    }
    let mut fields: Vec<(u8, Bound<'_, PyAny>)> = Vec::with_capacity(dict.len());

    for (key, value) in dict.iter() {
        let tag = key
            .extract::<u8>()
            .map_err(|_| PyTypeError::new_err("Struct tag must be int in range 0-255"))?;
        fields.push((tag, value));
    }

    fields.sort_by_key(|(tag, _)| *tag);

    for (tag, value) in fields {
        encode_value(writer, tag, &value, depth + 1)?;
    }

    Ok(())
}

fn encode_value(
    writer: &mut TarsWriter,
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
        if is_struct_dict(dict)? {
            writer.write_tag(tag, TarsType::StructBegin);
            write_struct_fields(writer, dict, depth + 1)?;
            writer.write_tag(0, TarsType::StructEnd);
            return Ok(());
        }

        writer.write_tag(tag, TarsType::Map);
        writer.write_int(0, dict.len() as i64);
        for (k, v) in dict.iter() {
            if k.hash().is_err() {
                return Err(PyTypeError::new_err("Map key must be hashable"));
            }
            encode_value(writer, 0, &k, depth + 1)?;
            encode_value(writer, 1, &v, depth + 1)?;
        }
        return Ok(());
    }

    if value.is_instance_of::<PySequence>() && !value.is_instance_of::<PyList>() {
        let seq = value.extract::<Bound<'_, PySequence>>()?;
        writer.write_tag(tag, TarsType::List);
        let len = seq.len()?;
        writer.write_int(0, len as i64);

        for i in 0..len {
            let item = seq.get_item(i)?;
            encode_value(writer, 0, &item, depth + 1)?;
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
    let dict = PyDict::new(py);

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
        .map_err(|e| PyValueError::new_err(format!("Failed to read list size: {e}")))?
        as usize;

    let list = PyList::empty(py);
    let mut bytes_candidate: Vec<u8> = Vec::with_capacity(len);
    let mut is_bytes = true;

    for _ in 0..len {
        let (_, item_type) = reader
            .read_head()
            .map_err(|e| PyValueError::new_err(format!("Failed to read list item head: {e}")))?;
        let item = decode_value(py, reader, item_type, depth + 1)?;
        if is_bytes {
            if item_type == TarsType::Int1 || item_type == TarsType::ZeroTag {
                if let Ok(v) = item.extract::<i64>() {
                    if (0..=255).contains(&v) {
                        bytes_candidate.push(v as u8);
                    } else {
                        is_bytes = false;
                    }
                } else {
                    is_bytes = false;
                }
            } else {
                is_bytes = false;
            }
        }
        list.append(item)?;
    }

    if is_bytes {
        return Ok(PyBytes::new(py, &bytes_candidate).into_any());
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
        .map_err(|e| PyValueError::new_err(format!("Failed to read SimpleList size: {e}")))?
        as usize;

    let bytes = reader
        .read_bytes(len)
        .map_err(|e| PyValueError::new_err(format!("Failed to read SimpleList bytes: {e}")))?;

    if let Ok(s) = from_utf8(bytes) {
        return Ok(s.into_pyobject(py)?.into_any());
    }

    Ok(PyBytes::new(py, bytes).into_any())
}

/// 启发式探测字节数据是否为一个有效的 Tars Struct。
///
/// Args:
///     data: 可能包含 Tars Struct 的 bytes。
///
/// Returns:
///     若解析成功且完全消费输入，返回 TarsDict；否则返回 None。
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
        .map_err(|e| PyValueError::new_err(format!("Failed to read map size: {e}")))?
        as usize;
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

fn is_struct_dict(dict: &Bound<'_, PyDict>) -> PyResult<bool> {
    for (key, _) in dict.iter() {
        if key.extract::<u8>().is_err() {
            return Ok(false);
        }
    }
    Ok(true)
}
