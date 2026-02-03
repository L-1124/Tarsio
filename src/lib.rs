use pyo3::prelude::*;

pub mod binding;
pub mod codec;

/// A Python module implemented in Rust.
#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<binding::schema::Struct>()?;
    m.add_function(wrap_pyfunction!(binding::ser::encode, m)?)?;
    m.add_function(wrap_pyfunction!(binding::de::decode, m)?)?;
    m.add_function(wrap_pyfunction!(binding::raw::encode_raw, m)?)?;
    m.add_function(wrap_pyfunction!(binding::raw::decode_raw, m)?)?;

    Ok(())
}
