use parking_lot::RwLock;
use pyo3::gc::{PyTraverseError, PyVisit};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyString, PyType};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Weak};

#[derive(Debug, Clone, PartialEq)]
pub enum WireType {
    Int,
    Bool,
    Long,
    Float,
    Double,
    String,
    Struct(usize),
    List(Box<WireType>),
    Map(Box<WireType>, Box<WireType>),
}

#[derive(Debug)]
pub enum TypeExpr {
    Primitive(WireType),
    Struct(Py<PyType>),
    TarsDict,
    NamedTuple(Py<PyType>, Vec<TypeExpr>),
    Dataclass(Py<PyType>),
    Any,
    NoneType,
    Set(Box<TypeExpr>),
    Enum(Py<PyType>, Box<TypeExpr>),
    Union(Vec<TypeExpr>, UnionCache),
    List(Box<TypeExpr>),
    Tuple(Vec<TypeExpr>),
    VarTuple(Box<TypeExpr>),
    Map(Box<TypeExpr>, Box<TypeExpr>),
    Optional(Box<TypeExpr>),
}

impl TypeExpr {
    pub fn is_optional(&self) -> bool {
        matches!(self, TypeExpr::Optional(_))
    }

    pub fn traverse(&self, visit: &PyVisit<'_>) -> Result<(), PyTraverseError> {
        match self {
            TypeExpr::Primitive(_) => Ok(()),
            TypeExpr::Struct(cls) => visit.call(cls),
            TypeExpr::TarsDict => Ok(()),
            TypeExpr::NamedTuple(cls, items) => {
                visit.call(cls)?;
                for item in items {
                    item.traverse(visit)?;
                }
                Ok(())
            }
            TypeExpr::Dataclass(cls) => visit.call(cls),
            TypeExpr::Any => Ok(()),
            TypeExpr::NoneType => Ok(()),
            TypeExpr::Set(inner) => inner.traverse(visit),
            TypeExpr::Enum(cls, inner) => {
                visit.call(cls)?;
                inner.traverse(visit)
            }
            TypeExpr::Union(items, _) => {
                for item in items {
                    item.traverse(visit)?;
                }
                Ok(())
            }
            TypeExpr::List(inner) => inner.traverse(visit),
            TypeExpr::Tuple(items) => {
                for item in items {
                    item.traverse(visit)?;
                }
                Ok(())
            }
            TypeExpr::VarTuple(inner) => inner.traverse(visit),
            TypeExpr::Map(k, v) => {
                k.traverse(visit)?;
                v.traverse(visit)
            }
            TypeExpr::Optional(inner) => inner.traverse(visit),
        }
    }
}

/// Union 类型的变体分发缓存.
///
/// 使用轻量级 `parking_lot::RwLock` 降低缓存读写开销.
/// Key 为 Python 类型对象的地址 (usize), Value 为对应变体在 `TypeExpr::Union` 中的索引.
#[derive(Default)]
pub struct UnionCache {
    map: RwLock<FxHashMap<usize, usize>>,
}

impl std::fmt::Debug for UnionCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnionCache").finish_non_exhaustive()
    }
}

impl Clone for UnionCache {
    /// 克隆时清空缓存, 确保新实例从头开始学习.
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl PartialEq for UnionCache {
    /// 缓存内容不参与类型语义相等性判断.
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl UnionCache {
    /// 获取指定类型在变体列表中的索引.
    pub fn get(&self, type_ptr: usize) -> Option<usize> {
        self.map.read().get(&type_ptr).copied()
    }

    /// 记录类型与变体索引的映射关系.
    pub fn insert(&self, type_ptr: usize, idx: usize) {
        self.map.write().insert(type_ptr, idx);
    }
}

#[derive(Debug)]
pub struct Constraints {
    pub gt: Option<f64>,
    pub lt: Option<f64>,
    pub ge: Option<f64>,
    pub le: Option<f64>,
    pub min_len: Option<usize>,
    pub max_len: Option<usize>,
    /// Python 正则对象 (re.Pattern).
    pub pattern: Option<Py<PyAny>>,
}

#[derive(Debug)]
pub struct FieldDef {
    pub name: String,
    /// 字段名对应的 Python 字符串(已 intern),用于热点路径复用.
    pub name_py: Py<PyString>,
    pub tag: u8,
    pub ty: TypeExpr,
    pub default_value: Option<Py<PyAny>>,
    pub default_factory: Option<Py<PyAny>>,
    pub is_optional: bool,
    pub is_required: bool,
    pub init: bool,
    pub wrap_simplelist: bool,
    pub constraints: Option<Box<Constraints>>,
}

#[derive(Debug)]
pub struct StructMetaData {
    pub name_to_index: HashMap<String, usize>,
    pub name_ptr_to_index: HashMap<usize, usize>,
}

#[derive(Debug)]
pub struct StructDef {
    pub class_ptr: usize,
    pub name: String,
    pub fields_sorted: Vec<FieldDef>,
    pub tag_lookup_vec: Vec<Option<usize>>,
    pub meta: Arc<StructMetaData>,
    pub frozen: bool,
    pub order: bool,
    pub forbid_unknown_tags: bool,
    pub eq: bool,
    pub omit_defaults: bool,
    pub repr_omit_defaults: bool,
    pub kw_only: bool,
    pub dict: bool,
    pub weakref: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct SchemaConfig {
    pub frozen: bool,
    pub order: bool,
    pub forbid_unknown_tags: bool,
    pub eq: bool,
    pub omit_defaults: bool,
    pub repr_omit_defaults: bool,
    pub kw_only: bool,
    pub dict: bool,
    pub weakref: bool,
}

#[pyclass(module = "tarsio._core")]
pub struct StructConfig {
    #[pyo3(get)]
    pub frozen: bool,
    #[pyo3(get)]
    pub eq: bool,
    #[pyo3(get)]
    pub order: bool,
    #[pyo3(get)]
    pub kw_only: bool,
    #[pyo3(get)]
    pub repr_omit_defaults: bool,
    #[pyo3(get)]
    pub omit_defaults: bool,
    #[pyo3(get)]
    pub weakref: bool,
    #[pyo3(get)]
    pub dict: bool,
    #[pyo3(get)]
    pub rename: Option<Py<PyAny>>,
}

impl StructConfig {
    pub fn from_schema_config(config: &SchemaConfig) -> Self {
        StructConfig {
            frozen: config.frozen,
            eq: config.eq,
            order: config.order,
            kw_only: config.kw_only,
            repr_omit_defaults: config.repr_omit_defaults,
            omit_defaults: config.omit_defaults,
            weakref: config.weakref,
            dict: config.dict,
            rename: None,
        }
    }
}

/// 字段元数据与约束定义.
#[pyclass(module = "tarsio._core")]
pub struct Meta {
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
    #[pyo3(signature=(gt=None, lt=None, ge=None, le=None, min_len=None, max_len=None, pattern=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        gt: Option<f64>,
        lt: Option<f64>,
        ge: Option<f64>,
        le: Option<f64>,
        min_len: Option<usize>,
        max_len: Option<usize>,
        pattern: Option<String>,
    ) -> Self {
        Self {
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

/// `field` 默认值哨兵类型.
#[pyclass(module = "tarsio._core", name = "_NoDefaultType")]
pub struct NoDefaultType;

#[pymethods]
impl NoDefaultType {
    fn __repr__(&self) -> &'static str {
        "NODEFAULT"
    }
}

/// 字段默认值规格（内部使用）.
#[pyclass(module = "tarsio._core", name = "_FieldSpec")]
pub struct FieldSpec {
    pub tag: Option<u8>,
    pub has_default: bool,
    pub default_value: Option<Py<PyAny>>,
    pub default_factory: Option<Py<PyAny>>,
    pub wrap_simplelist: bool,
}

/// 获取 `NODEFAULT` 单例.
pub fn nodefault_singleton(py: Python<'_>) -> PyResult<Py<PyAny>> {
    let core = py.import("tarsio._core")?;
    Ok(core.getattr("NODEFAULT")?.unbind())
}

/// 判断对象是否为 `NODEFAULT` 哨兵.
pub fn is_nodefault(obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    let py = obj.py();
    let nodefault = nodefault_singleton(py)?;
    Ok(obj.is(nodefault.bind(py)))
}

/// 创建字段默认值规格.
///
/// Returns:
///     内部 `_FieldSpec` 对象。
///
/// Raises:
///     TypeError: 参数非法、未知关键字、或默认值与工厂冲突时抛出。
#[pyfunction(signature = (**kwargs))]
pub fn field(py: Python<'_>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Py<FieldSpec>> {
    let nodefault = nodefault_singleton(py)?;
    let nodefault_ref = nodefault.bind(py);
    let mut tag: Option<u8> = None;
    let mut default_value: Option<Py<PyAny>> = None;
    let mut default_factory: Option<Py<PyAny>> = None;
    let mut wrap_simplelist = false;

    if let Some(k) = kwargs {
        for (key, value) in k.iter() {
            let key_str: String = key.extract().map_err(|_| {
                pyo3::exceptions::PyTypeError::new_err("field() keyword names must be strings")
            })?;
            match key_str.as_str() {
                "tag" => {
                    let int_tag = value.extract::<i64>().map_err(|_| {
                        pyo3::exceptions::PyTypeError::new_err(
                            "field() 'tag' must be an integer in range 0..=255",
                        )
                    })?;
                    if !(0..=255).contains(&int_tag) {
                        return Err(pyo3::exceptions::PyTypeError::new_err(
                            "field() 'tag' must be an integer in range 0..=255",
                        ));
                    }
                    tag = Some(int_tag as u8);
                }
                "default" => {
                    if !value.is(nodefault_ref) {
                        default_value = Some(value.unbind());
                    }
                }
                "default_factory" => {
                    if !value.is(nodefault_ref) {
                        default_factory = Some(value.unbind());
                    }
                }
                "wrap_simplelist" => {
                    wrap_simplelist = value.extract::<bool>().map_err(|_| {
                        pyo3::exceptions::PyTypeError::new_err(
                            "field() 'wrap_simplelist' must be a boolean",
                        )
                    })?;
                }
                _ => {
                    return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                        "field() got an unexpected keyword argument '{}'",
                        key_str
                    )));
                }
            }
        }
    }

    if default_value.is_some() && default_factory.is_some() {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "field() cannot specify both 'default' and 'default_factory'",
        ));
    }

    if let Some(factory) = default_factory.as_ref()
        && !factory.bind(py).is_callable()
    {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "field() 'default_factory' must be a callable",
        ));
    }

    let has_default = default_value.is_some() || default_factory.is_some();
    Py::new(
        py,
        FieldSpec {
            tag,
            has_default,
            default_value,
            default_factory,
            wrap_simplelist,
        },
    )
}

#[pyclass(subclass, weakref, module = "tarsio._core", name = "_StructBase")]
pub struct Struct;

#[pyclass(
    subclass,
    extends = PyDict,
    module = "tarsio._core",
    name = "TarsDict"
)]
pub struct TarsDict;

pub const SCHEMA_ATTR: &str = "__tarsio_schema__";

thread_local! {
    // 线程内 schema 缓存,用于减少高频 getattr 开销。
    // 使用 Weak 引用，避免循环引用导致的内存泄漏。
    pub static SCHEMA_CACHE: RefCell<FxHashMap<usize, Weak<StructDef>>> = RefCell::new(FxHashMap::default());
}

#[pyclass(module = "tarsio._core", name = "Schema")]
pub struct Schema {
    pub def: Arc<StructDef>,
}

#[pymethods]
impl Schema {
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        for field in &self.def.fields_sorted {
            visit.call(&field.name_py)?;
            if let Some(v) = &field.default_value {
                visit.call(v)?;
            }
            if let Some(v) = &field.default_factory {
                visit.call(v)?;
            }
            if let Some(constraints) = &field.constraints
                && let Some(pattern) = &constraints.pattern
            {
                visit.call(pattern)?;
            }
            field.ty.traverse(&visit)?;
        }
        Ok(())
    }
}
