# Tarsio 知识库

**每次回答都以"Tarsio:"开头**
**项目**: Rust 核心驱动的高性能 Python Tars (JCE) 协议库。
**架构**: Rust (`src/`) 处理协议编解码 (重活); Python (`python/`) 提供 Struct 模型与 API (接口)。

## 📂 关键目录

| 路径             | 说明                                            |
|------------------|-------------------------------------------------|
| `src/codec/`     | **核心逻辑**。纯 Rust JCE 编解码，零拷贝优化。  |
| `src/binding/`   | **PyO3 绑定**。`SchemaRegistry` 及 FFI 胶水层。 |
| `python/tarsio/` | **用户 API**。`Struct`, `Field` 定义。          |
| `python/tests/`  | **集成测试**。所有 Python 测试平铺于此。        |

## 🛠️ 开发工具链

* **管理**: `uv` (Python 依赖), `cargo` (Rust).
* **构建**: `maturin` (混合构建).
* **命令**:
    * `uv run maturin develop`: 编译 Rust 扩展。
    * `uv run pytest`: 运行集成测试。
    * `cargo test`: 运行 Rust 核心测试。

## 🤖 AI 编码指令 (CRITICAL)

### 1. 架构边界

* **Rust (src)**: 处理所有字节操作、内存管理、WireType 逻辑。禁止 `panic!`。
* **Python (python)**: 提供 `typing.Annotated` 风格的 API。运行时通过类型注解构建 Schema 传给 Rust。实际的 `Struct`、`Meta`、`encode`、`decode` 等均为 Rust PyO3 绑定对象，Python 层仅负责导出和 CLI 工具。

### 2. 测试规范 (严格遵守)

* **Python 测试 (`python/tests/`)**:
    * **风格**: 必须用 `pytest` 函数式写法 (`def test_...`)。**禁止使用 class (`class Test...`)**。
    * **结构**: 所有文件平铺在 `python/tests/`，**禁止子目录**。
    * **内容**: 只验证**可观察行为** (Input/Output/Exception)。禁止断言 Rust 内部实现细节。
    * **原子性**: 一个测试只测一个点。
* **Rust 测试 (`src/`)**:
    * 单元测试写在对应文件的 `mod tests`。
    * 使用 `proptest` 进行 Fuzzing/Roundtrip 测试。

### 3. 代码风格

* **语言**: 注释和 Commit Message 必须用**中文**。
* **规范**: 遵循 `CONTRIBUTING.md` 中的详细规约。

### 4. 常见陷阱

* **不要** 在 Python 测试中模拟 Rust 的 WireType 布局，除非是专门的 Protocol Baseline 测试。
* **不要** 创建新的测试子目录，保持扁平结构。
* **务必** 在修改 Rust 代码后运行 `uv run maturin develop` 更新扩展。
