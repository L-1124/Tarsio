use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyType};
use simdutf8::basic::from_utf8;

use crate::binding::schema::{StructDef, WireType, get_schema};
use crate::codec::consts::TarsType;
use crate::codec::reader::TarsReader;

const MAX_DEPTH: usize = 100;

/// 将 Tars 二进制数据解码为 Struct 实例（Schema API）。
///
/// Args:
///     cls: 目标 Struct 类型。
///     data: 待解码的 bytes。
///
/// Returns:
///     解码得到的实例。
///
/// Raises:
///     TypeError: cls 未注册 Schema。
///     ValueError: 数据格式不正确、缺少必填字段、或递归深度超过限制。
#[pyfunction]
pub fn decode<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
    data: &[u8],
) -> PyResult<Bound<'py, PyAny>> {
    decode_object(py, cls, data)
}

/// 内部：将字节解码为 Tars Struct 实例。
pub fn decode_object<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
    data: &[u8],
) -> PyResult<Bound<'py, PyAny>> {
    let type_ptr = cls.as_ptr() as usize;

    // 校验 schema 是否存在并获取
    let def = get_schema(type_ptr).ok_or_else(|| {
        let class_name = cls
            .name()
            .map(|n| n.to_string())
            .unwrap_or_else(|_| "Unknown".to_string());
        PyTypeError::new_err(format!(
            "Cannot deserialize to '{}': No schema found",
            class_name
        ))
    })?;

    let mut reader = TarsReader::new(data);
    deserialize_struct(py, &mut reader, &def, 0)
}

/// 从读取器中反序列化结构体。
fn deserialize_struct<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    def: &StructDef,
    depth: usize,
) -> PyResult<Bound<'py, PyAny>> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded during deserialization",
        ));
    }

    // 从 StructDef 中获取 Python 类
    let class_obj = def.bind_class(py);

    // 使用 PyType_GenericAlloc 直接分配内存，绕过 __new__ 方法调用
    // 这比 call_method1("__new__") 快得多，因为它避免了完整的 Python 方法调用开销
    let instance = unsafe {
        let type_ptr = class_obj.as_ptr() as *mut ffi::PyTypeObject;
        let obj_ptr = ffi::PyType_GenericAlloc(type_ptr, 0);
        if obj_ptr.is_null() {
            return Err(PyErr::fetch(py));
        }
        Bound::from_owned_ptr(py, obj_ptr)
    };

    let mut seen = vec![false; def.fields_sorted.len()];

    for field in &def.fields_sorted {
        if let Some(default_value) = field.default_value.as_ref() {
            instance.setattr(field.name.as_str(), default_value.bind(py))?;
        } else if field.is_optional {
            instance.setattr(field.name.as_str(), py.None())?;
        }
    }

    // 读取字段，直到遇到 StructEnd 或 EOF
    while !reader.is_end() {
        let (tag, type_id) = match reader.peek_head() {
            Ok(h) => h,
            Err(_) => break,
        };

        // 判断是否为 StructEnd
        if type_id == TarsType::StructEnd {
            reader
                .read_head()
                .map_err(|e| PyValueError::new_err(format!("Read head error: {}", e)))?; // Consume StructEnd
            break;
        }

        reader
            .read_head()
            .map_err(|e| PyValueError::new_err(format!("Read head error: {}", e)))?; // Consume the head

        if let Some(&idx) = def.tag_index.get(&tag) {
            let field = &def.fields_sorted[idx];
            let value = deserialize_value(py, reader, type_id, &field.wire_type, depth + 1)?;
            instance.setattr(field.name.as_str(), value)?;
            seen[idx] = true;
        } else {
            // 未知 tag，跳过
            reader.skip_field(type_id).map_err(|e| {
                PyValueError::new_err(format!("Failed to skip unknown field: {}", e))
            })?;
        }
    }

    // 检查所有必填字段是否已设置
    for (idx, field) in def.fields_sorted.iter().enumerate() {
        if field.is_required && !seen[idx] {
            return Err(PyValueError::new_err(format!(
                "Missing required field '{}' in deserialization",
                field.name
            )));
        }
    }

    Ok(instance)
}

/// 根据 WireType 反序列化单个值。
fn deserialize_value<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    type_id: TarsType,
    wire_type: &WireType,
    depth: usize,
) -> PyResult<Bound<'py, PyAny>> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded during deserialization",
        ));
    }

    match wire_type {
        WireType::Int | WireType::Long => {
            let v = reader
                .read_int(type_id)
                .map_err(|e| PyValueError::new_err(format!("Failed to read int: {}", e)))?;
            Ok(v.into_pyobject(py)?.into_any())
        }
        WireType::Float => {
            let v = reader
                .read_float(type_id)
                .map_err(|e| PyValueError::new_err(format!("Failed to read float: {}", e)))?;
            Ok(v.into_pyobject(py)?.into_any())
        }
        WireType::Double => {
            let v = reader
                .read_double(type_id)
                .map_err(|e| PyValueError::new_err(format!("Failed to read double: {}", e)))?;
            Ok(v.into_pyobject(py)?.into_any())
        }
        WireType::String => {
            let bytes = reader.read_string(type_id).map_err(|e| {
                PyValueError::new_err(format!("Failed to read string bytes: {}", e))
            })?;

            // 使用 simdutf8 进行 SIMD 加速 UTF-8 验证
            // 注: 对于超大字符串 (>几MB) 可考虑释放 GIL, 但:
            // - simdutf8 速度 >20GB/s, 1MB 验证仅需 ~50μs
            // - GIL 切换开销约 ~100ns-1μs
            // - 当前场景收益不明显, 保持简单实现
            let s = from_utf8(bytes).map_err(|_| PyValueError::new_err("Invalid UTF-8 string"))?;
            Ok(s.into_pyobject(py)?.into_any())
        }
        WireType::Struct(ptr) => {
            // 递归反序列化嵌套结构体
            let nested_def = get_schema(*ptr)
                .ok_or_else(|| PyTypeError::new_err("Nested struct schema not found"))?;
            deserialize_struct(py, reader, &nested_def, depth + 1)
        }
        WireType::List(inner) => {
            // 处理 SimpleList（字节数组）
            if type_id == TarsType::SimpleList {
                let _sub_type = reader.read_u8().map_err(|e| {
                    PyValueError::new_err(format!("Failed to read SimpleList subtype: {}", e))
                })?;
                let len = reader.read_size().map_err(|e| {
                    PyValueError::new_err(format!("Failed to read SimpleList size: {}", e))
                })? as usize;
                let bytes = reader.read_bytes(len).map_err(|e| {
                    PyValueError::new_err(format!("Failed to read SimpleList bytes: {}", e))
                })?;
                return Ok(PyBytes::new(py, bytes).into_any());
            }

            // 普通列表
            let len = reader
                .read_size()
                .map_err(|e| PyValueError::new_err(format!("Failed to read list size: {}", e)))?
                as usize;

            let list = PyList::empty(py);
            for _ in 0..len {
                let (_, item_type) = reader.read_head().map_err(|e| {
                    PyValueError::new_err(format!("Failed to read list item head: {}", e))
                })?;
                let item = deserialize_value(py, reader, item_type, inner, depth + 1)?;
                list.append(item)?;
            }
            Ok(list.into_any())
        }
        WireType::Map(k_type, v_type) => {
            let len = reader
                .read_size()
                .map_err(|e| PyValueError::new_err(format!("Failed to read map size: {}", e)))?
                as usize;

            let dict = PyDict::new(py);
            for _ in 0..len {
                let (_, kt) = reader.read_head().map_err(|e| {
                    PyValueError::new_err(format!("Failed to read map key head: {}", e))
                })?;
                let key = deserialize_value(py, reader, kt, k_type, depth + 1)?;

                let (_, vt) = reader.read_head().map_err(|e| {
                    PyValueError::new_err(format!("Failed to read map value head: {}", e))
                })?;
                let val = deserialize_value(py, reader, vt, v_type, depth + 1)?;

                dict.set_item(key, val)?;
            }
            Ok(dict.into_any())
        }
    }
}
