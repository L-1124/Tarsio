use crate::codec::consts::JceType;
use std::fmt;

/// 错误上下文，用于跟踪反序列化路径以提供有用的错误信息。
#[derive(Debug, Clone, Default)]
pub struct ErrorContext {
    /// 栈结构: (字段名, Tag, TarsType)
    stack: Vec<(String, u8, u8)>,
}

impl ErrorContext {
    pub fn new() -> Self {
        Self {
            stack: Vec::with_capacity(8),
        }
    }

    pub fn push_field(&mut self, name: &str, tag: u8, tars_type: u8) {
        self.stack.push((name.to_string(), tag, tars_type));
    }

    pub fn push_tag(&mut self, tag: u8, tars_type: u8) {
        self.stack.push((tag.to_string(), tag, tars_type));
    }

    pub fn pop(&mut self) {
        self.stack.pop();
    }
}

impl fmt::Display for ErrorContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for (name, tag, tars_type) in &self.stack {
            if !first {
                write!(f, " -> ")?;
            }
            first = false;

            let type_name = match JceType::try_from(*tars_type) {
                Ok(JceType::Int1) | Ok(JceType::Int2) | Ok(JceType::Int4) | Ok(JceType::Int8) => {
                    "Int"
                }
                Ok(JceType::Float) => "Float",
                Ok(JceType::Double) => "Double",
                Ok(JceType::String1) | Ok(JceType::String4) => "String",
                Ok(JceType::Map) => "Map",
                Ok(JceType::List) | Ok(JceType::SimpleList) => "List",
                Ok(JceType::StructBegin) | Ok(JceType::StructEnd) => "Struct",
                Ok(JceType::ZeroTag) => "ZeroTag",
                Err(_) => "Unknown",
            };

            write!(f, "{}[{}]({})", name, tag, type_name)?;
        }
        Ok(())
    }
}
