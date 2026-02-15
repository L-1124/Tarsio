use pyo3::prelude::*;

/// 字段元数据与约束定义.
///
/// 用于在 `Annotated` 中替代纯整数 Tag, 提供额外的运行时校验.
///
/// Args:
///     tag: JCE Tag ID (0-255).
///     gt: 大于 (Greater Than).
///     lt: 小于 (Less Than).
///     ge: 大于等于 (Greater or Equal).
///     le: 小于等于 (Less or Equal).
///     min_len: 最小长度 (String/List/Map).
///     max_len: 最大长度.
///     pattern: 正则表达式 (仅 String).
///
/// Examples:
///     ```python
///     from typing import Annotated
///     from tarsio import Struct, Meta
///
///     class Product(Struct):
///         # 价格必须 > 0
///         price: Annotated[int, Meta(tag=0, gt=0)]
///         # 代码必须是 1-10 位大写字母
///         code: Annotated[str, Meta(tag=1, min_len=1, max_len=10, pattern=r"^[A-Z]+$")]
///     ```
#[pyclass(module = "tarsio._core")]
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
