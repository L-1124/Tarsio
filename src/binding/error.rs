use pyo3::create_exception;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::fmt;

create_exception!(tarsio._core, ValidationError, PyValueError);

#[derive(Debug, Clone)]
pub enum PathItem {
    Field(String),
    Tag(u8),
    Index(usize),
    Key(String),
    Root,
}

impl fmt::Display for PathItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PathItem::Field(s) => write!(f, ".{}", s),
            PathItem::Tag(t) => write!(f, ".<tag:{}>", t),
            PathItem::Index(i) => write!(f, "[{}]", i),
            PathItem::Key(k) => write!(f, "[\"{}\"]", k),
            PathItem::Root => write!(f, "<root>"),
        }
    }
}

#[derive(Debug)]
pub struct DeError {
    pub msg: String,
    pub path: Vec<PathItem>,
    pub cause: Option<PyErr>,
    pub passthrough: bool,
}

impl DeError {
    pub fn new(msg: String) -> Self {
        Self {
            msg,
            path: Vec::new(),
            cause: None,
            passthrough: false,
        }
    }

    pub fn wrap(err: PyErr) -> Self {
        Self {
            msg: err.to_string(),
            path: Vec::new(),
            cause: Some(err),
            passthrough: false,
        }
    }

    pub fn passthrough(err: PyErr) -> Self {
        Self {
            msg: err.to_string(),
            path: Vec::new(),
            cause: Some(err),
            passthrough: true,
        }
    }

    pub fn prepend(mut self, item: PathItem) -> Self {
        self.path.push(item);
        self
    }

    pub fn to_pyerr(mut self, py: Python<'_>) -> PyErr {
        if self.passthrough
            && let Some(cause) = &self.cause
        {
            return cause.clone_ref(py);
        }

        // 如果根本原因是 ValidationError，直接抛出，不附加路径信息
        if let Some(cause) = &self.cause
            && cause.is_instance_of::<ValidationError>(py)
        {
            return cause.clone_ref(py);
        }

        // 确保路径以 Root 开头（如果不为空）或者至少表明这是根
        if self.path.is_empty() || !matches!(self.path.last(), Some(PathItem::Root)) {
            self.path.push(PathItem::Root);
        }

        // 因为 prepend 是 push 到末尾，所以需要反转
        self.path.reverse();

        let mut path_str = String::new();
        for item in &self.path {
            use std::fmt::Write;
            let _ = write!(&mut path_str, "{}", item);
        }

        let msg = format!("Error at {}: {}", path_str, self.msg);

        if let Some(cause) = self.cause {
            let new_err = ValidationError::new_err(msg);
            new_err.set_cause(py, Some(cause));
            new_err
        } else {
            ValidationError::new_err(msg)
        }
    }
}

impl<E: std::error::Error> From<E> for DeError {
    fn from(err: E) -> Self {
        Self::new(err.to_string())
    }
}

pub type DeResult<T> = Result<T, DeError>;
