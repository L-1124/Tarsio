//! Binding 层统一类型 IR 入口。
//!
//! 该模块集中导出运行时 Schema、约束与类型表达式。解析层的临时
//! `TypeInfoIR` 仍作为前端适配输入存在，编译后统一落到这里的类型。

pub use crate::binding::core::{
    Constraints, FieldDef, StructDef, StructMetaData, TypeExpr, UnionCache, WireType,
};
