use crate::consts::JceType;
use crate::error::JceDecodeError;
use crate::reader::JceReader;
use crate::writer::JceWriter;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyTuple};

/// 递归深度限制
const MAX_DEPTH: usize = 100;

/// OMIT_DEFAULT 选项标志
const OPT_OMIT_DEFAULT: i32 = 1;

#[pyfunction]
#[pyo3(signature = (obj, schema, options=0, context=None))]
pub fn dumps(
    py: Python<'_>,
    obj: PyObject,
    schema: &Bound<'_, PyList>,
    options: i32,
    context: Option<PyObject>,
) -> PyResult<PyObject> {
    let mut writer = JceWriter::new();
    let context = context.unwrap_or_else(|| PyDict::new(py).into());

    encode_struct(py, &mut writer, obj.bind(py), schema, options, &context, 0)?;

    Ok(PyBytes::new(py, writer.get_buffer()).into())
}

#[pyfunction]
#[pyo3(signature = (data, schema, options=0, context=None))]
pub fn loads(
    py: Python<'_>,
    data: &[u8],
    schema: &Bound<'_, PyList>,
    options: i32,
    context: Option<PyObject>,
) -> PyResult<PyObject> {
    let mut reader = JceReader::new(data);
    let context = context.unwrap_or_else(|| PyDict::new(py).into());

    decode_struct(py, &mut reader, schema, options, &context, 0)
}

fn encode_struct(
    py: Python<'_>,
    writer: &mut JceWriter,
    obj: &Bound<'_, PyAny>,
    schema: &Bound<'_, PyList>,
    options: i32,
    context: &PyObject,
    depth: usize,
) -> PyResult<()> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err("Max recursion depth exceeded"));
    }

    for item in schema.iter() {
        let tuple = item
            .downcast::<PyTuple>()
            .map_err(|_| PyTypeError::new_err("Schema item must be a tuple"))?;

        // schema: (name, tag, type, default, has_serializer, has_deserializer)
        let name: String = tuple.get_item(0)?.extract()?;
        let tag: u8 = tuple.get_item(1)?.extract()?;
        let jce_type_code: u8 = tuple.get_item(2)?.extract()?;
        let default_val = tuple.get_item(3)?;
        let has_serializer: bool = tuple.get_item(4)?.extract()?;

        let mut value = obj.getattr(&name)?;

        // Check OMIT_DEFAULT
        if (options & OPT_OMIT_DEFAULT) != 0 {
            if value.eq(default_val)? {
                continue;
            }
        }

        // Call serializer hook if present
        if has_serializer {
            let serializers = obj.getattr("__jce_serializers__")?;
            let serializer_name: String = serializers.get_item(&name)?.extract()?;
            let serializer_func = obj.getattr(&serializer_name)?;
            value = serializer_func.call1((value, context))?;
        }

        let jce_type = JceType::try_from(jce_type_code)
            .map_err(|id| PyValueError::new_err(format!("Invalid JCE type code: {}", id)))?;

        encode_field(
            py,
            writer,
            tag,
            &value,
            jce_type,
            options,
            context,
            depth + 1,
        )?;
    }
    Ok(())
}

fn encode_field(
    py: Python<'_>,
    writer: &mut JceWriter,
    tag: u8,
    value: &Bound<'_, PyAny>,
    jce_type: JceType,
    options: i32,
    context: &PyObject,
    depth: usize,
) -> PyResult<()> {
    match jce_type {
        JceType::Int1 | JceType::Int2 | JceType::Int4 | JceType::Int8 | JceType::ZeroTag => {
            let val: i64 = value.extract()?;
            writer.write_int(tag, val);
        }
        JceType::Float => {
            let val: f32 = value.extract()?;
            writer.write_float(tag, val);
        }
        JceType::Double => {
            let val: f64 = value.extract()?;
            writer.write_double(tag, val);
        }
        JceType::String1 | JceType::String4 => {
            let val: String = value.extract()?;
            writer.write_string(tag, &val);
        }
        JceType::SimpleList => {
            let val: Vec<u8> = value.extract()?;
            writer.write_bytes(tag, &val);
        }
        JceType::StructBegin => {
            writer.write_tag(tag, JceType::StructBegin);
            let nested_schema = value.getattr("__jce_schema__")?.downcast_into::<PyList>()?;
            encode_struct(py, writer, value, &nested_schema, options, context, depth)?;
            writer.write_tag(0, JceType::StructEnd);
        }
        _ => {
            return Err(PyValueError::new_err(format!(
                "Unsupported JCE type for encoding: {:?}",
                jce_type
            )))
        }
    }
    Ok(())
}

fn decode_struct(
    py: Python<'_>,
    reader: &mut JceReader,
    schema: &Bound<'_, PyList>,
    options: i32,
    context: &PyObject,
    depth: usize,
) -> PyResult<PyObject> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err("Max recursion depth exceeded"));
    }

    let result_dict = PyDict::new(py);

    // Map tag to schema item for quick lookup
    let mut tag_map = std::collections::HashMap::new();
    let schema_items: Vec<Bound<'_, PyTuple>> = schema
        .iter()
        .map(|item| item.downcast_into::<PyTuple>())
        .collect::<Result<Vec<_>, _>>()?;

    for tuple in &schema_items {
        let tag: u8 = tuple.get_item(1)?.extract()?;
        tag_map.insert(tag, tuple);
    }

    while !reader.is_end() {
        let (tag, jce_type) = match reader.read_head() {
            Ok(h) => h,
            Err(e) => return Err(map_decode_error(e)),
        };

        if jce_type == JceType::StructEnd {
            break;
        }

        if let Some(tuple) = tag_map.get(&tag) {
            let name: String = tuple.get_item(0)?.extract()?;
            let jce_type_code: u8 = tuple.get_item(2)?.extract()?;
            let _has_deserializer: bool = tuple.get_item(5)?.extract()?;

            let expected_type = JceType::try_from(jce_type_code).map_err(|id| {
                PyValueError::new_err(format!("Invalid JCE type code in schema: {}", id))
            })?;

            let value = decode_field(
                py,
                reader,
                jce_type,
                expected_type,
                options,
                context,
                depth + 1,
            )?;
            result_dict.set_item(name, value)?;
        } else {
            if let Err(e) = reader.skip_field(jce_type) {
                return Err(map_decode_error(e));
            }
        }
    }

    // Fill defaults for missing fields
    for tuple in &schema_items {
        let name: String = tuple.get_item(0)?.extract()?;
        if !result_dict.contains(&name)? {
            let default_val = tuple.get_item(3)?;
            result_dict.set_item(name, default_val)?;
        }
    }

    Ok(result_dict.into())
}

fn decode_field(
    py: Python<'_>,
    reader: &mut JceReader,
    actual_type: JceType,
    _expected_type: JceType,
    _options: i32,
    _context: &PyObject,
    _depth: usize,
) -> PyResult<PyObject> {
    match actual_type {
        JceType::Int1 | JceType::Int2 | JceType::Int4 | JceType::Int8 | JceType::ZeroTag => {
            let val = reader.read_int(actual_type).map_err(map_decode_error)?;
            Ok(val.into_pyobject(py)?.into())
        }
        JceType::Float => {
            let val = reader.read_float().map_err(map_decode_error)?;
            Ok(val.into_pyobject(py)?.into())
        }
        JceType::Double => {
            let val = reader.read_double().map_err(map_decode_error)?;
            Ok(val.into_pyobject(py)?.into())
        }
        JceType::String1 | JceType::String4 => {
            let val = reader.read_string(actual_type).map_err(map_decode_error)?;
            Ok(val.into_pyobject(py)?.into())
        }
        JceType::SimpleList => {
            let (_, t) = reader.read_head().map_err(map_decode_error)?;
            if t != JceType::Int1 {
                return Err(PyValueError::new_err("SimpleList must contain Int1 (byte)"));
            }
            let _ = reader.read_int(t).map_err(map_decode_error)?;

            let (_, t2) = reader.read_head().map_err(map_decode_error)?;
            let len = reader.read_int(t2).map_err(map_decode_error)? as usize;

            let buf = reader.read_bytes(len).map_err(map_decode_error)?;
            Ok(PyBytes::new(py, &buf).into())
        }
        JceType::StructBegin => {
            reader.skip_field(actual_type).map_err(map_decode_error)?;
            Ok(py.None())
        }
        _ => {
            reader.skip_field(actual_type).map_err(map_decode_error)?;
            Ok(py.None())
        }
    }
}

fn map_decode_error(err: JceDecodeError) -> PyErr {
    PyValueError::new_err(err.to_string())
}
