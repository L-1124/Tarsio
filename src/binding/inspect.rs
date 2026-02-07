use pyo3::prelude::*;
use pyo3::types::{PyAny, PyTuple, PyType};

use crate::binding::introspect::{
    ConstraintsIR, TypeInfoIR, introspect_struct_fields, introspect_type_info_ir,
};

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

#[pyclass(module = "tarsio._core.inspect")]
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

#[pyclass(module = "tarsio._core.inspect")]
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

#[pyclass(module = "tarsio._core.inspect")]
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

#[pyclass(module = "tarsio._core.inspect")]
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

#[pyclass(module = "tarsio._core.inspect")]
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

#[pyclass(module = "tarsio._core.inspect")]
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

#[pyclass(module = "tarsio._core.inspect")]
pub struct TupleType {
    #[pyo3(get)]
    pub item_type: Py<PyAny>,
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

#[pyclass(module = "tarsio._core.inspect")]
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

#[pyclass(module = "tarsio._core.inspect")]
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

#[pyclass(module = "tarsio._core.inspect")]
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

#[pyclass(module = "tarsio._core.inspect")]
pub struct StructInfo {
    #[pyo3(get)]
    pub cls: Py<PyType>,
    #[pyo3(get)]
    pub fields: Py<PyTuple>,
}

#[pyfunction]
pub fn type_info(py: Python<'_>, tp: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let (typ, constraints) = introspect_type_info_ir(py, tp)?;
    let constraints_obj = build_constraints(py, constraints.as_ref())?;
    build_type_info(py, &typ, constraints_obj)
}

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

fn build_type_info(
    py: Python<'_>,
    typ: &TypeInfoIR,
    constraints: Option<Py<Constraints>>,
) -> PyResult<Py<PyAny>> {
    match typ {
        TypeInfoIR::Int => Ok(Py::new(py, IntType { constraints })?.into_any()),
        TypeInfoIR::Str => Ok(Py::new(py, StrType { constraints })?.into_any()),
        TypeInfoIR::Float => Ok(Py::new(py, FloatType { constraints })?.into_any()),
        TypeInfoIR::Bool => Ok(Py::new(py, BoolType { constraints })?.into_any()),
        TypeInfoIR::Bytes => Ok(Py::new(py, BytesType { constraints })?.into_any()),
        TypeInfoIR::List(inner) => {
            let item_type = build_type_info(py, inner, None)?;
            Ok(Py::new(
                py,
                ListType {
                    item_type,
                    constraints,
                },
            )?
            .into_any())
        }
        TypeInfoIR::Tuple(inner) => {
            let item_type = build_type_info(py, inner, None)?;
            Ok(Py::new(
                py,
                TupleType {
                    item_type,
                    constraints,
                },
            )?
            .into_any())
        }
        TypeInfoIR::Map(k, v) => {
            let key_type = build_type_info(py, k, None)?;
            let value_type = build_type_info(py, v, None)?;
            Ok(Py::new(
                py,
                MapType {
                    key_type,
                    value_type,
                    constraints,
                },
            )?
            .into_any())
        }
        TypeInfoIR::Optional(inner) => {
            let inner_type = build_type_info(py, inner, None)?;
            Ok(Py::new(
                py,
                OptionalType {
                    inner_type,
                    constraints,
                },
            )?
            .into_any())
        }
        TypeInfoIR::Struct(cls) => Ok(Py::new(
            py,
            StructType {
                cls: cls.clone_ref(py),
                constraints,
            },
        )?
        .into_any()),
    }
}
