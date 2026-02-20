use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyType};
use simdutf8::basic::from_utf8;
use std::sync::Arc;

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

    let mut stack = vec![TraceFrame::Struct(StructFrame {
        parent: root.clone_ref(py),
        def,
        parent_path: "<root>".to_string(),
        depth: 0,
    })];
    run_trace_frames(py, &mut reader, &mut stack)?;

    Ok(root)
}

#[derive(Clone)]
enum TraceTypeHint {
    StructDef(Arc<StructDef>),
    List(Box<TraceTypeHint>),
    Map(Box<TraceTypeHint>, Box<TraceTypeHint>),
}

struct StructFrame {
    parent: Py<TraceNode>,
    def: Option<Arc<StructDef>>,
    parent_path: String,
    depth: usize,
}

struct ValueFrame {
    node: Py<TraceNode>,
    type_id: TarsType,
    type_hint: Option<TraceTypeHint>,
    path: String,
    depth: usize,
}

struct ListFrame {
    parent: Py<TraceNode>,
    path: String,
    len: usize,
    idx: usize,
    inner_hint: Option<TraceTypeHint>,
    depth: usize,
}

enum MapPhase {
    EntryStart,
    AfterKey { key_node: Py<TraceNode> },
    AfterValue,
}

struct MapFrame {
    parent: Py<TraceNode>,
    path: String,
    len: usize,
    idx: usize,
    key_hint: Option<TraceTypeHint>,
    val_hint: Option<TraceTypeHint>,
    depth: usize,
    phase: MapPhase,
}

enum TraceFrame {
    Struct(StructFrame),
    Value(ValueFrame),
    List(ListFrame),
    Map(MapFrame),
}

#[inline]
fn check_trace_depth(depth: usize) -> PyResult<()> {
    if depth >= crate::binding::utils::MAX_DEPTH {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Trace recursion depth exceeded (max={}, observed={})",
            crate::binding::utils::MAX_DEPTH,
            depth
        )));
    }
    Ok(())
}

fn type_hint_from_expr(py: Python<'_>, type_expr: Option<&TypeExpr>) -> Option<TraceTypeHint> {
    match type_expr? {
        TypeExpr::Struct(cls_obj) => {
            let cls = cls_obj.bind(py);
            ensure_schema_for_class(py, cls)
                .ok()
                .map(TraceTypeHint::StructDef)
        }
        TypeExpr::List(inner) => {
            type_hint_from_expr(py, Some(inner.as_ref())).map(|v| TraceTypeHint::List(Box::new(v)))
        }
        TypeExpr::Map(k, v) => {
            let kh = type_hint_from_expr(py, Some(k.as_ref()))?;
            let vh = type_hint_from_expr(py, Some(v.as_ref()))?;
            Some(TraceTypeHint::Map(Box::new(kh), Box::new(vh)))
        }
        _ => None,
    }
}

fn run_trace_frames<'py>(
    py: Python<'py>,
    reader: &mut TarsReader,
    stack: &mut Vec<TraceFrame>,
) -> PyResult<()> {
    while let Some(frame) = stack.pop() {
        match frame {
            TraceFrame::Struct(frame) => {
                check_trace_depth(frame.depth)?;
                if reader.is_end() {
                    continue;
                }
                let (tag, type_id) = match reader.peek_head() {
                    Ok(h) => h,
                    Err(_) => continue,
                };
                if type_id == TarsType::StructEnd {
                    let _ = reader.read_head();
                    continue;
                }

                let _ = reader.read_head();

                let mut name = None;
                let mut type_name = None;
                let mut field_hint = None;
                if let Some(def) = frame.def.as_deref()
                    && (tag as usize) < def.tag_lookup_vec.len()
                    && let Some(idx) = def.tag_lookup_vec[tag as usize]
                {
                    let f = &def.fields_sorted[idx];
                    name = Some(f.name.clone());
                    type_name = Some(format!("{:?}", f.ty));
                    field_hint = type_hint_from_expr(py, Some(&f.ty));
                }

                let path = if let Some(n) = &name {
                    format!("{}.{}", frame.parent_path, n)
                } else {
                    format!("{}.<tag:{}>", frame.parent_path, tag)
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
                frame
                    .parent
                    .borrow_mut(py)
                    .children
                    .push(node.clone_ref(py));
                let next_depth = frame.depth + 1;

                stack.push(TraceFrame::Struct(frame));
                stack.push(TraceFrame::Value(ValueFrame {
                    node,
                    type_id,
                    type_hint: field_hint,
                    path,
                    depth: next_depth,
                }));
            }
            TraceFrame::Value(frame) => match frame.type_id {
                TarsType::ZeroTag
                | TarsType::Int1
                | TarsType::Int2
                | TarsType::Int4
                | TarsType::Int8 => {
                    let v = reader.read_int(frame.type_id).unwrap_or(0);
                    frame.node.borrow_mut(py).value =
                        Some(v.into_pyobject(py)?.into_any().unbind());
                }
                TarsType::Float => {
                    let v = reader.read_float(frame.type_id).unwrap_or(0.0);
                    frame.node.borrow_mut(py).value =
                        Some(v.into_pyobject(py)?.into_any().unbind());
                }
                TarsType::Double => {
                    let v = reader.read_double(frame.type_id).unwrap_or(0.0);
                    frame.node.borrow_mut(py).value =
                        Some(v.into_pyobject(py)?.into_any().unbind());
                }
                TarsType::String1 | TarsType::String4 => {
                    if let Ok(bytes) = reader.read_string(frame.type_id) {
                        if let Ok(s) = from_utf8(bytes) {
                            frame.node.borrow_mut(py).value =
                                Some(s.into_pyobject(py)?.into_any().unbind());
                        } else {
                            frame.node.borrow_mut(py).value =
                                Some(PyBytes::new(py, bytes).into_any().unbind());
                        }
                    }
                }
                TarsType::StructBegin => {
                    let nested_def = match frame.type_hint {
                        Some(TraceTypeHint::StructDef(def)) => Some(def),
                        _ => None,
                    };
                    stack.push(TraceFrame::Struct(StructFrame {
                        parent: frame.node,
                        def: nested_def,
                        parent_path: frame.path,
                        depth: frame.depth,
                    }));
                }
                TarsType::List => {
                    let len = reader.read_size().unwrap_or(0) as usize;
                    frame.node.borrow_mut(py).value = Some(
                        format!("<List len={}>", len)
                            .into_pyobject(py)?
                            .into_any()
                            .unbind(),
                    );
                    let inner_hint = match frame.type_hint {
                        Some(TraceTypeHint::List(inner)) => Some(*inner),
                        _ => None,
                    };
                    stack.push(TraceFrame::List(ListFrame {
                        parent: frame.node,
                        path: frame.path,
                        len,
                        idx: 0,
                        inner_hint,
                        depth: frame.depth,
                    }));
                }
                TarsType::Map => {
                    let len = reader.read_size().unwrap_or(0) as usize;
                    frame.node.borrow_mut(py).value = Some(
                        format!("<Map len={}>", len)
                            .into_pyobject(py)?
                            .into_any()
                            .unbind(),
                    );
                    let (key_hint, val_hint) = match frame.type_hint {
                        Some(TraceTypeHint::Map(key, val)) => (Some(*key), Some(*val)),
                        _ => (None, None),
                    };
                    stack.push(TraceFrame::Map(MapFrame {
                        parent: frame.node,
                        path: frame.path,
                        len,
                        idx: 0,
                        key_hint,
                        val_hint,
                        depth: frame.depth,
                        phase: MapPhase::EntryStart,
                    }));
                }
                TarsType::SimpleList => {
                    let _subtype = reader.read_u8().unwrap_or(0);
                    let len = reader.read_size().unwrap_or(0) as usize;
                    let bytes = reader.read_bytes(len).unwrap_or(&[]);
                    frame.node.borrow_mut(py).value =
                        Some(PyBytes::new(py, bytes).into_any().unbind());
                    frame.node.borrow_mut(py).jce_type = "SimpleList".to_string();
                }
                _ => {
                    frame.node.borrow_mut(py).value =
                        Some("UNSUPPORTED".into_pyobject(py)?.into_any().unbind());
                }
            },
            TraceFrame::List(frame) => {
                check_trace_depth(frame.depth)?;
                if frame.idx >= frame.len {
                    continue;
                }
                let (tag, item_type_id) = reader.read_head().unwrap_or((0, TarsType::ZeroTag));
                let item_path = format!("{}[{}]", frame.path, frame.idx);
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
                frame
                    .parent
                    .borrow_mut(py)
                    .children
                    .push(child.clone_ref(py));

                stack.push(TraceFrame::List(ListFrame {
                    parent: frame.parent,
                    path: frame.path,
                    len: frame.len,
                    idx: frame.idx + 1,
                    inner_hint: frame.inner_hint.clone(),
                    depth: frame.depth,
                }));
                stack.push(TraceFrame::Value(ValueFrame {
                    node: child,
                    type_id: item_type_id,
                    type_hint: frame.inner_hint,
                    path: item_path,
                    depth: frame.depth + 1,
                }));
            }
            TraceFrame::Map(frame) => {
                check_trace_depth(frame.depth)?;
                match frame.phase {
                    MapPhase::EntryStart => {
                        if frame.idx >= frame.len {
                            continue;
                        }
                        let (ktag, ktype) = reader.read_head().unwrap_or((0, TarsType::ZeroTag));
                        let key_path = format!("{}[{}].key", frame.path, frame.idx);
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
                        frame
                            .parent
                            .borrow_mut(py)
                            .children
                            .push(key_node.clone_ref(py));

                        stack.push(TraceFrame::Map(MapFrame {
                            parent: frame.parent,
                            path: frame.path,
                            len: frame.len,
                            idx: frame.idx,
                            key_hint: frame.key_hint.clone(),
                            val_hint: frame.val_hint,
                            depth: frame.depth,
                            phase: MapPhase::AfterKey {
                                key_node: key_node.clone_ref(py),
                            },
                        }));
                        stack.push(TraceFrame::Value(ValueFrame {
                            node: key_node,
                            type_id: ktype,
                            type_hint: frame.key_hint,
                            path: key_path,
                            depth: frame.depth + 1,
                        }));
                    }
                    MapPhase::AfterKey { key_node } => {
                        let key_repr = if let Some(v) = &key_node.borrow(py).value {
                            v.bind(py)
                                .str()
                                .ok()
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| "key".to_string())
                        } else {
                            "key".to_string()
                        };

                        let (vtag, vtype) = reader.read_head().unwrap_or((1, TarsType::ZeroTag));
                        let val_path = format!("{}[{:?}]", frame.path, key_repr);
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
                        frame
                            .parent
                            .borrow_mut(py)
                            .children
                            .push(val_node.clone_ref(py));

                        stack.push(TraceFrame::Map(MapFrame {
                            parent: frame.parent,
                            path: frame.path,
                            len: frame.len,
                            idx: frame.idx + 1,
                            key_hint: frame.key_hint,
                            val_hint: frame.val_hint.clone(),
                            depth: frame.depth,
                            phase: MapPhase::AfterValue,
                        }));
                        stack.push(TraceFrame::Value(ValueFrame {
                            node: val_node,
                            type_id: vtype,
                            type_hint: frame.val_hint,
                            path: val_path,
                            depth: frame.depth + 1,
                        }));
                    }
                    MapPhase::AfterValue => {
                        stack.push(TraceFrame::Map(MapFrame {
                            parent: frame.parent,
                            path: frame.path,
                            len: frame.len,
                            idx: frame.idx,
                            key_hint: frame.key_hint,
                            val_hint: frame.val_hint,
                            depth: frame.depth,
                            phase: MapPhase::EntryStart,
                        }));
                    }
                }
            }
        }
    }

    Ok(())
}
