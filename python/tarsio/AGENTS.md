# 项目知识库

**生成时间:** 2026-02-03
**上下文:** 用户面 Python 包 (Tarsio)

## 概览

Tars (JCE) 协议的高级 API。
将用户代码桥接到高性能 Rust 核心。

## 结构

* `__init__.py`: 公共导出，主要 API 表面。
* `_core`: Rust 扩展模块导入 (通过 PyO3/Maturin 编译)。

## 约定

* **类型提示 (Type Hints)**: 具有语义；严格用于运行时 JCE schema 生成。
* **模型定义**: 子类化 `tarsio.Struct` + 类型注解字段。

## CLI

* `__main__.py`: 命令行工具入口点。
