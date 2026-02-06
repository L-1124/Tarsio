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

## 文档编写规范（docs/）

* `docs/` 用于承载**面向用户**的使用说明、示例、CLI 指南与 API 参考。
* 文档应优先描述：
    * **怎么用（Usage）**
    * **会发生什么（Behavior）**
* 文档不应包含：
    * Rust 内部实现细节
    * 私有约定、内部 API、未稳定的行为
* 文档是用户理解 Tarsio 的**单一可信来源（Single Source of Truth）**，必须保持准确性与可验证性。

### 目录结构规范

```tree
docs/
  ├── index.md                # 文档首页
  ├── usage/                  # 使用指南（面向用户）
  ├── api/                    # API 参考（自动生成为主）
```

#### 目录约定

* 文档入口：`docs/index.md`
* 使用指南：`docs/usage/*.md`
* API 参考：`docs/api/*.md`
* 新增文档页面时必须同步更新 `mkdocs.yml` 的 `nav`，保持导航完整。

### Markdown 编写规范

#### 1. 标题与摘要

* 每个页面必须包含一个 **H1 标题**（`# Title`）
* 标题下必须提供 **1–3 行摘要**，说明该页面的用途与范围

示例：

```markdown
# Struct

Tarsio 的结构体模型，用于定义可编码/解码的 JCE 数据结构。
本页介绍 Struct 的声明方式、字段规则与常见用法。
```

#### 2. 代码块规范

* 所有代码块必须标注语言：

```markdown
```python
```bash
```text
```

* CLI 输出必须使用 `text`，避免因环境差异导致误导：

```text
$ tarsio encode user.json
OK (12 bytes)
```

* 示例必须可运行，不允许伪代码

#### 3. 链接规范

* 优先使用**相对路径**，确保文档可迁移：

```markdown
参见 [Struct](../usage/struct.md)
```

* 外链需注明来源（如 RFC、官方文档）

### 内容规范

#### 1. 文档结构（每页必须包含）

1. **标题（H1）**
2. **摘要（1–3 行）**
3. **示例代码（必须有）**
4. **核心概念解释**
5. **注意事项（必选）**
6. **相关链接（可选）**

#### 2. 示例规范

* 示例必须短小（不超过 15 行）
* 不允许省略 import
* 不允许使用未定义的变量
* 示例必须能在用户环境中直接运行

示例：

```python
from tarsio import Struct, Tag
from typing import Annotated

class User(Struct):
    id: Annotated[int, Tag(0)]
    name: Annotated[str, Tag(1)]
```

### API 文档规范

#### Python API

* 使用 **mkdocstrings** 自动生成：

```markdown
::: tarsio.Struct
```

* API 文档的真实来源是 **docstring**，文档页面只补充：
    * 示例
    * 注意事项
    * 行为说明

## 语言风格

* 简体中文 + 英文技术词汇
* 句子短、结构清晰
* 避免口语化、废话式开头（如“本章将介绍……”）
