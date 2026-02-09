use pyo3::prelude::*;
use pyo3::types::{PyAny, PyTuple, PyType};

use crate::binding::introspect::{
    ConstraintsIR, TypeInfoIR, introspect_struct_fields, introspect_type_info_ir,
};

/// 字段约束信息.
///
/// Attributes:
///     gt: 大于约束。
///     lt: 小于约束。
///     ge: 大于等于约束。
///     le: 小于等于约束。
///     min_len: 最小长度约束。
///     max_len: 最大长度约束。
///     pattern: 正则模式约束。
/// 类型内省基类.
///
/// Attributes:
///     kind: 类型分支标识。
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", name = "Type", subclass)]
pub struct TypeBase;

#[pyclass(module = "tarsio._core.inspect")]
pub struct Constraints {
    #[pyo3(get)]
    pub gt: Option<f64>,
    #[pyo3(get)]
    pub lt: Option<f64>,
    #[pyo3(get)]
    pub ge: Option<f64>,
    #[pyo3(get)]
    pub le: Option<f64>,
    #[pyo3(get)]
    pub min_len: Option<usize>,
    #[pyo3(get)]
    pub max_len: Option<usize>,
    #[pyo3(get)]
    pub pattern: Option<String>,
}

/// 整数类型（JCE int 家族的抽象视图）.
///
/// Attributes:
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = TypeBase)]
pub struct IntType {
    #[pyo3(get)]
    pub constraints: Option<Py<Constraints>>,
}

#[pymethods]
impl IntType {
    #[getter]
    fn kind(&self) -> &'static str {
        "int"
    }
}

/// 字符串类型.
///
/// Attributes:
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = TypeBase)]
pub struct StrType {
    #[pyo3(get)]
    pub constraints: Option<Py<Constraints>>,
}

#[pymethods]
impl StrType {
    #[getter]
    fn kind(&self) -> &'static str {
        "str"
    }
}

/// 浮点类型（运行时对应 double 语义）.
///
/// Attributes:
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = TypeBase)]
pub struct FloatType {
    #[pyo3(get)]
    pub constraints: Option<Py<Constraints>>,
}

#[pymethods]
impl FloatType {
    #[getter]
    fn kind(&self) -> &'static str {
        "float"
    }
}

/// 布尔类型（在 JCE 编码层面通常以 int 表达）.
///
/// Attributes:
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = TypeBase)]
pub struct BoolType {
    #[pyo3(get)]
    pub constraints: Option<Py<Constraints>>,
}

#[pymethods]
impl BoolType {
    #[getter]
    fn kind(&self) -> &'static str {
        "bool"
    }
}

/// 二进制类型（运行时会被视为 byte-list 的特殊形式）.
///
/// Attributes:
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = TypeBase)]
pub struct BytesType {
    #[pyo3(get)]
    pub constraints: Option<Py<Constraints>>,
}

#[pymethods]
impl BytesType {
    #[getter]
    fn kind(&self) -> &'static str {
        "bytes"
    }
}

/// 动态类型（运行时根据值推断编码）.
///
/// Attributes:
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = TypeBase)]
pub struct AnyType {
    #[pyo3(get)]
    pub constraints: Option<Py<Constraints>>,
}

#[pymethods]
impl AnyType {
    #[getter]
    fn kind(&self) -> &'static str {
        "any"
    }
}

/// None 类型（通常仅出现在 Union/Optional 中）.
///
/// Attributes:
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = TypeBase)]
pub struct NoneType {
    #[pyo3(get)]
    pub constraints: Option<Py<Constraints>>,
}

#[pymethods]
impl NoneType {
    #[getter]
    fn kind(&self) -> &'static str {
        "none"
    }
}

/// Enum 类型.
///
/// Attributes:
///     cls: 枚举类型。
///     value_type: 枚举值的类型内省结果。
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = TypeBase)]
pub struct EnumType {
    #[pyo3(get)]
    pub cls: Py<PyType>,
    #[pyo3(get)]
    pub value_type: Py<PyAny>,
    #[pyo3(get)]
    pub constraints: Option<Py<Constraints>>,
}

#[pymethods]
impl EnumType {
    #[getter]
    fn kind(&self) -> &'static str {
        "enum"
    }
}

/// Union 类型（非 Optional 形式）。
///
/// Attributes:
///     variants: 变体类型列表。
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = TypeBase)]
pub struct UnionType {
    #[pyo3(get)]
    pub variants: Py<PyTuple>,
    #[pyo3(get)]
    pub constraints: Option<Py<Constraints>>,
}

#[pymethods]
impl UnionType {
    #[getter]
    fn kind(&self) -> &'static str {
        "union"
    }
}

/// 列表类型：`list[T]`.
///
/// Attributes:
///     item_type: 元素类型。
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = TypeBase)]
pub struct ListType {
    #[pyo3(get)]
    pub item_type: Py<PyAny>,
    #[pyo3(get)]
    pub constraints: Option<Py<Constraints>>,
}

#[pymethods]
impl ListType {
    #[getter]
    fn kind(&self) -> &'static str {
        "list"
    }
}

/// 元组类型：固定长度、固定类型 `tuple[T1, T2, ...]`.
///
/// Attributes:
///     items: 元素类型列表。
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = TypeBase)]
pub struct TupleType {
    #[pyo3(get)]
    pub items: Py<PyTuple>,
    #[pyo3(get)]
    pub constraints: Option<Py<Constraints>>,
}

#[pymethods]
impl TupleType {
    #[getter]
    fn kind(&self) -> &'static str {
        "tuple"
    }
}

/// 元组类型：可变长度、元素类型相同 `tuple[T, ...]`.
///
/// Attributes:
///     item_type: 元素类型。
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = TypeBase)]
pub struct VarTupleType {
    #[pyo3(get)]
    pub item_type: Py<PyAny>,
    #[pyo3(get)]
    pub constraints: Option<Py<Constraints>>,
}

#[pymethods]
impl VarTupleType {
    #[getter]
    fn kind(&self) -> &'static str {
        "var_tuple"
    }
}

/// 映射类型：`dict[K, V]`.
///
/// Attributes:
///     key_type: 键类型。
///     value_type: 值类型。
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = TypeBase)]
pub struct MapType {
    #[pyo3(get)]
    pub key_type: Py<PyAny>,
    #[pyo3(get)]
    pub value_type: Py<PyAny>,
    #[pyo3(get)]
    pub constraints: Option<Py<Constraints>>,
}

#[pymethods]
impl MapType {
    #[getter]
    fn kind(&self) -> &'static str {
        "map"
    }
}

/// 集合类型：`set[T]` / `frozenset[T]`.
///
/// Attributes:
///     item_type: 元素类型。
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = TypeBase)]
pub struct SetType {
    #[pyo3(get)]
    pub item_type: Py<PyAny>,
    #[pyo3(get)]
    pub constraints: Option<Py<Constraints>>,
}

#[pymethods]
impl SetType {
    #[getter]
    fn kind(&self) -> &'static str {
        "set"
    }
}

/// 可选类型：`T | None` 或 `typing.Optional[T]`.
///
/// Attributes:
///     inner_type: 内层类型。
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = TypeBase)]
pub struct OptionalType {
    #[pyo3(get)]
    pub inner_type: Py<PyAny>,
    #[pyo3(get)]
    pub constraints: Option<Py<Constraints>>,
}

#[pymethods]
impl OptionalType {
    #[getter]
    fn kind(&self) -> &'static str {
        "optional"
    }
}

/// Struct 类型：字段类型为另一个 `tarsio.Struct` 子类.
///
/// Attributes:
///     cls: Struct 类型。
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = TypeBase)]
pub struct StructType {
    #[pyo3(get)]
    pub cls: Py<PyType>,
    #[pyo3(get)]
    pub constraints: Option<Py<Constraints>>,
}

#[pymethods]
impl StructType {
    #[getter]
    fn kind(&self) -> &'static str {
        "struct"
    }
}

/// TypedDict 类型（字段映射以 dict 形式编码）.
///
/// Attributes:
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = TypeBase)]
pub struct TypedDictType {
    #[pyo3(get)]
    pub constraints: Option<Py<Constraints>>,
}

#[pymethods]
impl TypedDictType {
    #[getter]
    fn kind(&self) -> &'static str {
        "typeddict"
    }
}

/// NamedTuple 类型（按 tuple 语义编码）.
///
/// Attributes:
///     items: 元素类型列表。
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = TypeBase)]
pub struct NamedTupleType {
    #[pyo3(get)]
    pub cls: Py<PyType>,
    #[pyo3(get)]
    pub items: Py<PyTuple>,
    #[pyo3(get)]
    pub constraints: Option<Py<Constraints>>,
}

#[pymethods]
impl NamedTupleType {
    #[getter]
    fn kind(&self) -> &'static str {
        "namedtuple"
    }
}

/// Dataclass 类型（鸭子类型，按 map 语义编码）.
///
/// Attributes:
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = TypeBase)]
pub struct DataclassType {
    #[pyo3(get)]
    pub cls: Py<PyType>,
    #[pyo3(get)]
    pub constraints: Option<Py<Constraints>>,
}

#[pymethods]
impl DataclassType {
    #[getter]
    fn kind(&self) -> &'static str {
        "dataclass"
    }
}

/// TarsDict 类型（动态 struct 字段映射）.
///
/// Attributes:
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = TypeBase)]
pub struct TarsDictType {
    #[pyo3(get)]
    pub constraints: Option<Py<Constraints>>,
}

#[pymethods]
impl TarsDictType {
    #[getter]
    fn kind(&self) -> &'static str {
        "tarsdict"
    }
}

/// 结构体字段信息.
///
/// Attributes:
///     name: 字段名。
///     tag: 字段 tag。
///     typ: 字段类型内省结果。
///     default: 字段默认值。
///     has_default: 是否显式有默认值。
///     optional: 是否可选。
///     required: 是否必填。
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect")]
pub struct FieldInfo {
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub tag: u8,
    #[pyo3(get, name = "type")]
    pub typ: Py<PyAny>,
    #[pyo3(get)]
    pub default: Py<PyAny>,
    #[pyo3(get)]
    pub has_default: bool,
    #[pyo3(get)]
    pub optional: bool,
    #[pyo3(get)]
    pub required: bool,
    #[pyo3(get)]
    pub constraints: Option<Py<Constraints>>,
}

/// 结构体信息（类级 Schema 视图）。
///
/// Attributes:
///     cls: 结构体类型。
///     fields: 字段列表，按 tag 升序。
#[pyclass(module = "tarsio._core.inspect")]
pub struct StructInfo {
    #[pyo3(get)]
    pub cls: Py<PyType>,
    #[pyo3(get)]
    pub fields: Py<PyTuple>,
}

/// 将类型标注解析为 Tarsio 的类型内省结果.
///
/// Args:
///     tp: 需要解析的类型标注。
///
/// Returns:
///     `TypeInfo` 分支对象。
///
/// Raises:
///     TypeError: 当类型标注不受支持或包含未支持的前向引用时抛出。
#[pyfunction]
pub fn type_info(py: Python<'_>, tp: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let (typ, constraints) = introspect_type_info_ir(py, tp)?;
    let constraints_obj = build_constraints(py, constraints.as_ref())?;
    build_type_info(py, &typ, constraints_obj)
}

/// 解析 Struct 类并返回字段定义信息.
///
/// Args:
///     cls: 需要解析的 `tarsio.Struct` 子类。
///
/// Returns:
///     `StructInfo` 对象；若无可用字段或为未具体化模板则返回 None。
///
/// Raises:
///     TypeError: 当字段缺少 tag、tag 重复、混用整数 tag 与 Meta，或字段类型不受支持时抛出。
#[pyfunction]
pub fn struct_info(py: Python<'_>, cls: &Bound<'_, PyType>) -> PyResult<Option<StructInfo>> {
    let Some(fields_ir) = introspect_struct_fields(py, cls)? else {
        return Ok(None);
    };

    let mut fields: Vec<Py<FieldInfo>> = Vec::with_capacity(fields_ir.len());
    for field_ir in fields_ir {
        let constraints_obj = build_constraints(py, field_ir.constraints.as_ref())?;
        let typ_constraints = constraints_obj.as_ref().map(|c| c.clone_ref(py));
        let typ_obj = build_type_info(py, &field_ir.typ, typ_constraints)?;
        let default = field_ir.default_value.unwrap_or_else(|| py.None());

        fields.push(Py::new(
            py,
            FieldInfo {
                name: field_ir.name,
                tag: field_ir.tag,
                typ: typ_obj,
                default,
                has_default: field_ir.has_default,
                optional: field_ir.is_optional,
                required: field_ir.is_required,
                constraints: constraints_obj,
            },
        )?);
    }

    let fields_tuple = PyTuple::new(py, fields)?;
    Ok(Some(StructInfo {
        cls: cls.clone().unbind(),
        fields: fields_tuple.unbind(),
    }))
}

/// 构造约束对象.
///
/// Args:
///     constraints: 内部约束表示。
///
/// Returns:
///     Python 侧约束对象或 None。
fn build_constraints(
    py: Python<'_>,
    constraints: Option<&ConstraintsIR>,
) -> PyResult<Option<Py<Constraints>>> {
    let Some(c) = constraints else {
        return Ok(None);
    };
    Ok(Some(Py::new(
        py,
        Constraints {
            gt: c.gt,
            lt: c.lt,
            ge: c.ge,
            le: c.le,
            min_len: c.min_len,
            max_len: c.max_len,
            pattern: c.pattern.clone(),
        },
    )?))
}

/// 构建类型内省对象.
///
/// Args:
///     typ: 语义类型信息。
///     constraints: 约束对象。
///
/// Returns:
///     Python 侧类型内省对象。
fn build_type_info(
    py: Python<'_>,
    typ: &TypeInfoIR,
    constraints: Option<Py<Constraints>>,
) -> PyResult<Py<PyAny>> {
    match typ {
        TypeInfoIR::Int => Ok(Py::new(py, (IntType { constraints }, TypeBase))?.into_any()),
        TypeInfoIR::Str => Ok(Py::new(py, (StrType { constraints }, TypeBase))?.into_any()),
        TypeInfoIR::Float => Ok(Py::new(py, (FloatType { constraints }, TypeBase))?.into_any()),
        TypeInfoIR::Bool => Ok(Py::new(py, (BoolType { constraints }, TypeBase))?.into_any()),
        TypeInfoIR::Bytes => Ok(Py::new(py, (BytesType { constraints }, TypeBase))?.into_any()),
        TypeInfoIR::Any => Ok(Py::new(py, (AnyType { constraints }, TypeBase))?.into_any()),
        TypeInfoIR::NoneType => Ok(Py::new(py, (NoneType { constraints }, TypeBase))?.into_any()),
        TypeInfoIR::TypedDict => {
            Ok(Py::new(py, (TypedDictType { constraints }, TypeBase))?.into_any())
        }
        TypeInfoIR::NamedTuple(cls, items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(build_type_info(py, item, None)?);
            }
            let items_tuple = PyTuple::new(py, out)?;
            Ok(Py::new(
                py,
                (
                    NamedTupleType {
                        cls: cls.clone_ref(py),
                        items: items_tuple.unbind(),
                        constraints,
                    },
                    TypeBase,
                ),
            )?
            .into_any())
        }
        TypeInfoIR::Dataclass(cls) => Ok(Py::new(
            py,
            (
                DataclassType {
                    cls: cls.clone_ref(py),
                    constraints,
                },
                TypeBase,
            ),
        )?
        .into_any()),
        TypeInfoIR::Enum(cls, inner) => {
            let value_type = build_type_info(py, inner, None)?;
            Ok(Py::new(
                py,
                (
                    EnumType {
                        cls: cls.clone_ref(py),
                        value_type,
                        constraints,
                    },
                    TypeBase,
                ),
            )?
            .into_any())
        }
        TypeInfoIR::Union(variants) => {
            let mut items = Vec::with_capacity(variants.len());
            for item in variants {
                items.push(build_type_info(py, item, None)?);
            }
            let variants_tuple = PyTuple::new(py, items)?;
            Ok(Py::new(
                py,
                (
                    UnionType {
                        variants: variants_tuple.unbind(),
                        constraints,
                    },
                    TypeBase,
                ),
            )?
            .into_any())
        }
        TypeInfoIR::List(inner) => {
            let item_type = build_type_info(py, inner, None)?;
            Ok(Py::new(
                py,
                (
                    ListType {
                        item_type,
                        constraints,
                    },
                    TypeBase,
                ),
            )?
            .into_any())
        }
        TypeInfoIR::Tuple(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(build_type_info(py, item, None)?);
            }
            let items_tuple = PyTuple::new(py, out)?;
            Ok(Py::new(
                py,
                (
                    TupleType {
                        items: items_tuple.unbind(),
                        constraints,
                    },
                    TypeBase,
                ),
            )?
            .into_any())
        }
        TypeInfoIR::VarTuple(inner) => {
            let item_type = build_type_info(py, inner, None)?;
            Ok(Py::new(
                py,
                (
                    VarTupleType {
                        item_type,
                        constraints,
                    },
                    TypeBase,
                ),
            )?
            .into_any())
        }
        TypeInfoIR::Map(k, v) => {
            let key_type = build_type_info(py, k, None)?;
            let value_type = build_type_info(py, v, None)?;
            Ok(Py::new(
                py,
                (
                    MapType {
                        key_type,
                        value_type,
                        constraints,
                    },
                    TypeBase,
                ),
            )?
            .into_any())
        }
        TypeInfoIR::Set(inner) => {
            let item_type = build_type_info(py, inner, None)?;
            Ok(Py::new(
                py,
                (
                    SetType {
                        item_type,
                        constraints,
                    },
                    TypeBase,
                ),
            )?
            .into_any())
        }
        TypeInfoIR::Optional(inner) => {
            let inner_type = build_type_info(py, inner, None)?;
            Ok(Py::new(
                py,
                (
                    OptionalType {
                        inner_type,
                        constraints,
                    },
                    TypeBase,
                ),
            )?
            .into_any())
        }
        TypeInfoIR::Struct(cls) => Ok(Py::new(
            py,
            (
                StructType {
                    cls: cls.clone_ref(py),
                    constraints,
                },
                TypeBase,
            ),
        )?
        .into_any()),
        TypeInfoIR::TarsDict => {
            Ok(Py::new(py, (TarsDictType { constraints }, TypeBase))?.into_any())
        }
    }
}
