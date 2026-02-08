# 贡献指南

本指南用于统一 Tarsio 的开发、测试与文档规范，确保协议行为可验证、接口稳定、文档可信。

## 一、开发环境

### 工具链

* Rust：使用 `cargo`（nightly/stable 取决于 `rust-toolchain.toml`）。
* Python：使用 `uv` 管理依赖与虚拟环境。
* 构建：使用 `maturin` 连接 Rust/Python 构建流程。

### 常用命令

```bash
uv sync                  # 安装依赖
uv run maturin develop   # 编译 Rust 扩展并安装到 venv
uv run pytest            # 运行 Python 测试
cargo test               # 运行 Rust 测试
```

## 二、测试规范

### Python 集成测试

#### 核心原则

测试用于证明协议成立，不追求覆盖率。Python 测试是 Rust codec 的黑盒验证层。

* **框架**：强制 `pytest`。
* **形式**：仅允许函数式测试（`def test_xxx()`），禁止类式测试（`class TestXxx`）。
* **位置**：
    * 功能测试必须位于 `python/tests/` 根目录。
    * 类型检查测试位于 `python/tests/typechecking/`。
    * 性能基准测试位于 `python/tests/benchmarks/`。
* **原子性**：一个测试只验证一个行为，不混用 encode/decode。

#### 去重与分层

* 同一不变量只保留一个权威测试（如 round-trip、unknown tag）。
* 协议层允许断言 hex 字节作为基线。
* API 层只断言输入/输出/异常。
* 综合层用少量组合测试证明整体协议。

#### 命名约定

* 文件：`test_<module>.py`（如 `test_structs.py`）。
* 函数：`test_<func>_<expected_behavior>`。
* Fixtures：`snake_case`，体现语义角色。

#### 边界约定

* 仅验证可观察行为（Encode/Decode I/O、Exceptions、Optional/Required）。
* 禁止假设 Rust 内部结构（WireType、tag 顺序）。
* 泛型测试对象：Primitive 实例（`Box[int]`）与 Struct 实例（`Box[User]`），禁止测试 Template 本身。
* 异常必须断言准确类型（`TypeError`、`ValueError`）。

### Rust 测试

* 单元测试写在对应文件底部的 `mod tests`。
* Property Tests 使用 `proptest` 验证 codec roundtrip。

## 三、代码质量与提交规范

### 代码质量

* `cargo fmt` 与 `cargo clippy` 必须通过。
* 核心逻辑禁止 `panic!`，必须返回 `Result`。

### 提交规范（Conventional Commits）

格式：`<type>(<scope>): <subject>`

* **Types**：`feat`、`fix`、`perf`、`refactor`、`style`、`test`、`docs`、`chore`。
* **语言**：提交信息使用中文。

## 四、文档与注释规范

### Python

* Docstrings 统一采用 Google Style。
* public API、class、方法、函数必须有 docstring。
* 测试函数必须包含单行中文 docstring（英文标点）。
* 有参数时包含 `Args`；有返回值时包含 `Returns`；generator/fixture 包含 `Yields`；有异常语义建议包含 `Raises`。
* 描述可观察行为与语义，不描述实现细节。
* 禁止在测试中用 docstring 解释 Rust 内部实现。

### Rust

* Doc comments 使用 `///`，模块级使用 `//!`。
* 文档优先描述用途、边界、输入/输出与错误语义。
* 关键路径（编解码、schema、FFI）必须明确不变量与错误返回，禁止以 `panic!` 作为错误处理。
* Rust 侧文档禁止泄露敏感信息或保留调试用临时代码说明。

### PyO3

* 对外导出的 `#[pyfunction]`/`#[pymethods]`/`#[pyclass]` 必须提供面向 Python 用户的文档说明。
* 文档结构与 Python docstring 保持一致，推荐使用 `Args`、`Returns`、`Raises`。
* 作为 Python 层文档来源时，仅描述可观察行为，禁止描述 Rust 内部实现细节。

### docs/

#### 定位与边界

* `docs/` 仅面向用户，承载使用说明、示例、CLI 指南与 API 参考。
* 优先描述“怎么用（Usage）”与“会发生什么（Behavior）”。
* 不包含 Rust 内部实现细节、私有约定或未稳定行为。

#### 目录结构

```tree
docs/
  ├── index.md                # 文档首页
  ├── usage/                  # 使用指南（面向用户）
  ├── api/                    # API 参考（自动生成为主）
```

* 文档入口：`docs/index.md`。
* 使用指南：`docs/usage/*.md`。
* API 参考：`docs/api/*.md`。
* 新增页面需同步更新 `mkdocs.yml` 的 `nav`。

#### Markdown 规范

* 每页必须包含 H1 标题与 1–3 行摘要。
* 所有代码块标注语言；CLI 输出使用 `text`。
* 示例必须可运行、短小（不超过 15 行）、不省略 import。
* 优先使用相对路径链接；外链注明来源。

#### 页面结构（每页必须包含）

1. 标题（H1）
2. 摘要（1–3 行）
3. 示例代码
4. 核心概念解释
5. 注意事项
6. 相关链接（可选）

#### Python API 文档

* 使用 mkdocstrings 自动生成：

```markdown
::: tarsio.Struct
```

* API 页面仅补充示例、注意事项与行为说明，docstring 是唯一真实来源。

## 五、语言与风格约定

* 使用简体中文搭配必要英文技术词汇。
* 句子短、结构清晰。
* 避免口语化与废话式开头（如“本章将介绍……”）。
* 注释与文档统一使用中文，英文标点。
