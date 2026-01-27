use pyo3::prelude::*;
use pyo3::types::{PyCapsule, PyList, PyTuple};
use std::collections::HashMap;

#[derive(Debug)]
pub struct FieldDef {
    pub name: String,
    pub tag: u8,
    pub jce_type: u8,
    pub default_val: Py<PyAny>,
    pub has_serializer: bool,
}

#[derive(Debug)]
pub struct CompiledSchema {
    pub fields: Vec<FieldDef>,
    pub tag_map: HashMap<u8, usize>,
}

/// 将 Python 列表定义的结构编译为包装在 PyCapsule 中的 Rust CompiledSchema。
///
/// 参数:
///     schema_list: 元组列表 (name, tag, type, default, has_serializer)。
///
/// 返回:
///     包含 CompiledSchema 的 PyCapsule。
pub fn compile_schema(py: Python<'_>, schema_list: &Bound<'_, PyList>) -> PyResult<Py<PyCapsule>> {
    let mut fields = Vec::with_capacity(schema_list.len());
    let mut tag_map = HashMap::new();

    for (idx, item) in schema_list.iter().enumerate() {
        let tuple = item
            .cast::<PyTuple>()
            .map_err(|_| pyo3::exceptions::PyTypeError::new_err("Schema item must be a tuple"))?;

        if tuple.len() != 5 {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "Schema item must have 5 elements, got {}",
                tuple.len()
            )));
        }

        let name: String = tuple.get_item(0)?.extract()?;
        let tag: u8 = tuple.get_item(1)?.extract()?;
        let jce_type_code: u8 = tuple.get_item(2)?.extract()?;
        let default_val = tuple.get_item(3)?.unbind();
        let has_serializer: bool = tuple.get_item(4)?.extract()?;

        if tag_map.contains_key(&tag) {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "Duplicate tag {} in schema",
                tag
            )));
        }

        tag_map.insert(tag, idx);
        fields.push(FieldDef {
            name,
            tag,
            jce_type: jce_type_code,
            default_val,
            has_serializer,
        });
    }

    let compiled = CompiledSchema { fields, tag_map };
    // let name = CString::new("jce._jce_core.CompiledSchema").unwrap();

    // PyCapsule::new 自动处理析构函数，确保 Box 被释放。
    // 该名称用于在检索 capsule 时进行类型检查。
    let capsule = PyCapsule::new(py, compiled, None)?;
    Ok(capsule.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_schema() {
        #[allow(deprecated)]
        pyo3::prepare_freethreaded_python();
        Python::attach(|py| {
            let schema_list = PyList::empty(py);

            schema_list.append(("uid", 0, 0, 0, false)).unwrap();
            schema_list
                .append(("name", 1, 6, "unknown", false))
                .unwrap();

            let capsule = compile_schema(py, &schema_list).unwrap();
            let bound = capsule.bind(py);
            assert!(bound.is_valid_checked(None));

            // 验证内容
            let ptr = bound.pointer_checked(None).expect("Capsule pointer error");
            let schema: &CompiledSchema = unsafe { &*(ptr.as_ptr() as *const CompiledSchema) };
            assert_eq!(schema.fields.len(), 2);
            assert_eq!(schema.fields[0].name, "uid");
            assert_eq!(schema.fields[1].name, "name");
            assert_eq!(schema.tag_map.get(&0), Some(&0));
            assert_eq!(schema.tag_map.get(&1), Some(&1));
        });
    }

    #[test]
    fn test_duplicate_tag() {
        #[allow(deprecated)]
        pyo3::prepare_freethreaded_python();
        Python::attach(|py| {
            let schema_list = PyList::empty(py);
            schema_list.append(("f1", 0, 0, 0, false)).unwrap();
            schema_list.append(("f2", 0, 0, 0, false)).unwrap();

            let res = compile_schema(py, &schema_list);
            assert!(res.is_err());
        });
    }
}
