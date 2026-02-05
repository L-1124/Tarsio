# 贡献指南

## 开发环境

* **工具链**:
    * Rust: 使用 `cargo` (nightly/stable 取决于 `rust-toolchain.toml`)
    * Python: 使用 `uv` 管理依赖和环境
    * 构建: 使用 `maturin` 桥接 Rust/Python
* **常用命令**:

    ```bash
    uv sync                  # 安装依赖
    uv run maturin develop   # 编译 Rust 扩展并安装到 venv
    uv run pytest            # 运行 Python 测试
    cargo test               # 运行 Rust 测试
    ```

## Python 集成测试规范 (python/tests/)

> **核心原则**: 测试用于**证明协议成立**，而非追求覆盖率。Python 测试是 Rust codec 的**黑盒验证层**。

### 1. 测试风格

* **框架**: 强制 `pytest`。
* **形式**: 仅允许**函数式测试** (`def test_xxx()`)，**禁止**类式测试 (`class TestXxx`)。
* **位置**: 必须位于 `python/tests/` 根目录，**禁止子目录**。
* **原子性**: 一个测试只验证一个行为。不要混用 encode/decode 验证。

### 2. 去重原则

* **唯一真实源**: 同一不变量（如 round-trip, unknown tag）只保留一个权威测试。
* **分层**:
    * **协议层**: 允许断言 hex 字节（基线）。
    * **API 层**: 只断言输入/输出/异常。
    * **综合**: 用少量组合测试证明整体协议。

### 3. 命名约定

* 文件: `test_<module>.py` (如 `test_struct.py`)
* 函数: `test_<func>_<expected_behavior>`
* Fixtures: `snake_case`，体现语义角色。

### 4. 边界约定

* **Python 职责**: 验证可观察行为 (Encode/Decode I/O, Exceptions, Optional/Required)。
* **禁止**: 假设 Rust 内部结构 (WireType, tag 顺序)。
* **泛型**: 测试 Primitive 实例 (`Box[int]`) 和 Struct 实例 (`Box[User]`)，禁止测试 Template 本身。
* **异常**: 必须断言准确的异常类型 (`TypeError`, `ValueError`)。

## Rust 开发约定 (src/)

### 测试

* **单元测试**: 写在文件底部的 `mod tests`。
* **Property Tests**: 使用 `proptest` 进行 codec roundtrip 验证。
* **Benchmarks**: 使用 `criterion` 监控关键路径。

### 质量

* `cargo fmt` 和 `cargo clippy` 必须通过。
* 核心代码禁止 `panic!`，必须返回 `Result`。

## 提交规范 (Conventional Commits)

格式: `<type>(<scope>): <subject>`

* **Types**: `feat`, `fix`, `perf`, `refactor`, `style`, `test`, `docs`, `chore`.
* **语言**: 提交信息使用**中文**。

## 注释规范

* 所有注释使用**中文**，英文标点。
* 文档与注释一律保持可读, 禁止堆砌实现细节。

### Python 文档规范

* Docstrings: 统一采用 Google Style。
* 覆盖范围:
    * public API, class, 方法, 函数必须有 docstring。
    * 测试函数必须包含单行中文 docstring(英文标点)。
* 结构要求:
    * 有参数时必须包含 `Args`。
    * 有返回值时必须包含 `Returns`。
    * generator/fixture 必须包含 `Yields`。
    * 有异常语义时建议包含 `Raises`。
* 约束:
    * docstring 描述可观察行为与语义, 不描述实现细节。
    * 禁止在测试中用 docstring 解释 Rust 内部实现。

### Rust 文档规范

* Doc comments: 使用 `///` 为 public item 编写文档, 模块级使用 `//!`。
* 结构建议:
    * 用短句说明用途与边界条件, 优先描述输入/输出与错误语义。
    * 关键路径(编解码, schema, FFI)需要明确不变量与错误返回, 禁止以 `panic!` 作为错误处理。
* PyO3 绑定文档规范:
    * 对外导出的 `#[pyfunction]`/`#[pymethods]`/`#[pyclass]` 必须提供面向 Python 用户的文档说明。
    * 文档内容与 Python docstring 保持一致的结构与语义, 推荐使用 Google Style 小节标题: `Args`, `Returns`, `Raises`。
    * Rust doc comments 作为 Python 层文档来源时, 优先描述可观察行为, 禁止描述 Rust 内部实现细节。
* 约束:
    * 文档与注释使用中文, 英文标点。
    * Rust 侧禁止在文档中泄露敏感信息或写入调试用的临时代码说明。

## 文档编写规范 (docs/)

* 目标:
    * `docs/` 用于承载面向用户的使用说明与 API 参考, 作为可观察行为的权威来源。
    * 文档优先描述"怎么用"与"会发生什么", 避免描述 Rust 内部实现细节与私有约定。
* 目录约定:
    * 入口: `docs/index.md`。
    * 使用指南: `docs/usage/*.md`。
    * API 参考: `docs/api/*.md`。
    * 新增文档页面时需要同步更新 `mkdocs.yml` 的 `nav`。
* Markdown 约定:
    * 每个页面必须包含一个 H1 标题, 并在开头提供 1-3 行摘要。
    * 代码块必须标注语言 (如 `python`, `bash`, `text`)。
    * 命令行示例的输出建议使用 `text` 代码块, 避免依赖机器环境差异。
    * 链接优先使用相对路径, 保持站点内可迁移。
* API 文档约定:
    * Python API 参考优先使用 mkdocstrings 指令 `::: tarsio.<symbol>` 生成, 避免手写重复内容。
    * 若文档需要展示签名与参数说明, 以 docstring 为单一真实源, 文档只补充示例与注意事项。
