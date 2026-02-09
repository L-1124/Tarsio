# AGENTS.md

Tarsio 项目是 Rust 核心驱动的高性能 Python Tars (JCE) 协议库。此文件为编码智能体提供稳定、可执行的项目信息与规范。

## Project overview

* Rust（`src/`）负责协议编解码与字节处理，禁止 `panic!`。
* Python（`python/`）提供 `typing.Annotated` 风格 API，Rust 侧对象为真实实现。

## Repository layout

| 路径 | 说明 |
| --- | --- |
| `src/codec/` | 核心 JCE 编解码实现。 |
| `src/binding/` | PyO3 绑定与 FFI 胶水层。 |
| `python/tarsio/` | 用户 API 导出与类型定义。 |
| `python/tests/` | Python 功能测试（必须平铺）。 |
| `python/tests/typechecking/` | 静态类型检查测试。 |
| `python/tests/benchmarks/` | 性能基准测试。 |

## Dev environment

* 依赖管理：`uv`（Python），`cargo`（Rust）。
* 混合构建：`maturin`。

### Setup commands

* 安装依赖：`uv sync`
* 编译 Rust 扩展：`uv run maturin develop`
* 运行 Python 测试：`uv run pytest`
* 运行 Rust 测试：`cargo test`

## Testing instructions

### 既有测试架构（优先复用，不新增文件）

* Python 测试集中在 `python/tests/`，按模块分文件（如 `test_structs.py`、`test_schema.py`、`test_protocols.py`）。
* Rust 测试紧贴实现文件，统一放在对应模块底部的 `mod tests`。
* 优先在现有测试文件中追加或调整用例；除非确有必要，避免新增测试文件。

### Python 集成测试（python/tests/）

* 仅使用 `pytest` 函数式测试（`def test_...`），禁止类式测试。
* **位置规范**：
    * 功能测试必须位于 `python/tests/` 根目录。
    * 类型检查测试位于 `python/tests/typechecking/`。
    * 性能基准测试位于 `python/tests/benchmarks/`。
* 一个测试只验证一个行为，不混用 encode/decode。
* 只验证可观察行为（输入/输出/异常）。
* 异常必须断言准确类型（`TypeError`、`ValueError`）。
* 分层：协议层可断言 hex；API 层只断言输入/输出/异常；综合层少量组合测试。
* 泛型测试对象：`Box[int]` 与 `Box[User]`，禁止测试 Template 本身。

#### 现有测试文件用途

* `conftest.py`：公共 fixtures 与基础测试结构体（`User`、`OptionalUser`、`Node`），以及内存字节流。
* `test_cli.py`：CLI 行为（hex 输入解析、输出格式、verbose、文件输入/输出、错误处理、帮助信息）。
* `test_protocols.py`：协议级基线（编解码规则、未知 tag、`encode_raw`/`decode_raw`、边界与递归）。
* `test_schema.py`：Schema/Meta/Inspect 行为与约束校验。
* `test_structs.py`：Struct API 不变量（构造器、默认值、配置项、演进兼容、错误处理）。
* `test_typing.py`：typing/collections/generics 行为（NewType/Final/Alias/Union/Any/Box）。

### Rust 测试（src/）

* 单元测试写在对应文件底部 `mod tests`。
* Property Tests 使用 `proptest` 做 codec roundtrip 验证。

## Code quality

* `cargo fmt` 与 `cargo clippy` 必须通过。
* 核心逻辑禁止 `panic!`，必须返回 `Result`。

## Commit messages

* 使用 Conventional Commits：`<type>(<scope>): <subject>`。
* 提交信息使用中文。

## Documentation rules

### Python

* Docstrings 使用 Google Style。
* public API/class/方法/函数必须有 docstring。
* 测试函数必须包含单行中文 docstring（英文标点）。
* `Args` / `Returns` / `Yields` / `Raises` 按需提供。
* 仅描述可观察行为，禁止描述实现细节。

### Rust

* public item 使用 `///`，模块级使用 `//!`。
* 文档优先描述用途与错误语义。
* 禁止在文档中泄露实现细节或调试临时代码。

### PyO3

* `#[pyfunction]`/`#[pymethods]`/`#[pyclass]` 必须提供面向 Python 用户的文档说明。
* 结构与 Python docstring 一致，推荐 `Args` / `Returns` / `Raises`。

### docs/

* 仅面向用户，描述 Usage 与 Behavior。
* 不包含 Rust 内部实现细节或未稳定行为。
* 新增页面必须同步更新 `mkdocs.yml` 的 `nav`。

## Agent behavior

* 每次回答都以 `皇上启奏:` 开头。
* **核心规约**：遵循 `CONTRIBUTING.md` 中的详细规约。**在执行任务前，如有必要，必须完整阅读该指南以确保合规。**
* 禁止在 Python 测试中模拟 Rust WireType，除非是明确的协议基线测试。
* 修改 Rust 代码后，必须运行 `uv run maturin develop` 更新扩展。
* 仅在明确要求时，才能 `git commit` 或 `git push`。
