use pyo3::exceptions::{PyRuntimeError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyDict, PyFrozenSet, PySequence, PySet, PyString};
use std::cell::RefCell;

use bytes::BufMut;

use crate::binding::codec::raw::{serialize_any, serialize_struct_fields, write_tarsdict_fields};
use crate::binding::schema::{TarsDict, TypeExpr, UnionCache, WireType, ensure_schema_for_class};
use crate::binding::utils::{
    PySequenceFast, check_depth, check_exact_sequence_type, class_from_type, dataclass_fields,
    maybe_shrink_buffer, try_coerce_buffer_to_bytes,
};
use crate::binding::validation::value_matches_type;
use crate::codec::consts::TarsType;
use crate::codec::writer::TarsWriter;

thread_local! {
    static ENCODE_BUFFER: RefCell<Vec<u8>> = RefCell::new(Vec::with_capacity(128));
}

fn serialize_tuple_like(
    writer: &mut TarsWriter<impl BufMut>,
    tag: u8,
    items: &[TypeExpr],
    val: &Bound<'_, PyAny>,
    depth: usize,
) -> PyResult<()> {
    writer.write_tag(tag, TarsType::List);
    let expected = items.len();
    if let Some(is_list) = check_exact_sequence_type(val) {
        let seq_fast = PySequenceFast::new_exact(val, is_list)?;
        let len = seq_fast.len();
        if len != expected {
            return Err(PyTypeError::new_err(
                "Tuple value length does not match annotation",
            ));
        }
        writer.write_int(0, len as i64);
        for (idx, item_type) in items.iter().enumerate() {
            let item = seq_fast.get_item(val.py(), idx)?;
            serialize_impl(writer, 0, item_type, &item, depth + 1)?;
        }
    } else {
        let seq = val.extract::<Bound<'_, PySequence>>()?;
        let len = seq.len()?;
        if len != expected {
            return Err(PyTypeError::new_err(
                "Tuple value length does not match annotation",
            ));
        }
        writer.write_int(0, len as i64);
        for (idx, item_type) in items.iter().enumerate() {
            let item = seq.get_item(idx)?;
            serialize_impl(writer, 0, item_type, &item, depth + 1)?;
        }
    }
    Ok(())
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
            serialize_struct_fields(
                &mut writer,
                obj,
                &def,
                0,
                true,
                &serialize_impl_standard,
            )?;
        }

        let result = PyBytes::new(py, &buffer[..]).unbind();

        maybe_shrink_buffer(&mut buffer);

        Ok(result)
    })
}

pub(crate) fn encode_struct_payload_to_vec(
    obj: &Bound<'_, PyAny>,
    def: &crate::binding::schema::StructDef,
    depth: usize,
) -> PyResult<Vec<u8>> {
    let mut payload = Vec::with_capacity(64);
    {
        let mut nested_writer = TarsWriter::with_buffer(&mut payload);
        serialize_struct_fields(
            &mut nested_writer,
            obj,
            def,
            depth + 1,
            true,
            &serialize_impl_standard,
        )?;
    }
    Ok(payload)
}

pub(crate) fn encode_tarsdict_payload_to_vec(
    val: &Bound<'_, PyAny>,
    depth: usize,
) -> PyResult<Vec<u8>> {
    if !val.is_instance_of::<TarsDict>() {
        return Err(PyTypeError::new_err("TarsDict value type mismatch"));
    }
    let dict = val.cast::<PyDict>()?;
    let mut payload = Vec::with_capacity(64);
    {
        let mut nested_writer = TarsWriter::with_buffer(&mut payload);
        write_tarsdict_fields(
            &mut nested_writer,
            dict,
            depth + 1,
            &serialize_impl_standard,
        )?;
    }
    Ok(payload)
}

pub(crate) fn serialize_impl(
    writer: &mut TarsWriter<impl BufMut>,
    tag: u8,
    type_expr: &TypeExpr,
    val: &Bound<'_, PyAny>,
    depth: usize,
) -> PyResult<()> {
    check_depth(depth)?;

    match type_expr {
        TypeExpr::Primitive(wire_type) => match wire_type {
            WireType::Int => {
                let v: i64 = val.extract()?;
                writer.write_int(tag, v);
            }
            WireType::Bool => {
                let v: bool = val.extract()?;
                writer.write_int(tag, i64::from(v));
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
                let v: &str = val.extract()?;
                writer.write_string(tag, v);
            }
            _ => {
                return Err(PyTypeError::new_err(
                    "Unsupported primitive wire type in serialization",
                ));
            }
        },
        TypeExpr::Any => {
            serialize_any(writer, tag, val, depth + 1, &serialize_impl_standard)?;
        }
        TypeExpr::NoneType => {
            return Err(PyTypeError::new_err(
                "NoneType must be encoded via Optional or Union",
            ));
        }
        TypeExpr::Enum(enum_cls, inner) => {
            let enum_type = enum_cls.bind(val.py());
            if !val.is_instance(enum_type.as_any())? {
                return Err(PyTypeError::new_err("Enum value type mismatch"));
            }
            let value = val.getattr("value")?;
            serialize_impl(writer, tag, inner, &value, depth + 1)?;
        }
        TypeExpr::Union(variants, cache) => {
            let variant = select_union_variant(val.py(), variants, cache, val)?;
            serialize_impl(writer, tag, variant, val, depth + 1)?;
        }
        TypeExpr::Struct(cls_obj) => {
            let cls = class_from_type(val.py(), cls_obj);
            let def = ensure_schema_for_class(val.py(), &cls)?;
            writer.write_tag(tag, TarsType::StructBegin);
            serialize_struct_fields(writer, val, &def, depth + 1, true, &serialize_impl_standard)?;
            writer.write_tag(0, TarsType::StructEnd);
        }
        TypeExpr::TarsDict => {
            if !val.is_instance_of::<TarsDict>() {
                return Err(PyTypeError::new_err("TarsDict value type mismatch"));
            }
            let dict = val.cast::<PyDict>()?;
            writer.write_tag(tag, TarsType::StructBegin);
            write_tarsdict_fields(writer, dict, depth + 1, &serialize_impl_standard)?;
            writer.write_tag(0, TarsType::StructEnd);
        }
        TypeExpr::NamedTuple(cls, items) => {
            if !val.is_instance(cls.bind(val.py()).as_any())? {
                return Err(PyTypeError::new_err("NamedTuple value type mismatch"));
            }
            serialize_tuple_like(writer, tag, items, val, depth + 1)?;
        }
        TypeExpr::Dataclass(cls) => {
            if !val.is_instance(cls.bind(val.py()).as_any())? {
                return Err(PyTypeError::new_err("Dataclass value type mismatch"));
            }
            writer.write_tag(tag, TarsType::Map);
            let fields = dataclass_fields(val)?.ok_or_else(|| {
                PyTypeError::new_err("Dataclass value missing __dataclass_fields__")
            })?;
            let len = fields.len();
            writer.write_int(0, len as i64);
            for (name_any, _field) in fields {
                let value = val.getattr(name_any.cast::<PyString>()?)?;
                serialize_impl(
                    writer,
                    0,
                    &TypeExpr::Primitive(WireType::String),
                    &name_any,
                    depth + 1,
                )?;
                serialize_impl(writer, 1, &TypeExpr::Any, &value, depth + 1)?;
            }
        }
        TypeExpr::List(inner) | TypeExpr::VarTuple(inner) => {
            if matches!(**inner, TypeExpr::Primitive(WireType::Int))
                && let Some(bytes) = try_coerce_buffer_to_bytes(val)?
            {
                writer.write_bytes(tag, bytes.as_bytes());
                return Ok(());
            }

            writer.write_tag(tag, TarsType::List);
            if let Some(is_list) = check_exact_sequence_type(val) {
                let seq_fast = PySequenceFast::new_exact(val, is_list)?;
                let len = seq_fast.len();
                writer.write_int(0, len as i64);
                for i in 0..len {
                    let item = seq_fast.get_item(val.py(), i)?;
                    serialize_impl(writer, 0, inner, &item, depth + 1)?;
                }
            } else {
                let seq = val.extract::<Bound<'_, PySequence>>()?;
                let len = seq.len()?;
                writer.write_int(0, len as i64);
                for i in 0..len {
                    let item = seq.get_item(i)?;
                    serialize_impl(writer, 0, inner, &item, depth + 1)?;
                }
            }
        }
        TypeExpr::Tuple(items) => {
            serialize_tuple_like(writer, tag, items, val, depth + 1)?;
        }
        TypeExpr::Set(inner) => {
            writer.write_tag(tag, TarsType::List);
            if val.is_instance_of::<PySet>() {
                let set = val.cast::<PySet>()?;
                let len = set.len() as i64;
                writer.write_int(0, len);
                for item in set.iter() {
                    serialize_impl(writer, 0, inner, &item, depth + 1)?;
                }
                return Ok(());
            }
            if val.is_instance_of::<PyFrozenSet>() {
                let set = val.cast::<PyFrozenSet>()?;
                let len = set.len() as i64;
                writer.write_int(0, len);
                for item in set.iter() {
                    serialize_impl(writer, 0, inner, &item, depth + 1)?;
                }
                return Ok(());
            }
            return Err(PyTypeError::new_err("Set value must be set or frozenset"));
        }
        TypeExpr::Map(k_type, v_type) => {
            writer.write_tag(tag, TarsType::Map);
            if let Ok(dict) = val.extract::<Bound<'_, PyDict>>() {
                let len = dict.len();
                writer.write_int(0, len as i64);

                for (k, v) in dict {
                    serialize_impl(writer, 0, k_type, &k, depth + 1)?;
                    serialize_impl(writer, 1, v_type, &v, depth + 1)?;
                }
            } else if let Some(fields) = dataclass_fields(val)? {
                let len = fields.len();
                writer.write_int(0, len as i64);
                for (name_any, _field) in fields {
                    let value = val.getattr(name_any.cast::<PyString>()?)?;
                    serialize_impl(writer, 0, k_type, &name_any, depth + 1)?;
                    serialize_impl(writer, 1, v_type, &value, depth + 1)?;
                }
            } else {
                return Err(PyTypeError::new_err(
                    "Map value must be dict or dataclass instance",
                ));
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

pub(crate) fn serialize_impl_standard(
    writer: &mut TarsWriter<impl BufMut>,
    tag: u8,
    type_expr: &TypeExpr,
    val: &Bound<'_, PyAny>,
    depth: usize,
) -> PyResult<()> {
    serialize_impl(writer, tag, type_expr, val, depth)
}

/// 从 Union 变体列表中选择与给定值匹配的变体.
///
/// 优先查询 `UnionCache` 以实现 O(1) 快速分发. 若未命中, 则回退到 O(N) 线性扫描,
/// 并将成功匹配的类型记录到缓存中. 依据“同类型视为同一分发目标”的原则进行加速.
fn select_union_variant<'py>(
    py: Python<'py>,
    variants: &'py [TypeExpr],
    cache: &UnionCache,
    value: &Bound<'py, PyAny>,
) -> PyResult<&'py TypeExpr> {
    if value.is_none() {
        for (idx, variant) in variants.iter().enumerate() {
            if matches!(variant, TypeExpr::Optional(_) | TypeExpr::NoneType) {
                let _ = idx;
                return Ok(variant);
            }
        }
        return Err(PyTypeError::new_err("Union does not accept None"));
    }

    // 1. O(1) Lookup: 检查缓存中是否存在精确类型匹配
    let type_ptr = value.get_type().as_ptr() as usize;

    if let Some(idx) = cache.get(type_ptr) {
        // 基于“同 Python 类型视为同一分发目标”的原则，直接使用缓存索引
        if idx < variants.len() {
            return Ok(&variants[idx]);
        }
    }

    // 2. O(N) Scan: 缓存未命中时回退到线性扫描
    for (idx, variant) in variants.iter().enumerate() {
        if value_matches_type(py, variant, value)? {
            // 记录匹配成功的索引，以便后续相同类型的值实现 O(1) 分发
            cache.insert(type_ptr, idx);
            return Ok(variant);
        }
    }
    Err(PyTypeError::new_err(
        "Value does not match any union variant",
    ))
}
