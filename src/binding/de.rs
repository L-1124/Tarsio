use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyType};
use simdutf8::basic::from_utf8;
use std::cmp::Ordering;

use crate::ValidationError;
use crate::binding::schema::{Constraints, StructDef, TypeExpr, WireType, schema_from_class};
use crate::codec::consts::TarsType;
use crate::codec::reader::TarsReader;

const MAX_DEPTH: usize = 100;

/// 将 Tars 二进制数据解码为 Struct 实例(Schema API).
///
/// Args:
///     cls: 目标 Struct 类型.
///     data: 待解码的 bytes.
///
/// Returns:
///     解码得到的实例.
///
/// Raises:
///     TypeError: cls 未注册 Schema.
///     ValueError: 数据格式不正确、缺少必填字段、或递归深度超过限制.
#[pyfunction]
pub fn decode<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
    data: &[u8],
) -> PyResult<Bound<'py, PyAny>> {
    decode_object(py, cls, data)
}

/// 内部:将字节解码为 Tars Struct 实例.
pub fn decode_object<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
    data: &[u8],
) -> PyResult<Bound<'py, PyAny>> {
    // 校验 schema 是否存在并获取
    let def = schema_from_class(py, cls)?.ok_or_else(|| {
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

/// 从读取器中反序列化结构体.
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

    // 使用 PyType_GenericAlloc 直接分配内存,绕过 __new__ 方法调用
    // 这比 call_method1("__new__") 快得多,因为它避免了完整的 Python 方法调用开销
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
        let val = if let Some(default_value) = field.default_value.as_ref() {
            default_value.bind(py).clone()
        } else if field.is_optional {
            py.None().into_bound(py)
        } else {
            continue;
        };

        unsafe {
            let name_py = field.name.as_str().into_pyobject(py)?;
            let res =
                ffi::PyObject_GenericSetAttr(instance.as_ptr(), name_py.as_ptr(), val.as_ptr());
            if res != 0 {
                return Err(PyErr::fetch(py));
            }
        }
    }

    // 读取字段,直到遇到 StructEnd 或 EOF
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

        let idx_opt = if (tag as usize) < def.tag_lookup_vec.len() {
            def.tag_lookup_vec[tag as usize]
        } else {
            None
        };

        if let Some(idx) = idx_opt {
            let field = &def.fields_sorted[idx];
            let value = deserialize_value(py, reader, type_id, &field.ty, depth + 1)?;
            if let Some(constraints) = field.constraints.as_deref() {
                validate_value(field.name.as_str(), &value, constraints)?;
            }
            unsafe {
                let name_py = field.name.as_str().into_pyobject(py)?;
                let res = ffi::PyObject_GenericSetAttr(
                    instance.as_ptr(),
                    name_py.as_ptr(),
                    value.as_ptr(),
                );
                if res != 0 {
                    return Err(PyErr::fetch(py));
                }
            }
            seen[idx] = true;
        } else {
            // 未知 tag,跳过
            if def.forbid_unknown_tags {
                return Err(PyValueError::new_err(format!(
                    "Unknown tag {} found in deserialization (forbid_unknown_tags=True)",
                    tag
                )));
            }
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

fn validate_value<'py>(
    field_name: &str,
    value: &Bound<'py, PyAny>,
    constraints: &Constraints,
) -> PyResult<()> {
    if constraints.gt.is_some()
        || constraints.ge.is_some()
        || constraints.lt.is_some()
        || constraints.le.is_some()
    {
        let v: f64 = value.extract().map_err(|_| {
            ValidationError::new_err(format!(
                "Field '{}' must be a number to apply numeric constraints",
                field_name
            ))
        })?;

        if let Some(gt) = constraints.gt
            && v.partial_cmp(&gt) != Some(Ordering::Greater)
        {
            return Err(ValidationError::new_err(format!(
                "Field '{}' must be > {}",
                field_name, gt
            )));
        }
        if let Some(ge) = constraints.ge
            && matches!(v.partial_cmp(&ge), Some(Ordering::Less) | None)
        {
            return Err(ValidationError::new_err(format!(
                "Field '{}' must be >= {}",
                field_name, ge
            )));
        }
        if let Some(lt) = constraints.lt
            && v.partial_cmp(&lt) != Some(Ordering::Less)
        {
            return Err(ValidationError::new_err(format!(
                "Field '{}' must be < {}",
                field_name, lt
            )));
        }
        if let Some(le) = constraints.le
            && matches!(v.partial_cmp(&le), Some(Ordering::Greater) | None)
        {
            return Err(ValidationError::new_err(format!(
                "Field '{}' must be <= {}",
                field_name, le
            )));
        }
    }

    if constraints.min_len.is_some() || constraints.max_len.is_some() {
        let len = value.len().map_err(|_| {
            ValidationError::new_err(format!(
                "Field '{}' must have length to apply length constraints",
                field_name
            ))
        })?;

        if let Some(min_len) = constraints.min_len
            && len < min_len
        {
            return Err(ValidationError::new_err(format!(
                "Field '{}' length must be >= {}",
                field_name, min_len
            )));
        }
        if let Some(max_len) = constraints.max_len
            && len > max_len
        {
            return Err(ValidationError::new_err(format!(
                "Field '{}' length must be <= {}",
                field_name, max_len
            )));
        }
    }

    if let Some(pattern) = constraints.pattern.as_ref() {
        let s: &str = value.extract().map_err(|_| {
            ValidationError::new_err(format!(
                "Field '{}' must be a string to apply pattern constraint",
                field_name
            ))
        })?;
        if !pattern.is_match(s) {
            return Err(ValidationError::new_err(format!(
                "Field '{}' does not match pattern",
                field_name
            )));
        }
    }

    Ok(())
}

/// 根据 TypeExpr 反序列化单个值.
fn deserialize_value<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    type_id: TarsType,
    type_expr: &TypeExpr,
    depth: usize,
) -> PyResult<Bound<'py, PyAny>> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded during deserialization",
        ));
    }

    match type_expr {
        TypeExpr::Primitive(wire_type) => match wire_type {
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
                let s =
                    from_utf8(bytes).map_err(|_| PyValueError::new_err("Invalid UTF-8 string"))?;
                Ok(s.into_pyobject(py)?.into_any())
            }
            _ => Err(PyValueError::new_err("Unexpected wire type for primitive")),
        },
        TypeExpr::Struct(ptr) => {
            let obj_ptr = *ptr as *mut ffi::PyObject;
            let nested_any = unsafe { Bound::from_borrowed_ptr(py, obj_ptr) };
            let nested_cls = nested_any.cast::<PyType>()?;
            let nested_def = schema_from_class(py, nested_cls)?
                .ok_or_else(|| PyTypeError::new_err("Nested struct schema not found"))?;
            deserialize_struct(py, reader, &nested_def, depth + 1)
        }
        TypeExpr::List(inner) | TypeExpr::Tuple(inner) => {
            // 处理 SimpleList(字节数组)
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

            if matches!(type_expr, TypeExpr::Tuple(_)) {
                Ok(list.to_tuple().into_any())
            } else {
                Ok(list.into_any())
            }
        }
        TypeExpr::Map(k_type, v_type) => {
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
        TypeExpr::Optional(inner) => deserialize_value(py, reader, type_id, inner, depth),
    }
}
