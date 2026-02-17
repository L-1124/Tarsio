use crate::ValidationError;
use crate::binding::core::{Constraints, TarsDict, TypeExpr, WireType};
use crate::binding::utils::{class_from_ptr, dataclass_fields};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyDict, PyFloat, PyFrozenSet, PySequence, PySet, PyString};

#[inline]
fn field_prefix(field_name: Option<&str>) -> String {
    match field_name {
        Some(name) => format!("Field '{}'", name),
        None => "Value".to_string(),
    }
}

#[inline]
fn has_numeric_constraints(c: &Constraints) -> bool {
    c.gt.is_some() || c.ge.is_some() || c.lt.is_some() || c.le.is_some()
}

#[inline]
fn has_length_constraints(c: &Constraints) -> bool {
    c.min_len.is_some() || c.max_len.is_some()
}

pub(crate) fn validate_numeric_constraints_raw(
    value: f64,
    constraints: &Constraints,
    field_name: Option<&str>,
) -> PyResult<()> {
    let label = field_prefix(field_name);

    if let Some(gt) = constraints.gt
        && value.partial_cmp(&gt) != Some(std::cmp::Ordering::Greater)
    {
        return Err(ValidationError::new_err(format!(
            "{} must be > {}, got {}",
            label, gt, value
        )));
    }
    if let Some(ge) = constraints.ge
        && matches!(
            value.partial_cmp(&ge),
            Some(std::cmp::Ordering::Less) | None
        )
    {
        return Err(ValidationError::new_err(format!(
            "{} must be >= {}, got {}",
            label, ge, value
        )));
    }
    if let Some(lt) = constraints.lt
        && value.partial_cmp(&lt) != Some(std::cmp::Ordering::Less)
    {
        return Err(ValidationError::new_err(format!(
            "{} must be < {}, got {}",
            label, lt, value
        )));
    }
    if let Some(le) = constraints.le
        && matches!(
            value.partial_cmp(&le),
            Some(std::cmp::Ordering::Greater) | None
        )
    {
        return Err(ValidationError::new_err(format!(
            "{} must be <= {}, got {}",
            label, le, value
        )));
    }

    Ok(())
}

pub(crate) fn validate_length_constraints_raw(
    len: usize,
    constraints: &Constraints,
    field_name: Option<&str>,
) -> PyResult<()> {
    let label = field_prefix(field_name);

    if let Some(min_len) = constraints.min_len
        && len < min_len
    {
        return Err(ValidationError::new_err(format!(
            "{} length must be >= {}, got {}",
            label, min_len, len
        )));
    }
    if let Some(max_len) = constraints.max_len
        && len > max_len
    {
        return Err(ValidationError::new_err(format!(
            "{} length must be <= {}, got {}",
            label, max_len, len
        )));
    }

    Ok(())
}

pub(crate) fn validate_constraints_on_value(
    value: &Bound<'_, PyAny>,
    constraints: &Constraints,
    field_name: Option<&str>,
) -> PyResult<()> {
    let label = field_prefix(field_name);

    if has_numeric_constraints(constraints) {
        let numeric: f64 = value.extract().map_err(|_| {
            ValidationError::new_err(format!(
                "{} must be a number to apply numeric constraints",
                label
            ))
        })?;
        validate_numeric_constraints_raw(numeric, constraints, field_name)?;
    }

    if has_length_constraints(constraints) {
        let len = value.len().map_err(|_| {
            ValidationError::new_err(format!(
                "{} must have length to apply length constraints",
                label
            ))
        })?;
        validate_length_constraints_raw(len, constraints, field_name)?;
    }

    if let Some(pattern_py) = constraints.pattern.as_ref() {
        let py = value.py();
        let pattern = pattern_py.bind(py);
        let matched = pattern.call_method1("search", (value,))?;
        if matched.is_none() {
            return Err(ValidationError::new_err(format!(
                "{} does not match pattern",
                label
            )));
        }
    }

    Ok(())
}

pub(crate) fn value_matches_type<'py>(
    py: Python<'py>,
    typ: &TypeExpr,
    value: &Bound<'py, PyAny>,
) -> PyResult<bool> {
    match typ {
        TypeExpr::Any => Ok(true),
        TypeExpr::NoneType => Ok(value.is_none()),
        TypeExpr::Primitive(wire_type) => match wire_type {
            WireType::Int => {
                if value.is_instance_of::<pyo3::types::PyBool>() {
                    Ok(false)
                } else {
                    Ok(value.extract::<i64>().is_ok())
                }
            }
            WireType::Bool => Ok(value.is_instance_of::<pyo3::types::PyBool>()),
            WireType::Long => Ok(value.extract::<i64>().is_ok()),
            WireType::Float | WireType::Double => Ok(value.is_instance_of::<PyFloat>()),
            WireType::String => Ok(value.is_instance_of::<PyString>()),
            _ => Ok(false),
        },
        TypeExpr::Enum(enum_cls, _) => Ok(value.is_instance(enum_cls.bind(py).as_any())?),
        TypeExpr::Struct(ptr) => {
            let cls = class_from_ptr(py, *ptr)?;
            Ok(value.is_instance(cls.as_any())?)
        }
        TypeExpr::TarsDict => Ok(value.is_instance_of::<TarsDict>()),
        TypeExpr::NamedTuple(cls, _) => Ok(value.is_instance(cls.bind(py).as_any())?),
        TypeExpr::Dataclass(cls) => Ok(value.is_instance(cls.bind(py).as_any())?),
        TypeExpr::List(inner) => {
            if matches!(**inner, TypeExpr::Primitive(WireType::Int))
                && value.is_instance_of::<PyBytes>()
            {
                return Ok(true);
            }
            Ok(value.is_instance_of::<PySequence>()
                && !value.is_instance_of::<PyString>()
                && !value.is_instance_of::<PyBytes>()
                && !value.is_instance_of::<PyDict>())
        }
        TypeExpr::VarTuple(_) => Ok(value.is_instance_of::<PySequence>()
            && !value.is_instance_of::<PyString>()
            && !value.is_instance_of::<PyBytes>()
            && !value.is_instance_of::<PyDict>()),
        TypeExpr::Tuple(items) => {
            if value.is_instance_of::<PyString>()
                || value.is_instance_of::<PyBytes>()
                || value.is_instance_of::<PyDict>()
            {
                return Ok(false);
            }
            let seq = match value.extract::<Bound<'_, PySequence>>() {
                Ok(v) => v,
                Err(_) => return Ok(false),
            };
            let len = seq.len()?;
            if len != items.len() {
                return Ok(false);
            }
            for (idx, item_type) in items.iter().enumerate() {
                let item = seq.get_item(idx)?;
                if !value_matches_type(py, item_type, &item)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        TypeExpr::Set(_) => {
            Ok(value.is_instance_of::<PySet>() || value.is_instance_of::<PyFrozenSet>())
        }
        TypeExpr::Map(_, _) => {
            Ok(value.is_instance_of::<PyDict>() || dataclass_fields(value)?.is_some())
        }
        TypeExpr::Optional(inner) => {
            if value.is_none() {
                Ok(true)
            } else {
                value_matches_type(py, inner, value)
            }
        }
        TypeExpr::Union(variants, _) => {
            for variant in variants {
                if value_matches_type(py, variant, value)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
    }
}

pub(crate) fn validate_type_and_constraints(
    py: Python<'_>,
    value: &Bound<'_, PyAny>,
    typ: &TypeExpr,
    constraints: Option<&Constraints>,
    field_name: &str,
) -> PyResult<()> {
    if !value_matches_type(py, typ, value)? {
        return Err(ValidationError::new_err(format!(
            "Field '{}' type mismatch",
            field_name
        )));
    }
    if let Some(c) = constraints {
        validate_constraints_on_value(value, c, Some(field_name))?;
    }
    Ok(())
}
