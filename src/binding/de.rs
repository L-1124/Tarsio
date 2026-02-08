use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PySet, PyType};
use simdutf8::basic::from_utf8;
use std::cmp::Ordering;

use crate::ValidationError;
use crate::binding::introspect::StructKind;
use crate::binding::schema::{Constraints, StructDef, TypeExpr, WireType, ensure_schema_for_class};
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
    let def = ensure_schema_for_class(py, cls)?;

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
            seen[idx] = true;
            values[idx] = Some(value);
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

    for (idx, field) in def.fields_sorted.iter().enumerate() {
        if seen[idx] {
            continue;
        }
        let value = if let Some(default_value) = field.default_value.as_ref() {
            Some(default_value.bind(py).clone())
        } else if let Some(factory) = field.default_factory.as_ref() {
            let produced = factory.bind(py).call0()?;
            Some(produced)
        } else if field.is_optional {
            Some(py.None().into_bound(py))
        } else if field.is_required {
            return Err(PyValueError::new_err(format!(
                "Missing required field '{}' in deserialization",
                field.name
            )));
        } else {
            None
        };
        values[idx] = value;
    }

    match def.kind {
        StructKind::TarsStruct => {
            let instance = unsafe {
                // SAFETY: class_obj 由 Schema 持有,生命周期覆盖本次反序列化。
                // 这里使用 PyType_GenericAlloc 创建未初始化对象,后续逐字段写入。
                let type_ptr = class_obj.as_ptr() as *mut ffi::PyTypeObject;
                let obj_ptr = ffi::PyType_GenericAlloc(type_ptr, 0);
                if obj_ptr.is_null() {
                    return Err(PyErr::fetch(py));
                }
                Bound::from_owned_ptr(py, obj_ptr)
            };
            for (idx, field) in def.fields_sorted.iter().enumerate() {
                if let Some(val) = values[idx].as_ref() {
                    unsafe {
                        // SAFETY: name_py/val 均由 PyO3 管理引用计数。
                        // 若设置属性失败,显式 drop 以确保引用及时释放,避免半初始化对象泄漏。
                        let name_py = field.name_py.bind(py);
                        let res = ffi::PyObject_GenericSetAttr(
                            instance.as_ptr(),
                            name_py.as_ptr(),
                            val.as_ptr(),
                        );
                        if res != 0 {
                            let err = PyErr::fetch(py);
                            drop(instance);
                            return Err(err);
                        }
                    }
                }
            }
            Ok(instance)
        }
        StructKind::TypedDict => {
            let dict = PyDict::new(py);
            for (idx, field) in def.fields_sorted.iter().enumerate() {
                if let Some(val) = values[idx].as_ref() {
                    dict.set_item(field.name_py.bind(py), val)?;
                }
            }
            Ok(dict.into_any())
        }
        StructKind::TarsDict => {
            let dict = PyDict::new(py);
            for (idx, field) in def.fields_sorted.iter().enumerate() {
                if let Some(val) = values[idx].as_ref() {
                    dict.set_item(field.name_py.bind(py), val)?;
                }
            }
            let instance = class_obj.call1((dict,))?;
            Ok(instance)
        }
        StructKind::NamedTuple | StructKind::Dataclass => {
            let kwargs = PyDict::new(py);
            for (idx, field) in def.fields_sorted.iter().enumerate() {
                if def.kind == StructKind::Dataclass && !field.init {
                    continue;
                }
                if let Some(val) = values[idx].as_ref() {
                    kwargs.set_item(field.name_py.bind(py), val)?;
                }
            }
            let instance = class_obj.call((), Some(&kwargs))?;
            if def.kind == StructKind::Dataclass {
                let builtins = py.import("builtins")?;
                let object = builtins.getattr("object")?;
                for (idx, field) in def.fields_sorted.iter().enumerate() {
                    if field.init {
                        continue;
                    }
                    if let Some(val) = values[idx].as_ref() {
                        object.call_method1(
                            "__setattr__",
                            (instance.clone(), field.name_py.bind(py), val),
                        )?;
                    }
                }
            }
            Ok(instance)
        }
    }
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
            WireType::Bool => {
                let v = reader
                    .read_int(type_id)
                    .map_err(|e| PyValueError::new_err(format!("Failed to read int: {}", e)))?;
                let b = v != 0;
                let obj = b.into_pyobject(py)?.to_owned();
                Ok(obj.into_any())
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
        TypeExpr::Any => decode_any_value(py, reader, type_id, depth),
        TypeExpr::NoneType => Ok(py.None().into_bound(py)),
        TypeExpr::Enum(enum_cls, inner) => {
            let value = deserialize_value(py, reader, type_id, inner, depth + 1)?;
            let cls = enum_cls.bind(py);
            let enum_value = cls.call1((value,))?;
            Ok(enum_value)
        }
        TypeExpr::Set(inner) => {
            if type_id != TarsType::List {
                return Err(PyValueError::new_err("Set value must be encoded as List"));
            }
            let len = reader
                .read_size()
                .map_err(|e| PyValueError::new_err(format!("Failed to read list size: {}", e)))?;
            if len < 0 {
                return Err(PyValueError::new_err("Invalid list size"));
            }
            let len = len as usize;
            let set = PySet::empty(py)?;
            for _ in 0..len {
                let (_, item_type) = reader.read_head().map_err(|e| {
                    PyValueError::new_err(format!("Failed to read list item head: {}", e))
                })?;
                let item = deserialize_value(py, reader, item_type, inner, depth + 1)?;
                set.add(item)?;
            }
            Ok(set.into_any())
        }
        TypeExpr::Union(variants) => decode_union_value(py, reader, type_id, variants, depth),
        TypeExpr::Struct(ptr) => {
            let obj_ptr = *ptr as *mut ffi::PyObject;
            // SAFETY: ptr 指向 Schema 内部持有的 PyType,生命周期受 Py<PyType> 保障。
            let nested_any = unsafe { Bound::from_borrowed_ptr(py, obj_ptr) };
            let nested_cls = nested_any.cast::<PyType>()?;
            let nested_def = ensure_schema_for_class(py, nested_cls)?;
            deserialize_struct(py, reader, &nested_def, depth + 1)
        }
        TypeExpr::List(inner) | TypeExpr::VarTuple(inner) => {
            if type_id == TarsType::SimpleList {
                let sub_type = reader.read_u8().map_err(|e| {
                    PyValueError::new_err(format!("Failed to read SimpleList subtype: {}", e))
                })?;
                if sub_type != 0 {
                    return Err(PyValueError::new_err("SimpleList must contain Byte (0)"));
                }
                let len = reader.read_size().map_err(|e| {
                    PyValueError::new_err(format!("Failed to read SimpleList size: {}", e))
                })?;
                if len < 0 {
                    return Err(PyValueError::new_err("Invalid SimpleList size"));
                }
                let len = len as usize;
                let bytes = reader.read_bytes(len).map_err(|e| {
                    PyValueError::new_err(format!("Failed to read SimpleList bytes: {}", e))
                })?;
                return Ok(PyBytes::new(py, bytes).into_any());
            }

            let len = reader
                .read_size()
                .map_err(|e| PyValueError::new_err(format!("Failed to read list size: {}", e)))?;
            if len < 0 {
                return Err(PyValueError::new_err("Invalid list size"));
            }
            let len = len as usize;

            let list_any = unsafe {
                // SAFETY: PyList_New 返回新引用并预留 len 个槽位。若返回空指针则抛错。
                let ptr = ffi::PyList_New(len as isize);
                if ptr.is_null() {
                    return Err(PyErr::fetch(py));
                }
                Bound::from_owned_ptr(py, ptr)
            };
            for idx in 0..len {
                let (_, item_type) = reader.read_head().map_err(|e| {
                    PyValueError::new_err(format!("Failed to read list item head: {}", e))
                })?;
                let item = deserialize_value(py, reader, item_type, inner, depth + 1)?;
                let set_res = unsafe {
                    // SAFETY: PyList_SetItem 会“偷”引用, item.into_ptr 转移所有权。
                    // 每个索引只写入一次,与 PyList_New 的预分配长度一致。
                    ffi::PyList_SetItem(list_any.as_ptr(), idx as isize, item.into_ptr())
                };
                if set_res != 0 {
                    return Err(PyErr::fetch(py));
                }
            }

            if matches!(type_expr, TypeExpr::VarTuple(_)) {
                let list_any_clone = list_any.clone();
                let list = list_any_clone.cast::<PyList>()?;
                Ok(list.to_tuple().into_any())
            } else {
                Ok(list_any)
            }
        }
        TypeExpr::Tuple(items) => {
            if type_id != TarsType::List {
                return Err(PyValueError::new_err("Tuple value must be encoded as List"));
            }
            let len = reader
                .read_size()
                .map_err(|e| PyValueError::new_err(format!("Failed to read list size: {}", e)))?;
            if len < 0 {
                return Err(PyValueError::new_err("Invalid list size"));
            }
            let len = len as usize;
            if len != items.len() {
                return Err(PyValueError::new_err(
                    "Tuple length does not match annotation",
                ));
            }
            let list_any = unsafe {
                let ptr = ffi::PyList_New(len as isize);
                if ptr.is_null() {
                    return Err(PyErr::fetch(py));
                }
                Bound::from_owned_ptr(py, ptr)
            };
            for (idx, item_type) in items.iter().enumerate() {
                let (_, item_type_id) = reader.read_head().map_err(|e| {
                    PyValueError::new_err(format!("Failed to read list item head: {}", e))
                })?;
                let item = deserialize_value(py, reader, item_type_id, item_type, depth + 1)?;
                let set_res = unsafe {
                    ffi::PyList_SetItem(list_any.as_ptr(), idx as isize, item.into_ptr())
                };
                if set_res != 0 {
                    return Err(PyErr::fetch(py));
                }
            }
            let list = list_any.cast::<PyList>()?;
            Ok(list.to_tuple().into_any())
        }
        TypeExpr::Map(k_type, v_type) => {
            let len = reader
                .read_size()
                .map_err(|e| PyValueError::new_err(format!("Failed to read map size: {}", e)))?;
            if len < 0 {
                return Err(PyValueError::new_err("Invalid map size"));
            }
            let len = len as usize;

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

fn decode_union_value<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    type_id: TarsType,
    variants: &[TypeExpr],
    depth: usize,
) -> PyResult<Bound<'py, PyAny>> {
    for variant in variants {
        if union_variant_matches_type_id(variant, type_id) {
            return deserialize_value(py, reader, type_id, variant, depth + 1);
        }
    }
    Err(PyValueError::new_err(
        "Union value does not match any variant",
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
        TypeExpr::List(_) | TypeExpr::VarTuple(_) | TypeExpr::Tuple(_) => {
            matches!(type_id, TarsType::List | TarsType::SimpleList)
        }
        TypeExpr::Set(_) => type_id == TarsType::List,
        TypeExpr::Map(_, _) => type_id == TarsType::Map,
        TypeExpr::Optional(inner) => union_variant_matches_type_id(inner, type_id),
    }
}

fn decode_any_value<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    type_id: TarsType,
    depth: usize,
) -> PyResult<Bound<'py, PyAny>> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded during deserialization",
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
            decode_any_struct_fields(py, reader, depth + 1).map(|d| d.into_any())
        }
        TarsType::List => decode_any_list(py, reader, depth + 1),
        TarsType::SimpleList => decode_any_simple_list(py, reader),
        TarsType::Map => decode_any_map(py, reader, depth + 1),
        TarsType::StructEnd => Err(PyValueError::new_err("Unexpected StructEnd")),
    }
}

fn decode_any_struct_fields<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    depth: usize,
) -> PyResult<Bound<'py, PyDict>> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded during deserialization",
        ));
    }
    let dict = PyDict::new(py);
    while !reader.is_end() {
        let (tag, type_id) = reader
            .read_head()
            .map_err(|e| PyValueError::new_err(format!("Read head error: {e}")))?;
        if type_id == TarsType::StructEnd {
            return Ok(dict);
        }
        if dict.contains(tag)? {
            return Err(PyValueError::new_err(format!(
                "Duplicate tag {tag} in struct"
            )));
        }
        let value = decode_any_value(py, reader, type_id, depth + 1)?;
        dict.set_item(tag, value)?;
    }
    Ok(dict)
}

fn decode_any_list<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    depth: usize,
) -> PyResult<Bound<'py, PyAny>> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded during deserialization",
        ));
    }
    let len = reader
        .read_size()
        .map_err(|e| PyValueError::new_err(format!("Failed to read list size: {e}")))?;
    if len < 0 {
        return Err(PyValueError::new_err("Invalid list size"));
    }
    let len = len as usize;
    let list_any = unsafe {
        // SAFETY: PyList_New 返回新引用并预留 len 个槽位。若返回空指针则抛错。
        let ptr = ffi::PyList_New(len as isize);
        if ptr.is_null() {
            return Err(PyErr::fetch(py));
        }
        Bound::from_owned_ptr(py, ptr)
    };
    for idx in 0..len {
        let (_, item_type) = reader
            .read_head()
            .map_err(|e| PyValueError::new_err(format!("Failed to read list item head: {e}")))?;
        let item = decode_any_value(py, reader, item_type, depth + 1)?;
        let set_res = unsafe {
            // SAFETY: PyList_SetItem 会“偷”引用, item.into_ptr 转移所有权。
            // 每个索引只写入一次,与 PyList_New 的预分配长度一致。
            ffi::PyList_SetItem(list_any.as_ptr(), idx as isize, item.into_ptr())
        };
        if set_res != 0 {
            return Err(PyErr::fetch(py));
        }
    }
    Ok(list_any)
}

fn decode_any_simple_list<'py>(
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
        .map_err(|e| PyValueError::new_err(format!("Failed to read SimpleList size: {e}")))?;
    if len < 0 {
        return Err(PyValueError::new_err("Invalid SimpleList size"));
    }
    let len = len as usize;
    let bytes = reader
        .read_bytes(len)
        .map_err(|e| PyValueError::new_err(format!("Failed to read SimpleList bytes: {e}")))?;
    if let Ok(s) = from_utf8(bytes) {
        return Ok(s.into_pyobject(py)?.into_any());
    }
    Ok(PyBytes::new(py, bytes).into_any())
}

fn decode_any_map<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    depth: usize,
) -> PyResult<Bound<'py, PyAny>> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded during deserialization",
        ));
    }
    let len = reader
        .read_size()
        .map_err(|e| PyValueError::new_err(format!("Failed to read map size: {e}")))?;
    if len < 0 {
        return Err(PyValueError::new_err("Invalid map size"));
    }
    let len = len as usize;
    let dict = PyDict::new(py);
    for _ in 0..len {
        let (_, kt) = reader
            .read_head()
            .map_err(|e| PyValueError::new_err(format!("Failed to read map key head: {e}")))?;
        let key = decode_any_value(py, reader, kt, depth + 1)?;

        let (_, vt) = reader
            .read_head()
            .map_err(|e| PyValueError::new_err(format!("Failed to read map value head: {e}")))?;
        let val = decode_any_value(py, reader, vt, depth + 1)?;

        if key.hash().is_err() {
            return Err(PyTypeError::new_err("Map key must be hashable"));
        }
        dict.set_item(key, val)?;
    }
    Ok(dict.into_any())
}
