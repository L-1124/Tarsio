use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyType};
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
        TypeExpr::DateTime => {
            let micros = reader
                .read_int(type_id)
                .map_err(|e| PyValueError::new_err(format!("Failed to read int: {}", e)))?;
            micros_to_datetime(py, micros)
        }
        TypeExpr::Date => {
            let days = reader
                .read_int(type_id)
                .map_err(|e| PyValueError::new_err(format!("Failed to read int: {}", e)))?;
            days_to_date(py, days)
        }
        TypeExpr::Time => {
            let nanos = reader
                .read_int(type_id)
                .map_err(|e| PyValueError::new_err(format!("Failed to read int: {}", e)))?;
            nanos_to_time(py, nanos)
        }
        TypeExpr::Timedelta => {
            let micros = reader
                .read_int(type_id)
                .map_err(|e| PyValueError::new_err(format!("Failed to read int: {}", e)))?;
            micros_to_timedelta(py, micros)
        }
        TypeExpr::Uuid => {
            let bytes = read_uuid_bytes(reader, type_id)?;
            bytes_to_uuid(py, bytes)
        }
        TypeExpr::Decimal => {
            let bytes = reader.read_string(type_id).map_err(|e| {
                PyValueError::new_err(format!("Failed to read string bytes: {}", e))
            })?;
            let s = from_utf8(bytes).map_err(|_| PyValueError::new_err("Invalid UTF-8 string"))?;
            string_to_decimal(py, s)
        }
        TypeExpr::Enum(enum_cls, inner) => {
            let value = deserialize_value(py, reader, type_id, inner, depth + 1)?;
            let cls = enum_cls.bind(py);
            let enum_value = cls.call1((value,))?;
            Ok(enum_value)
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
        TypeExpr::List(inner) | TypeExpr::Tuple(inner) => {
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

            if matches!(type_expr, TypeExpr::Tuple(_)) {
                let list_any_clone = list_any.clone();
                let list = list_any_clone.cast::<PyList>()?;
                Ok(list.to_tuple().into_any())
            } else {
                Ok(list_any)
            }
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
    if type_id != TarsType::StructBegin {
        return Err(PyValueError::new_err(
            "Union value must be encoded as Struct",
        ));
    }

    let mut variant_idx: Option<usize> = None;
    let mut value: Option<Bound<'py, PyAny>> = None;

    while !reader.is_end() {
        let (tag, t) = reader
            .read_head()
            .map_err(|e| PyValueError::new_err(format!("Read head error: {}", e)))?;
        if t == TarsType::StructEnd {
            break;
        }

        match tag {
            0 => {
                let idx = reader.read_int(t).map_err(|e| {
                    PyValueError::new_err(format!("Failed to read union tag: {}", e))
                })?;
                if idx < 0 || (idx as usize) >= variants.len() {
                    return Err(PyValueError::new_err("Union tag out of range"));
                }
                variant_idx = Some(idx as usize);
            }
            1 => {
                let Some(idx) = variant_idx else {
                    return Err(PyValueError::new_err(
                        "Union value appears before union tag",
                    ));
                };
                let variant = &variants[idx];
                let decoded = deserialize_value(py, reader, t, variant, depth + 1)?;
                value = Some(decoded);
            }
            _ => {
                reader.skip_field(t).map_err(|e| {
                    PyValueError::new_err(format!("Failed to skip union field: {}", e))
                })?;
            }
        }
    }

    let Some(idx) = variant_idx else {
        return Err(PyValueError::new_err("Missing union tag"));
    };
    let variant = &variants[idx];
    if matches!(variant, TypeExpr::NoneType) {
        return Ok(py.None().into_bound(py));
    }
    value.ok_or_else(|| PyValueError::new_err("Missing union value"))
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
    let mut bytes_candidate: Vec<u8> = Vec::with_capacity(len);
    let mut is_bytes = true;

    for idx in 0..len {
        let (_, item_type) = reader
            .read_head()
            .map_err(|e| PyValueError::new_err(format!("Failed to read list item head: {e}")))?;
        let item = match item_type {
            TarsType::ZeroTag
            | TarsType::Int1
            | TarsType::Int2
            | TarsType::Int4
            | TarsType::Int8 => {
                let v = reader
                    .read_int(item_type)
                    .map_err(|e| PyValueError::new_err(format!("Failed to read int: {e}")))?;
                if is_bytes {
                    if item_type == TarsType::Int1 || item_type == TarsType::ZeroTag {
                        if (0..=255).contains(&v) {
                            bytes_candidate.push(v as u8);
                        } else {
                            is_bytes = false;
                        }
                    } else {
                        is_bytes = false;
                    }
                }
                v.into_pyobject(py)?.into_any()
            }
            _ => {
                if is_bytes {
                    is_bytes = false;
                }
                decode_any_value(py, reader, item_type, depth + 1)?
            }
        };
        let set_res = unsafe {
            // SAFETY: PyList_SetItem 会“偷”引用, item.into_ptr 转移所有权。
            // 每个索引只写入一次,与 PyList_New 的预分配长度一致。
            ffi::PyList_SetItem(list_any.as_ptr(), idx as isize, item.into_ptr())
        };
        if set_res != 0 {
            return Err(PyErr::fetch(py));
        }
    }

    if is_bytes {
        return Ok(PyBytes::new(py, &bytes_candidate).into_any());
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

fn read_uuid_bytes<'py>(reader: &mut TarsReader<'py>, type_id: TarsType) -> PyResult<&'py [u8]> {
    if type_id != TarsType::SimpleList {
        return Err(PyValueError::new_err(
            "UUID must be encoded as SimpleList bytes",
        ));
    }
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
    reader
        .read_bytes(len)
        .map_err(|e| PyValueError::new_err(format!("Failed to read SimpleList bytes: {e}")))
}

fn micros_to_datetime<'py>(py: Python<'py>, micros: i64) -> PyResult<Bound<'py, PyAny>> {
    let datetime_mod = py.import("datetime")?;
    let tz = datetime_mod.getattr("timezone")?.getattr("utc")?;
    let seconds = micros as f64 / 1_000_000.0;
    let dt = datetime_mod
        .getattr("datetime")?
        .call_method1("fromtimestamp", (seconds, tz))?;
    Ok(dt)
}

fn days_to_date<'py>(py: Python<'py>, days: i64) -> PyResult<Bound<'py, PyAny>> {
    let datetime_mod = py.import("datetime")?;
    let epoch = datetime_mod.getattr("date")?.call1((1970, 1, 1))?;
    let delta = datetime_mod.getattr("timedelta")?.call1((days,))?;
    let date = epoch.call_method1("__add__", (delta,))?;
    Ok(date)
}

fn nanos_to_time<'py>(py: Python<'py>, nanos: i64) -> PyResult<Bound<'py, PyAny>> {
    let total_micros = nanos / 1_000;
    let micros = total_micros % 1_000_000;
    let total_secs = total_micros / 1_000_000;
    let hour = total_secs / 3600;
    let minute = (total_secs % 3600) / 60;
    let second = total_secs % 60;
    let datetime_mod = py.import("datetime")?;
    let time_obj = datetime_mod
        .getattr("time")?
        .call1((hour, minute, second, micros))?;
    Ok(time_obj)
}

fn micros_to_timedelta<'py>(py: Python<'py>, micros: i64) -> PyResult<Bound<'py, PyAny>> {
    let datetime_mod = py.import("datetime")?;
    let td = datetime_mod.getattr("timedelta")?.call1((0, 0, micros))?;
    Ok(td)
}

fn bytes_to_uuid<'py>(py: Python<'py>, bytes: &[u8]) -> PyResult<Bound<'py, PyAny>> {
    let uuid_mod = py.import("uuid")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("bytes", PyBytes::new(py, bytes))?;
    let uuid = uuid_mod.getattr("UUID")?.call((), Some(&kwargs))?;
    Ok(uuid)
}

fn string_to_decimal<'py>(py: Python<'py>, value: &str) -> PyResult<Bound<'py, PyAny>> {
    let decimal_mod = py.import("decimal")?;
    let decimal = decimal_mod.getattr("Decimal")?.call1((value,))?;
    Ok(decimal)
}
