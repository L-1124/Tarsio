# 项目知识库

**生成时间:** 2026-02-03
**上下文:** Rust 核心库 (Tarsio Core)

## 概览

Tarsio 的 Rust 核心部分，负责所有底层协议处理、性能优化及 Python 绑定实现。

## 结构

* `binding/`: PyO3 绑定层，处理 FFI 和 Python 对象映射。
* `codec/`: 纯 Rust 实现的 JCE 协议编解码器。
* `lib.rs`: 库入口，定义 Python 扩展模块 `_core`。

## 测试约定 (Rust)

### 1. 单元测试 (Unit Tests)

* **位置**: 在源码文件底部的 `mod tests` 模块中。
* **配置**: 使用 `#[cfg(test)]` 宏标记测试模块。
* **范围**: 测试私有函数、核心逻辑及边界条件。
* **示例**:

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn test_encode_sanity() {
          let mut buf = BytesMut::new();
          // ...
          assert_eq!(...);
      }
  }
  ```

### 2. 基于属性的测试 (Property-based Tests)

* **框架**: 使用 `proptest` crate。
* **场景**: 编解码器的**往返测试 (Roundtrip)**。验证 `decode(encode(x)) == x` 对任意输入成立。
* **位置**: 通常位于 `codec/` 下的测试模块中。
* **宏**: 使用 `proptest!` 宏定义策略。

### 3. 基准测试 (Benchmarks)

* **框架**: 使用 `criterion` crate。
* **目标**: 关键路径性能监控（读/写/扫描）。
* **命令**: `cargo bench`。

### 4. 代码质量

* **格式化**: 提交前必须运行 `cargo fmt`。
* **Linting**: 必须通过 `cargo clippy --all-targets --all-features`。
* **Panic**: 核心代码禁止 `panic!`（除了测试中），必须返回 `Result`。

## 常用命令

```bash
# 运行所有 Rust 测试
cargo test

# 运行特定测试
cargo test test_name

# 运行 Clippy 检查
cargo clippy
```
