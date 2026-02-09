use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyType};
use simdutf8::basic::from_utf8;

use crate::binding::schema::{StructDef, TypeExpr, ensure_schema_for_class};
use crate::codec::consts::TarsType;
use crate::codec::reader::TarsReader;

#[pyclass(module = "tarsio._core", get_all)]
pub struct TraceNode {
    pub tag: u8,
    pub jce_type: String,
    pub value: Option<Py<PyAny>>,
    pub children: Vec<Py<TraceNode>>,
    pub name: Option<String>,
    pub type_name: Option<String>,
    pub path: String,
}

#[pymethods]
impl TraceNode {
    fn __repr__(&self) -> String {
        format!(
            "<TraceNode tag={} type={} path={}>",
            self.tag, self.jce_type, self.path
        )
    }

    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("tag", self.tag)?;
        dict.set_item("jce_type", &self.jce_type)?;
        dict.set_item("value", self.value.as_ref())?;

        let children_dicts = PyList::empty(py);
        for child_py in &self.children {
            let child = child_py.borrow(py);
            children_dicts.append(child.to_dict(py)?)?;
        }
        dict.set_item("children", children_dicts)?;

        dict.set_item("name", &self.name)?;
        dict.set_item("type_name", &self.type_name)?;
        dict.set_item("path", &self.path)?;
        Ok(dict.into())
    }
}

/// 解析二进制数据并生成追踪树.
#[pyfunction]
#[pyo3(signature = (data, cls=None))]
pub fn decode_trace<'py>(
    py: Python<'py>,
    data: &[u8],
    cls: Option<&Bound<'py, PyType>>,
) -> PyResult<Py<TraceNode>> {
    let mut reader = TarsReader::new(data);
    let mut def = None;
    if let Some(c) = cls
        && let Ok(d) = ensure_schema_for_class(py, c)
    {
        def = Some(d);
    }

    let root = Py::new(
        py,
        TraceNode {
            tag: 0,
            jce_type: "ROOT".to_string(),
            value: None,
            children: Vec::new(),
            name: None,
            type_name: cls.and_then(|c| c.name().ok().map(|s| s.to_string())),
            path: "<root>".to_string(),
        },
    )?;

    parse_struct_fields(py, &mut reader, &root, def.as_deref(), "<root>", 0)?;

    Ok(root)
}

fn parse_struct_fields<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    parent: &Py<TraceNode>,
    def: Option<&StructDef>,
    parent_path: &str,
    depth: usize,
) -> PyResult<()> {
    if depth > 500 {
        return Ok(());
    }

    while !reader.is_end() {
        let (tag, type_id) = match reader.peek_head() {
            Ok(h) => h,
            Err(_) => break,
        };

        if type_id == TarsType::StructEnd {
            reader.read_head().ok();
            break;
        }

        reader.read_head().ok();

        let mut field_info = None;
        if let Some(d) = def
            && (tag as usize) < d.tag_lookup_vec.len()
            && let Some(idx) = d.tag_lookup_vec[tag as usize]
        {
            field_info = Some(&d.fields_sorted[idx]);
        }

        let name = field_info.map(|f| f.name.clone());
        let type_name = field_info.map(|f| format!("{:?}", f.ty));
        let path = if let Some(n) = &name {
            format!("{}.{}", parent_path, n)
        } else {
            format!("{}.<tag:{}>", parent_path, tag)
        };

        let node = Py::new(
            py,
            TraceNode {
                tag,
                jce_type: format!("{:?}", type_id),
                value: None,
                children: Vec::new(),
                name,
                type_name,
                path: path.clone(),
            },
        )?;

        parent.borrow_mut(py).children.push(node.clone_ref(py));

        let field_type = field_info.map(|f| &f.ty);
        parse_value(py, reader, type_id, &node, field_type, &path, depth + 1)?;
    }
    Ok(())
}

fn parse_value<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    type_id: TarsType,
    node: &Py<TraceNode>,
    type_expr: Option<&TypeExpr>,
    path: &str,
    depth: usize,
) -> PyResult<()> {
    match type_id {
        TarsType::ZeroTag | TarsType::Int1 | TarsType::Int2 | TarsType::Int4 | TarsType::Int8 => {
            let v = reader.read_int(type_id).unwrap_or(0);
            node.borrow_mut(py).value = Some(v.into_pyobject(py)?.into_any().unbind());
        }
        TarsType::Float => {
            let v = reader.read_float(type_id).unwrap_or(0.0);
            node.borrow_mut(py).value = Some(v.into_pyobject(py)?.into_any().unbind());
        }
        TarsType::Double => {
            let v = reader.read_double(type_id).unwrap_or(0.0);
            node.borrow_mut(py).value = Some(v.into_pyobject(py)?.into_any().unbind());
        }
        TarsType::String1 | TarsType::String4 => {
            if let Ok(bytes) = reader.read_string(type_id) {
                if let Ok(s) = from_utf8(bytes) {
                    node.borrow_mut(py).value = Some(s.into_pyobject(py)?.into_any().unbind());
                } else {
                    node.borrow_mut(py).value = Some(PyBytes::new(py, bytes).into_any().unbind());
                }
            }
        }
        TarsType::StructBegin => {
            let nested_def = if let Some(TypeExpr::Struct(ptr)) = type_expr {
                unsafe {
                    let obj_ptr = *ptr as *mut pyo3::ffi::PyObject;
                    let any_obj = Bound::from_borrowed_ptr(py, obj_ptr);
                    let cls = any_obj.cast::<PyType>().ok();
                    if let Some(c) = cls {
                        ensure_schema_for_class(py, c).ok()
                    } else {
                        None
                    }
                }
            } else {
                None
            };
            parse_struct_fields(py, reader, node, nested_def.as_deref(), path, depth)?;
        }
        TarsType::List => {
            let len = reader.read_size().unwrap_or(0) as usize;
            node.borrow_mut(py).value = Some(
                format!("<List len={}>", len)
                    .into_pyobject(py)?
                    .into_any()
                    .unbind(),
            );

            let inner_type = if let Some(TypeExpr::List(inner)) = type_expr {
                Some(inner.as_ref())
            } else {
                None
            };

            for i in 0..len {
                let (tag, item_type_id) = reader.read_head().unwrap_or((0, TarsType::ZeroTag));
                let item_path = format!("{}[{}]", path, i);
                let child = Py::new(
                    py,
                    TraceNode {
                        tag,
                        jce_type: format!("{:?}", item_type_id),
                        value: None,
                        children: Vec::new(),
                        name: None,
                        type_name: None,
                        path: item_path.clone(),
                    },
                )?;
                node.borrow_mut(py).children.push(child.clone_ref(py));
                parse_value(
                    py,
                    reader,
                    item_type_id,
                    &child,
                    inner_type,
                    &item_path,
                    depth + 1,
                )?;
            }
        }
        TarsType::Map => {
            let len = reader.read_size().unwrap_or(0) as usize;
            node.borrow_mut(py).value = Some(
                format!("<Map len={}>", len)
                    .into_pyobject(py)?
                    .into_any()
                    .unbind(),
            );

            let (key_type, val_type) = if let Some(TypeExpr::Map(k, v)) = type_expr {
                (Some(k.as_ref()), Some(v.as_ref()))
            } else {
                (None, None)
            };

            for i in 0..len {
                let (ktag, ktype) = reader.read_head().unwrap_or((0, TarsType::ZeroTag));
                let key_path = format!("{}[{}].key", path, i);
                let key_node = Py::new(
                    py,
                    TraceNode {
                        tag: ktag,
                        jce_type: format!("{:?}", ktype),
                        value: None,
                        children: Vec::new(),
                        name: Some("<key>".into()),
                        type_name: None,
                        path: key_path.clone(),
                    },
                )?;
                parse_value(py, reader, ktype, &key_node, key_type, &key_path, depth + 1)?;

                let key_repr = if let Some(v) = &key_node.borrow(py).value {
                    v.bind(py)
                        .str()
                        .ok()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "key".into())
                } else {
                    "key".into()
                };

                let (vtag, vtype) = reader.read_head().unwrap_or((1, TarsType::ZeroTag));
                let val_path = format!("{}[{:?}]", path, key_repr);
                let val_node = Py::new(
                    py,
                    TraceNode {
                        tag: vtag,
                        jce_type: format!("{:?}", vtype),
                        value: None,
                        children: Vec::new(),
                        name: Some(format!("value_of_{}", key_repr)),
                        type_name: None,
                        path: val_path.clone(),
                    },
                )?;

                node.borrow_mut(py).children.push(key_node);
                node.borrow_mut(py).children.push(val_node.clone_ref(py));

                parse_value(py, reader, vtype, &val_node, val_type, &val_path, depth + 1)?;
            }
        }
        TarsType::SimpleList => {
            let _subtype = reader.read_u8().unwrap_or(0);
            let len = reader.read_size().unwrap_or(0) as usize;
            let bytes = reader.read_bytes(len).unwrap_or(&[]);
            node.borrow_mut(py).value = Some(PyBytes::new(py, bytes).into_any().unbind());
            node.borrow_mut(py).jce_type = "SimpleList".to_string();
        }
        _ => {
            node.borrow_mut(py).value = Some("UNSUPPORTED".into_pyobject(py)?.into_any().unbind());
        }
    }
    Ok(())
}
