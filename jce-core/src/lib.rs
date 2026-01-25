pub mod consts;
pub mod error;
pub mod reader;
pub mod serde;
pub mod writer;

use pyo3::prelude::*;

#[pymodule]
fn _jce_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(serde::dumps, m)?)?;
    m.add_function(wrap_pyfunction!(serde::loads, m)?)?;
    m.add_function(wrap_pyfunction!(serde::dumps_generic, m)?)?;
    m.add_function(wrap_pyfunction!(serde::loads_generic, m)?)?;
    Ok(())
}
