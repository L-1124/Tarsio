use pyo3::prelude::*;

#[pyclass]
pub struct Meta {
    #[pyo3(get, set)]
    pub tag: Option<u8>,
    #[pyo3(get, set)]
    pub gt: Option<f64>,
    #[pyo3(get, set)]
    pub lt: Option<f64>,
    #[pyo3(get, set)]
    pub ge: Option<f64>,
    #[pyo3(get, set)]
    pub le: Option<f64>,
    #[pyo3(get, set)]
    pub min_len: Option<usize>,
    #[pyo3(get, set)]
    pub max_len: Option<usize>,
    #[pyo3(get, set)]
    pub pattern: Option<String>,
}

#[pymethods]
impl Meta {
    #[new]
    #[pyo3(signature=(tag=None, gt=None, lt=None, ge=None, le=None, min_len=None, max_len=None, pattern=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        tag: Option<u8>,
        gt: Option<f64>,
        lt: Option<f64>,
        ge: Option<f64>,
        le: Option<f64>,
        min_len: Option<usize>,
        max_len: Option<usize>,
        pattern: Option<String>,
    ) -> Self {
        Self {
            tag,
            gt,
            lt,
            ge,
            le,
            min_len,
            max_len,
            pattern,
        }
    }
}
