use std::cell::RefCell;

use pyo3::exceptions::{PyRuntimeError, PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBool, PyBytes, PyDict, PyFloat, PyList, PySequence, PyString, PyType};

use bytes::BufMut;

use crate::binding::introspect::StructKind;
use crate::binding::schema::{StructDef, TypeExpr, WireType, ensure_schema_for_class};
use crate::codec::consts::TarsType;
use crate::codec::writer::TarsWriter;

const MAX_DEPTH: usize = 100;

thread_local! {
    static ENCODE_BUFFER: RefCell<Vec<u8>> = RefCell::new(Vec::with_capacity(128));
}

/// 将一个已注册的 Struct 实例编码为 Tars 二进制数据(Schema API).
///
/// Args:
///     obj: Struct 实例.
///
/// Returns:
///     编码后的 bytes.
///
/// Raises:
///     TypeError: obj 不是已注册的 Struct.
///     ValueError: 缺少必填字段、类型不匹配、或递归深度超过限制.
#[pyfunction]
pub fn encode(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<Py<PyBytes>> {
    encode_object_to_pybytes(py, obj)
}

pub fn encode_object_to_pybytes(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<Py<PyBytes>> {
    let cls = obj.get_type();
    let def = ensure_schema_for_class(py, &cls)?;

    ENCODE_BUFFER.with(|cell| {
        let mut buffer = cell.try_borrow_mut().map_err(|_| {
            PyRuntimeError::new_err("Re-entrant encode detected: thread-local buffer is already borrowed. Possible cause: __repr__/__str__/__eq__ (e.g. debug printing, exception formatting) triggered encode during an ongoing encode.")
        })?;
        buffer.clear();

        {
            let mut writer = TarsWriter::with_buffer(&mut *buffer);
            serialize_struct_fields(&mut writer, obj, &def, 0)?;
        }

        Ok(PyBytes::new(py, &buffer[..]).unbind())
    })
}

fn serialize_struct_fields(
    writer: &mut TarsWriter<impl BufMut>,
    obj: &Bound<'_, PyAny>,
    def: &StructDef,
    depth: usize,
) -> PyResult<()> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded or circular reference detected",
        ));
    }

    for field in &def.fields_sorted {
        let value = match def.kind {
            StructKind::TypedDict => {
                let dict = obj
                    .cast::<PyDict>()
                    .map_err(|_| PyTypeError::new_err("TypedDict value must be a dict instance"))?;
                dict.get_item(field.name.as_str())?
            }
            _ => obj.getattr(field.name.as_str()).ok(),
        };

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
                serialize_impl(writer, field.tag, &field.ty, &val, depth + 1)?;
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

fn serialize_impl(
    writer: &mut TarsWriter<impl BufMut>,
    tag: u8,
    type_expr: &TypeExpr,
    val: &Bound<'_, PyAny>,
    depth: usize,
) -> PyResult<()> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded or circular reference detected",
        ));
    }

    match type_expr {
        TypeExpr::Primitive(wire_type) => match wire_type {
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
            _ => {
                return Err(PyTypeError::new_err(
                    "Unsupported primitive wire type in serialization",
                ));
            }
        },
        TypeExpr::Any => {
            serialize_any(writer, tag, val, depth + 1)?;
        }
        TypeExpr::NoneType => {
            return Err(PyTypeError::new_err(
                "NoneType must be encoded via Optional or Union",
            ));
        }
        TypeExpr::DateTime => {
            let micros = datetime_to_micros(val.py(), val)?;
            writer.write_int(tag, micros);
        }
        TypeExpr::Date => {
            let days = date_to_days(val.py(), val)?;
            writer.write_int(tag, days);
        }
        TypeExpr::Time => {
            let nanos = time_to_nanos(val)?;
            writer.write_int(tag, nanos);
        }
        TypeExpr::Timedelta => {
            let micros = timedelta_to_micros(val)?;
            writer.write_int(tag, micros);
        }
        TypeExpr::Uuid => {
            let bytes = uuid_to_bytes(val.py(), val)?;
            writer.write_bytes(tag, &bytes);
        }
        TypeExpr::Decimal => {
            let s = decimal_to_string(val)?;
            writer.write_string(tag, &s);
        }
        TypeExpr::Enum(enum_cls, inner) => {
            let enum_type = enum_cls.bind(val.py());
            if !val.is_instance(enum_type.as_any())? {
                return Err(PyTypeError::new_err("Enum value type mismatch"));
            }
            let value = val.getattr("value")?;
            serialize_impl(writer, tag, inner, &value, depth + 1)?;
        }
        TypeExpr::Union(variants) => {
            let (variant_idx, inner_opt) = select_union_variant(val.py(), variants, val)?;
            writer.write_tag(tag, TarsType::StructBegin);
            writer.write_int(0, variant_idx as i64);
            if let Some(inner) = inner_opt {
                serialize_impl(writer, 1, inner, val, depth + 1)?;
            }
            writer.write_tag(0, TarsType::StructEnd);
        }
        TypeExpr::Struct(ptr) => {
            writer.write_tag(tag, TarsType::StructBegin);
            let cls = class_from_ptr(val.py(), *ptr)?;
            let def = ensure_schema_for_class(val.py(), &cls)?;
            serialize_struct_fields(writer, val, &def, depth + 1)?;
            writer.write_tag(0, TarsType::StructEnd);
        }
        TypeExpr::List(inner) | TypeExpr::Tuple(inner) => {
            if matches!(**inner, TypeExpr::Primitive(WireType::Int))
                && val.is_instance_of::<PyBytes>()
                && let Ok(bytes) = val.extract::<&[u8]>()
            {
                writer.write_bytes(tag, bytes);
                return Ok(());
            }

            writer.write_tag(tag, TarsType::List);
            let seq = val.extract::<Bound<'_, PySequence>>()?;
            let len = seq.len()?;
            writer.write_int(0, len as i64);

            for i in 0..len {
                let item = seq.get_item(i)?;
                serialize_impl(writer, 0, inner, &item, depth + 1)?;
            }
        }
        TypeExpr::Map(k_type, v_type) => {
            writer.write_tag(tag, TarsType::Map);
            let dict = val.extract::<Bound<'_, PyDict>>()?;
            let len = dict.len();
            writer.write_int(0, len as i64);

            for (k, v) in dict {
                serialize_impl(writer, 0, k_type, &k, depth + 1)?;
                serialize_impl(writer, 1, v_type, &v, depth + 1)?;
            }
        }
        TypeExpr::Optional(inner) => {
            if val.is_none() {
                return Ok(());
            }
            serialize_impl(writer, tag, inner, val, depth + 1)?;
        }
    }
    Ok(())
}

fn class_from_ptr<'py>(py: Python<'py>, ptr: usize) -> PyResult<Bound<'py, PyType>> {
    let obj_ptr = ptr as *mut ffi::PyObject;
    if obj_ptr.is_null() {
        return Err(PyTypeError::new_err("Invalid struct pointer"));
    }
    let any = unsafe { Bound::from_borrowed_ptr(py, obj_ptr) };
    let cls = any.cast::<PyType>()?;
    Ok(cls.clone())
}

fn select_union_variant<'py>(
    py: Python<'py>,
    variants: &'py [TypeExpr],
    value: &Bound<'py, PyAny>,
) -> PyResult<(usize, Option<&'py TypeExpr>)> {
    if value.is_none() {
        for (idx, variant) in variants.iter().enumerate() {
            if matches!(variant, TypeExpr::NoneType) {
                return Ok((idx, None));
            }
        }
        return Err(PyTypeError::new_err("Union does not accept None"));
    }

    for (idx, variant) in variants.iter().enumerate() {
        if value_matches_type(py, variant, value)? {
            return Ok((idx, Some(variant)));
        }
    }
    Err(PyTypeError::new_err(
        "Value does not match any union variant",
    ))
}

fn value_matches_type<'py>(
    py: Python<'py>,
    typ: &TypeExpr,
    value: &Bound<'py, PyAny>,
) -> PyResult<bool> {
    match typ {
        TypeExpr::Any => Ok(true),
        TypeExpr::NoneType => Ok(value.is_none()),
        TypeExpr::Primitive(wire_type) => match wire_type {
            WireType::Int => {
                if value.is_instance_of::<PyBool>() {
                    Ok(false)
                } else {
                    Ok(value.extract::<i64>().is_ok())
                }
            }
            WireType::Long => Ok(value.extract::<i64>().is_ok()),
            WireType::Float | WireType::Double => Ok(value.is_instance_of::<PyFloat>()),
            WireType::String => Ok(value.is_instance_of::<PyString>()),
            _ => Ok(false),
        },
        TypeExpr::DateTime => is_datetime_instance(py, value),
        TypeExpr::Date => is_date_instance(py, value),
        TypeExpr::Time => is_time_instance(py, value),
        TypeExpr::Timedelta => is_timedelta_instance(py, value),
        TypeExpr::Uuid => is_uuid_instance(py, value),
        TypeExpr::Decimal => is_decimal_instance(py, value),
        TypeExpr::Enum(enum_cls, _) => Ok(value.is_instance(enum_cls.bind(py).as_any())?),
        TypeExpr::Struct(ptr) => {
            let cls = class_from_ptr(py, *ptr)?;
            let def = ensure_schema_for_class(py, &cls)?;
            match def.kind {
                StructKind::TypedDict => Ok(value.is_instance_of::<PyDict>()),
                _ => Ok(value.is_instance(cls.as_any())?),
            }
        }
        TypeExpr::List(_) | TypeExpr::Tuple(_) => Ok(value.is_instance_of::<PySequence>()
            && !value.is_instance_of::<PyString>()
            && !value.is_instance_of::<PyBytes>()
            && !value.is_instance_of::<PyDict>()),
        TypeExpr::Map(_, _) => Ok(value.is_instance_of::<PyDict>()),
        TypeExpr::Optional(inner) => {
            if value.is_none() {
                Ok(true)
            } else {
                value_matches_type(py, inner, value)
            }
        }
        TypeExpr::Union(variants) => {
            for variant in variants {
                if value_matches_type(py, variant, value)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
    }
}

fn serialize_any(
    writer: &mut TarsWriter<impl BufMut>,
    tag: u8,
    value: &Bound<'_, PyAny>,
    depth: usize,
) -> PyResult<()> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded or circular reference detected",
        ));
    }
    if value.is_none() {
        return Err(PyTypeError::new_err("Any cannot encode None directly"));
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

    if is_datetime_instance(value.py(), value)? {
        let micros = datetime_to_micros(value.py(), value)?;
        writer.write_int(tag, micros);
        return Ok(());
    }
    if is_date_instance(value.py(), value)? {
        let days = date_to_days(value.py(), value)?;
        writer.write_int(tag, days);
        return Ok(());
    }
    if is_time_instance(value.py(), value)? {
        let nanos = time_to_nanos(value)?;
        writer.write_int(tag, nanos);
        return Ok(());
    }
    if is_timedelta_instance(value.py(), value)? {
        let micros = timedelta_to_micros(value)?;
        writer.write_int(tag, micros);
        return Ok(());
    }
    if is_uuid_instance(value.py(), value)? {
        let bytes = uuid_to_bytes(value.py(), value)?;
        writer.write_bytes(tag, &bytes);
        return Ok(());
    }
    if is_decimal_instance(value.py(), value)? {
        let s = decimal_to_string(value)?;
        writer.write_string(tag, &s);
        return Ok(());
    }
    let enum_mod = value.py().import("enum")?;
    let enum_cls = enum_mod.getattr("Enum")?;
    if value.is_instance(&enum_cls)? {
        let inner = value.getattr("value")?;
        serialize_any(writer, tag, &inner, depth + 1)?;
        return Ok(());
    }

    let cls = value.get_type();
    if let Ok(def) = ensure_schema_for_class(value.py(), &cls) {
        writer.write_tag(tag, TarsType::StructBegin);
        serialize_struct_fields(writer, value, &def, depth + 1)?;
        writer.write_tag(0, TarsType::StructEnd);
        return Ok(());
    }

    if value.is_instance_of::<PyDict>() {
        let dict = value.cast::<PyDict>()?;
        writer.write_tag(tag, TarsType::Map);
        writer.write_int(0, dict.len() as i64);
        for (k, v) in dict {
            serialize_any(writer, 0, &k, depth + 1)?;
            serialize_any(writer, 1, &v, depth + 1)?;
        }
        return Ok(());
    }

    if value.is_instance_of::<PyList>() || value.is_instance_of::<PySequence>() {
        let seq = value.extract::<Bound<'_, PySequence>>()?;
        writer.write_tag(tag, TarsType::List);
        let len = seq.len()?;
        writer.write_int(0, len as i64);
        for i in 0..len {
            let item = seq.get_item(i)?;
            serialize_any(writer, 0, &item, depth + 1)?;
        }
        return Ok(());
    }

    Err(PyTypeError::new_err("Unsupported Any value type"))
}

fn is_datetime_instance(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<bool> {
    let datetime_mod = py.import("datetime")?;
    let dt_cls = datetime_mod.getattr("datetime")?;
    value.is_instance(&dt_cls)
}

fn is_date_instance(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<bool> {
    let datetime_mod = py.import("datetime")?;
    let date_cls = datetime_mod.getattr("date")?;
    value.is_instance(&date_cls)
}

fn is_time_instance(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<bool> {
    let datetime_mod = py.import("datetime")?;
    let time_cls = datetime_mod.getattr("time")?;
    value.is_instance(&time_cls)
}

fn is_timedelta_instance(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<bool> {
    let datetime_mod = py.import("datetime")?;
    let td_cls = datetime_mod.getattr("timedelta")?;
    value.is_instance(&td_cls)
}

fn is_uuid_instance(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<bool> {
    let uuid_mod = py.import("uuid")?;
    let uuid_cls = uuid_mod.getattr("UUID")?;
    value.is_instance(&uuid_cls)
}

fn is_decimal_instance(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<bool> {
    let decimal_mod = py.import("decimal")?;
    let decimal_cls = decimal_mod.getattr("Decimal")?;
    value.is_instance(&decimal_cls)
}

fn datetime_to_micros(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<i64> {
    let datetime_mod = py.import("datetime")?;
    let tz = datetime_mod.getattr("timezone")?.getattr("utc")?;
    let tzinfo = value.getattr("tzinfo")?;
    let value = if tzinfo.is_none() {
        let kwargs = PyDict::new(py);
        kwargs.set_item("tzinfo", tz)?;
        value.call_method("replace", (), Some(&kwargs))?
    } else {
        value.clone()
    };
    let ts: f64 = value.call_method0("timestamp")?.extract()?;
    Ok((ts * 1_000_000.0) as i64)
}

fn date_to_days(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<i64> {
    let datetime_mod = py.import("datetime")?;
    let epoch = datetime_mod.getattr("date")?.call1((1970, 1, 1))?;
    let delta = value.call_method1("__sub__", (epoch,))?;
    let days: i64 = delta.getattr("days")?.extract()?;
    Ok(days)
}

fn time_to_nanos(value: &Bound<'_, PyAny>) -> PyResult<i64> {
    let hour: i64 = value.getattr("hour")?.extract()?;
    let minute: i64 = value.getattr("minute")?.extract()?;
    let second: i64 = value.getattr("second")?.extract()?;
    let micro: i64 = value.getattr("microsecond")?.extract()?;
    Ok(((hour * 3600 + minute * 60 + second) * 1_000_000 + micro) * 1_000)
}

fn timedelta_to_micros(value: &Bound<'_, PyAny>) -> PyResult<i64> {
    let days: i64 = value.getattr("days")?.extract()?;
    let seconds: i64 = value.getattr("seconds")?.extract()?;
    let micros: i64 = value.getattr("microseconds")?.extract()?;
    Ok(days * 86_400_000_000 + seconds * 1_000_000 + micros)
}

fn uuid_to_bytes(_py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    let bytes = value.getattr("bytes")?;
    let slice: &[u8] = bytes.extract()?;
    Ok(slice.to_vec())
}

fn decimal_to_string(value: &Bound<'_, PyAny>) -> PyResult<String> {
    value.str()?.extract::<String>()
}
