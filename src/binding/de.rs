use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PySet, PyType};
use simdutf8::basic::from_utf8;
use std::cmp::Ordering;

use crate::ValidationError;
use crate::binding::any_codec::{
    decode_any_struct_fields, decode_any_value, read_size_non_negative,
};
use crate::binding::error::{DeError, DeResult, PathItem};
use crate::binding::raw::decode_raw;
use crate::binding::schema::{
    Constraints, StructDef, TarsDict, TypeExpr, WireType, ensure_schema_for_class,
};
use crate::codec::consts::TarsType;
use crate::codec::reader::TarsReader;

const MAX_DEPTH: usize = 100;

#[inline]
fn check_depth(depth: usize) -> DeResult<()> {
    if depth > MAX_DEPTH {
        return Err(DeError::new(
            "Recursion limit exceeded during deserialization".into(),
        ));
    }
    Ok(())
}

// read_size_non_negative 由 any_codec 提供

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
    if cls.is_subclass_of::<TarsDict>()? {
        let dict = decode_raw(py, data)?;
        if cls.is(dict.get_type().as_any()) {
            return Ok(dict.into_any());
        }
        let instance = cls.call1((dict,))?;
        return Ok(instance);
    }
    // 校验 schema 是否存在并获取
    let def = ensure_schema_for_class(py, cls)?;

    let mut reader = TarsReader::new(data);
    deserialize_struct(py, &mut reader, &def, 0).map_err(|e| e.to_pyerr(py))
}

/// 从读取器中反序列化结构体.
fn deserialize_struct<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    def: &StructDef,
    depth: usize,
) -> DeResult<Bound<'py, PyAny>> {
    check_depth(depth)?;

    let class_obj = def.bind_class(py);
    let mut seen = vec![false; def.fields_sorted.len()];
    let mut values: Vec<Option<Bound<'py, PyAny>>> = vec![None; def.fields_sorted.len()];

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
                .map_err(|e| DeError::new(format!("Read head error: {}", e)))?; // Consume StructEnd
            break;
        }

        reader
            .read_head()
            .map_err(|e| DeError::new(format!("Read head error: {}", e)))?; // Consume the head

        let idx_opt = if (tag as usize) < def.tag_lookup_vec.len() {
            def.tag_lookup_vec[tag as usize]
        } else {
            None
        };

        if let Some(idx) = idx_opt {
            let field = &def.fields_sorted[idx];
            let value = deserialize_value(py, reader, type_id, &field.ty, depth + 1)
                .map_err(|e| e.prepend(PathItem::Field(field.name.clone())))?;
            if let Some(constraints) = field.constraints.as_deref() {
                validate_value(field.name.as_str(), &value, constraints)
                    .map_err(|e| e.prepend(PathItem::Field(field.name.clone())))?;
            }
            seen[idx] = true;
            values[idx] = Some(value);
        } else {
            // 未知 tag,跳过
            if def.forbid_unknown_tags {
                return Err(DeError::new(format!(
                    "Unknown tag {} found in deserialization (forbid_unknown_tags=True)",
                    tag
                )));
            }
            reader
                .skip_field(type_id)
                .map_err(|e| DeError::new(format!("Failed to skip unknown field: {}", e)))?;
        }
    }

    for (idx, field) in def.fields_sorted.iter().enumerate() {
        if seen[idx] {
            continue;
        }
        let value = if let Some(default_value) = field.default_value.as_ref() {
            Some(default_value.bind(py).clone())
        } else if let Some(factory) = field.default_factory.as_ref() {
            let produced = factory.bind(py).call0().map_err(DeError::wrap)?;
            Some(produced)
        } else if field.is_optional {
            Some(py.None().into_bound(py))
        } else if field.is_required {
            return Err(DeError::new(format!(
                "Missing required field '{}' in deserialization",
                field.name
            )));
        } else {
            None
        };
        values[idx] = value;
    }

    let instance = unsafe {
        // SAFETY: class_obj 由 Schema 持有,生命周期覆盖本次反序列化。
        // 这里使用 PyType_GenericAlloc 创建未初始化对象,后续逐字段写入。
        let type_ptr = class_obj.as_ptr() as *mut ffi::PyTypeObject;
        let obj_ptr = ffi::PyType_GenericAlloc(type_ptr, 0);
        if obj_ptr.is_null() {
            return Err(DeError::wrap(PyErr::fetch(py)));
        }
        Bound::from_owned_ptr(py, obj_ptr)
    };
    for (idx, field) in def.fields_sorted.iter().enumerate() {
        if let Some(val) = values[idx].as_ref() {
            unsafe {
                // SAFETY: name_py/val 均由 PyO3 管理引用计数。
                // 若设置属性失败,显式 drop 以确保引用及时释放,避免半初始化对象泄漏。
                let name_py = field.name_py.bind(py);
                let res =
                    ffi::PyObject_GenericSetAttr(instance.as_ptr(), name_py.as_ptr(), val.as_ptr());
                if res != 0 {
                    let err = PyErr::fetch(py);
                    drop(instance);
                    return Err(DeError::wrap(err));
                }
            }
        }
    }
    Ok(instance)
}

fn validate_value<'py>(
    field_name: &str,
    value: &Bound<'py, PyAny>,
    constraints: &Constraints,
) -> DeResult<()> {
    if constraints.gt.is_some()
        || constraints.ge.is_some()
        || constraints.lt.is_some()
        || constraints.le.is_some()
    {
        let v: f64 = value.extract().map_err(|_| {
            DeError::wrap(ValidationError::new_err(format!(
                "Field '{}' must be a number to apply numeric constraints",
                field_name
            )))
        })?;

        if let Some(gt) = constraints.gt
            && v.partial_cmp(&gt) != Some(Ordering::Greater)
        {
            return Err(DeError::wrap(ValidationError::new_err(format!(
                "Field '{}' must be > {}",
                field_name, gt
            ))));
        }
        if let Some(ge) = constraints.ge
            && matches!(v.partial_cmp(&ge), Some(Ordering::Less) | None)
        {
            return Err(DeError::wrap(ValidationError::new_err(format!(
                "Field '{}' must be >= {}",
                field_name, ge
            ))));
        }
        if let Some(lt) = constraints.lt
            && v.partial_cmp(&lt) != Some(Ordering::Less)
        {
            return Err(DeError::wrap(ValidationError::new_err(format!(
                "Field '{}' must be < {}",
                field_name, lt
            ))));
        }
        if let Some(le) = constraints.le
            && matches!(v.partial_cmp(&le), Some(Ordering::Greater) | None)
        {
            return Err(DeError::wrap(ValidationError::new_err(format!(
                "Field '{}' must be <= {}",
                field_name, le
            ))));
        }
    }

    if constraints.min_len.is_some() || constraints.max_len.is_some() {
        let len = value.len().map_err(|_| {
            DeError::wrap(ValidationError::new_err(format!(
                "Field '{}' must have length to apply length constraints",
                field_name
            )))
        })?;

        if let Some(min_len) = constraints.min_len
            && len < min_len
        {
            return Err(DeError::wrap(ValidationError::new_err(format!(
                "Field '{}' length must be >= {}",
                field_name, min_len
            ))));
        }
        if let Some(max_len) = constraints.max_len
            && len > max_len
        {
            return Err(DeError::wrap(ValidationError::new_err(format!(
                "Field '{}' length must be <= {}",
                field_name, max_len
            ))));
        }
    }

    if let Some(pattern) = constraints.pattern.as_ref() {
        let s: &str = value.extract().map_err(|_| {
            DeError::wrap(ValidationError::new_err(format!(
                "Field '{}' must be a string to apply pattern constraint",
                field_name
            )))
        })?;
        if !pattern.is_match(s) {
            return Err(DeError::wrap(ValidationError::new_err(format!(
                "Field '{}' does not match pattern",
                field_name
            ))));
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
) -> DeResult<Bound<'py, PyAny>> {
    check_depth(depth)?;

    match type_expr {
        TypeExpr::Primitive(wire_type) => match wire_type {
            WireType::Int | WireType::Long => {
                let v = reader
                    .read_int(type_id)
                    .map_err(|e| DeError::new(format!("Failed to read int: {}", e)))?;
                Ok(v.into_pyobject(py)
                    .map_err(|e| DeError::new(e.to_string()))?
                    .into_any())
            }
            WireType::Bool => {
                let v = reader
                    .read_int(type_id)
                    .map_err(|e| DeError::new(format!("Failed to read int: {}", e)))?;
                let b = v != 0;
                let obj = b
                    .into_pyobject(py)
                    .map_err(|e| DeError::new(e.to_string()))?
                    .to_owned();
                Ok(obj.into_any())
            }
            WireType::Float => {
                let v = reader
                    .read_float(type_id)
                    .map_err(|e| DeError::new(format!("Failed to read float: {}", e)))?;
                Ok(v.into_pyobject(py)
                    .map_err(|e| DeError::new(e.to_string()))?
                    .into_any())
            }
            WireType::Double => {
                let v = reader
                    .read_double(type_id)
                    .map_err(|e| DeError::new(format!("Failed to read double: {}", e)))?;
                Ok(v.into_pyobject(py)
                    .map_err(|e| DeError::new(e.to_string()))?
                    .into_any())
            }
            WireType::String => {
                let bytes = reader
                    .read_string(type_id)
                    .map_err(|e| DeError::new(format!("Failed to read string bytes: {}", e)))?;
                let s =
                    from_utf8(bytes).map_err(|_| DeError::new("Invalid UTF-8 string".into()))?;
                Ok(s.into_pyobject(py)
                    .map_err(|e| DeError::new(e.to_string()))?
                    .into_any())
            }
            _ => Err(DeError::new("Unexpected wire type for primitive".into())),
        },
        TypeExpr::Any => decode_any_value(py, reader, type_id, depth),
        TypeExpr::NoneType => Ok(py.None().into_bound(py)),
        TypeExpr::Enum(enum_cls, inner) => {
            let value = deserialize_value(py, reader, type_id, inner, depth + 1)?;
            let cls = enum_cls.bind(py);
            let enum_value = cls.call1((value,)).map_err(DeError::wrap)?;
            Ok(enum_value)
        }
        TypeExpr::Set(inner) => {
            if type_id != TarsType::List {
                return Err(DeError::new("Set value must be encoded as List".into()));
            }
            let len = read_size_non_negative(reader, "list")?;
            let set = PySet::empty(py).map_err(DeError::wrap)?;
            for _ in 0..len {
                let (_, item_type) = reader
                    .read_head()
                    .map_err(|e| DeError::new(format!("Failed to read list item head: {}", e)))?;
                let item = deserialize_value(py, reader, item_type, inner, depth + 1)?;
                set.add(item).map_err(DeError::wrap)?;
            }
            Ok(set.into_any())
        }
        TypeExpr::Union(variants) => decode_union_value(py, reader, type_id, variants, depth),
        TypeExpr::Struct(ptr) => {
            let obj_ptr = *ptr as *mut ffi::PyObject;
            // SAFETY: ptr 指向 Schema 内部持有的 PyType,生命周期受 Py<PyType> 保障。
            let nested_any = unsafe { Bound::from_borrowed_ptr(py, obj_ptr) };
            let nested_cls = nested_any
                .cast::<PyType>()
                .map_err(|e| DeError::new(e.to_string()))?;
            let nested_def = ensure_schema_for_class(py, nested_cls).map_err(DeError::wrap)?;
            deserialize_struct(py, reader, &nested_def, depth + 1)
        }
        TypeExpr::TarsDict => {
            if type_id != TarsType::StructBegin {
                return Err(DeError::new(
                    "TarsDict value must be encoded as Struct".into(),
                ));
            }
            let dict = decode_any_struct_fields(py, reader, depth + 1)?;
            let tarsdict_type = py.get_type::<TarsDict>();
            let instance = tarsdict_type.call1((dict,)).map_err(DeError::wrap)?;
            Ok(instance.into_any())
        }
        TypeExpr::NamedTuple(cls, items) => {
            if type_id != TarsType::List {
                return Err(DeError::new(
                    "NamedTuple value must be encoded as List".into(),
                ));
            }
            let len = read_size_non_negative(reader, "list")?;
            if len != items.len() {
                return Err(DeError::new(
                    "Tuple length does not match annotation".into(),
                ));
            }
            let list_any = unsafe {
                let ptr = ffi::PyList_New(len as isize);
                if ptr.is_null() {
                    return Err(DeError::wrap(PyErr::fetch(py)));
                }
                Bound::from_owned_ptr(py, ptr)
            };
            for (idx, item_type) in items.iter().enumerate() {
                let (_, item_type_id) = reader
                    .read_head()
                    .map_err(|e| DeError::new(format!("Failed to read list item head: {}", e)))?;
                let item = deserialize_value(py, reader, item_type_id, item_type, depth + 1)
                    .map_err(|e| e.prepend(PathItem::Index(idx)))?;
                let set_res = unsafe {
                    ffi::PyList_SetItem(list_any.as_ptr(), idx as isize, item.into_ptr())
                };
                if set_res != 0 {
                    return Err(DeError::wrap(PyErr::fetch(py)));
                }
            }
            let list = list_any
                .cast::<PyList>()
                .map_err(|e| DeError::new(e.to_string()))?;
            let tuple = list.to_tuple();
            let instance = cls.bind(py).call1(tuple).map_err(DeError::wrap)?;
            Ok(instance.into_any())
        }
        TypeExpr::Dataclass(cls) => {
            if type_id != TarsType::Map {
                return Err(DeError::new(
                    "Dataclass value must be encoded as Map".into(),
                ));
            }
            let len = read_size_non_negative(reader, "map")?;
            let dict = PyDict::new(py);
            for _ in 0..len {
                let (_, kt) = reader
                    .read_head()
                    .map_err(|e| DeError::new(format!("Failed to read map key head: {}", e)))?;
                let key = deserialize_value(
                    py,
                    reader,
                    kt,
                    &TypeExpr::Primitive(WireType::String),
                    depth + 1,
                )?;

                let (_, vt) = reader
                    .read_head()
                    .map_err(|e| DeError::new(format!("Failed to read map value head: {}", e)))?;

                let key_str = key.to_string();

                let val = deserialize_value(py, reader, vt, &TypeExpr::Any, depth + 1)
                    .map_err(|e| e.prepend(PathItem::Key(key_str)))?;

                dict.set_item(key, val).map_err(DeError::wrap)?;
            }
            let instance = cls.bind(py).call((), Some(&dict)).map_err(DeError::wrap)?;
            Ok(instance.into_any())
        }
        TypeExpr::List(inner) | TypeExpr::VarTuple(inner) => {
            if type_id == TarsType::SimpleList {
                let sub_type = reader.read_u8().map_err(|e| {
                    DeError::new(format!("Failed to read SimpleList subtype: {}", e))
                })?;
                if sub_type != 0 {
                    return Err(DeError::new("SimpleList must contain Byte (0)".into()));
                }
                let len = read_size_non_negative(reader, "SimpleList")?;
                let bytes = reader
                    .read_bytes(len)
                    .map_err(|e| DeError::new(format!("Failed to read SimpleList bytes: {}", e)))?;
                return Ok(PyBytes::new(py, bytes).into_any());
            }

            let len = read_size_non_negative(reader, "list")?;

            let list_any = unsafe {
                // SAFETY: PyList_New 返回新引用并预留 len 个槽位。若返回空指针则抛错。
                let ptr = ffi::PyList_New(len as isize);
                if ptr.is_null() {
                    return Err(DeError::wrap(PyErr::fetch(py)));
                }
                Bound::from_owned_ptr(py, ptr)
            };
            for idx in 0..len {
                let (_, item_type) = reader
                    .read_head()
                    .map_err(|e| DeError::new(format!("Failed to read list item head: {}", e)))?;
                let item = deserialize_value(py, reader, item_type, inner, depth + 1)
                    .map_err(|e| e.prepend(PathItem::Index(idx)))?;
                let set_res = unsafe {
                    // SAFETY: PyList_SetItem 会“偷”引用, item.into_ptr 转移所有权。
                    // 每个索引只写入一次,与 PyList_New 的预分配长度一致。
                    ffi::PyList_SetItem(list_any.as_ptr(), idx as isize, item.into_ptr())
                };
                if set_res != 0 {
                    return Err(DeError::wrap(PyErr::fetch(py)));
                }
            }

            if matches!(type_expr, TypeExpr::VarTuple(_)) {
                let list_any_clone = list_any.clone();
                let list = list_any_clone
                    .cast::<PyList>()
                    .map_err(|e| DeError::new(e.to_string()))?;
                Ok(list.to_tuple().into_any())
            } else {
                Ok(list_any)
            }
        }
        TypeExpr::Tuple(items) => {
            if type_id != TarsType::List {
                return Err(DeError::new("Tuple value must be encoded as List".into()));
            }
            let len = read_size_non_negative(reader, "list")?;
            if len != items.len() {
                return Err(DeError::new(
                    "Tuple length does not match annotation".into(),
                ));
            }
            let list_any = unsafe {
                let ptr = ffi::PyList_New(len as isize);
                if ptr.is_null() {
                    return Err(DeError::wrap(PyErr::fetch(py)));
                }
                Bound::from_owned_ptr(py, ptr)
            };
            for (idx, item_type) in items.iter().enumerate() {
                let (_, item_type_id) = reader
                    .read_head()
                    .map_err(|e| DeError::new(format!("Failed to read list item head: {}", e)))?;
                let item = deserialize_value(py, reader, item_type_id, item_type, depth + 1)
                    .map_err(|e| e.prepend(PathItem::Index(idx)))?;
                let set_res = unsafe {
                    ffi::PyList_SetItem(list_any.as_ptr(), idx as isize, item.into_ptr())
                };
                if set_res != 0 {
                    return Err(DeError::wrap(PyErr::fetch(py)));
                }
            }
            let list = list_any
                .cast::<PyList>()
                .map_err(|e| DeError::new(e.to_string()))?;
            Ok(list.to_tuple().into_any())
        }
        TypeExpr::Map(k_type, v_type) => {
            let len = read_size_non_negative(reader, "map")?;

            let dict = PyDict::new(py);
            for _ in 0..len {
                let (_, kt) = reader
                    .read_head()
                    .map_err(|e| DeError::new(format!("Failed to read map key head: {}", e)))?;
                let key = deserialize_value(py, reader, kt, k_type, depth + 1)
                    .map_err(|e| e.prepend(PathItem::Key("<key>".into())))?;

                let (_, vt) = reader
                    .read_head()
                    .map_err(|e| DeError::new(format!("Failed to read map value head: {}", e)))?;

                let key_str = key.to_string();

                let val = deserialize_value(py, reader, vt, v_type, depth + 1)
                    .map_err(|e| e.prepend(PathItem::Key(key_str)))?;

                dict.set_item(key, val).map_err(DeError::wrap)?;
            }
            Ok(dict.into_any())
        }
        TypeExpr::Optional(inner) => deserialize_value(py, reader, type_id, inner, depth),
    }
}

fn decode_union_value<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    type_id: TarsType,
    variants: &[TypeExpr],
    depth: usize,
) -> DeResult<Bound<'py, PyAny>> {
    for variant in variants {
        if union_variant_matches_type_id(variant, type_id) {
            return deserialize_value(py, reader, type_id, variant, depth + 1);
        }
    }
    Err(DeError::new(
        "Union value does not match any variant".into(),
    ))
}

fn union_variant_matches_type_id(variant: &TypeExpr, type_id: TarsType) -> bool {
    match variant {
        TypeExpr::Any => true,
        TypeExpr::NoneType => false,
        TypeExpr::Primitive(wire_type) => match wire_type {
            WireType::Int | WireType::Long => matches!(
                type_id,
                TarsType::ZeroTag
                    | TarsType::Int1
                    | TarsType::Int2
                    | TarsType::Int4
                    | TarsType::Int8
            ),
            WireType::Bool => matches!(
                type_id,
                TarsType::ZeroTag
                    | TarsType::Int1
                    | TarsType::Int2
                    | TarsType::Int4
                    | TarsType::Int8
            ),
            WireType::Float => matches!(type_id, TarsType::ZeroTag | TarsType::Float),
            WireType::Double => matches!(
                type_id,
                TarsType::ZeroTag | TarsType::Float | TarsType::Double
            ),
            WireType::String => matches!(type_id, TarsType::String1 | TarsType::String4),
            _ => false,
        },
        TypeExpr::Enum(_, inner) => union_variant_matches_type_id(inner, type_id),
        TypeExpr::Union(items) => items
            .iter()
            .any(|item| union_variant_matches_type_id(item, type_id)),
        TypeExpr::Struct(_) => type_id == TarsType::StructBegin,
        TypeExpr::TarsDict => type_id == TarsType::StructBegin,
        TypeExpr::NamedTuple(_, _) => matches!(type_id, TarsType::List | TarsType::SimpleList),
        TypeExpr::Dataclass(_) => type_id == TarsType::Map,
        TypeExpr::List(_) | TypeExpr::VarTuple(_) | TypeExpr::Tuple(_) => {
            matches!(type_id, TarsType::List | TarsType::SimpleList)
        }
        TypeExpr::Set(_) => type_id == TarsType::List,
        TypeExpr::Map(_, _) => type_id == TarsType::Map,
        TypeExpr::Optional(inner) => union_variant_matches_type_id(inner, type_id),
    }
}
