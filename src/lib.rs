use pyo3::create_exception;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

pub mod binding;
pub mod codec;

create_exception!(_core, ValidationError, PyValueError);

/// Rust 实现的 Python 模块.
#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<binding::schema::Struct>()?;
    m.add_class::<binding::meta::Meta>()?;
    m.add("ValidationError", m.py().get_type::<ValidationError>())?;
    m.add_function(wrap_pyfunction!(binding::ser::encode, m)?)?;
    m.add_function(wrap_pyfunction!(binding::de::decode, m)?)?;
    m.add_function(wrap_pyfunction!(binding::raw::encode_raw, m)?)?;
    m.add_function(wrap_pyfunction!(binding::raw::decode_raw, m)?)?;
    m.add_function(wrap_pyfunction!(binding::raw::probe_struct, m)?)?;

    Ok(())
}
