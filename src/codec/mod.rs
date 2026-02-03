//! JCE 编解码底层实现模块。
//!
//! 提供对于 JCE 二进制流的低级读写操作。
//! 通常用户不需要直接使用此模块，而是通过 `api::runtime` 进行高级操作。

pub mod consts;

pub mod error;
pub mod reader;
pub mod scanner;
pub mod writer;
