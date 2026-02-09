use bytes::BufMut;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{
    PyAny, PyBool, PyBytes, PyDict, PyFloat, PyFrozenSet, PyList, PySequence, PySet, PyString,
};
use simdutf8::basic::from_utf8;
use std::cell::RefCell;

use crate::binding::schema::{StructDef, TarsDict, TypeExpr, ensure_schema_for_class};
use crate::codec::consts::TarsType;
use crate::codec::reader::TarsReader;
use crate::codec::writer::TarsWriter;

const MAX_ANY_ENCODE_DEPTH: usize = 100;
const MAX_ANY_DECODE_DEPTH: usize = 100;

#[inline]
fn check_any_encode_depth(depth: usize) -> PyResult<()> {
    if depth > MAX_ANY_ENCODE_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded or circular reference detected",
        ));
    }
    Ok(())
}

#[inline]
fn check_any_decode_depth(depth: usize) -> PyResult<()> {
    if depth > MAX_ANY_DECODE_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded during deserialization",
        ));
    }
    Ok(())
}

thread_local! {
    static STDLIB_CACHE: RefCell<Option<StdlibCache>> = const { RefCell::new(None) };
}

struct StdlibCache {
    enum_type: Py<PyAny>,
}

fn with_stdlib_cache<F, R>(py: Python<'_>, f: F) -> PyResult<R>
where
    F: FnOnce(&StdlibCache) -> PyResult<R>,
{
    STDLIB_CACHE.with(|cell| {
        let mut cache_opt = cell.borrow_mut();
        if cache_opt.is_none() {
            let enum_mod = py.import("enum")?;
            let enum_type = enum_mod.getattr("Enum")?.unbind();

            *cache_opt = Some(StdlibCache { enum_type });
        }
        f(cache_opt.as_ref().unwrap())
    })
}

pub(crate) struct PySequenceFast {
    ptr: *mut ffi::PyObject,
    len: isize,
    is_list: bool,
}

impl PySequenceFast {
    pub(crate) fn new_exact(obj: &Bound<'_, PyAny>, is_list: bool) -> PyResult<Self> {
        // SAFETY:
        // 1. ptr 是一个有效的 Python 对象指针，已知是 list 或 tuple。
        // 2. 我们刚刚增加了引用计数，确保它保持存活。
        let ptr = obj.as_ptr();
        unsafe { ffi::Py_INCREF(ptr) };

        let len = unsafe {
            if is_list {
                let list_ptr = ptr as *mut ffi::PyListObject;
                (*list_ptr).ob_base.ob_size
            } else {
                let tuple_ptr = ptr as *mut ffi::PyTupleObject;
                (*tuple_ptr).ob_base.ob_size
            }
        };
        Ok(Self { ptr, len, is_list })
    }

    pub(crate) fn len(&self) -> usize {
        self.len as usize
    }

    pub(crate) fn get_item<'py>(&self, py: Python<'py>, idx: usize) -> PyResult<Bound<'py, PyAny>> {
        if idx as isize >= self.len {
            return Err(PyValueError::new_err("Index out of bounds"));
        }
        // SAFETY:
        // 1. ptr 保持强引用存活。
        // 2. GetItem 返回借用引用(Borrowed Reference)。
        // 3. 不缓存 items 指针，避免列表扩容导致的 UAF。
        unsafe {
            let item_ptr = if self.is_list {
                ffi::PyList_GetItem(self.ptr, idx as isize)
            } else {
                ffi::PyTuple_GetItem(self.ptr, idx as isize)
            };
            if item_ptr.is_null() {
                return Err(PyErr::fetch(py));
            }
            Ok(Bound::from_borrowed_ptr(py, item_ptr))
        }
    }
}

impl Drop for PySequenceFast {
    fn drop(&mut self) {
        // SAFETY:
        // self.ptr 是一个拥有的引用（强引用）。
        // 当此包装器被删除时，我们需要减少它的引用计数。
        unsafe {
            ffi::Py_DECREF(self.ptr);
        }
    }
}

pub(crate) fn check_exact_sequence_type(obj: &Bound<'_, PyAny>) -> Option<bool> {
    // SAFETY:
    // 在有效的 Python 对象指针上调用标准类型检查宏是安全的。
    unsafe {
        if ffi::PyList_CheckExact(obj.as_ptr()) != 0 {
            Some(true)
        } else if ffi::PyTuple_CheckExact(obj.as_ptr()) != 0 {
            Some(false)
        } else {
            None
        }
    }
}

pub(crate) fn dataclass_fields<'py>(
    value: &Bound<'py, PyAny>,
) -> PyResult<Option<Bound<'py, PyDict>>> {
    let cls = value.get_type();
    let fields_any = match cls.getattr("__dataclass_fields__") {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    match fields_any.cast::<PyDict>() {
        Ok(fields) => Ok(Some(fields.clone())),
        Err(_) => Ok(None),
    }
}

pub(crate) fn serialize_struct_fields<W, F>(
    writer: &mut TarsWriter<W>,
    obj: &Bound<'_, PyAny>,
    def: &StructDef,
    depth: usize,
    serialize_typed: &F,
) -> PyResult<()>
where
    W: BufMut,
    F: Fn(&mut TarsWriter<W>, u8, &TypeExpr, &Bound<'_, PyAny>, usize) -> PyResult<()>,
{
    check_any_encode_depth(depth)?;

    for field in &def.fields_sorted {
        let value = obj.getattr(field.name_py.bind(obj.py())).ok();

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
                serialize_typed(writer, field.tag, &field.ty, &val, depth + 1)?;
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

pub(crate) fn write_tarsdict_fields<W, F>(
    writer: &mut TarsWriter<W>,
    dict: &Bound<'_, PyDict>,
    depth: usize,
    serialize_typed: &F,
) -> PyResult<()>
where
    W: BufMut,
    F: Fn(&mut TarsWriter<W>, u8, &TypeExpr, &Bound<'_, PyAny>, usize) -> PyResult<()>,
{
    check_any_encode_depth(depth)?;

    let mut items: Vec<(u8, Bound<'_, PyAny>)> = Vec::with_capacity(dict.len());
    for (key, value) in dict.iter() {
        if value.is_none() {
            continue;
        }
        let tag = key
            .extract::<u8>()
            .map_err(|_| PyTypeError::new_err("Struct tag must be int in range 0-255"))?;
        items.push((tag, value));
    }

    items.sort_by_key(|(tag, _)| *tag);
    for (tag, value) in items {
        serialize_any(writer, tag, &value, depth + 1, serialize_typed)?;
    }
    Ok(())
}

pub(crate) fn serialize_any<W, F>(
    writer: &mut TarsWriter<W>,
    tag: u8,
    value: &Bound<'_, PyAny>,
    depth: usize,
    serialize_typed: &F,
) -> PyResult<()>
where
    W: BufMut,
    F: Fn(&mut TarsWriter<W>, u8, &TypeExpr, &Bound<'_, PyAny>, usize) -> PyResult<()>,
{
    check_any_encode_depth(depth)?;
    if value.is_none() {
        return Err(PyTypeError::new_err("Unsupported class type: NoneType"));
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
        let v = value.cast::<PyString>()?.to_str()?;
        writer.write_string(tag, v);
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

    let is_enum = with_stdlib_cache(value.py(), |cache| {
        let py = value.py();
        if value.is_instance(cache.enum_type.bind(py).as_any())? {
            let inner = value.getattr("value")?;
            serialize_any(writer, tag, &inner, depth + 1, serialize_typed)?;
            return Ok(true);
        }
        Ok(false)
    })?;

    if is_enum {
        return Ok(());
    }

    if value.is_instance_of::<TarsDict>() {
        let dict = value.cast::<PyDict>()?;
        writer.write_tag(tag, TarsType::StructBegin);
        write_tarsdict_fields(writer, dict, depth + 1, serialize_typed)?;
        writer.write_tag(0, TarsType::StructEnd);
        return Ok(());
    }

    let cls = value.get_type();
    if let Ok(def) = ensure_schema_for_class(value.py(), &cls) {
        writer.write_tag(tag, TarsType::StructBegin);
        serialize_struct_fields(writer, value, &def, depth + 1, serialize_typed)?;
        writer.write_tag(0, TarsType::StructEnd);
        return Ok(());
    }

    if let Some(fields) = dataclass_fields(value)? {
        writer.write_tag(tag, TarsType::Map);
        let len = fields.len();
        writer.write_int(0, len as i64);
        for (name_any, _field) in fields {
            let name: String = name_any.extract()?;
            let field_value = value.getattr(name.as_str())?;
            serialize_any(writer, 0, &name_any, depth + 1, serialize_typed)?;
            serialize_any(writer, 1, &field_value, depth + 1, serialize_typed)?;
        }
        return Ok(());
    }

    if value.is_instance_of::<PyDict>() {
        let dict = value.cast::<PyDict>()?;
        writer.write_tag(tag, TarsType::Map);
        writer.write_int(0, dict.len() as i64);
        for (k, v) in dict {
            serialize_any(writer, 0, &k, depth + 1, serialize_typed)?;
            serialize_any(writer, 1, &v, depth + 1, serialize_typed)?;
        }
        return Ok(());
    }

    if value.is_instance_of::<PySet>() {
        let set = value.cast::<PySet>()?;
        writer.write_tag(tag, TarsType::List);
        let len = set.len() as i64;
        writer.write_int(0, len);
        for item in set.iter() {
            serialize_any(writer, 0, &item, depth + 1, serialize_typed)?;
        }
        return Ok(());
    }
    if value.is_instance_of::<PyFrozenSet>() {
        let set = value.cast::<PyFrozenSet>()?;
        writer.write_tag(tag, TarsType::List);
        let len = set.len() as i64;
        writer.write_int(0, len);
        for item in set.iter() {
            serialize_any(writer, 0, &item, depth + 1, serialize_typed)?;
        }
        return Ok(());
    }

    if value.is_instance_of::<PyList>() || value.is_instance_of::<PySequence>() {
        writer.write_tag(tag, TarsType::List);
        if let Some(is_list) = check_exact_sequence_type(value) {
            let seq_fast = PySequenceFast::new_exact(value, is_list)?;
            let len = seq_fast.len();
            writer.write_int(0, len as i64);
            for i in 0..len {
                let item = seq_fast.get_item(value.py(), i)?;
                serialize_any(writer, 0, &item, depth + 1, serialize_typed)?;
            }
        } else {
            let seq = value.extract::<Bound<'_, PySequence>>()?;
            let len = seq.len()?;
            writer.write_int(0, len as i64);
            for i in 0..len {
                let item = seq.get_item(i)?;
                serialize_any(writer, 0, &item, depth + 1, serialize_typed)?;
            }
        }
        return Ok(());
    }

    Err(PyTypeError::new_err("Unsupported Any value type"))
}

#[inline]
pub(crate) fn read_size_non_negative(reader: &mut TarsReader, context: &str) -> PyResult<usize> {
    let len = reader
        .read_size()
        .map_err(|e| PyValueError::new_err(format!("Failed to read {} size: {}", context, e)))?;
    if len < 0 {
        return Err(PyValueError::new_err(format!("Invalid {} size", context)));
    }
    Ok(len as usize)
}

fn read_simple_list_bytes<'a>(reader: &'a mut TarsReader) -> PyResult<&'a [u8]> {
    let subtype = reader
        .read_u8()
        .map_err(|e| PyValueError::new_err(format!("Failed to read SimpleList subtype: {e}")))?;
    if subtype != 0 {
        return Err(PyValueError::new_err("SimpleList must contain Byte (0)"));
    }
    let len = read_size_non_negative(reader, "SimpleList")?;
    let bytes = reader
        .read_bytes(len)
        .map_err(|e| PyValueError::new_err(format!("Failed to read SimpleList bytes: {e}")))?;
    Ok(bytes)
}

pub(crate) fn decode_any_value<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    type_id: TarsType,
    depth: usize,
    auto_simplelist: bool,
) -> PyResult<Bound<'py, PyAny>> {
    check_any_decode_depth(depth)?;
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
            decode_any_struct_fields(py, reader, depth + 1, auto_simplelist).map(|d| d.into_any())
        }
        TarsType::List => decode_any_list(py, reader, depth + 1, auto_simplelist),
        TarsType::SimpleList => decode_any_simple_list(py, reader, auto_simplelist),
        TarsType::Map => decode_any_map(py, reader, depth + 1, auto_simplelist),
        TarsType::StructEnd => Err(PyValueError::new_err("Unexpected StructEnd")),
    }
}

pub(crate) fn decode_any_struct_fields<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    depth: usize,
    auto_simplelist: bool,
) -> PyResult<Bound<'py, PyDict>> {
    check_any_decode_depth(depth)?;
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
        let value = decode_any_value(py, reader, type_id, depth + 1, auto_simplelist)?;
        dict.set_item(tag, value)?;
    }
    Ok(dict)
}

fn decode_any_list<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    depth: usize,
    auto_simplelist: bool,
) -> PyResult<Bound<'py, PyAny>> {
    check_any_decode_depth(depth)?;
    let len = read_size_non_negative(reader, "list")?;
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
        let item = decode_any_value(py, reader, item_type, depth + 1, auto_simplelist)?;
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

fn decode_any_map<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    depth: usize,
    auto_simplelist: bool,
) -> PyResult<Bound<'py, PyAny>> {
    check_any_decode_depth(depth)?;
    let len = read_size_non_negative(reader, "map")?;
    let dict = PyDict::new(py);
    for _ in 0..len {
        let (_, kt) = reader
            .read_head()
            .map_err(|e| PyValueError::new_err(format!("Failed to read map key head: {e}")))?;
        let key = decode_any_value(py, reader, kt, depth + 1, auto_simplelist)?;

        let (_, vt) = reader
            .read_head()
            .map_err(|e| PyValueError::new_err(format!("Failed to read map value head: {e}")))?;
        let val = decode_any_value(py, reader, vt, depth + 1, auto_simplelist)?;

        if key.hash().is_err() {
            return Err(PyTypeError::new_err("Map key must be hashable"));
        }
        dict.set_item(key, val)?;
    }
    Ok(dict.into_any())
}

fn decode_any_simple_list<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    auto_simplelist: bool,
) -> PyResult<Bound<'py, PyAny>> {
    let bytes = read_simple_list_bytes(reader)?;

    if !auto_simplelist {
        return Ok(PyBytes::new(py, bytes).into_any());
    }

    if bytes.is_empty() {
        return Ok(PyBytes::new(py, bytes).into_any());
    }

    if looks_like_tars_struct_any(py, bytes) {
        return Ok(PyBytes::new(py, bytes).into_any());
    }

    if let Ok(s) = from_utf8(bytes) {
        return Ok(s.into_pyobject(py)?.into_any());
    }

    Ok(PyBytes::new(py, bytes).into_any())
}

fn looks_like_tars_struct_any(py: Python<'_>, data: &[u8]) -> bool {
    if data.is_empty() {
        return false;
    }

    let type_id = data[0] & 0x0F;
    if type_id > 13 {
        return false;
    }

    let mut reader = TarsReader::new(data);
    if decode_any_struct_fields(py, &mut reader, 0, true).is_ok() {
        return reader.is_end();
    }

    false
}
