use pyo3::prelude::*;
use pyo3::pyclass_init::PyClassInitializer;
use pyo3::types::{PyAny, PyTuple, PyType};
use std::collections::HashSet;

use crate::binding::parse::{
    ConstraintsIR, FieldInfoIR, TypeInfoIR, introspect_struct_fields, introspect_type_info_ir,
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

/// 基础类型基类.
#[pyclass(
    module = "tarsio._core.inspect",
    name = "BasicType",
    extends = TypeBase,
    subclass
)]
pub struct BasicTypeBase;

/// 复合类型基类.
#[pyclass(
    module = "tarsio._core.inspect",
    name = "CompoundType",
    extends = TypeBase,
    subclass
)]
pub struct CompoundTypeBase;

fn constraint_gt(constraints: &Option<ConstraintsIR>) -> Option<f64> {
    constraints.as_ref().and_then(|c| c.gt)
}

fn constraint_lt(constraints: &Option<ConstraintsIR>) -> Option<f64> {
    constraints.as_ref().and_then(|c| c.lt)
}

fn constraint_ge(constraints: &Option<ConstraintsIR>) -> Option<f64> {
    constraints.as_ref().and_then(|c| c.ge)
}

fn constraint_le(constraints: &Option<ConstraintsIR>) -> Option<f64> {
    constraints.as_ref().and_then(|c| c.le)
}

fn constraint_min_length(constraints: &Option<ConstraintsIR>) -> Option<usize> {
    constraints.as_ref().and_then(|c| c.min_len)
}

fn constraint_max_length(constraints: &Option<ConstraintsIR>) -> Option<usize> {
    constraints.as_ref().and_then(|c| c.max_len)
}

fn constraint_pattern(constraints: &Option<ConstraintsIR>) -> Option<String> {
    constraints.as_ref().and_then(|c| c.pattern.clone())
}

/// 整数类型（JCE int 家族的抽象视图）.
///
/// Attributes:
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = BasicTypeBase)]
pub struct IntType {
    constraints: Option<ConstraintsIR>,
}

#[pymethods]
impl IntType {
    #[getter]
    fn kind(&self) -> &'static str {
        "int"
    }

    #[getter]
    fn gt(&self) -> Option<f64> {
        constraint_gt(&self.constraints)
    }

    #[getter]
    fn lt(&self) -> Option<f64> {
        constraint_lt(&self.constraints)
    }

    #[getter]
    fn ge(&self) -> Option<f64> {
        constraint_ge(&self.constraints)
    }

    #[getter]
    fn le(&self) -> Option<f64> {
        constraint_le(&self.constraints)
    }
}

/// 字符串类型.
///
/// Attributes:
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = BasicTypeBase)]
pub struct StrType {
    constraints: Option<ConstraintsIR>,
}

#[pymethods]
impl StrType {
    #[getter]
    fn kind(&self) -> &'static str {
        "str"
    }

    #[getter]
    fn min_length(&self) -> Option<usize> {
        constraint_min_length(&self.constraints)
    }

    #[getter]
    fn max_length(&self) -> Option<usize> {
        constraint_max_length(&self.constraints)
    }

    #[getter]
    fn pattern(&self) -> Option<String> {
        constraint_pattern(&self.constraints)
    }
}

/// 浮点类型（运行时对应 double 语义）.
///
/// Attributes:
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = BasicTypeBase)]
pub struct FloatType {
    constraints: Option<ConstraintsIR>,
}

#[pymethods]
impl FloatType {
    #[getter]
    fn kind(&self) -> &'static str {
        "float"
    }

    #[getter]
    fn gt(&self) -> Option<f64> {
        constraint_gt(&self.constraints)
    }

    #[getter]
    fn lt(&self) -> Option<f64> {
        constraint_lt(&self.constraints)
    }

    #[getter]
    fn ge(&self) -> Option<f64> {
        constraint_ge(&self.constraints)
    }

    #[getter]
    fn le(&self) -> Option<f64> {
        constraint_le(&self.constraints)
    }
}

/// 布尔类型（在 JCE 编码层面通常以 int 表达）.
///
/// Attributes:
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = BasicTypeBase)]
pub struct BoolType {}

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
#[pyclass(module = "tarsio._core.inspect", extends = BasicTypeBase)]
pub struct BytesType {
    constraints: Option<ConstraintsIR>,
}

#[pymethods]
impl BytesType {
    #[getter]
    fn kind(&self) -> &'static str {
        "bytes"
    }

    #[getter]
    fn min_length(&self) -> Option<usize> {
        constraint_min_length(&self.constraints)
    }

    #[getter]
    fn max_length(&self) -> Option<usize> {
        constraint_max_length(&self.constraints)
    }
}

/// 动态类型（运行时根据值推断编码）.
///
/// Attributes:
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = BasicTypeBase)]
pub struct AnyType {}

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
#[pyclass(module = "tarsio._core.inspect", extends = BasicTypeBase)]
pub struct NoneType {}

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
#[pyclass(module = "tarsio._core.inspect", extends = CompoundTypeBase)]
pub struct EnumType {
    #[pyo3(get)]
    pub cls: Py<PyType>,
    #[pyo3(get)]
    pub value_type: Py<PyAny>,
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
#[pyclass(module = "tarsio._core.inspect", extends = CompoundTypeBase)]
pub struct UnionType {
    #[pyo3(get)]
    pub variants: Py<PyTuple>,
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
#[pyclass(module = "tarsio._core.inspect", extends = CompoundTypeBase)]
pub struct ListType {
    #[pyo3(get)]
    pub item_type: Py<PyAny>,
    constraints: Option<ConstraintsIR>,
}

#[pymethods]
impl ListType {
    #[getter]
    fn kind(&self) -> &'static str {
        "list"
    }

    #[getter]
    fn min_length(&self) -> Option<usize> {
        constraint_min_length(&self.constraints)
    }

    #[getter]
    fn max_length(&self) -> Option<usize> {
        constraint_max_length(&self.constraints)
    }
}

/// 元组类型：固定长度、固定类型 `tuple[T1, T2, ...]`.
///
/// Attributes:
///     items: 元素类型列表。
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = CompoundTypeBase)]
pub struct TupleType {
    #[pyo3(get)]
    pub items: Py<PyTuple>,
    constraints: Option<ConstraintsIR>,
}

#[pymethods]
impl TupleType {
    #[getter]
    fn kind(&self) -> &'static str {
        "tuple"
    }

    #[getter]
    fn min_length(&self) -> Option<usize> {
        constraint_min_length(&self.constraints)
    }

    #[getter]
    fn max_length(&self) -> Option<usize> {
        constraint_max_length(&self.constraints)
    }
}

/// 元组类型：可变长度、元素类型相同 `tuple[T, ...]`.
///
/// Attributes:
///     item_type: 元素类型。
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = CompoundTypeBase)]
pub struct VarTupleType {
    #[pyo3(get)]
    pub item_type: Py<PyAny>,
    constraints: Option<ConstraintsIR>,
}

#[pymethods]
impl VarTupleType {
    #[getter]
    fn kind(&self) -> &'static str {
        "var_tuple"
    }

    #[getter]
    fn min_length(&self) -> Option<usize> {
        constraint_min_length(&self.constraints)
    }

    #[getter]
    fn max_length(&self) -> Option<usize> {
        constraint_max_length(&self.constraints)
    }
}

/// 映射类型：`dict[K, V]`.
///
/// Attributes:
///     key_type: 键类型。
///     value_type: 值类型。
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = CompoundTypeBase)]
pub struct MapType {
    #[pyo3(get)]
    pub key_type: Py<PyAny>,
    #[pyo3(get)]
    pub value_type: Py<PyAny>,
    constraints: Option<ConstraintsIR>,
}

#[pymethods]
impl MapType {
    #[getter]
    fn kind(&self) -> &'static str {
        "map"
    }

    #[getter]
    fn min_length(&self) -> Option<usize> {
        constraint_min_length(&self.constraints)
    }

    #[getter]
    fn max_length(&self) -> Option<usize> {
        constraint_max_length(&self.constraints)
    }
}

/// 集合类型：`set[T]` / `frozenset[T]`.
///
/// Attributes:
///     item_type: 元素类型。
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = CompoundTypeBase)]
pub struct SetType {
    #[pyo3(get)]
    pub item_type: Py<PyAny>,
    constraints: Option<ConstraintsIR>,
}

#[pymethods]
impl SetType {
    #[getter]
    fn kind(&self) -> &'static str {
        "set"
    }

    #[getter]
    fn min_length(&self) -> Option<usize> {
        constraint_min_length(&self.constraints)
    }

    #[getter]
    fn max_length(&self) -> Option<usize> {
        constraint_max_length(&self.constraints)
    }
}

/// 可选类型：`T | None` 或 `typing.Optional[T]`.
///
/// Attributes:
///     inner_type: 内层类型。
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = CompoundTypeBase)]
pub struct OptionalType {
    #[pyo3(get)]
    pub inner_type: Py<PyAny>,
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
///     fields: 字段列表，按 tag 升序。
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = CompoundTypeBase)]
pub struct StructType {
    #[pyo3(get)]
    pub cls: Py<PyType>,
    #[pyo3(get)]
    pub fields: Py<PyTuple>,
}

#[pymethods]
impl StructType {
    #[getter]
    fn kind(&self) -> &'static str {
        "struct"
    }
}

/// 引用类型：用于递归结构中的循环引用节点.
///
/// Attributes:
///     cls: 被引用的 Struct 类型。
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = CompoundTypeBase)]
pub struct RefType {
    #[pyo3(get)]
    pub cls: Py<PyType>,
}

#[pymethods]
impl RefType {
    #[getter]
    fn kind(&self) -> &'static str {
        "ref"
    }
}

/// TypedDict 类型（字段映射以 dict 形式编码）.
///
/// Attributes:
///     constraints: 字段约束。
#[pyclass(module = "tarsio._core.inspect", extends = CompoundTypeBase)]
pub struct TypedDictType {}

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
#[pyclass(module = "tarsio._core.inspect", extends = CompoundTypeBase)]
pub struct NamedTupleType {
    #[pyo3(get)]
    pub cls: Py<PyType>,
    #[pyo3(get)]
    pub items: Py<PyTuple>,
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
#[pyclass(module = "tarsio._core.inspect", extends = CompoundTypeBase)]
pub struct DataclassType {
    #[pyo3(get)]
    pub cls: Py<PyType>,
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
#[pyclass(module = "tarsio._core.inspect", extends = CompoundTypeBase)]
pub struct TarsDictType {}

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
#[pyclass(module = "tarsio._core.inspect", name = "Field")]
pub struct Field {
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
}

pub type FieldInfo = Field;

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
    let mut build_ctx = TypeBuildContext::default();
    build_type_info(py, &typ, constraints, &mut build_ctx)
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

    let mut build_ctx = TypeBuildContext::default();
    build_ctx.enter_struct(cls.clone().unbind(), py);
    let mut fields: Vec<Py<Field>> = Vec::with_capacity(fields_ir.len());
    for field_ir in fields_ir {
        fields.push(build_field(py, field_ir, &mut build_ctx)?);
    }
    build_ctx.leave_struct(cls.clone().unbind(), py);

    let fields_tuple = PyTuple::new(py, fields)?;
    Ok(Some(StructInfo {
        cls: cls.clone().unbind(),
        fields: fields_tuple.unbind(),
    }))
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
    constraints: Option<ConstraintsIR>,
    build_ctx: &mut TypeBuildContext,
) -> PyResult<Py<PyAny>> {
    match typ {
        TypeInfoIR::Int => Ok(Py::new(
            py,
            PyClassInitializer::from(TypeBase)
                .add_subclass(BasicTypeBase)
                .add_subclass(IntType { constraints }),
        )?
        .into_any()),
        TypeInfoIR::Str => Ok(Py::new(
            py,
            PyClassInitializer::from(TypeBase)
                .add_subclass(BasicTypeBase)
                .add_subclass(StrType { constraints }),
        )?
        .into_any()),
        TypeInfoIR::Float => Ok(Py::new(
            py,
            PyClassInitializer::from(TypeBase)
                .add_subclass(BasicTypeBase)
                .add_subclass(FloatType { constraints }),
        )?
        .into_any()),
        TypeInfoIR::Bool => Ok(Py::new(
            py,
            PyClassInitializer::from(TypeBase)
                .add_subclass(BasicTypeBase)
                .add_subclass(BoolType {}),
        )?
        .into_any()),
        TypeInfoIR::Bytes => Ok(Py::new(
            py,
            PyClassInitializer::from(TypeBase)
                .add_subclass(BasicTypeBase)
                .add_subclass(BytesType { constraints }),
        )?
        .into_any()),
        TypeInfoIR::Any => Ok(Py::new(
            py,
            PyClassInitializer::from(TypeBase)
                .add_subclass(BasicTypeBase)
                .add_subclass(AnyType {}),
        )?
        .into_any()),
        TypeInfoIR::NoneType => Ok(Py::new(
            py,
            PyClassInitializer::from(TypeBase)
                .add_subclass(BasicTypeBase)
                .add_subclass(NoneType {}),
        )?
        .into_any()),
        TypeInfoIR::TypedDict => Ok(Py::new(
            py,
            PyClassInitializer::from(TypeBase)
                .add_subclass(CompoundTypeBase)
                .add_subclass(TypedDictType {}),
        )?
        .into_any()),
        TypeInfoIR::NamedTuple(cls, items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(build_type_info(py, item, None, build_ctx)?);
            }
            let items_tuple = PyTuple::new(py, out)?;
            Ok(Py::new(
                py,
                PyClassInitializer::from(TypeBase)
                    .add_subclass(CompoundTypeBase)
                    .add_subclass(NamedTupleType {
                        cls: cls.clone_ref(py),
                        items: items_tuple.unbind(),
                    }),
            )?
            .into_any())
        }
        TypeInfoIR::Dataclass(cls) => Ok(Py::new(
            py,
            PyClassInitializer::from(TypeBase)
                .add_subclass(CompoundTypeBase)
                .add_subclass(DataclassType {
                    cls: cls.clone_ref(py),
                }),
        )?
        .into_any()),
        TypeInfoIR::Enum(cls, inner) => {
            let value_type = build_type_info(py, inner, None, build_ctx)?;
            Ok(Py::new(
                py,
                PyClassInitializer::from(TypeBase)
                    .add_subclass(CompoundTypeBase)
                    .add_subclass(EnumType {
                        cls: cls.clone_ref(py),
                        value_type,
                    }),
            )?
            .into_any())
        }
        TypeInfoIR::Union(variants) => {
            let mut items = Vec::with_capacity(variants.len());
            for item in variants {
                items.push(build_type_info(py, item, None, build_ctx)?);
            }
            let variants_tuple = PyTuple::new(py, items)?;
            Ok(Py::new(
                py,
                PyClassInitializer::from(TypeBase)
                    .add_subclass(CompoundTypeBase)
                    .add_subclass(UnionType {
                        variants: variants_tuple.unbind(),
                    }),
            )?
            .into_any())
        }
        TypeInfoIR::List(inner) => {
            let item_type = build_type_info(py, inner, None, build_ctx)?;
            Ok(Py::new(
                py,
                PyClassInitializer::from(TypeBase)
                    .add_subclass(CompoundTypeBase)
                    .add_subclass(ListType {
                        item_type,
                        constraints,
                    }),
            )?
            .into_any())
        }
        TypeInfoIR::Tuple(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(build_type_info(py, item, None, build_ctx)?);
            }
            let items_tuple = PyTuple::new(py, out)?;
            Ok(Py::new(
                py,
                PyClassInitializer::from(TypeBase)
                    .add_subclass(CompoundTypeBase)
                    .add_subclass(TupleType {
                        items: items_tuple.unbind(),
                        constraints,
                    }),
            )?
            .into_any())
        }
        TypeInfoIR::VarTuple(inner) => {
            let item_type = build_type_info(py, inner, None, build_ctx)?;
            Ok(Py::new(
                py,
                PyClassInitializer::from(TypeBase)
                    .add_subclass(CompoundTypeBase)
                    .add_subclass(VarTupleType {
                        item_type,
                        constraints,
                    }),
            )?
            .into_any())
        }
        TypeInfoIR::Map(k, v) => {
            let key_type = build_type_info(py, k, None, build_ctx)?;
            let value_type = build_type_info(py, v, None, build_ctx)?;
            Ok(Py::new(
                py,
                PyClassInitializer::from(TypeBase)
                    .add_subclass(CompoundTypeBase)
                    .add_subclass(MapType {
                        key_type,
                        value_type,
                        constraints,
                    }),
            )?
            .into_any())
        }
        TypeInfoIR::Set(inner) => {
            let item_type = build_type_info(py, inner, None, build_ctx)?;
            Ok(Py::new(
                py,
                PyClassInitializer::from(TypeBase)
                    .add_subclass(CompoundTypeBase)
                    .add_subclass(SetType {
                        item_type,
                        constraints,
                    }),
            )?
            .into_any())
        }
        TypeInfoIR::Optional(inner) => {
            let inner_type = build_type_info(py, inner, None, build_ctx)?;
            Ok(Py::new(
                py,
                PyClassInitializer::from(TypeBase)
                    .add_subclass(CompoundTypeBase)
                    .add_subclass(OptionalType { inner_type }),
            )?
            .into_any())
        }
        TypeInfoIR::Struct(cls) => {
            if build_ctx.is_visiting(cls, py) {
                return Ok(Py::new(
                    py,
                    PyClassInitializer::from(TypeBase)
                        .add_subclass(CompoundTypeBase)
                        .add_subclass(RefType {
                            cls: cls.clone_ref(py),
                        }),
                )?
                .into_any());
            }

            let cls_bound = cls.bind(py);
            let fields_ir = introspect_struct_fields(py, cls_bound)?;
            build_ctx.enter_struct(cls.clone_ref(py), py);
            let fields = if let Some(fields_ir) = fields_ir {
                let mut out = Vec::with_capacity(fields_ir.len());
                for field_ir in fields_ir {
                    out.push(build_field(py, field_ir, build_ctx)?);
                }
                PyTuple::new(py, out)?
            } else {
                PyTuple::empty(py)
            };
            build_ctx.leave_struct(cls.clone_ref(py), py);

            Ok(Py::new(
                py,
                PyClassInitializer::from(TypeBase)
                    .add_subclass(CompoundTypeBase)
                    .add_subclass(StructType {
                        cls: cls.clone_ref(py),
                        fields: fields.unbind(),
                    }),
            )?
            .into_any())
        }
        TypeInfoIR::TarsDict => Ok(Py::new(
            py,
            PyClassInitializer::from(TypeBase)
                .add_subclass(CompoundTypeBase)
                .add_subclass(TarsDictType {}),
        )?
        .into_any()),
    }
}

#[derive(Default)]
struct TypeBuildContext {
    visiting_structs: HashSet<usize>,
}

impl TypeBuildContext {
    fn type_key(&self, cls: &Py<PyType>, py: Python<'_>) -> usize {
        cls.bind(py).as_ptr() as usize
    }

    fn enter_struct(&mut self, cls: Py<PyType>, py: Python<'_>) {
        self.visiting_structs.insert(self.type_key(&cls, py));
    }

    fn leave_struct(&mut self, cls: Py<PyType>, py: Python<'_>) {
        self.visiting_structs.remove(&self.type_key(&cls, py));
    }

    fn is_visiting(&self, cls: &Py<PyType>, py: Python<'_>) -> bool {
        self.visiting_structs.contains(&self.type_key(cls, py))
    }
}

fn build_field(
    py: Python<'_>,
    field_ir: FieldInfoIR,
    build_ctx: &mut TypeBuildContext,
) -> PyResult<Py<Field>> {
    let typ_obj = build_type_info(py, &field_ir.typ, field_ir.constraints.clone(), build_ctx)?;
    let default = field_ir.default_value.unwrap_or_else(|| py.None());

    Py::new(
        py,
        Field {
            name: field_ir.name,
            tag: field_ir.tag,
            typ: typ_obj,
            default,
            has_default: field_ir.has_default,
            optional: field_ir.is_optional,
            required: field_ir.is_required,
        },
    )
}
