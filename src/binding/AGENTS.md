# 项目知识库

**生成时间:** 2026-02-03
**上下文:** 连接 Python 对象与 Rust codec 的 PyO3 胶水层

## 概览

将 Python 对象桥接到 Rust 编解码器。

## 结构

* **schema.rs**: 注册表 + Struct 定义。
* **ser.rs**: 编码逻辑。
* **de.rs**: 解码逻辑。

## 关键机制

`LazyLock<RwLock<SchemaRegistry>>` 将 Python 类型映射到 JCE schemas。

## 约定

* 使用 `#[pyclass]`, `#[pymethods]`。
* 处处处理 `PyResult`。
