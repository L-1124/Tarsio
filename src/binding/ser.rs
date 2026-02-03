use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PySequence};

use crate::binding::schema::{WireType, get_schema};
use crate::codec::consts::TarsType;
use crate::codec::writer::TarsWriter;

/// Serialize a Python Struct to Tars bytes (codec-style API).
#[pyfunction]
pub fn encode(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<Py<PyBytes>> {
    let bytes = encode_object(obj)?;
    Ok(PyBytes::new(py, &bytes).unbind())
}

/// Internal: Serialize a Python object to Tars bytes using its schema.
pub fn encode_object(obj: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    let cls = obj.get_type();
    let ptr = cls.as_ptr() as usize;

    // Check if it is a registered struct
    if get_schema(ptr).is_some() {
        let mut writer = TarsWriter::new();
        serialize_struct_fields(&mut writer, ptr, obj)?;
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
) -> PyResult<()> {
    let def = get_schema(type_ptr)
        .ok_or_else(|| PyTypeError::new_err("Schema not found during serialization"))?;

    for field in &def.fields_sorted {
        // Use getattr to fetch field value
        match obj.getattr(field.name.as_str()) {
            Ok(val) => {
                if val.is_none() {
                    // Skip optional fields if they are None
                    continue;
                }
                serialize_impl(writer, field.tag, &field.wire_type, &val)?;
            }
            Err(_) => {
                // If required, ensure it exists or error
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
) -> PyResult<()> {
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
            serialize_struct_fields(writer, *ptr, val)?;
            writer.write_tag(0, TarsType::StructEnd);
        }
        WireType::List(inner) => {
            // Optimization for bytes -> SimpleList
            if let WireType::Int = **inner {
                // If it's effectively bytes, use SimpleList
                // Check if it's PyBytes first
                if val.is_instance_of::<PyBytes>() {
                    // Safe cast or extract?
                    // val.extract::<&[u8]>() is easiest
                    if let Ok(bytes) = val.extract::<&[u8]>() {
                        writer.write_bytes(tag, bytes);
                        return Ok(());
                    }
                }
            }

            writer.write_tag(tag, TarsType::List);
            // PySequence is not a specific class like PyDict, it's a protocol?
            // PyO3 exposes PySequence type?
            // Yes `pyo3::types::PySequence`.
            // Use extract instead of downcast to avoid deprecation ambiguity?
            let seq = val.extract::<Bound<'_, PySequence>>()?;
            let len = seq.len()?;
            writer.write_int(0, len as i64); // Length

            for i in 0..len {
                let item = seq.get_item(i)?;
                serialize_impl(writer, 0, inner, &item)?;
            }
        }
        WireType::Map(k_type, v_type) => {
            writer.write_tag(tag, TarsType::Map);
            // Use extract for PyDict?
            let dict = val.extract::<Bound<'_, PyDict>>()?;
            let len = dict.len();
            writer.write_int(0, len as i64); // Length

            for (k, v) in dict {
                serialize_impl(writer, 0, k_type, &k)?;
                serialize_impl(writer, 1, v_type, &v)?;
            }
        }
    }
    Ok(())
}
