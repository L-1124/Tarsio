use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyModule, PyString, PyTuple, PyType};
use std::collections::{HashMap, HashSet};

use crate::binding::meta::Meta;
use crate::binding::schema::{Struct, TarsDict};

#[derive(Debug)]
pub struct ConstraintsIR {
    pub gt: Option<f64>,
    pub lt: Option<f64>,
    pub ge: Option<f64>,
    pub le: Option<f64>,
    pub min_len: Option<usize>,
    pub max_len: Option<usize>,
    pub pattern: Option<String>,
}

#[derive(Debug)]
pub enum TypeInfoIR {
    Int,
    Str,
    Float,
    Bool,
    Bytes,
    Any,
    NoneType,
    TypedDict,
    NamedTuple(Py<PyType>, Vec<TypeInfoIR>),
    Dataclass(Py<PyType>),
    Set(Box<TypeInfoIR>),
    Enum(Py<PyType>, Box<TypeInfoIR>),
    Union(Vec<TypeInfoIR>),
    List(Box<TypeInfoIR>),
    Tuple(Vec<TypeInfoIR>),
    VarTuple(Box<TypeInfoIR>),
    Map(Box<TypeInfoIR>, Box<TypeInfoIR>),
    Optional(Box<TypeInfoIR>),
    Struct(Py<PyType>),
    TarsDict,
}

#[derive(Debug)]
pub struct FieldInfoIR {
    pub name: String,
    pub tag: u8,
    pub typ: TypeInfoIR,
    pub default_value: Option<Py<PyAny>>,
    pub default_factory: Option<Py<PyAny>>,
    pub has_default: bool,
    pub is_optional: bool,
    pub is_required: bool,
    pub init: bool,
    pub constraints: Option<ConstraintsIR>,
}

struct IntrospectionContext<'py> {
    typing: Bound<'py, PyModule>,
    builtins: Bound<'py, PyModule>,
    types_mod: Bound<'py, PyModule>,
    typing_is_typeddict: Option<Bound<'py, PyAny>>,
    dataclasses_is_dataclass: Option<Bound<'py, PyAny>>,
    annotated: Bound<'py, PyAny>,
    union_origin: Bound<'py, PyAny>,
    forward_ref: Bound<'py, PyAny>,
    typevar_cls: Bound<'py, PyAny>,
    literal_cls: Bound<'py, PyAny>,
    final_cls: Option<Bound<'py, PyAny>>,
    type_alias: Option<Bound<'py, PyAny>>,
    type_alias_types: Vec<Bound<'py, PyAny>>,
    required_cls: Option<Bound<'py, PyAny>>,
    not_required_cls: Option<Bound<'py, PyAny>>,
    any_type: Bound<'py, PyAny>,
    none_type: Bound<'py, PyType>,
    builtin_int: Bound<'py, PyAny>,
    builtin_str: Bound<'py, PyAny>,
    builtin_float: Bound<'py, PyAny>,
    builtin_bool: Bound<'py, PyAny>,
    builtin_bytes: Bound<'py, PyAny>,
    builtin_list: Bound<'py, PyAny>,
    builtin_tuple: Bound<'py, PyAny>,
    builtin_dict: Bound<'py, PyAny>,
    builtin_set: Bound<'py, PyAny>,
    builtin_frozenset: Bound<'py, PyAny>,
    collection_cls: Bound<'py, PyAny>,
    sequence_cls: Bound<'py, PyAny>,
    mutable_sequence_cls: Bound<'py, PyAny>,
    set_cls: Bound<'py, PyAny>,
    mutable_set_cls: Bound<'py, PyAny>,
    mapping_cls: Bound<'py, PyAny>,
    mutable_mapping_cls: Bound<'py, PyAny>,
    union_type: Option<Bound<'py, PyAny>>,
    enum_base: Bound<'py, PyAny>,
}

impl<'py> IntrospectionContext<'py> {
    fn new(py: Python<'py>) -> PyResult<Self> {
        let typing = py.import("typing")?;
        let builtins = py.import("builtins")?;
        let collections_abc = py.import("collections.abc")?;
        let types_mod = py.import("types")?;
        let enum_mod = py.import("enum")?;
        let typing_extensions = py.import("typing_extensions").ok();
        let dataclasses = py.import("dataclasses").ok();

        let annotated = typing.getattr("Annotated")?;
        let union_origin = typing.getattr("Union")?;
        let forward_ref = typing.getattr("ForwardRef")?;
        let typevar_cls = typing.getattr("TypeVar")?;
        let literal_cls = typing.getattr("Literal")?;
        let any_type = typing.getattr("Any")?;

        let typing_is_typeddict = typing_extensions
            .as_ref()
            .and_then(|m| m.getattr("is_typeddict").ok())
            .or_else(|| typing.getattr("is_typeddict").ok());
        let dataclasses_is_dataclass = dataclasses.and_then(|m| m.getattr("is_dataclass").ok());

        let final_cls = typing.getattr("Final").ok();
        let type_alias = typing.getattr("TypeAlias").ok();
        let mut type_alias_types = Vec::new();
        if let Ok(type_alias_type) = typing.getattr("TypeAliasType") {
            type_alias_types.push(type_alias_type);
        }
        if let Some(type_alias_type) = typing_extensions
            .as_ref()
            .and_then(|m| m.getattr("TypeAliasType").ok())
            && !type_alias_types.iter().any(|t| t.is(&type_alias_type))
        {
            type_alias_types.push(type_alias_type);
        }
        let required_cls = typing.getattr("Required").ok().or_else(|| {
            typing_extensions
                .as_ref()
                .and_then(|m| m.getattr("Required").ok())
        });
        let not_required_cls = typing.getattr("NotRequired").ok().or_else(|| {
            typing_extensions
                .as_ref()
                .and_then(|m| m.getattr("NotRequired").ok())
        });

        let none_type = py.None().bind(py).get_type();

        let builtin_int = builtins.getattr("int")?;
        let builtin_str = builtins.getattr("str")?;
        let builtin_float = builtins.getattr("float")?;
        let builtin_bool = builtins.getattr("bool")?;
        let builtin_bytes = builtins.getattr("bytes")?;
        let builtin_list = builtins.getattr("list")?;
        let builtin_tuple = builtins.getattr("tuple")?;
        let builtin_dict = builtins.getattr("dict")?;
        let builtin_set = builtins.getattr("set")?;
        let builtin_frozenset = builtins.getattr("frozenset")?;

        let collection_cls = collections_abc.getattr("Collection")?;
        let sequence_cls = collections_abc.getattr("Sequence")?;
        let mutable_sequence_cls = collections_abc.getattr("MutableSequence")?;
        let set_cls = collections_abc.getattr("Set")?;
        let mutable_set_cls = collections_abc.getattr("MutableSet")?;
        let mapping_cls = collections_abc.getattr("Mapping")?;
        let mutable_mapping_cls = collections_abc.getattr("MutableMapping")?;

        let union_type = types_mod.getattr("UnionType").ok();
        let enum_base = enum_mod.getattr("Enum")?;
        Ok(Self {
            typing,
            builtins,
            types_mod,
            typing_is_typeddict,
            dataclasses_is_dataclass,
            annotated,
            union_origin,
            forward_ref,
            typevar_cls,
            literal_cls,
            final_cls,
            type_alias,
            type_alias_types,
            required_cls,
            not_required_cls,
            any_type,
            none_type,
            builtin_int,
            builtin_str,
            builtin_float,
            builtin_bool,
            builtin_bytes,
            builtin_list,
            builtin_tuple,
            builtin_dict,
            builtin_set,
            builtin_frozenset,
            collection_cls,
            sequence_cls,
            mutable_sequence_cls,
            set_cls,
            mutable_set_cls,
            mapping_cls,
            mutable_mapping_cls,
            union_type,
            enum_base,
        })
    }
}

pub fn introspect_struct_fields<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
) -> PyResult<Option<Vec<FieldInfoIR>>> {
    let ctx = IntrospectionContext::new(py)?;
    if is_generic_template(cls)? {
        return Ok(None);
    }

    if !detect_struct_kind_with_ctx(py, cls, &ctx)? {
        return Ok(None);
    }

    introspect_tars_struct_fields(py, cls, &ctx)
}

pub fn introspect_type_info_ir<'py>(
    py: Python<'py>,
    tp: &Bound<'py, PyAny>,
) -> PyResult<(TypeInfoIR, Option<ConstraintsIR>)> {
    let ctx = IntrospectionContext::new(py)?;
    introspect_type_info_ir_with_ctx(py, tp, &ctx)
}

fn introspect_type_info_ir_with_ctx<'py>(
    py: Python<'py>,
    tp: &Bound<'py, PyAny>,
    ctx: &IntrospectionContext<'py>,
) -> PyResult<(TypeInfoIR, Option<ConstraintsIR>)> {
    let origin = ctx.typing.call_method1("get_origin", (tp,))?;
    if !origin.is_none() && origin.is(&ctx.annotated) {
        let args_any = ctx.typing.call_method1("get_args", (tp,))?;
        let args = args_any.cast::<PyTuple>()?;
        let (real_type, _tag, constraints) = parse_annotated_args("_", args)?;
        let typevar_map = HashMap::new();
        let (typ, _is_optional) = translate_type_info_ir(py, &real_type, &typevar_map, ctx)?;
        return Ok((typ, constraints));
    }

    let typevar_map = HashMap::new();
    let (typ, _is_optional) = translate_type_info_ir(py, tp, &typevar_map, ctx)?;
    Ok((typ, None))
}

type GenericOrigin<'py> = (Option<Bound<'py, PyAny>>, Option<Bound<'py, PyTuple>>);

fn is_generic_template(cls: &Bound<'_, PyType>) -> PyResult<bool> {
    if let Ok(params) = cls.getattr(intern!(cls.py(), "__parameters__"))
        && let Ok(tuple) = params.cast::<PyTuple>()
    {
        return Ok(!tuple.is_empty());
    }
    Ok(false)
}

fn resolve_generic_origin<'py>(cls: &Bound<'py, PyType>) -> PyResult<GenericOrigin<'py>> {
    if let (Ok(origin), Ok(args)) = (
        cls.getattr(intern!(cls.py(), "__origin__")),
        cls.getattr(intern!(cls.py(), "__args__")),
    ) && !origin.is_none()
        && !args.is_none()
    {
        if let Ok(tup) = args.clone().cast_into::<PyTuple>() {
            return Ok((Some(origin), Some(tup)));
        }
        if let Ok(seq) = args.try_iter() {
            let collected: Vec<Py<PyAny>> = seq
                .map(|item| item.map(|v| v.unbind()))
                .collect::<Result<_, _>>()?;
            let tup = PyTuple::new(cls.py(), collected)?;
            return Ok((Some(origin), Some(tup)));
        }
    }

    if let Ok(orig_bases) = cls.getattr(intern!(cls.py(), "__orig_bases__"))
        && let Ok(bases) = orig_bases.cast::<PyTuple>()
    {
        for base in bases.iter() {
            if let (Ok(base_origin), Ok(base_args)) = (
                base.getattr(intern!(cls.py(), "__origin__")),
                base.getattr(intern!(cls.py(), "__args__")),
            ) && !base_origin.is_none()
                && !base_args.is_none()
            {
                if let Ok(tup) = base_args.clone().cast_into::<PyTuple>() {
                    return Ok((Some(base_origin), Some(tup)));
                }
                if let Ok(seq) = base_args.try_iter() {
                    let collected: Vec<Py<PyAny>> = seq
                        .map(|item| item.map(|v| v.unbind()))
                        .collect::<Result<_, _>>()?;
                    let tup = PyTuple::new(cls.py(), collected)?;
                    return Ok((Some(base_origin), Some(tup)));
                }
            }
        }
    }
    Ok((None, None))
}

fn build_typevar_map<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
    ctx: &IntrospectionContext<'py>,
) -> PyResult<HashMap<usize, Bound<'py, PyAny>>> {
    let mut map: HashMap<usize, Bound<'py, PyAny>> = HashMap::new();

    let (origin, args) = resolve_generic_origin(cls)?;
    if let (Some(origin), Some(args)) = (origin, args)
        && let Ok(params_any) = origin.getattr(intern!(py, "__parameters__"))
        && let Ok(params) = params_any.cast::<PyTuple>()
    {
        for (param, arg) in params.iter().zip(args.iter()) {
            map.insert(param.as_ptr() as usize, arg);
        }
        return Ok(map);
    }

    if let Ok(params_any) = cls.getattr(intern!(py, "__parameters__"))
        && let Ok(params) = params_any.cast::<PyTuple>()
    {
        for param in params.iter() {
            if param.is_instance(&ctx.typevar_cls)? {
                map.insert(param.as_ptr() as usize, param);
            }
        }
    }

    Ok(map)
}

fn get_type_hints_with_fallback<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
    ctx: &IntrospectionContext<'py>,
) -> PyResult<Bound<'py, PyDict>> {
    let kwargs = PyDict::new(py);
    kwargs.set_item("include_extras", true)?;

    let localns = PyDict::new(py);
    let cls_name: String = cls.getattr(intern!(py, "__name__"))?.extract()?;
    localns.set_item(cls_name.as_str(), cls)?;
    kwargs.set_item("localns", &localns)?;

    let hints_any = ctx
        .typing
        .call_method("get_type_hints", (cls,), Some(&kwargs))?;
    let mut hints = hints_any.cast::<PyDict>()?.clone();
    if !hints.is_empty() {
        return Ok(hints);
    }

    let (origin, _args) = resolve_generic_origin(cls)?;
    if let Some(origin) = origin
        && let Ok(origin_type) = origin.cast::<PyType>()
    {
        let origin_name: String = origin_type.getattr(intern!(py, "__name__"))?.extract()?;
        localns.set_item(origin_name.as_str(), origin_type)?;
        let origin_hints_any =
            ctx.typing
                .call_method("get_type_hints", (origin_type,), Some(&kwargs))?;
        hints = origin_hints_any.cast::<PyDict>()?.clone();
    }

    Ok(hints)
}

#[inline(always)]
pub fn detect_struct_kind<'py>(py: Python<'py>, cls: &Bound<'py, PyType>) -> PyResult<bool> {
    let ctx = IntrospectionContext::new(py)?;
    detect_struct_kind_with_ctx(py, cls, &ctx)
}

#[inline(always)]
fn detect_struct_kind_with_ctx<'py>(
    _py: Python<'py>,
    cls: &Bound<'py, PyType>,
    _ctx: &IntrospectionContext<'py>,
) -> PyResult<bool> {
    cls.is_subclass_of::<Struct>()
}

#[inline(always)]
fn is_subclass<'py>(
    cls: &Bound<'py, PyType>,
    base: &Bound<'py, PyAny>,
    ctx: &IntrospectionContext<'py>,
) -> PyResult<bool> {
    let issubclass = ctx.builtins.getattr("issubclass")?;
    issubclass.call1((cls, base))?.is_truthy()
}

fn is_instance_of_any<'py>(
    value: &Bound<'py, PyAny>,
    candidates: &[Bound<'py, PyAny>],
) -> PyResult<bool> {
    for candidate in candidates {
        if value.is_instance(candidate)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn is_identity_of_any<'py>(value: &Bound<'py, PyAny>, candidates: &[Bound<'py, PyAny>]) -> bool {
    candidates.iter().any(|candidate| value.is(candidate))
}

fn introspect_tars_struct_fields<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
    ctx: &IntrospectionContext<'py>,
) -> PyResult<Option<Vec<FieldInfoIR>>> {
    let typevar_map = build_typevar_map(py, cls, ctx)?;
    let hints = get_type_hints_with_fallback(py, cls, ctx)?;
    if hints.is_empty() {
        return Ok(None);
    }

    let mut fields = Vec::new();
    let mut tags_seen: Vec<Option<String>> = vec![None; 256];

    for (name_obj, type_hint) in hints.iter() {
        let name: String = name_obj.extract()?;
        if name.starts_with("__") {
            continue;
        }

        let origin = ctx.typing.call_method1("get_origin", (&type_hint,))?;
        if origin.is_none() || !origin.is(&ctx.annotated) {
            continue;
        }

        let args_any = ctx.typing.call_method1("get_args", (&type_hint,))?;
        let args = args_any.cast::<PyTuple>()?;
        let (real_type, tag, constraints) = parse_annotated_args(name.as_str(), args)?;

        if let Some(existing) = tags_seen[tag as usize].as_ref() {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "Duplicate tag {} in '{}' and '{}'",
                tag, existing, name
            )));
        }
        tags_seen[tag as usize] = Some(name.clone());

        let (typ, is_optional) = translate_type_info_ir(py, &real_type, &typevar_map, ctx)?;

        let (has_default, default_val) = lookup_default_value(py, cls, name.as_str(), ctx)?;
        let (has_default, default_value) = if has_default {
            (true, default_val)
        } else {
            (false, None)
        };

        let is_required = !is_optional && default_value.is_none();

        fields.push(FieldInfoIR {
            name,
            tag,
            typ,
            default_value,
            default_factory: None,
            has_default,
            is_optional,
            is_required,
            init: true,
            constraints,
        });
    }

    if fields.is_empty() {
        return Ok(None);
    }
    fields.sort_by_key(|f| f.tag);
    Ok(Some(fields))
}

fn parse_annotated_args<'py>(
    field_name: &str,
    args: &Bound<'py, PyTuple>,
) -> PyResult<(Bound<'py, PyAny>, u8, Option<ConstraintsIR>)> {
    let (real_type, tag, constraints) = parse_annotated_payload(field_name, args, true)?;
    Ok((
        real_type,
        tag.expect("parse_annotated_payload must return tag when required"),
        constraints,
    ))
}

fn parse_annotated_args_loose<'py>(
    field_name: &str,
    args: &Bound<'py, PyTuple>,
) -> PyResult<(Bound<'py, PyAny>, Option<u8>, Option<ConstraintsIR>)> {
    parse_annotated_payload(field_name, args, false)
}

fn parse_annotated_payload<'py>(
    field_name: &str,
    args: &Bound<'py, PyTuple>,
    require_tag: bool,
) -> PyResult<(Bound<'py, PyAny>, Option<u8>, Option<ConstraintsIR>)> {
    if require_tag && args.len() < 2 {
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "Missing tag for field '{}'",
            field_name
        )));
    }

    let real_type = args.get_item(0)?;
    let mut found_int_tag: Option<u8> = None;
    let mut found_meta: Option<PyRef<'py, Meta>> = None;

    for item in args.iter().skip(1) {
        if let Ok(int_tag) = item.extract::<i64>() {
            if !(0..=255).contains(&int_tag) {
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "Tag must be in range 0..=255 for field '{}'",
                    field_name
                )));
            }
            if found_int_tag.is_some() {
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "Multiple integer tags are not allowed for field '{}'",
                    field_name
                )));
            }
            found_int_tag = Some(int_tag as u8);
            continue;
        }

        if let Ok(meta) = item.extract::<PyRef<'py, Meta>>() {
            if found_meta.is_some() {
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "Multiple Meta objects are not allowed for field '{}'",
                    field_name
                )));
            }
            found_meta = Some(meta);
        }
    }

    if found_int_tag.is_some() && found_meta.is_some() {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "Do not mix integer tag and Meta object",
        ));
    }

    if let Some(meta) = found_meta {
        let Some(tag) = meta.tag else {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "Meta object must include 'tag' for field '{}'",
                field_name
            )));
        };

        let constraints = ConstraintsIR {
            gt: meta.gt,
            lt: meta.lt,
            ge: meta.ge,
            le: meta.le,
            min_len: meta.min_len,
            max_len: meta.max_len,
            pattern: meta.pattern.clone(),
        };
        return Ok((real_type, Some(tag), Some(constraints)));
    }

    if require_tag && found_int_tag.is_none() {
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "Missing tag for field '{}'",
            field_name
        )));
    }

    Ok((real_type, found_int_tag, None))
}

fn translate_type_info_ir<'py>(
    py: Python<'py>,
    tp: &Bound<'py, PyAny>,
    typevar_map: &HashMap<usize, Bound<'py, PyAny>>,
    ctx: &IntrospectionContext<'py>,
) -> PyResult<(TypeInfoIR, bool)> {
    let mut resolved = resolve_typevar(py, tp, typevar_map, ctx)?;

    if resolved.is_instance_of::<PyString>() {
        let s: String = resolved.extract()?;
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "Forward references not supported yet: {}",
            s
        )));
    }

    if resolved.is_instance(&ctx.forward_ref)? {
        let repr: String = resolved.repr()?.extract()?;
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "Forward references not supported yet: {}",
            repr
        )));
    }

    let none_type = ctx.none_type.as_any();

    let mut forced_optional = false;

    loop {
        if let Ok(super_type) = resolved.getattr("__supertype__") {
            resolved = super_type;
            continue;
        }

        if is_instance_of_any(&resolved, &ctx.type_alias_types)? {
            if let Ok(value) = resolved.getattr("__value__") {
                resolved = value;
                continue;
            }
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "TypeAliasType requires an inner type",
            ));
        }

        let origin = ctx.typing.call_method1("get_origin", (&resolved,))?;
        if origin.is_none() {
            break;
        }

        if origin.is(&ctx.annotated) {
            let args_any = ctx.typing.call_method1("get_args", (&resolved,))?;
            let args = args_any.cast::<PyTuple>()?;
            let (real_type, _tag, _constraints) = parse_annotated_args_loose("_", args)?;
            resolved = real_type;
            continue;
        }

        if let Some(final_cls) = ctx.final_cls.as_ref()
            && origin.is(final_cls)
        {
            let args_any = ctx.typing.call_method1("get_args", (&resolved,))?;
            let args = args_any.cast::<PyTuple>()?;
            if args.is_empty() {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "Final requires an inner type",
                ));
            }
            resolved = args.get_item(0)?;
            continue;
        }

        if let Some(type_alias) = ctx.type_alias.as_ref()
            && origin.is(type_alias)
        {
            let args_any = ctx.typing.call_method1("get_args", (&resolved,))?;
            let args = args_any.cast::<PyTuple>()?;
            if args.is_empty() {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "TypeAlias requires an inner type",
                ));
            }
            resolved = args.get_item(0)?;
            continue;
        }

        if is_identity_of_any(&origin, &ctx.type_alias_types) {
            let args_any = ctx.typing.call_method1("get_args", (&resolved,))?;
            let args = args_any.cast::<PyTuple>()?;
            if args.is_empty() {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "TypeAliasType requires an inner type",
                ));
            }
            resolved = args.get_item(0)?;
            continue;
        }

        if let Some(required_cls) = ctx.required_cls.as_ref()
            && origin.is(required_cls)
        {
            let args_any = ctx.typing.call_method1("get_args", (&resolved,))?;
            let args = args_any.cast::<PyTuple>()?;
            if args.is_empty() {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "Required requires an inner type",
                ));
            }
            resolved = args.get_item(0)?;
            continue;
        }

        if let Some(not_required_cls) = ctx.not_required_cls.as_ref()
            && origin.is(not_required_cls)
        {
            let args_any = ctx.typing.call_method1("get_args", (&resolved,))?;
            let args = args_any.cast::<PyTuple>()?;
            if args.is_empty() {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "NotRequired requires an inner type",
                ));
            }
            forced_optional = true;
            resolved = args.get_item(0)?;
            continue;
        }

        if origin.is(&ctx.literal_cls) {
            let args_any = ctx.typing.call_method1("get_args", (&resolved,))?;
            let args = args_any.cast::<PyTuple>()?;
            if args.is_empty() {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "Literal requires at least one value",
                ));
            }

            let mut variants = Vec::new();
            let mut seen = HashSet::new();
            let mut has_none = false;

            for val in args.iter() {
                if val.is_none() || val.is(none_type) {
                    has_none = true;
                    continue;
                }
                let val_type = val.get_type();
                let (typ, _opt) = translate_type_info_ir(py, val_type.as_any(), typevar_map, ctx)?;
                let key = format!("{:?}", typ);
                if seen.insert(key) {
                    variants.push(typ);
                }
            }

            if variants.is_empty() {
                return Ok((TypeInfoIR::NoneType, true));
            }
            if has_none {
                forced_optional = true;
            }
            if variants.len() == 1 {
                return Ok((variants.remove(0), forced_optional));
            }
            return Ok((TypeInfoIR::Union(variants), forced_optional));
        }

        let is_union =
            origin.is(&ctx.union_origin) || ctx.union_type.as_ref().is_some_and(|u| origin.is(u));
        if is_union {
            let args_any = ctx.typing.call_method1("get_args", (&resolved,))?;
            let args = args_any.cast::<PyTuple>()?;
            let mut variants = Vec::new();
            let mut has_none = false;
            for a in args.iter() {
                if a.is_none() || a.is(none_type) {
                    has_none = true;
                } else {
                    let (inner, _opt_inner) = translate_type_info_ir(py, &a, typevar_map, ctx)?;
                    variants.push(inner);
                }
            }
            if variants.is_empty() {
                return Ok((TypeInfoIR::NoneType, true));
            }
            if has_none && variants.len() == 1 {
                return Ok((TypeInfoIR::Optional(Box::new(variants.remove(0))), true));
            }
            return Ok((TypeInfoIR::Union(variants), has_none || forced_optional));
        }

        if origin.is(&ctx.collection_cls)
            || origin.is(&ctx.sequence_cls)
            || origin.is(&ctx.mutable_sequence_cls)
        {
            let args_any = ctx.typing.call_method1("get_args", (&resolved,))?;
            let args = args_any.cast::<PyTuple>()?;
            if args.is_empty() {
                let repr: String = resolved.repr()?.extract()?;
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "Unsupported Tars type: {}",
                    repr
                )));
            }
            let (inner, _opt) = translate_type_info_ir(py, &args.get_item(0)?, typevar_map, ctx)?;
            return Ok((TypeInfoIR::List(Box::new(inner)), forced_optional));
        }

        if origin.is(&ctx.set_cls)
            || origin.is(&ctx.mutable_set_cls)
            || origin.is(&ctx.builtin_set)
            || origin.is(&ctx.builtin_frozenset)
        {
            let args_any = ctx.typing.call_method1("get_args", (&resolved,))?;
            let args = args_any.cast::<PyTuple>()?;
            if args.is_empty() {
                let repr: String = resolved.repr()?.extract()?;
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "Unsupported Tars type: {}",
                    repr
                )));
            }
            let (inner, _opt) = translate_type_info_ir(py, &args.get_item(0)?, typevar_map, ctx)?;
            return Ok((TypeInfoIR::Set(Box::new(inner)), forced_optional));
        }

        if origin.is(&ctx.mapping_cls) || origin.is(&ctx.mutable_mapping_cls) {
            let args_any = ctx.typing.call_method1("get_args", (&resolved,))?;
            let args = args_any.cast::<PyTuple>()?;
            if args.len() < 2 {
                let repr: String = resolved.repr()?.extract()?;
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "Unsupported Tars type: {}",
                    repr
                )));
            }
            let (k, _opt_k) = translate_type_info_ir(py, &args.get_item(0)?, typevar_map, ctx)?;
            let (v, _opt_v) = translate_type_info_ir(py, &args.get_item(1)?, typevar_map, ctx)?;
            return Ok((TypeInfoIR::Map(Box::new(k), Box::new(v)), forced_optional));
        }

        if origin.is(&ctx.builtin_list) || origin.is(&ctx.builtin_tuple) {
            let args_any = ctx.typing.call_method1("get_args", (&resolved,))?;
            let args = args_any.cast::<PyTuple>()?;
            if args.is_empty() {
                let repr: String = resolved.repr()?.extract()?;
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "Unsupported Tars type: {}",
                    repr
                )));
            }
            if origin.is(&ctx.builtin_tuple) {
                let ellipsis = py.Ellipsis();
                if args.len() == 2 && args.get_item(1)?.is(&ellipsis) {
                    let inner_any = args.get_item(0)?;
                    let (inner, _opt) = translate_type_info_ir(py, &inner_any, typevar_map, ctx)?;
                    return Ok((TypeInfoIR::VarTuple(Box::new(inner)), forced_optional));
                }
                let mut items = Vec::with_capacity(args.len());
                for item in args.iter() {
                    if item.is(&ellipsis) {
                        let repr: String = resolved.repr()?.extract()?;
                        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                            "Unsupported tuple type: {}",
                            repr
                        )));
                    }
                    let (inner, _opt) = translate_type_info_ir(py, &item, typevar_map, ctx)?;
                    items.push(inner);
                }
                return Ok((TypeInfoIR::Tuple(items), forced_optional));
            }

            let (inner, _opt) = translate_type_info_ir(py, &args.get_item(0)?, typevar_map, ctx)?;
            return Ok((TypeInfoIR::List(Box::new(inner)), forced_optional));
        }

        if origin.is(&ctx.builtin_dict) {
            let args_any = ctx.typing.call_method1("get_args", (&resolved,))?;
            let args = args_any.cast::<PyTuple>()?;
            if args.len() < 2 {
                let repr: String = resolved.repr()?.extract()?;
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "Unsupported Tars type: {}",
                    repr
                )));
            }
            let (k, _opt_k) = translate_type_info_ir(py, &args.get_item(0)?, typevar_map, ctx)?;
            let (v, _opt_v) = translate_type_info_ir(py, &args.get_item(1)?, typevar_map, ctx)?;
            return Ok((TypeInfoIR::Map(Box::new(k), Box::new(v)), forced_optional));
        }

        break;
    }

    if resolved.is(&ctx.any_type) {
        return Ok((TypeInfoIR::Any, forced_optional));
    }

    if resolved.is(none_type) || resolved.is_none() {
        return Ok((TypeInfoIR::NoneType, true));
    }

    if resolved.is(&ctx.builtin_int) {
        return Ok((TypeInfoIR::Int, forced_optional));
    }
    if resolved.is(&ctx.builtin_str) {
        return Ok((TypeInfoIR::Str, forced_optional));
    }
    if resolved.is(&ctx.builtin_float) {
        return Ok((TypeInfoIR::Float, forced_optional));
    }
    if resolved.is(&ctx.builtin_bool) {
        return Ok((TypeInfoIR::Bool, forced_optional));
    }
    if resolved.is(&ctx.builtin_bytes) {
        return Ok((TypeInfoIR::Bytes, forced_optional));
    }

    if let Ok(resolved_type) = resolved.clone().cast_into::<PyType>() {
        if is_namedtuple_type(&resolved_type, ctx)? {
            let items = build_namedtuple_items(py, &resolved_type, typevar_map, ctx)?;
            return Ok((
                TypeInfoIR::NamedTuple(resolved_type.unbind(), items),
                forced_optional,
            ));
        }
        if is_typeddict_type(&resolved_type, ctx)? {
            return Ok((TypeInfoIR::TypedDict, forced_optional));
        }
        if is_dataclass_type(&resolved_type, ctx)? {
            return Ok((
                TypeInfoIR::Dataclass(resolved_type.unbind()),
                forced_optional,
            ));
        }
    }

    if let Ok(resolved_type) = resolved.clone().cast_into::<PyType>()
        && is_subclass(&resolved_type, &ctx.enum_base, ctx)?
    {
        let members_any = resolved_type.getattr("__members__")?;
        let values_any = members_any.call_method0("values")?;
        let mut variants = Vec::new();
        let mut seen = HashSet::new();
        let mut has_member = false;
        for member in values_any.try_iter()? {
            let member = member?;
            has_member = true;
            let value = member.getattr("value")?;
            let value_type = value.get_type();
            let (typ, _opt) = translate_type_info_ir(py, value_type.as_any(), typevar_map, ctx)?;
            let key = format!("{:?}", typ);
            if seen.insert(key) {
                variants.push(typ);
            }
        }
        if !has_member {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "Enum must define at least one member",
            ));
        }
        let inner = if variants.len() == 1 {
            variants.remove(0)
        } else {
            TypeInfoIR::Union(variants)
        };
        return Ok((
            TypeInfoIR::Enum(resolved_type.unbind(), Box::new(inner)),
            forced_optional,
        ));
    }

    if let Ok(resolved_type) = resolved.clone().cast_into::<PyType>()
        && resolved_type.is_subclass_of::<Struct>()?
    {
        return Ok((TypeInfoIR::Struct(resolved_type.unbind()), forced_optional));
    }

    if let Ok(resolved_type) = resolved.clone().cast_into::<PyType>()
        && resolved_type.is_subclass_of::<TarsDict>()?
    {
        return Ok((TypeInfoIR::TarsDict, forced_optional));
    }

    let repr: String = resolved.repr()?.extract()?;
    Err(pyo3::exceptions::PyTypeError::new_err(format!(
        "Unsupported Tars type: {}",
        repr
    )))
}

fn is_typeddict_type<'py>(
    cls: &Bound<'py, PyType>,
    ctx: &IntrospectionContext<'py>,
) -> PyResult<bool> {
    if let Some(check) = ctx.typing_is_typeddict.as_ref() {
        return check.call1((cls,))?.is_truthy();
    }

    let has_total = cls.getattr("__total__").is_ok();
    let has_annotations = cls.getattr("__annotations__").is_ok();
    let has_keys =
        cls.getattr("__required_keys__").is_ok() || cls.getattr("__optional_keys__").is_ok();
    Ok(has_total && has_annotations && has_keys)
}

fn is_namedtuple_type<'py>(
    cls: &Bound<'py, PyType>,
    ctx: &IntrospectionContext<'py>,
) -> PyResult<bool> {
    if !is_subclass(cls, &ctx.builtin_tuple, ctx)? {
        return Ok(false);
    }
    let fields_any = match cls.getattr("_fields") {
        Ok(v) => v,
        Err(_) => return Ok(false),
    };
    Ok(fields_any.cast::<PyTuple>().is_ok())
}

fn build_namedtuple_items<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
    typevar_map: &HashMap<usize, Bound<'py, PyAny>>,
    ctx: &IntrospectionContext<'py>,
) -> PyResult<Vec<TypeInfoIR>> {
    let fields_any = cls.getattr("_fields")?;
    let fields = fields_any.cast::<PyTuple>()?;
    let annotations_any = cls.getattr("__annotations__").ok();
    let annotations = annotations_any.and_then(|a| a.cast::<PyDict>().ok().cloned());

    let mut items = Vec::with_capacity(fields.len());
    for name_any in fields.iter() {
        let name: String = name_any.extract()?;
        if let Some(ann) = annotations
            .as_ref()
            .and_then(|a| a.get_item(name.as_str()).ok().flatten())
        {
            let (inner, _opt) = translate_type_info_ir(py, &ann, typevar_map, ctx)?;
            items.push(inner);
        } else {
            items.push(TypeInfoIR::Any);
        }
    }
    Ok(items)
}

fn is_dataclass_type<'py>(
    cls: &Bound<'py, PyType>,
    ctx: &IntrospectionContext<'py>,
) -> PyResult<bool> {
    if let Some(check) = ctx.dataclasses_is_dataclass.as_ref() {
        return check.call1((cls,))?.is_truthy();
    }
    Ok(cls.getattr("__dataclass_fields__").is_ok())
}

fn resolve_typevar<'py>(
    _py: Python<'py>,
    tp: &Bound<'py, PyAny>,
    typevar_map: &HashMap<usize, Bound<'py, PyAny>>,
    ctx: &IntrospectionContext<'py>,
) -> PyResult<Bound<'py, PyAny>> {
    if tp.is_instance(&ctx.typevar_cls)?
        && let Some(mapped) = typevar_map.get(&(tp.as_ptr() as usize))
    {
        return Ok(mapped.clone());
    }
    Ok(tp.clone())
}

fn lookup_default_value<'py>(
    py: Python<'py>,
    cls: &Bound<'py, PyType>,
    field_name: &str,
    ctx: &IntrospectionContext<'py>,
) -> PyResult<(bool, Option<Py<PyAny>>)> {
    let member_descriptor = ctx.types_mod.getattr("MemberDescriptorType")?;
    let getset_descriptor = ctx.types_mod.getattr("GetSetDescriptorType")?;

    let mro_any = cls.getattr(intern!(py, "__mro__"))?;
    let mro = mro_any.cast::<PyTuple>()?;

    for base in mro.iter() {
        if let Ok(defaults_any) = base.getattr(intern!(py, "__tarsio_defaults__"))
            && let Ok(defaults) = defaults_any.cast::<PyDict>()
            && let Some(v) = defaults.get_item(field_name)?
        {
            return Ok((true, Some(v.unbind())));
        }

        if let Ok(base_dict_any) = base.getattr(intern!(py, "__dict__"))
            && let Ok(base_dict) = base_dict_any.cast::<PyDict>()
            && let Some(v) = base_dict.get_item(field_name)?
        {
            if v.is_instance(&member_descriptor)? || v.is_instance(&getset_descriptor)? {
                continue;
            }
            return Ok((true, Some(v.unbind())));
        }
    }

    Ok((false, None))
}
