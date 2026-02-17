use std::cell::RefCell;

use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyType};

thread_local! {
    static STDLIB_CACHE: RefCell<Option<StdlibCache>> = const { RefCell::new(None) };
}

pub(crate) struct StdlibCache {
    pub(crate) enum_type: Py<PyAny>,
}

pub(crate) fn with_stdlib_cache<F, R>(py: Python<'_>, f: F) -> PyResult<R>
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

pub const MAX_DEPTH: usize = 100;
// Capacity threshold (1MB). If buffer exceeds this, we shrink it back.
pub const BUFFER_SHRINK_THRESHOLD: usize = 1024 * 1024;
// Default initial capacity (128 bytes).
pub const BUFFER_DEFAULT_CAPACITY: usize = 128;

/// 编码后智能缩容:避免单次大包导致内存长期驻留.
#[inline]
pub(crate) fn maybe_shrink_buffer(buffer: &mut Vec<u8>) {
    let used = buffer.len();
    if buffer.capacity() > BUFFER_SHRINK_THRESHOLD && used < (BUFFER_SHRINK_THRESHOLD / 4) {
        let target = if used == 0 {
            BUFFER_DEFAULT_CAPACITY
        } else {
            used.next_power_of_two().max(BUFFER_DEFAULT_CAPACITY)
        };
        buffer.shrink_to(target);
    }
}

/// 根据 schema 中记录的类指针恢复 Python 类型对象.
#[inline]
pub(crate) fn class_from_ptr<'py>(py: Python<'py>, ptr: usize) -> PyResult<Bound<'py, PyType>> {
    let obj_ptr = ptr as *mut ffi::PyObject;
    if obj_ptr.is_null() {
        return Err(PyTypeError::new_err("Invalid struct pointer"));
    }
    // SAFETY: 指针 ptr 来自 Schema 系统，生命周期受控。
    let any = unsafe { Bound::from_borrowed_ptr(py, obj_ptr) };
    let cls = any.cast::<PyType>()?;
    Ok(cls.clone())
}

#[inline]
pub fn check_depth(depth: usize) -> PyResult<()> {
    if depth > MAX_DEPTH {
        return Err(PyValueError::new_err(
            "Recursion limit exceeded or circular reference detected",
        ));
    }
    Ok(())
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
