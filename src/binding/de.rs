use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyType};

use crate::binding::schema::{StructDef, WireType, get_schema};
use crate::codec::consts::TarsType;
use crate::codec::reader::TarsReader;

/// Deserialize Tars bytes into a Python Struct (codec-style API).
#[pyfunction]
pub fn decode<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
    data: &[u8],
) -> PyResult<Bound<'py, PyAny>> {
    decode_object(py, cls, data)
}

/// Internal: Deserialize bytes into a Tars Struct instance.
pub fn decode_object<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
    data: &[u8],
) -> PyResult<Bound<'py, PyAny>> {
    let type_ptr = cls.as_ptr() as usize;

    // Verify schema exists and get it
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
    deserialize_struct(py, &mut reader, &def)
}

/// Deserialize a struct from the reader.
fn deserialize_struct<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    def: &StructDef,
) -> PyResult<Bound<'py, PyAny>> {
    // Get the Python class from the StructDef
    let class_obj = def.bind_class(py);

    // Create new instance using __new__ directly, bypassing __init__
    // This allows us to set attributes after construction.
    let instance = class_obj.call_method1("__new__", (&class_obj,))?;

    // Build a map of tag -> field for quick lookup
    let mut field_map: std::collections::HashMap<u8, (&str, &WireType, bool)> =
        std::collections::HashMap::new();
    for field in &def.fields_sorted {
        field_map.insert(
            field.tag,
            (field.name.as_str(), &field.wire_type, field.is_optional),
        );
    }

    // Initialize all fields to None to allow required-field validation later.
    for field in &def.fields_sorted {
        instance.setattr(field.name.as_str(), py.None())?;
    }

    // Read fields from the stream until we hit StructEnd or EOF
    while !reader.is_end() {
        let (tag, type_id) = match reader.peek_head() {
            Ok(h) => h,
            Err(_) => break,
        };

        // Check if this is StructEnd
        if type_id == TarsType::StructEnd {
            reader
                .read_head()
                .map_err(|e| PyValueError::new_err(format!("Read head error: {}", e)))?; // Consume StructEnd
            break;
        }

        reader
            .read_head()
            .map_err(|e| PyValueError::new_err(format!("Read head error: {}", e)))?; // Consume the head

        if let Some(&(name, wire_type, _is_optional)) = field_map.get(&tag) {
            // Deserialize the value
            let value = deserialize_value(py, reader, type_id, wire_type)?;
            instance.setattr(name, value)?;
        } else {
            // Unknown tag, skip it
            reader.skip_field(type_id).map_err(|e| {
                PyValueError::new_err(format!("Failed to skip unknown field: {}", e))
            })?;
        }
    }

    // Check that all required fields were set
    for field in &def.fields_sorted {
        if field.is_required {
            let val = instance.getattr(field.name.as_str())?;
            if val.is_none() {
                return Err(PyValueError::new_err(format!(
                    "Missing required field '{}' in deserialization",
                    field.name
                )));
            }
        }
    }

    Ok(instance)
}

/// Deserialize a single value based on WireType.
fn deserialize_value<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    type_id: TarsType,
    wire_type: &WireType,
) -> PyResult<Bound<'py, PyAny>> {
    match wire_type {
        WireType::Int | WireType::Long => {
            let v = reader
                .read_int(type_id)
                .map_err(|e| PyValueError::new_err(format!("Failed to read int: {}", e)))?;
            Ok(v.into_pyobject(py)?.into_any())
        }
        WireType::Float => {
            let v = reader
                .read_float()
                .map_err(|e| PyValueError::new_err(format!("Failed to read float: {}", e)))?;
            Ok(v.into_pyobject(py)?.into_any())
        }
        WireType::Double => {
            let v = reader
                .read_double()
                .map_err(|e| PyValueError::new_err(format!("Failed to read double: {}", e)))?;
            Ok(v.into_pyobject(py)?.into_any())
        }
        WireType::String => {
            let v = reader
                .read_string(type_id)
                .map_err(|e| PyValueError::new_err(format!("Failed to read string: {}", e)))?;
            Ok(v.as_ref().into_pyobject(py)?.into_any())
        }
        WireType::Struct(ptr) => {
            // Recursively deserialize nested struct
            let nested_def = get_schema(*ptr)
                .ok_or_else(|| PyTypeError::new_err("Nested struct schema not found"))?;
            deserialize_struct(py, reader, &nested_def)
        }
        WireType::List(inner) => {
            // Check for SimpleList (bytes)
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

            // Normal list
            let len = reader
                .read_size()
                .map_err(|e| PyValueError::new_err(format!("Failed to read list size: {}", e)))?
                as usize;

            let list = PyList::empty(py);
            for _ in 0..len {
                let (_, item_type) = reader.read_head().map_err(|e| {
                    PyValueError::new_err(format!("Failed to read list item head: {}", e))
                })?;
                let item = deserialize_value(py, reader, item_type, inner)?;
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
                let key = deserialize_value(py, reader, kt, k_type)?;

                let (_, vt) = reader.read_head().map_err(|e| {
                    PyValueError::new_err(format!("Failed to read map value head: {}", e))
                })?;
                let val = deserialize_value(py, reader, vt, v_type)?;

                dict.set_item(key, val)?;
            }
            Ok(dict.into_any())
        }
    }
}
