//! Struct 实例构造辅助逻辑。

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyString, PyTuple};
use smallvec::SmallVec;

use crate::binding::ir::{FieldDef, StructDef};
use crate::binding::validation::validate_type_and_constraints;

fn set_field_value(
    self_obj: &Bound<'_, PyAny>,
    field: &FieldDef,
    value: &Bound<'_, PyAny>,
) -> PyResult<()> {
    // SAFETY:
    // 1. `self_obj` 与 `field.name_py` 是同一解释器内的有效对象。
    // 2. `value` 在调用期间保持存活，C API 不会窃取其引用。
    // 3. 失败时 Python 异常已设置，立即抓取并返回。
    unsafe {
        let name_py = field.name_py.bind(self_obj.py());
        let res =
            pyo3::ffi::PyObject_GenericSetAttr(self_obj.as_ptr(), name_py.as_ptr(), value.as_ptr());
        if res != 0 {
            return Err(PyErr::fetch(self_obj.py()));
        }
    }
    Ok(())
}

fn missing_required_argument_error(field: &FieldDef) -> PyErr {
    pyo3::exceptions::PyTypeError::new_err(format!(
        "__init__() missing 1 required positional argument: '{}'",
        field.name
    ))
}

pub(crate) fn run_post_init(self_obj: &Bound<'_, PyAny>) -> PyResult<()> {
    let py = self_obj.py();
    match self_obj.getattr("__post_init__") {
        Ok(post_init) => {
            post_init.call0()?;
            Ok(())
        }
        Err(err) => {
            if err.is_instance_of::<pyo3::exceptions::PyAttributeError>(py) {
                Ok(())
            } else {
                Err(err)
            }
        }
    }
}

#[inline]
fn lookup_keyword_index(def: &StructDef, key: &Bound<'_, PyAny>) -> PyResult<Option<usize>> {
    if let Ok(key_str_obj) = key.cast::<PyString>() {
        let key_ptr = key_str_obj.as_ptr() as usize;
        if let Some(idx) = def.meta.name_ptr_to_index.get(&key_ptr) {
            return Ok(Some(*idx));
        }

        let key_str = key_str_obj.to_str()?;
        return Ok(def.meta.name_to_index.get(key_str).copied());
    }
    Ok(None)
}

pub fn construct_instance(
    def: &StructDef,
    self_obj: &Bound<'_, PyAny>,
    args: &Bound<'_, PyTuple>,
    kwargs: Option<&Bound<'_, PyDict>>,
) -> PyResult<()> {
    let py = self_obj.py();
    let num_positional = args.len();
    let num_fields = def.fields_sorted.len();

    if def.kw_only && num_positional > 0 {
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "__init__() takes 0 positional arguments but {} were given",
            num_positional
        )));
    }

    if num_positional > num_fields {
        let expected = num_fields + 1;
        let given = num_positional + 1;
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "__init__() takes {} positional arguments but {} were given",
            expected, given
        )));
    }

    let no_kwargs = kwargs.is_none_or(|k| k.is_empty());
    if no_kwargs && num_positional == num_fields {
        for (idx, field) in def.fields_sorted.iter().enumerate() {
            let val = args.get_item(idx)?;
            if !(field.is_optional && val.is_none()) {
                validate_type_and_constraints(
                    py,
                    &val,
                    &field.ty,
                    field.constraints.as_deref(),
                    field.name.as_str(),
                )?;
            }
            set_field_value(self_obj, field, &val)?;
        }
        run_post_init(self_obj)?;
        return Ok(());
    }

    let mut mapped_values: SmallVec<[Option<Py<PyAny>>; 16]> =
        std::iter::repeat_with(|| None).take(num_fields).collect();

    for (idx, slot) in mapped_values.iter_mut().enumerate().take(num_positional) {
        *slot = Some(args.get_item(idx)?.unbind());
    }

    if let Some(k) = kwargs {
        const KWARGS_DIRECT_ITER_THRESHOLD: usize = 8;
        let kw_len = k.len();
        let use_kwargs_iteration =
            kw_len <= KWARGS_DIRECT_ITER_THRESHOLD || kw_len.saturating_mul(2) < num_fields;

        if use_kwargs_iteration {
            for (key, value) in k.iter() {
                let idx = lookup_keyword_index(def, &key)?.ok_or_else(|| {
                    pyo3::exceptions::PyTypeError::new_err(format!(
                        "__init__() got an unexpected keyword argument '{}'",
                        key.extract::<String>()
                            .unwrap_or_else(|_| "<non-string-key>".to_string())
                    ))
                })?;

                if idx < num_positional || mapped_values[idx].is_some() {
                    return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                        "__init__() got multiple values for argument '{}'",
                        def.fields_sorted[idx].name
                    )));
                }
                mapped_values[idx] = Some(value.unbind());
            }
        } else {
            let mut matched = 0usize;
            for (idx, field) in def.fields_sorted.iter().enumerate() {
                if let Some(value) = k.get_item(field.name_py.bind(py))? {
                    matched += 1;
                    if idx < num_positional {
                        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                            "__init__() got multiple values for argument '{}'",
                            field.name
                        )));
                    }
                    mapped_values[idx] = Some(value.unbind());
                }
            }

            if matched != kw_len {
                for key in k.keys() {
                    if lookup_keyword_index(def, &key)?.is_none() {
                        let key_str = key
                            .extract::<String>()
                            .unwrap_or_else(|_| "<non-string-key>".to_string());
                        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                            "__init__() got an unexpected keyword argument '{}'",
                            key_str
                        )));
                    }
                }
            }
        }
    }

    for (idx, field) in def.fields_sorted.iter().enumerate() {
        let val_to_set = match mapped_values[idx].as_ref() {
            Some(v) => v.bind(py).clone(),
            None => {
                if let Some(default_value) = field.default_value.as_ref() {
                    default_value.bind(py).clone()
                } else if let Some(factory) = field.default_factory.as_ref() {
                    factory.bind(py).call0()?
                } else if field.is_optional || field.is_required {
                    return Err(missing_required_argument_error(field));
                } else {
                    return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                        "__init__() missing 1 required argument: '{}'",
                        field.name
                    )));
                }
            }
        };

        if !(field.is_optional && val_to_set.is_none()) {
            validate_type_and_constraints(
                py,
                &val_to_set,
                &field.ty,
                field.constraints.as_deref(),
                field.name.as_str(),
            )?;
        }
        set_field_value(self_obj, field, &val_to_set)?;
    }

    run_post_init(self_obj)?;
    Ok(())
}
