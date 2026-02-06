#![allow(
    clippy::collapsible_if,
    clippy::needless_borrows_for_generic_args,
    clippy::needless_match,
    clippy::manual_map
)]
use crate::binding::meta::Meta;
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::pyclass::CompareOp;
use pyo3::types::{PyAny, PyDict, PyList, PyString, PyTuple, PyType};
use regex::Regex;
use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::c_void;
use std::sync::Arc;

// ==========================================
// [L2] 线级中间表示:物理层(面向编解码)
// ==========================================

#[derive(Debug, Clone, PartialEq)]
pub enum WireType {
    Int,
    Long,
    Float,
    Double,
    String,
    Struct(usize),
    List(Box<WireType>),
    Map(Box<WireType>, Box<WireType>),
}

// ==========================================
// [L1] 语义中间表示:语义层(面向结构定义)
// ==========================================

#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpr {
    Primitive(WireType),
    Struct(usize),
    List(Box<TypeExpr>),
    Tuple(Box<TypeExpr>),
    Map(Box<TypeExpr>, Box<TypeExpr>),
    Optional(Box<TypeExpr>),
}

impl TypeExpr {
    pub fn lower(&self) -> WireType {
        match self {
            TypeExpr::Primitive(w) => w.clone(),
            TypeExpr::Struct(ptr) => WireType::Struct(*ptr),
            TypeExpr::List(inner) => WireType::List(Box::new(inner.lower())),
            TypeExpr::Tuple(inner) => WireType::List(Box::new(inner.lower())),
            TypeExpr::Map(k, v) => WireType::Map(Box::new(k.lower()), Box::new(v.lower())),
            TypeExpr::Optional(inner) => inner.lower(),
        }
    }

    pub fn is_optional(&self) -> bool {
        matches!(self, TypeExpr::Optional(_))
    }
}

// ==========================================
// 结构定义
// ==========================================

#[derive(Debug, Clone)]
pub struct Constraints {
    pub gt: Option<f64>,
    pub lt: Option<f64>,
    pub ge: Option<f64>,
    pub le: Option<f64>,
    pub min_len: Option<usize>,
    pub max_len: Option<usize>,
    pub pattern: Option<Regex>,
}

fn resolve_meta_constraints(
    field_name: &str,
    meta: &Meta,
) -> PyResult<(u8, Option<Box<Constraints>>)> {
    let tag = meta.tag.ok_or_else(|| {
        pyo3::exceptions::PyTypeError::new_err(format!(
            "Meta object must include 'tag' for field '{}'",
            field_name
        ))
    })?;

    let has_constraints = meta.gt.is_some()
        || meta.lt.is_some()
        || meta.ge.is_some()
        || meta.le.is_some()
        || meta.min_len.is_some()
        || meta.max_len.is_some()
        || meta.pattern.is_some();

    let constraints = if has_constraints {
        let pattern = meta
            .pattern
            .as_deref()
            .map(Regex::new)
            .transpose()
            .map_err(|e| {
                pyo3::exceptions::PyTypeError::new_err(format!(
                    "Invalid regex pattern for field '{}': {}",
                    field_name, e
                ))
            })?;
        Some(Box::new(Constraints {
            gt: meta.gt,
            lt: meta.lt,
            ge: meta.ge,
            le: meta.le,
            min_len: meta.min_len,
            max_len: meta.max_len,
            pattern,
        }))
    } else {
        None
    };

    Ok((tag, constraints))
}

#[derive(Debug)]
pub struct FieldDef {
    pub name: String,
    pub tag: u8,
    pub ty: TypeExpr,
    pub wire_type: WireType,
    pub default_value: Option<Py<PyAny>>,
    pub is_optional: bool,
    pub is_required: bool,
    pub constraints: Option<Box<Constraints>>,
}

#[derive(Debug)]
pub struct StructDef {
    pub class: Arc<Py<PyType>>,
    pub fields_sorted: Vec<FieldDef>,
    pub tag_lookup_vec: Vec<Option<usize>>,
    pub name_to_tag: HashMap<String, u8>,
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

impl StructDef {
    /// 绑定类到 Python 解释器并返回绑定引用.
    pub fn bind_class<'py>(&self, py: Python<'py>) -> Bound<'py, PyType> {
        self.class.bind(py).clone()
    }
}

pub const SCHEMA_ATTR: &str = "__tarsio_schema__";

const SCHEMA_CAPSULE_NAME: &CStr = c"tarsio.Schema";

unsafe extern "C" fn schema_capsule_destructor(capsule: *mut ffi::PyObject) {
    let ptr = unsafe { ffi::PyCapsule_GetPointer(capsule, SCHEMA_CAPSULE_NAME.as_ptr()) };
    if ptr.is_null() {
        return;
    }
    let boxed = unsafe { Box::from_raw(ptr as *mut Arc<StructDef>) };
    drop(boxed);
}

fn schema_to_capsule(py: Python<'_>, def: Arc<StructDef>) -> PyResult<Py<PyAny>> {
    let boxed = Box::new(def);
    let ptr = Box::into_raw(boxed) as *mut c_void;
    unsafe {
        let obj = ffi::PyCapsule_New(
            ptr,
            SCHEMA_CAPSULE_NAME.as_ptr(),
            Some(schema_capsule_destructor),
        );
        if obj.is_null() {
            return Err(PyErr::fetch(py));
        }
        Ok(Bound::from_owned_ptr(py, obj).unbind())
    }
}

fn schema_from_capsule(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<Option<Arc<StructDef>>> {
    unsafe {
        if ffi::PyCapsule_CheckExact(obj.as_ptr()) == 0 {
            return Ok(None);
        }
        let ptr = ffi::PyCapsule_GetPointer(obj.as_ptr(), SCHEMA_CAPSULE_NAME.as_ptr());
        if ptr.is_null() {
            return Err(PyErr::fetch(py));
        }
        let arc = &*(ptr as *mut Arc<StructDef>);
        Ok(Some(Arc::clone(arc)))
    }
}

pub fn schema_from_class<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
) -> PyResult<Option<Arc<StructDef>>> {
    if let Ok(schema_obj) = cls.getattr(SCHEMA_ATTR) {
        if let Some(def) = schema_from_capsule(py, &schema_obj)? {
            return Ok(Some(def));
        }
    }
    Ok(None)
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

#[pyclass]
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
    pub array_like: bool,
    #[pyo3(get)]
    pub gc: bool,
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
    fn from_schema_config(config: &SchemaConfig) -> PyResult<Self> {
        Ok(StructConfig {
            frozen: config.frozen,
            eq: config.eq,
            order: config.order,
            kw_only: config.kw_only,
            array_like: false,
            gc: true,
            repr_omit_defaults: config.repr_omit_defaults,
            omit_defaults: config.omit_defaults,
            weakref: config.weakref,
            dict: config.dict,
            rename: None,
        })
    }
}

pub fn compile_schema_for_class<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
    config: SchemaConfig,
) -> PyResult<Option<Arc<StructDef>>> {
    if let Ok(params) = cls.getattr("__parameters__") {
        if let Ok(tuple) = params.cast::<PyTuple>() {
            if !tuple.is_empty() {
                return Ok(None);
            }
        }
    }

    let typing = py.import("typing")?;
    let get_origin = typing.getattr("get_origin")?;
    let get_args = typing.getattr("get_args")?;

    let mut generic_origin: Option<Bound<'_, PyAny>> = None;
    let mut generic_args: Option<Bound<'_, PyTuple>> = None;
    if let Ok(origin) = cls.getattr("__origin__") {
        if let Ok(args) = cls.getattr("__args__") {
            if let Ok(args_tuple) = args.cast::<PyTuple>() {
                generic_origin = Some(origin);
                generic_args = Some(args_tuple.clone());
            }
        }
    }
    if generic_origin.is_none() {
        if let Ok(orig_bases) = cls.getattr("__orig_bases__") {
            if let Ok(bases) = orig_bases.cast::<PyTuple>() {
                for base in bases.iter() {
                    if let Ok(origin) = base.getattr("__origin__") {
                        if let Ok(args) = base.getattr("__args__") {
                            if let Ok(args_tuple) = args.cast::<PyTuple>() {
                                generic_origin = Some(origin);
                                generic_args = Some(args_tuple.clone());
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    let mut typevar_map = HashMap::new();
    if let (Some(origin), Some(args_tuple)) = (generic_origin.as_ref(), generic_args.as_ref()) {
        if let Ok(params) = origin.getattr("__parameters__") {
            let params_tuple = params.cast::<PyTuple>()?;
            for (param, arg) in params_tuple.iter().zip(args_tuple.iter()) {
                typevar_map.insert(param.as_ptr() as usize, arg);
            }
        }
    }

    let get_type_hints = typing.getattr("get_type_hints")?;

    let localns = PyDict::new(py);
    localns.set_item(cls.getattr("__name__")?, cls)?;

    let kwargs = PyDict::new(py);
    kwargs.set_item("include_extras", true)?;
    kwargs.set_item("localns", &localns)?;

    let mut hints = get_type_hints
        .call((cls,), Some(&kwargs))?
        .cast::<PyDict>()?
        .clone();

    if hints.is_empty() {
        if let Some(origin) = generic_origin.as_ref() {
            if let Ok(origin_name) = origin.getattr("__name__") {
                localns.set_item(origin_name, &origin)?;
            }
            hints = get_type_hints
                .call((origin,), Some(&kwargs))?
                .cast::<PyDict>()?
                .clone();
        }
    }

    if hints.is_empty() {
        return Ok(None);
    }

    let mut fields: Vec<FieldDef> = Vec::new();
    let mut tags_seen = HashMap::new();

    for (name, type_hint) in hints.iter() {
        let name = name.extract::<String>()?;
        if name.starts_with("__") {
            continue;
        }

        let origin = get_origin.call1((&type_hint,))?;

        let is_annotated = if let Ok(annotated) = typing.getattr("Annotated") {
            origin.is(&annotated)
        } else {
            false
        };

        if !is_annotated {
            continue;
        }

        let args = get_args.call1((&type_hint,))?;
        let args_len = args.len()?;
        if args_len < 2 {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "Missing tag for field '{}'",
                name
            )));
        }

        let real_type = args.get_item(0)?;
        let mut found_int_tag: Option<u8> = None;
        let mut found_meta: Option<Meta> = None;
        for i in 1..args_len {
            let item = args.get_item(i)?;

            if item.is_instance_of::<pyo3::types::PyInt>() {
                let tag = item.extract::<u8>().map_err(|_| {
                    pyo3::exceptions::PyTypeError::new_err(format!(
                        "Tag must be in range 0..=255 for field '{}'",
                        name
                    ))
                })?;
                if found_int_tag.is_some() {
                    return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                        "Multiple integer tags are not allowed for field '{}'",
                        name
                    )));
                }
                found_int_tag = Some(tag);
                continue;
            }

            if item.is_instance_of::<Meta>() {
                if found_meta.is_some() {
                    return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                        "Multiple Meta objects are not allowed for field '{}'",
                        name
                    )));
                }
                let meta_ref: pyo3::PyRef<'_, Meta> = item.extract()?;
                found_meta = Some(Meta {
                    tag: meta_ref.tag,
                    gt: meta_ref.gt,
                    lt: meta_ref.lt,
                    ge: meta_ref.ge,
                    le: meta_ref.le,
                    min_len: meta_ref.min_len,
                    max_len: meta_ref.max_len,
                    pattern: meta_ref.pattern.clone(),
                });
                continue;
            }
        }

        if found_int_tag.is_some() && found_meta.is_some() {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "Do not mix integer tag and Meta object",
            ));
        }

        let (tag, constraints) = if let Some(tag) = found_int_tag {
            (tag, None)
        } else if let Some(meta) = found_meta {
            resolve_meta_constraints(name.as_str(), &meta)?
        } else {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "Missing tag for field '{}'",
                name
            )));
        };

        if let Some(existing) = tags_seen.get(&tag) {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "Duplicate tag {} in '{}' and '{}'",
                tag, existing, name
            )));
        }
        tags_seen.insert(tag, name.clone());

        let type_expr = parse_type_expr(&real_type, &get_origin, &get_args, &typevar_map)?;

        let wire_type = type_expr.lower();
        let is_optional = type_expr.is_optional();
        let mut default_value = lookup_default_value(cls, name.as_str())?;
        if default_value.is_none() && is_optional {
            default_value = Some(py.None());
        }
        let is_required = !is_optional && default_value.is_none();

        fields.push(FieldDef {
            name: name.clone(),
            tag,
            ty: type_expr,
            wire_type,
            default_value,
            is_optional,
            is_required,
            constraints,
        });
    }

    if fields.is_empty() {
        return Ok(None);
    }

    fields.sort_by_key(|f| f.tag);

    let mut name_to_tag = HashMap::new();
    let mut max_tag = 0;

    for f in &fields {
        name_to_tag.insert(f.name.clone(), f.tag);
        if f.tag > max_tag {
            max_tag = f.tag;
        }
    }

    let mut tag_lookup_vec = vec![None; (max_tag as usize) + 1];
    for (idx, f) in fields.iter().enumerate() {
        tag_lookup_vec[f.tag as usize] = Some(idx);
    }

    let def = StructDef {
        class: Arc::new(cls.clone().unbind()),
        fields_sorted: fields,
        tag_lookup_vec,
        name_to_tag,
        frozen: config.frozen,
        order: config.order,
        forbid_unknown_tags: config.forbid_unknown_tags,
        eq: config.eq,
        omit_defaults: config.omit_defaults,
        repr_omit_defaults: config.repr_omit_defaults,
        kw_only: config.kw_only,
        dict: config.dict,
        weakref: config.weakref,
    };

    let def = Arc::new(def);
    let capsule = schema_to_capsule(py, Arc::clone(&def))?;
    cls.setattr(SCHEMA_ATTR, capsule)?;

    let mut field_names = Vec::with_capacity(def.fields_sorted.len());
    for field in &def.fields_sorted {
        field_names.push(field.name.as_str().into_pyobject(py)?.into_any().unbind());
    }
    let fields_tuple = PyTuple::new(py, field_names)?;
    cls.setattr("__struct_fields__", fields_tuple)?;

    let struct_config = Py::new(py, StructConfig::from_schema_config(&config)?)?;
    cls.setattr("__struct_config__", struct_config)?;

    let signature = build_signature(py, &def, &config)?;
    cls.setattr("__signature__", signature)?;
    Ok(Some(def))
}

fn build_signature(py: Python<'_>, def: &StructDef, config: &SchemaConfig) -> PyResult<Py<PyAny>> {
    let inspect = py.import("inspect")?;
    let param_cls = inspect.getattr("Parameter")?;
    let sig_cls = inspect.getattr("Signature")?;
    let params = PyList::empty(py);
    let mut seen_default = false;

    for field in &def.fields_sorted {
        let kwargs = PyDict::new(py);
        let mut has_default = false;

        if let Some(default_val) = field.default_value.as_ref() {
            kwargs.set_item("default", default_val.bind(py))?;
            has_default = true;
        } else if field.is_optional {
            kwargs.set_item("default", py.None())?;
            has_default = true;
        }

        let kind = if config.kw_only || (seen_default && !has_default) {
            param_cls.getattr("KEYWORD_ONLY")?
        } else {
            param_cls.getattr("POSITIONAL_OR_KEYWORD")?
        };

        if has_default {
            seen_default = true;
        }

        let param = if kwargs.is_empty() {
            param_cls.call1((field.name.as_str(), kind))?
        } else {
            param_cls.call((field.name.as_str(), kind), Some(&kwargs))?
        };
        params.append(param)?;
    }

    let sig = sig_cls.call1((params,))?;
    Ok(sig.unbind())
}

// ==========================================
// Python 类绑定
// ==========================================

/// Tarsio 的 Struct 基类.
///
/// 继承该类会在类创建时编译并注册 Schema, 字段使用 `typing.Annotated[T, tag]` 声明.
///
/// Examples:
///     ```python
///     from typing import Annotated
///     from tarsio import Struct
///
///     class User(Struct):
///         uid: Annotated[int, 0]
///         name: Annotated[str, 1]
///     ```
///
/// Notes:
///     解码时, wire 缺失字段会使用模型默认值; Optional 字段未显式赋默认值时视为 None.
#[pyclass(
    subclass,
    module = "tarsio._core",
    name = "_StructBase",
    freelist = 1000
)]
pub struct Struct;

#[pymethods]
impl Struct {
    #[new]
    #[pyo3(signature = (*_args, **_kwargs))]
    fn new(_args: &Bound<'_, PyTuple>, _kwargs: Option<&Bound<'_, PyDict>>) -> Self {
        Struct
    }

    #[pyo3(signature = (*args, **kwargs))]
    fn __init__(
        slf: &Bound<'_, Struct>,
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        let py = slf.py();
        let cls = slf.get_type();
        let def = schema_from_class(py, &cls)?.ok_or_else(|| {
            let class_name = cls
                .name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|_| "Unknown".to_string());
            pyo3::exceptions::PyTypeError::new_err(format!(
                "Cannot instantiate abstract schema class '{}'",
                class_name
            ))
        })?;

        construct_instance(&def, slf.as_any(), args, kwargs)
    }

    /// 将当前实例编码为 Tars 二进制数据.
    ///
    /// Returns:
    ///     编码后的 bytes.
    ///
    /// Raises:
    ///     ValueError: 缺少必填字段、类型不匹配、或递归深度超过限制.
    fn encode(slf: &Bound<'_, Struct>) -> PyResult<Py<pyo3::types::PyBytes>> {
        let py = slf.py();
        crate::binding::ser::encode_object_to_pybytes(py, slf.as_any())
    }

    /// 将 Tars 二进制数据解码为当前类的实例.
    ///
    /// Args:
    ///     data: 待解码的 bytes.
    ///
    /// Returns:
    ///     解码得到的实例.
    ///
    /// Raises:
    ///     TypeError: 目标类未注册 Schema.
    ///     ValueError: 数据格式不正确、缺少必填字段、或递归深度超过限制.
    #[classmethod]
    fn decode<'py>(cls: &Bound<'py, PyType>, data: &[u8]) -> PyResult<Bound<'py, PyAny>> {
        let py = cls.py();
        crate::binding::de::decode_object(py, cls, data)
    }

    fn __copy__(slf: &Bound<'_, Struct>) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        let cls = slf.get_type();
        let def = schema_from_class(py, &cls)?.ok_or_else(|| {
            pyo3::exceptions::PyTypeError::new_err("Schema not found during copy")
        })?;

        let instance = unsafe {
            let type_ptr = cls.as_ptr() as *mut ffi::PyTypeObject;
            let obj_ptr = ffi::PyType_GenericAlloc(type_ptr, 0);
            if obj_ptr.is_null() {
                return Err(PyErr::fetch(py));
            }
            Bound::from_owned_ptr(py, obj_ptr)
        };

        for field in &def.fields_sorted {
            let val = match slf.getattr(field.name.as_str()) {
                Ok(v) => v,
                Err(_) => {
                    if let Some(default_value) = field.default_value.as_ref() {
                        default_value.bind(py).clone()
                    } else if field.is_optional {
                        py.None().into_bound(py)
                    } else if field.is_required {
                        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                            "Missing required field '{}' during copy",
                            field.name
                        )));
                    } else {
                        continue;
                    }
                }
            };

            unsafe {
                let name_py = field.name.as_str().into_pyobject(py)?;
                let res =
                    ffi::PyObject_GenericSetAttr(instance.as_ptr(), name_py.as_ptr(), val.as_ptr());
                if res != 0 {
                    return Err(PyErr::fetch(py));
                }
            }
        }

        Ok(instance.unbind())
    }

    fn __repr__(slf: &Bound<'_, Struct>) -> PyResult<String> {
        let py = slf.py();
        let cls = slf.get_type();
        let class_name = cls.name()?.extract::<String>()?;

        // 尝试获取 Schema
        let def = match schema_from_class(py, &cls)? {
            Some(d) => d,
            None => return Ok(format!("{}()", class_name)),
        };

        let mut fields_str = Vec::new();
        for field in &def.fields_sorted {
            let val = match slf.getattr(field.name.as_str()) {
                Ok(v) => v,
                Err(_) => continue, // Skip missing fields
            };

            // 使用 Python 的 repr() 获取值的字符串表示
            if def.repr_omit_defaults {
                if let Some(default_val) = &field.default_value {
                    if val.eq(default_val)? {
                        continue;
                    }
                }
            }
            let val_repr = val.repr()?.extract::<String>()?;
            fields_str.push(format!("{}={}", field.name, val_repr));
        }

        Ok(format!("{}({})", class_name, fields_str.join(", ")))
    }

    fn __richcmp__(
        slf: &Bound<'_, Struct>,
        other: &Bound<'_, PyAny>,
        op: CompareOp,
    ) -> PyResult<Py<PyAny>> {
        let py = slf.py();
        match op {
            CompareOp::Eq => {
                if !other.is_instance_of::<Struct>() {
                    return Ok(false.into_pyobject(py)?.to_owned().into_any().unbind());
                }

                let cls1 = slf.get_type();
                let cls2 = other.get_type();
                let def = match schema_from_class(py, &cls1)? {
                    Some(d) => d,
                    None => return Ok(false.into_pyobject(py)?.to_owned().into_any().unbind()),
                };

                if !def.eq {
                    return Ok(py.NotImplemented());
                }

                if !cls1.is(&cls2) {
                    return Ok(false.into_pyobject(py)?.to_owned().into_any().unbind());
                }

                for field in &def.fields_sorted {
                    let v1 = slf.getattr(field.name.as_str())?;
                    let v2 = other.getattr(field.name.as_str())?;
                    if !v1.eq(v2)? {
                        return Ok(false.into_pyobject(py)?.to_owned().into_any().unbind());
                    }
                }
                Ok(true.into_pyobject(py)?.to_owned().into_any().unbind())
            }
            CompareOp::Ne => {
                let eq = Self::__richcmp__(slf, other, CompareOp::Eq)?;
                if eq.bind(py).is(&py.NotImplemented()) {
                    return Ok(py.NotImplemented());
                }
                let is_eq: bool = eq.bind(py).extract()?;
                Ok((!is_eq).into_pyobject(py)?.to_owned().into_any().unbind())
            }
            CompareOp::Lt | CompareOp::Le | CompareOp::Gt | CompareOp::Ge => {
                if !other.is_instance_of::<Struct>() {
                    return Ok(py.NotImplemented());
                }

                let cls1 = slf.get_type();
                let cls2 = other.get_type();
                let def = match schema_from_class(py, &cls1)? {
                    Some(d) => d,
                    None => return Ok(py.NotImplemented()),
                };

                if !def.order {
                    return Ok(py.NotImplemented());
                }

                if !cls1.is(&cls2) {
                    return Ok(py.NotImplemented());
                }

                let mut vals1 = Vec::with_capacity(def.fields_sorted.len());
                let mut vals2 = Vec::with_capacity(def.fields_sorted.len());
                for field in &def.fields_sorted {
                    vals1.push(slf.getattr(field.name.as_str())?);
                    vals2.push(other.getattr(field.name.as_str())?);
                }
                let t1 = PyTuple::new(py, vals1)?;
                let t2 = PyTuple::new(py, vals2)?;
                t1.rich_compare(t2, op)
                    .map(|v| v.to_owned().into_any().unbind())
            }
        }
    }

    fn __hash__(slf: &Bound<'_, Struct>) -> PyResult<isize> {
        let py = slf.py();
        let cls = slf.get_type();
        let def = match schema_from_class(py, &cls)? {
            Some(d) => d,
            None => {
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "unhashable type: '{}'",
                    cls.name()?
                )));
            }
        };

        if !def.frozen {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "unhashable type: '{}' (not frozen)",
                cls.name()?
            )));
        }

        let mut vals = Vec::with_capacity(def.fields_sorted.len());
        for field in &def.fields_sorted {
            vals.push(slf.getattr(field.name.as_str())?);
        }
        let tuple = PyTuple::new(py, vals)?;
        tuple.hash()
    }

    fn __setattr__(
        slf: &Bound<'_, Struct>,
        name: Bound<'_, PyAny>,
        value: Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let cls = slf.get_type();
        if let Some(def) = schema_from_class(slf.py(), &cls)? {
            if def.frozen {
                return Err(pyo3::exceptions::PyAttributeError::new_err(format!(
                    "can't set attributes of frozen instance '{}'",
                    cls.name()?
                )));
            }
        }
        unsafe {
            let res =
                pyo3::ffi::PyObject_GenericSetAttr(slf.as_ptr(), name.as_ptr(), value.as_ptr());
            if res != 0 {
                return Err(PyErr::fetch(slf.py()));
            }
        }
        Ok(())
    }
}

// ==========================================
// 构造器逻辑
// ==========================================

pub fn construct_instance(
    def: &StructDef,
    self_obj: &Bound<'_, PyAny>,
    args: &Bound<'_, PyTuple>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    // 位置参数按字段顺序映射到 args
    let num_positional = args.len();
    let given_args = args;

    if def.kw_only && num_positional > 0 {
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "__init__() takes 0 positional arguments but {} were given",
            num_positional
        )));
    }

    if num_positional > def.fields_sorted.len() {
        let expected = def.fields_sorted.len() + 1;
        let given = num_positional + 1;
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "__init__() takes {} positional arguments but {} were given",
            expected, given
        )));
    }

    for (i, field) in def.fields_sorted.iter().enumerate() {
        let val = if i < num_positional {
            // 位置参数提供
            // 检查与 kwargs 冲突
            if let Some(k) = kwargs {
                if k.contains(&field.name)? {
                    return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                        "__init__() got multiple values for argument '{}'",
                        field.name
                    )));
                }
            }
            Some(given_args.get_item(i)?)
        } else {
            // 从关键字参数中读取
            if let Some(k) = kwargs {
                match k.get_item(&field.name)? {
                    Some(v) => Some(v),
                    None => None,
                }
            } else {
                None
            }
        };

        let val_to_set = match val {
            Some(v) => v,
            None => {
                if let Some(default_value) = field.default_value.as_ref() {
                    default_value.bind(self_obj.py()).clone()
                } else if field.is_optional {
                    pyo3::types::PyNone::get(self_obj.py())
                        .to_owned()
                        .into_any()
                } else if field.is_required {
                    return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                        "__init__() missing 1 required positional argument: '{}'",
                        field.name
                    )));
                } else {
                    // 理论上被 required 校验覆盖,这里作为安全兜底
                    return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                        "__init__() missing 1 required argument: '{}'",
                        field.name
                    )));
                }
            }
        };

        // 使用 PyObject_GenericSetAttr 绕过 frozen 检查
        unsafe {
            let name_py = field.name.as_str().into_pyobject(self_obj.py())?;
            let res = pyo3::ffi::PyObject_GenericSetAttr(
                self_obj.as_ptr(),
                name_py.as_ptr(),
                val_to_set.as_ptr(),
            );
            if res != 0 {
                return Err(PyErr::fetch(self_obj.py()));
            }
        }
    }

    // 检查未声明的关键字参数
    if let Some(k) = kwargs {
        for key in k.keys() {
            let key_str = key.extract::<String>()?;
            if !def.name_to_tag.contains_key(&key_str) {
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "__init__() got an unexpected keyword argument '{}'",
                    key_str
                )));
            }
        }
    }

    Ok(())
}

fn lookup_default_value(cls: &Bound<'_, PyType>, field_name: &str) -> PyResult<Option<Py<PyAny>>> {
    let py = cls.py();
    let types_mod = py.import("types")?;
    let member_descriptor_type = types_mod.getattr("MemberDescriptorType")?;
    let getset_descriptor_type = types_mod.getattr("GetSetDescriptorType")?;
    let mro_any = cls.getattr("__mro__")?;
    let mro = mro_any.cast::<PyTuple>()?;
    for base in mro.iter() {
        if let Ok(defaults_any) = base.getattr("__tarsio_defaults__") {
            if let Ok(defaults) = defaults_any.cast::<PyDict>() {
                if let Some(v) = defaults.get_item(field_name)? {
                    return Ok(Some(v.unbind()));
                }
            }
        }

        let dict_any = base.getattr("__dict__")?;
        match dict_any.get_item(field_name) {
            Ok(v) => {
                if v.is_instance(&member_descriptor_type)?
                    || v.is_instance(&getset_descriptor_type)?
                {
                    continue;
                }
                return Ok(Some(v.unbind()));
            }
            Err(e) => {
                if e.is_instance_of::<pyo3::exceptions::PyKeyError>(py) {
                    continue;
                }
                return Err(e);
            }
        }
    }
    Ok(None)
}

// ==========================================
// 语法树解析器
// ==========================================

fn parse_type_expr(
    ty: &Bound<'_, PyAny>,
    get_origin: &Bound<'_, PyAny>,
    get_args: &Bound<'_, PyAny>,
    typevar_map: &HashMap<usize, Bound<'_, PyAny>>,
) -> PyResult<TypeExpr> {
    let ptr = ty.as_ptr() as usize;
    let resolved_ty = typevar_map.get(&ptr).unwrap_or(ty);

    if resolved_ty.is_instance_of::<PyString>() {
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "Forward references not supported yet: {}",
            resolved_ty
        )));
    }

    if let Ok(name) = resolved_ty.getattr("__name__") {
        if let Ok(name_str) = name.extract::<String>() {
            match name_str.as_str() {
                "int" => return Ok(TypeExpr::Primitive(WireType::Int)),
                "str" => return Ok(TypeExpr::Primitive(WireType::String)),
                "float" => return Ok(TypeExpr::Primitive(WireType::Double)),
                "bool" => return Ok(TypeExpr::Primitive(WireType::Int)),
                "bytes" => return Ok(TypeExpr::List(Box::new(TypeExpr::Primitive(WireType::Int)))),
                _ => {}
            }
        }
    }

    let origin = get_origin.call1((resolved_ty,))?;
    if !origin.is_none() {
        let original_name: String = origin.getattr("__name__")?.extract()?;
        let args = get_args.call1((resolved_ty,))?;

        if (original_name == "list" || original_name == "tuple") && args.len()? > 0 {
            let inner = args.get_item(0)?;
            let inner_expr = parse_type_expr(&inner, get_origin, get_args, typevar_map)?;
            if original_name == "list" {
                return Ok(TypeExpr::List(Box::new(inner_expr)));
            } else {
                return Ok(TypeExpr::Tuple(Box::new(inner_expr)));
            }
        }

        if original_name == "dict" && args.len()? >= 2 {
            let k = args.get_item(0)?;
            let v = args.get_item(1)?;
            let k_expr = parse_type_expr(&k, get_origin, get_args, typevar_map)?;
            let v_expr = parse_type_expr(&v, get_origin, get_args, typevar_map)?;
            return Ok(TypeExpr::Map(Box::new(k_expr), Box::new(v_expr)));
        }

        if original_name == "Union" || original_name == "UnionType" {
            let len = args.len()?;
            let mut has_none = false;
            let mut inner_types = Vec::new();

            for i in 0..len {
                let arg = args.get_item(i)?;
                let is_none = arg.is_none() || arg.repr()?.to_string().contains("NoneType");
                if is_none {
                    has_none = true;
                } else {
                    inner_types.push(arg);
                }
            }

            if has_none && inner_types.len() == 1 {
                let inner = &inner_types[0];
                let inner_expr = parse_type_expr(inner, get_origin, get_args, typevar_map)?;
                return Ok(TypeExpr::Optional(Box::new(inner_expr)));
            }
        }
    }

    if resolved_ty.hasattr("__module__")? {
        let ptr = resolved_ty.as_ptr() as usize;
        return Ok(TypeExpr::Struct(ptr));
    }

    Err(pyo3::exceptions::PyTypeError::new_err(format!(
        "Unsupported Tars type: {}",
        resolved_ty
    )))
}
