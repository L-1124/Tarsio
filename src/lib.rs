use pyo3::create_exception;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

pub mod binding;
pub mod codec;

create_exception!(_core, ValidationError, PyValueError);

/// Rust 实现的 Python 模块.
#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();
    binding::metaclass::add_struct_meta(m)?;
    m.add_class::<binding::schema::Struct>()?;
    m.add_class::<binding::schema::StructConfig>()?;
    m.add_class::<binding::meta::Meta>()?;
    m.add("ValidationError", m.py().get_type::<ValidationError>())?;
    m.add_function(wrap_pyfunction!(binding::ser::encode, m)?)?;
    m.add_function(wrap_pyfunction!(binding::de::decode, m)?)?;
    m.add_function(wrap_pyfunction!(binding::raw::encode_raw, m)?)?;
    m.add_function(wrap_pyfunction!(binding::raw::decode_raw, m)?)?;
    m.add_function(wrap_pyfunction!(binding::raw::probe_struct, m)?)?;

    let struct_meta = m.getattr("StructMeta")?;
    let struct_base = m.getattr("_StructBase")?;
    let bases = PyTuple::new(py, vec![struct_base])?;
    let ns = PyDict::new(py);
    ns.set_item("__module__", "tarsio._core")?;
    let struct_cls = struct_meta.call1(("Struct", bases, ns))?;
    m.add("Struct", struct_cls)?;

    Ok(())
}
