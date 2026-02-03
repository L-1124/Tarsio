use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PySequence};

use crate::binding::schema::{WireType, get_schema};
use crate::codec::consts::TarsType;
use crate::codec::writer::TarsWriter;

const MAX_DEPTH: usize = 100;

/// 将一个已注册的 Struct 实例编码为 Tars 二进制数据（Schema API）。
///
/// Args:
///     obj: Struct 实例。
///
/// Returns:
///     编码后的 bytes。
///
/// Raises:
///     TypeError: obj 不是已注册的 Struct。
///     ValueError: 缺少必填字段、类型不匹配、或递归深度超过限制。
#[pyfunction]
pub fn encode(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<Py<PyBytes>> {
    let bytes = encode_object(obj)?;
    Ok(PyBytes::new(py, &bytes).unbind())
}

/// 内部：使用 schema 将 Python 对象序列化为 Tars 字节。
pub fn encode_object(obj: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    let cls = obj.get_type();
    let ptr = cls.as_ptr() as usize;

    // 检查是否为已注册的 Struct
    if get_schema(ptr).is_some() {
        let mut writer = TarsWriter::new();
        serialize_struct_fields(&mut writer, ptr, obj, 0)?;
        Ok(writer.into_inner())
    } else {
        Err(PyTypeError::new_err(format!(
            "Object of type '{}' is not a registered Tars Struct",
            cls.name()?
        )))
    }
}

fn serialize_struct_fields(
    writer: &mut TarsWriter,
    type_ptr: usize,
    obj: &Bound<'_, PyAny>,
    depth: usize,
) -> PyResult<()> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded or circular reference detected",
        ));
    }

    let def = get_schema(type_ptr)
        .ok_or_else(|| PyTypeError::new_err("Schema not found during serialization"))?;

    for field in &def.fields_sorted {
        // 使用 getattr 获取字段值
        match obj.getattr(field.name.as_str()) {
            Ok(val) => {
                if val.is_none() {
                    // 可选字段为 None 时跳过
                    continue;
                }
                serialize_impl(writer, field.tag, &field.wire_type, &val, depth + 1)?;
            }
            Err(_) => {
                // 必需字段缺失则报错
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
    writer: &mut TarsWriter,
    tag: u8,
    wire_type: &WireType,
    val: &Bound<'_, PyAny>,
    depth: usize,
) -> PyResult<()> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded or circular reference detected",
        ));
    }

    match wire_type {
        WireType::Int => {
            let v: i64 = val.extract()?;
            writer.write_int(tag, v);
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
            let v: String = val.extract()?;
            writer.write_string(tag, &v);
        }
        WireType::Struct(ptr) => {
            writer.write_tag(tag, TarsType::StructBegin);
            serialize_struct_fields(writer, *ptr, val, depth + 1)?;
            writer.write_tag(0, TarsType::StructEnd);
        }
        WireType::List(inner) => {
            // 字节数组的 SimpleList 优化
            if let WireType::Int = **inner {
                // 若可视为字节数组则使用 SimpleList
                // 先检查是否为 PyBytes
                if val.is_instance_of::<PyBytes>() {
                    // 安全的转换或提取？
                    // val.extract::<&[u8]>() 最简单
                    if let Ok(bytes) = val.extract::<&[u8]>() {
                        writer.write_bytes(tag, bytes);
                        return Ok(());
                    }
                }
            }

            writer.write_tag(tag, TarsType::List);
            // PySequence 不是像 PyDict 那样的具体类，而是协议
            // PyO3 暴露了 PySequence 类型
            // 是的：`pyo3::types::PySequence`
            // 使用 extract 而不是 downcast，避免弃用歧义
            let seq = val.extract::<Bound<'_, PySequence>>()?;
            let len = seq.len()?;
            writer.write_int(0, len as i64); // 长度

            for i in 0..len {
                let item = seq.get_item(i)?;
                serialize_impl(writer, 0, inner, &item, depth + 1)?;
            }
        }
        WireType::Map(k_type, v_type) => {
            writer.write_tag(tag, TarsType::Map);
            // 对 PyDict 使用 extract？
            let dict = val.extract::<Bound<'_, PyDict>>()?;
            let len = dict.len();
            writer.write_int(0, len as i64); // 长度

            for (k, v) in dict {
                serialize_impl(writer, 0, k_type, &k, depth + 1)?;
                serialize_impl(writer, 1, v_type, &v, depth + 1)?;
            }
        }
    }
    Ok(())
}
