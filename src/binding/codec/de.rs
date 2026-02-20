use crate::binding::codec::raw::{
    decode_any_struct_fields, decode_any_value, decode_raw_from_bytes, read_size_non_negative,
};
use crate::binding::error::{DeError, DeResult, PathItem};
use crate::binding::schema::{
    Constraints, StructDef, TarsDict, TypeExpr, WireType, ensure_schema_for_class, run_post_init,
};
use crate::binding::utils::{check_depth, class_from_type, try_coerce_buffer_to_bytes};
use crate::binding::validation::{
    validate_constraints_on_value, validate_length_constraints_raw,
    validate_numeric_constraints_raw,
};
use crate::codec::consts::TarsType;
use crate::codec::reader::TarsReader;
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyDict, PySet, PyTuple, PyType};
use simdutf8::basic::from_utf8;

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
    data: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let bytes = try_coerce_buffer_to_bytes(data)?.ok_or_else(|| {
        pyo3::exceptions::PyTypeError::new_err("argument 'data': expected a bytes-like object")
    })?;
    decode_object(py, cls, bytes.as_bytes())
}

/// 内部:将字节解码为 Tars Struct 实例.
pub fn decode_object<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
    data: &[u8],
) -> PyResult<Bound<'py, PyAny>> {
    if cls.is_subclass_of::<TarsDict>()? {
        let dict = decode_raw_from_bytes(py, data)?;
        if cls.is(dict.get_type().as_any()) {
            return Ok(dict.into_any());
        }
        let instance = cls.call1((dict,))?;
        return Ok(instance);
    }
    // 校验 schema 是否存在并获取
    let def = ensure_schema_for_class(py, cls)?;

    let mut reader = TarsReader::new(data);
    let res = deserialize_struct(py, cls, &mut reader, &def, 0).map_err(|e| e.to_pyerr(py))?;
    if !reader.is_end() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "Trailing bytes after decode",
        ));
    }
    Ok(res)
}

/// 从读取器中反序列化结构体.
fn deserialize_struct<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
    reader: &mut TarsReader,
    def: &StructDef,
    depth: usize,
) -> DeResult<Bound<'py, PyAny>> {
    check_depth(depth).map_err(DeError::wrap)?;

    let field_count = def.fields_sorted.len();

    // 预分配 Python 对象
    // SAFETY:
    // 1. `cls` 是有效的 Python 类型对象；`PyType_GenericAlloc` 返回新引用。
    // 2. 若返回空指针则 Python 异常已设置，立即以 `PyErr::fetch` 包装返回。
    // 3. `Bound::from_owned_ptr` 正确接管该新引用所有权。
    let instance = unsafe {
        let type_ptr = cls.as_ptr() as *mut ffi::PyTypeObject;
        let obj_ptr = ffi::PyType_GenericAlloc(type_ptr, 0);
        if obj_ptr.is_null() {
            return Err(DeError::wrap(PyErr::fetch(py)));
        }
        Bound::from_owned_ptr(py, obj_ptr)
    };

    // 使用位掩码追踪已见字段 (支持高达 64 个字段)
    let mut seen_mask: u64 = 0;
    let mut seen_vec: Option<Vec<bool>> = if field_count > 64 {
        Some(vec![false; field_count])
    } else {
        None
    };

    // 读取字段,直到遇到 StructEnd 或 EOF
    while !reader.is_end() {
        let (tag, type_id) = match reader.read_head() {
            Ok(h) => h,
            Err(_) => break,
        };

        if type_id == TarsType::StructEnd {
            break;
        }

        let idx_opt = if (tag as usize) < def.tag_lookup_vec.len() {
            def.tag_lookup_vec[tag as usize]
        } else {
            None
        };

        if let Some(idx) = idx_opt {
            let field = &def.fields_sorted[idx];
            let value_result: DeResult<Bound<'py, PyAny>> = if field.wrap_simplelist {
                if type_id != TarsType::SimpleList {
                    Err(DeError::new(format!(
                        "Field '{}' expects SimpleList(bytes) payload",
                        field.name
                    )))
                } else {
                    let subtype = reader.read_u8().map_err(|e| {
                        DeError::new(format!("Failed to read SimpleList subtype: {e}"))
                    })?;
                    if subtype != 0 {
                        Err(DeError::new("SimpleList must contain Byte (0)".into()))
                    } else {
                        let len = read_size_non_negative(reader, "SimpleList")?;
                        let payload = reader.read_bytes(len).map_err(|e| {
                            DeError::new(format!("Failed to read SimpleList bytes: {e}"))
                        })?;
                        match &field.ty {
                            TypeExpr::Struct(cls_obj) => {
                                let nested_cls = class_from_type(py, cls_obj);
                                let nested_def = ensure_schema_for_class(py, &nested_cls)
                                    .map_err(DeError::wrap)?;
                                let mut inner_reader = TarsReader::new(payload);
                                let res = deserialize_struct(
                                    py,
                                    &nested_cls,
                                    &mut inner_reader,
                                    &nested_def,
                                    depth + 1,
                                )?;
                                if !inner_reader.is_end() {
                                    return Err(DeError::new(
                                        "Trailing bytes after SimpleList decode".into(),
                                    ));
                                }
                                Ok(res)
                            }
                            TypeExpr::TarsDict => {
                                let mut inner_reader = TarsReader::new(payload);
                                let dict = crate::binding::codec::raw::decode_struct_fields(
                                    py,
                                    &mut inner_reader,
                                    true,
                                    depth + 1,
                                )
                                .map_err(DeError::wrap)?;
                                if !inner_reader.is_end() {
                                    return Err(DeError::new(
                                        "Trailing bytes after SimpleList TarsDict decode".into(),
                                    ));
                                }
                                let tarsdict_type = py.get_type::<TarsDict>();
                                let instance =
                                    tarsdict_type.call1((dict,)).map_err(DeError::wrap)?;
                                Ok(instance.into_any())
                            }
                            _ => Err(DeError::new(format!(
                                "Field '{}' with wrap_simplelist=True must be Struct or TarsDict",
                                field.name
                            ))),
                        }
                    }
                }
            } else {
                deserialize_value(
                    py,
                    reader,
                    type_id,
                    &field.ty,
                    field.constraints.as_deref(),
                    depth + 1,
                )
            };
            let value = value_result.map_err(|e| e.prepend(PathItem::Field(field.name.clone())))?;

            if let Some(c) = field.constraints.as_deref() {
                validate_constraints_on_value(&value, c, Some(field.name.as_str()))
                    .map_err(DeError::wrap)
                    .map_err(|e| e.prepend(PathItem::Field(field.name.clone())))?;
            }

            // 直接设置属性
            // SAFETY:
            // 1. `instance`、字段名 `name_py`、以及 `value` 均为当前 GIL 下有效对象。
            // 2. `PyObject_GenericSetAttr` 不窃取 `value` 引用。
            // 3. 若返回非 0，Python 异常已设置并通过 `PyErr::fetch` 传播。
            unsafe {
                let name_py = field.name_py.bind(py);
                let res = ffi::PyObject_GenericSetAttr(
                    instance.as_ptr(),
                    name_py.as_ptr(),
                    value.as_ptr(),
                );
                if res != 0 {
                    return Err(DeError::wrap(PyErr::fetch(py)));
                }
            }

            if let Some(vec) = seen_vec.as_mut() {
                vec[idx] = true;
            } else {
                seen_mask |= 1 << idx;
            }
        } else {
            if def.forbid_unknown_tags {
                return Err(DeError::new(format!(
                    "Unknown tag {} found in deserialization (forbid_unknown_tags=True)",
                    tag
                )));
            }
            let _ = reader.skip_field(type_id);
        }
    }

    // 处理未出现的字段 (默认值/必填检查)
    for (idx, field) in def.fields_sorted.iter().enumerate() {
        let is_seen = if let Some(vec) = &seen_vec {
            vec[idx]
        } else {
            (seen_mask & (1 << idx)) != 0
        };

        if !is_seen {
            let value_opt = if let Some(default_value) = field.default_value.as_ref() {
                Some(default_value.bind(py).clone())
            } else if let Some(factory) = field.default_factory.as_ref() {
                Some(factory.bind(py).call0().map_err(DeError::wrap)?)
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

            if let Some(val) = value_opt {
                // SAFETY:
                // 1. 与上方字段写入相同，目标对象与属性名/属性值均有效。
                // 2. C API 失败时异常由 Python 设置，立即抓取返回。
                unsafe {
                    let name_py = field.name_py.bind(py);
                    let res = ffi::PyObject_GenericSetAttr(
                        instance.as_ptr(),
                        name_py.as_ptr(),
                        val.as_ptr(),
                    );
                    if res != 0 {
                        return Err(DeError::wrap(PyErr::fetch(py)));
                    }
                }
            }
        }
    }

    if let Err(err) = run_post_init(instance.as_any()) {
        if err.is_instance_of::<pyo3::exceptions::PyTypeError>(py)
            || err.is_instance_of::<pyo3::exceptions::PyValueError>(py)
        {
            return Err(DeError::wrap(err));
        }
        return Err(DeError::passthrough(err));
    }

    Ok(instance)
}

/// 根据 TypeExpr 反序列化单个值.
fn deserialize_value<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    type_id: TarsType,
    type_expr: &TypeExpr,
    constraints: Option<&Constraints>,
    depth: usize,
) -> DeResult<Bound<'py, PyAny>> {
    check_depth(depth).map_err(DeError::wrap)?;

    match type_expr {
        TypeExpr::Primitive(wire_type) => match wire_type {
            WireType::Int | WireType::Long => {
                let v = reader
                    .read_int(type_id)
                    .map_err(|e| DeError::new(format!("Failed to read int: {}", e)))?;

                if let Some(c) = constraints {
                    validate_numeric_constraints_raw(v as f64, c, None).map_err(DeError::wrap)?;
                }

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

                if let Some(c) = constraints {
                    validate_numeric_constraints_raw(v as f64, c, None).map_err(DeError::wrap)?;
                }

                Ok(v.into_pyobject(py)
                    .map_err(|e| DeError::new(e.to_string()))?
                    .into_any())
            }
            WireType::Double => {
                let v = reader
                    .read_double(type_id)
                    .map_err(|e| DeError::new(format!("Failed to read double: {}", e)))?;

                if let Some(c) = constraints {
                    validate_numeric_constraints_raw(v, c, None).map_err(DeError::wrap)?;
                }

                Ok(v.into_pyobject(py)
                    .map_err(|e| DeError::new(e.to_string()))?
                    .into_any())
            }
            WireType::String => {
                let bytes = reader
                    .read_string(type_id)
                    .map_err(|e| DeError::new(format!("Failed to read string bytes: {}", e)))?;

                if let Some(c) = constraints {
                    validate_length_constraints_raw(bytes.len(), c, None).map_err(DeError::wrap)?;
                }

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
            let value = deserialize_value(py, reader, type_id, inner, None, depth + 1)?;
            let cls = enum_cls.bind(py);
            let enum_value = cls.call1((value,)).map_err(DeError::wrap)?;
            Ok(enum_value)
        }
        TypeExpr::Set(inner) => {
            if type_id != TarsType::List {
                return Err(DeError::new("Set value must be encoded as List".into()));
            }
            let len = read_size_non_negative(reader, "list")?;

            if let Some(c) = constraints {
                validate_length_constraints_raw(len, c, None).map_err(DeError::wrap)?;
            }

            let set = PySet::empty(py).map_err(DeError::wrap)?;
            for _ in 0..len {
                let (_, item_type) = reader
                    .read_head()
                    .map_err(|e| DeError::new(format!("Failed to read list item head: {}", e)))?;
                let item = deserialize_value(py, reader, item_type, inner, None, depth + 1)?;
                set.add(item).map_err(DeError::wrap)?;
            }
            Ok(set.into_any())
        }
        TypeExpr::Union(variants, _) => {
            decode_union_value(py, reader, type_id, variants, constraints, depth)
        }
        TypeExpr::Struct(cls_obj) => {
            let nested_cls = class_from_type(py, cls_obj);
            let nested_def = ensure_schema_for_class(py, &nested_cls).map_err(DeError::wrap)?;
            if type_id != TarsType::StructBegin {
                return Err(DeError::new(
                    "Struct value must be encoded as Struct".into(),
                ));
            }
            deserialize_struct(py, &nested_cls, reader, &nested_def, depth + 2)
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
            if let Some(c) = constraints {
                validate_length_constraints_raw(len, c, None).map_err(DeError::wrap)?;
            }
            if len != items.len() {
                return Err(DeError::new(
                    "Tuple length does not match annotation".into(),
                ));
            }
            let tuple = build_fixed_tuple(py, reader, items, depth)?;
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
                    None,
                    depth + 1,
                )?;

                let (_, vt) = reader
                    .read_head()
                    .map_err(|e| DeError::new(format!("Failed to read map value head: {}", e)))?;

                let val = deserialize_value(py, reader, vt, &TypeExpr::Any, None, depth + 1)
                    .map_err(|e| e.prepend(PathItem::Key(key.to_string())))?;

                dict.set_item(key, val).map_err(DeError::wrap)?;
            }
            let instance = cls.bind(py).call((), Some(&dict)).map_err(DeError::wrap)?;
            Ok(instance.into_any())
        }
        TypeExpr::List(inner) => {
            if type_id == TarsType::SimpleList {
                let sub_type = reader.read_u8().map_err(|e| {
                    DeError::new(format!("Failed to read SimpleList subtype: {}", e))
                })?;
                if sub_type != 0 {
                    return Err(DeError::new("SimpleList must contain Byte (0)".into()));
                }
                let len = read_size_non_negative(reader, "SimpleList")?;

                if let Some(c) = constraints {
                    validate_length_constraints_raw(len, c, None).map_err(DeError::wrap)?;
                }

                let bytes = reader
                    .read_bytes(len)
                    .map_err(|e| DeError::new(format!("Failed to read SimpleList bytes: {}", e)))?;
                return Ok(PyBytes::new(py, bytes).into_any());
            }

            let len = read_size_non_negative(reader, "list")?;

            if let Some(c) = constraints {
                validate_length_constraints_raw(len, c, None).map_err(DeError::wrap)?;
            }

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
                let item = deserialize_value(py, reader, item_type, inner, None, depth + 1)
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
            Ok(list_any)
        }
        TypeExpr::VarTuple(inner) => {
            if type_id != TarsType::List {
                return Err(DeError::new("Tuple value must be encoded as List".into()));
            }
            let len = read_size_non_negative(reader, "list")?;
            if let Some(c) = constraints {
                validate_length_constraints_raw(len, c, None).map_err(DeError::wrap)?;
            }
            let tuple = build_var_tuple(py, reader, inner, len, depth)?;
            Ok(tuple.into_any())
        }
        TypeExpr::Tuple(items) => {
            if type_id != TarsType::List {
                return Err(DeError::new("Tuple value must be encoded as List".into()));
            }
            let len = read_size_non_negative(reader, "list")?;
            if let Some(c) = constraints {
                validate_length_constraints_raw(len, c, None).map_err(DeError::wrap)?;
            }
            if len != items.len() {
                return Err(DeError::new(
                    "Tuple length does not match annotation".into(),
                ));
            }
            let tuple = build_fixed_tuple(py, reader, items, depth)?;
            Ok(tuple.into_any())
        }
        TypeExpr::Map(k_type, v_type) => {
            let len = read_size_non_negative(reader, "map")?;

            if let Some(c) = constraints {
                validate_length_constraints_raw(len, c, None).map_err(DeError::wrap)?;
            }

            let dict = PyDict::new(py);
            for _ in 0..len {
                let (_, kt) = reader
                    .read_head()
                    .map_err(|e| DeError::new(format!("Failed to read map key head: {}", e)))?;
                let key = deserialize_value(py, reader, kt, k_type, None, depth + 1)
                    .map_err(|e| e.prepend(PathItem::Key("<key>".into())))?;

                let (_, vt) = reader
                    .read_head()
                    .map_err(|e| DeError::new(format!("Failed to read map value head: {}", e)))?;

                let val = deserialize_value(py, reader, vt, v_type, None, depth + 1)
                    .map_err(|e| e.prepend(PathItem::Key(key.to_string())))?;

                dict.set_item(key, val).map_err(DeError::wrap)?;
            }
            Ok(dict.into_any())
        }
        TypeExpr::Optional(inner) => {
            deserialize_value(py, reader, type_id, inner, constraints, depth)
        }
    }
}

fn build_fixed_tuple<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    items: &[TypeExpr],
    depth: usize,
) -> DeResult<Bound<'py, PyTuple>> {
    let tuple_any = unsafe {
        // SAFETY: PyTuple_New 返回新引用；空指针表示 Python 异常已设置。
        let ptr = ffi::PyTuple_New(items.len() as isize);
        if ptr.is_null() {
            return Err(DeError::wrap(PyErr::fetch(py)));
        }
        Bound::from_owned_ptr(py, ptr)
    };
    let tuple = tuple_any
        .cast_into::<PyTuple>()
        .map_err(|e| DeError::new(e.to_string()))?;

    for (idx, item_type) in items.iter().enumerate() {
        let (_, item_type_id) = reader
            .read_head()
            .map_err(|e| DeError::new(format!("Failed to read list item head: {}", e)))?;
        let item = deserialize_value(py, reader, item_type_id, item_type, None, depth + 1)
            .map_err(|e| e.prepend(PathItem::Index(idx)))?;
        let set_res = unsafe {
            // SAFETY: tuple 由 PyTuple_New 新建且每个槽位只写一次；idx 在范围内。
            // PyTuple_SetItem 会偷引用，因此使用 item.into_ptr() 转移所有权。
            ffi::PyTuple_SetItem(tuple.as_ptr(), idx as isize, item.into_ptr())
        };
        if set_res != 0 {
            return Err(DeError::wrap(PyErr::fetch(py)));
        }
    }

    Ok(tuple)
}

fn build_var_tuple<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    inner: &TypeExpr,
    len: usize,
    depth: usize,
) -> DeResult<Bound<'py, PyTuple>> {
    let tuple_any = unsafe {
        // SAFETY: PyTuple_New 返回新引用；空指针表示 Python 异常已设置。
        let ptr = ffi::PyTuple_New(len as isize);
        if ptr.is_null() {
            return Err(DeError::wrap(PyErr::fetch(py)));
        }
        Bound::from_owned_ptr(py, ptr)
    };
    let tuple = tuple_any
        .cast_into::<PyTuple>()
        .map_err(|e| DeError::new(e.to_string()))?;

    for idx in 0..len {
        let (_, item_type_id) = reader
            .read_head()
            .map_err(|e| DeError::new(format!("Failed to read list item head: {}", e)))?;
        let item = deserialize_value(py, reader, item_type_id, inner, None, depth + 1)
            .map_err(|e| e.prepend(PathItem::Index(idx)))?;
        let set_res = unsafe {
            // SAFETY: tuple 由 PyTuple_New 新建且每个槽位只写一次；idx 在范围内。
            // PyTuple_SetItem 会偷引用，因此使用 item.into_ptr() 转移所有权。
            ffi::PyTuple_SetItem(tuple.as_ptr(), idx as isize, item.into_ptr())
        };
        if set_res != 0 {
            return Err(DeError::wrap(PyErr::fetch(py)));
        }
    }

    Ok(tuple)
}

fn decode_union_value<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    type_id: TarsType,
    variants: &[TypeExpr],
    constraints: Option<&Constraints>,
    depth: usize,
) -> DeResult<Bound<'py, PyAny>> {
    for variant in variants {
        if union_variant_matches_type_id(variant, type_id) {
            return deserialize_value(py, reader, type_id, variant, constraints, depth + 1);
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
        TypeExpr::Union(items, _) => items
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
