use thiserror::Error;

/// JCE 编解码过程中可能发生的底层错误.
#[derive(Error, Debug, PartialEq)]
pub enum Error {
    /// 遇到了自定义的错误情况.
    #[error("Error at offset {offset}: {msg}")]
    Custom { offset: usize, msg: String },

    /// 缓冲区数据不足 (尝试读取超出范围的数据).
    #[error(
        "Unexpected end of buffer at offset {offset} (required {required}, available {available})"
    )]
    BufferOverflow {
        offset: usize,
        required: usize,
        available: usize,
    },

    /// 遇到了未知的或非法的 JCE 类型 ID.
    #[error("Invalid type id {type_id} at offset {offset} (expected {expected_types})")]
    InvalidType {
        offset: usize,
        type_id: u8,
        expected_types: &'static str,
    },
}

impl Error {
    /// 创建一个新的自定义错误.
    pub fn new(offset: usize, msg: impl Into<String>) -> Self {
        Self::Custom {
            offset,
            msg: msg.into(),
        }
    }

    /// 创建一个缓冲区溢出错误.
    pub fn buffer_overflow(offset: usize, required: usize, available: usize) -> Self {
        Self::BufferOverflow {
            offset,
            required,
            available,
        }
    }

    /// 创建一个非法类型错误.
    pub fn invalid_type(offset: usize, type_id: u8, expected_types: &'static str) -> Self {
        Self::InvalidType {
            offset,
            type_id,
            expected_types,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
