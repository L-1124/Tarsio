# 项目知识库

**生成时间:** 2026-02-03
**上下文:** 纯 Rust Tars 协议实现

## 概览

底层 JCE 协议处理。专注于零拷贝读取和高效写入。
解析 Tars 二进制格式的核心逻辑。

## 结构

```tree
src/codec/
├── reader.rs      # 反序列化 (Reader trait, TarsReader)
├── writer.rs      # 序列化 (Writer trait, TarsWriter)
├── consts.rs      # JCE tags 和 wire types
├── error.rs       # 编解码特定错误
└── mod.rs         # 模块导出
```

## 约定

* **内存**: 使用 `bytes` crate 进行缓冲区管理。避免不必要的分配。
* **安全**: 首选 Safe Rust。仅在零拷贝必须时使用 `unsafe`。
* **Traits**: `Reader` 和 `Writer` trait 抽象底层缓冲区操作。

## 备注

* 关键性能路径。
* `reader.rs` 处理未知 tag 的跳过逻辑。
* `writer.rs` 管理缓冲区增长。
