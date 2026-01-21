# JceStruct Agent 指南

本文档为在 JceStruct 仓库工作的 AI Agent 提供必要的上下文和规则。

**包名**: `jce` (Python 模块)  未发布到pypi
**当前版本**: `1.0.0`

## 1. 环境与命令

### 1.1 包管理

- **工具**: `uv`
- **依赖**: 在 `pyproject.toml` 中管理 (requires-python >= 3.10)。
- **安装**: `uv pip install -e .` 或 `uv pip install -e .[cli]` (包含 CLI 工具)。
- **开发依赖**: `uv sync --all-groups --all-extras` 。

### 1.2 测试

- **框架**: `pytest` (v9.0.0+)。
- **位置**: `tests/` 目录 (扁平结构, 无子目录)。
- **覆盖率**: 使用 `pytest-cov` 插件。
- **命令**:
  - 运行所有测试: `uv run pytest` 自带覆盖率报告。
  - 运行单个文件: `uv run pytest tests/test_struct.py`
  - 运行单个用例: `uv run pytest tests/test_struct.py::test_struct_defaults`
  - 详细模式: `uv run pytest -v`
  - 覆盖率报告: `uv run pytest --cov=jce --cov-report=term-missing`

### 1.3 代码检查与格式化

- **工具**: `ruff` (在 `pyproject.toml` 中配置)。
- **配置**:
  - 行长: 88 字符。
  - Python 目标: 3.10。
  - 文档风格: Google 约定。
  - 检查规则: E, F, B, I, UP, SIM, RUF, C4, PT, RET, ARG, ERA, PL, TRY, PERF, D。
- **命令**:
  - 检查: `uv run ruff check .`
  - 格式化: `uv run ruff format .`
  - 自动修复: `uv run ruff check --fix .`

### 1.4 构建

- **后端**: `hatchling`。
- **构建命令**: `python -m build` (或 `hatch build`)。

## 2. 代码风格与约定

### 2.1 通用

- **语言**: 文档字符串和注释应使用 **中文**。
- **符号**: 使用英文符号
- **缩进**: 4 个空格。
- **行长**: 软限制约 80-88 字符 (由 Ruff 强制执行)。

### 2.2 命名

- **类**: `PascalCase` (例如 `JceStruct`, `JceField`)。
- **函数/方法**: `snake_case` (例如 `model_validate`, `to_bytes`)。
- **变量**: `snake_case`。
- **常量**: `UPPER_SNAKE_CASE` (例如 `OPT_LITTLE_ENDIAN`)。
- **私有**: 使用单下划线前缀 `_` (例如 `_buffer`)。

### 2.3 导入

- **风格**: **相对导入** (例如 `from .types import INT`)。
- **排序**: 标准库 -> 第三方 (`pydantic`) -> 本地 (`jce`)。
- **分组**: 多行导入使用圆括号。

### 2.4 类型提示

- **严格性**: 高。所有函数签名和字段都使用类型提示。
- **工具**: `pydantic` v2.0+ 是数据建模的核心。
- **泛型**: 使用 `list[T]`, `dict[K, V]` (Python 3.9+ 风格)。
- **联合类型**: **不支持** (除了 `Optional[T]`)。
  - `T | None`: 支持。
  - `T1 | T2`: 定义时抛出 `TypeError` 以防止歧义。
- **Bytes 模式**: `loads` 和 `load` 函数支持 `bytes_mode` 参数:
  - `"raw"`: 保持原始字节。
  - `"string"`: 尝试转为 UTF-8 字符串。
  - `"auto"`: (默认) 智能模式, 尝试解析嵌套 JCE, 失败则转字符串, 最后保持原始字节。
- **JceStruct**: 字段必须使用带有 `jce_id` 的 `JceField`。

  ```python
  class User(JceStruct):
      uid: int = JceField(jce_id=0, jce_type=types.INT)
  ```

### 2.5 错误处理

- **基类**: `JceError` (在 `jce.exceptions` 中)。
- **具体错误**:
  - `JceEncodeError`: 编码失败。
  - `JceDecodeError`: 解码失败。
  - `JcePartialDataError`: 数据不完整(流处理时常用)。
  - `JceTypeError`: 类型不匹配。
  - `JceValueError`: 值无效或超出范围。
- **模式**: 使用描述性消息抛出具体异常。

### 2.6 日志

- **Logger**: 使用 `jce.log.logger`。
- **模式**: 如果有助于调试，在抛出异常前记录错误/警告。

## 3. 架构与模式

### 3.1 核心组件

- **`jce/__init__.py`**: 公开 API 入口, 导出所有核心类和函数。
- **`jce/api.py`**: 公开 API 层。提供 `dumps`/`loads` (字节操作) 和 `dump`/`load` (文件 IO), 支持 `bytes_mode` 和 `context`。
- **`jce/stream.py`**: 流式序列化支持。提供 `JceStreamWriter` / `JceStreamReader` 基类, 以及支持网络协议的 `LengthPrefixedWriter` / `LengthPrefixedReader`。
- **`jce/context.py`**: 序列化上下文定义 (`SerializationInfo`, `DeserializationInfo`)。
- **`jce/config.py`**: 配置对象 (`JceConfig`), 统一管理选项、上下文和 bytes_mode。
- **`jce/options.py`**: 选项标志常量定义 (`JceOption` IntFlag)。
- **`jce/struct.py`**: `JceStruct` 基类、`JceField` 工厂函数、`JceDict` 匿名结构体类。
- **`jce/types.py`**: JCE 类型定义 (`JceType` 基类及各类型实现)。
- **`jce/encoder.py`**: 二进制编码逻辑核心 (`DataWriter`, `JceEncoder`)。
- **`jce/decoder.py`**: 二进制解码逻辑核心 (`DataReader`, `GenericDecoder`, `SchemaDecoder`, `NodeDecoder`, `JceNode`)。
- **`jce/adapter.py`**: 类型适配器 (`JceTypeAdapter`), 提供类似 Pydantic TypeAdapter 的接口。
- **`jce/exceptions.py`**: 异常定义 (6 个异常类)。
- **`jce/const.py`**: JCE 协议常量定义 (类型码、魔数等)。
- **`jce/log.py`**: 日志配置。
- **`jce/__main__.py`**: CLI 工具入口 (可选依赖 click)。

### 3.2 协议实现

- 遵循 JCE (腾讯 Tars) 协议规范。
- 默认支持 **大端序**。
- 实现了特定优化，如 `Zero Tag` (0x0C) 和 `Simple List` (0x0D)。

### 3.3 数据建模

- 继承自 `JceStruct`。
- 使用类型注解和 `JceField` 定义字段。
- `jce_id` (标签) 是强制性的，且在结构体中必须唯一。
- **JceDict**: 用于匿名结构体 (键必须是 int, 代表 Tag ID)。
  - `JceDict({0: 100})` -> 编码为 JCE Struct。
  - `dict({0: 100})` -> 编码为 JCE Map。

### 3.4 Pydantic 兼容性

- **计算字段**: 逻辑上支持，但从 `model_json_schema()` 中排除 (标准 Pydantic 行为)。
- **IDE 支持**: `JceField` 依赖类属性注解进行 IDE 类型推断。
- **验证**: `model_validate` 和 `model_dump` 表现为标准 Pydantic v2 方法。

### 3.5 流式 API

针对网络流式数据, 提供长度前缀支持:

- **`LengthPrefixedWriter`**: 自动计算并添加长度头。
  - `pack(obj)`: 序列化并存入缓冲区。
  - `get_buffer()`: 获取打包后的字节流。
- **`LengthPrefixedReader`**: 增量解析带长度头的字节流。
  - `feed(data)`: 输入新到达的字节。
  - `for obj in unpacker:`: 迭代解析出完整对象。

### 3.6 字段自定义

通过装饰器自定义特定字段的编解码行为:

- **`@jce_field_serializer(field_name)`**:
  - 签名: `def method(self, value, info: SerializationInfo)`
  - 在编码前转换字段值。
- **`@jce_field_deserializer(field_name)`**:
  - 签名: `def method(cls, value, info: DeserializationInfo)`
  - 在解码后转换字段值。
- **Context**: 编解码函数可通过 `context` 参数传递外部状态, 在钩子中通过 `info.context` 访问。

## 4. 文档

- **Docstrings**: Google 风格。
- **语言**: **中文，但是使用半角标点**。
- **示例**:

  ```python
  def encode(self) -> bytes:
      """序列化当前对象.

      Returns:
          bytes: JCE格式的二进制数据.

      Raises:
          JceEncodeError: 编码失败时抛出.
      """
  ```

## 5. Git 与工作流

- **提交**: 使用约定式提交 (feat, fix, docs, test, chore)。
- **PR**: 提交前确保通过测试并通过代码检查。
- **发布**:
  - 通过 GitHub Actions 自动执行 (`release.yml`)。
  - 通过推送标签触发 (例如 `git tag v0.2.0 && git push origin v0.2.0`)。
  - 使用 `uv build` 和 Trusted Publishing (OIDC) 发布到 PyPI。
  - 创建带有自动生成说明的 GitHub Release。

# 6. 测试约定

### 6.1 测试风格

* **框架**: 强制使用 `pytest` (v9.0.2+)。
* **模式**: 采用**函数式测试** (`def test_xxx()`)，禁止使用类式测试 (`class TestXxx`) 以保持代码简洁。
* **位置**: 所有测试文件位于根目录的 `tests/` 文件夹下，采用**扁平结构**（不创建子目录）。
* **原子性**: 测试函数必须短小且聚焦。一个测试函数只验证一个行为，严禁编写“万能测试函数”。

### 6.2 命名约定

* **测试文件**: `test_<被测模块>.py`
* 例: `struct.py`  `test_struct.py`


* **测试函数**: `test_<被测函数>_<预期行为>`
* 例: `test_loads_with_invalid_data_raises_decode_error`


* **Fixtures**: 遵循 `snake_case`，名称应反映其提供的对象或状态。

### 6.3 文档字符串与注释

* **测试函数**:
* 使用**单行中文**。
* 描述预期行为，不描述实现细节。


* **Fixtures 与辅助函数**:
* 必须遵循 **Google Style** 注释规范。
* 包含 `Args`、`Returns` 或 `Yields` 声明。


* **类型注解**:
* 测试函数统一标注返回值为 `-> None`。
* Fixtures 必须标注返回值类型。



### 6.4 标准结构示例

```python
"""测试 JCE 编解码器的核心功能."""

import pytest
from io import BytesIO
from typing import Generator
from jce import JceStruct, JceDecodeError, fields

class User(JceStruct):
    """用于测试的简单结构体."""
    uid: int = fields.Int(1)
    name: str = fields.String(2)

@pytest.fixture
def sample_user() -> User:
    """提供一个预置的 User 实例。
    
    Returns:
        初始化后的 User 对象。
    """
    return User(uid=12345, name="Gemini")

@pytest.fixture
def mock_stream() -> Generator[BytesIO, None, None]:
    """提供内存字节流环境。
    
    Yields:
        BytesIO 实例。
    """
    stream = BytesIO()
    yield stream
    stream.close()

def test_user_encode_returns_correct_bytes(sample_user: User) -> None:
    """User 结构体应该能正确编码为字节流."""
    # Arrange (准备)
    expected_prefix = b"\x01\x15"  # 假设的编码开头

    # Act (执行)
    result = sample_user.encode()

    # Assert (断言)
    assert isinstance(result, bytes)
    assert result.startswith(expected_prefix)

def test_decode_with_truncated_data_raises_error() -> None:
    """当数据被截断时，decode 应该抛出 JceDecodeError."""
    invalid_data = b"\x01" 
    
    with pytest.raises(JceDecodeError, match="Unexpected end of buffer"):
        User.decode(invalid_data)

```

### 6.5 进阶最佳实践

#### 1. 参数化测试 (Parametrize)

当测试多个输入输出时，必须使用参数化并提供 `ids` 以增强报告可读性。

```python
@pytest.mark.parametrize(
    "val, expected",
    [(1, b"\x01"), (127, b"\x7f")],
    ids=["small_int", "max_byte_int"]
)
def test_int_encoding_variants(val: int, expected: bytes) -> None:
    """测试不同范围整数的编码结果."""
    ...

```

#### 2. 异常捕获

* 必须使用 `pytest.raises`。
* **严禁**使用宽泛的 `Exception`，必须指明具体的异常类（如 `ValueError`, `JceDecodeError`）。
* 建议使用 `match` 参数校验异常信息关键词。

#### 3. 模拟 (Mocking)

* 优先使用 `pytest-mock` 提供的 `mocker` fixture，避免手动使用 `patch` 装饰器。
* 禁止在测试中发起真实的 HTTP 请求或修改全局环境变量。

#### 4. I/O 处理

* **内存化**: 涉及文件读取的测试，优先使用 `io.BytesIO` 或 `io.StringIO`。
* **临时文件**: 若必须操作物理文件，使用 pytest 内置的 `tmp_path` fixture。

#### 5. 显式断言

* 避免 `assert result`（除非结果本身是布尔值）。
* 使用 `assert result == expected`、`assert "key" in dict` 等明确的比较。
* 涉及浮点数对比时，使用 `pytest.approx()`。

好的，已将 **`test_protocol.py`** 的特殊地位加入规范，作为库的基石：

---

### 6.6 核心协议测试 (Fundamental)

`test_protocol.py` 是库的**根本性测试**，必须保证 100% 通过。它定义了 JCE 协议实现的基准，任何破坏此文件测试用例的修改均被视为破坏性变更。

* **测试目标**: 验证确定的输入与预期的十六进制输出（Hex）完全一致。
* **断言要求**:
* 编码测试：断言生成的 bytes 转为 hex 字符串后与预期值完全匹配。
* 解码测试：断言从预期 hex 字符串还原的对象与原始对象等值。


## 7. 文档生成 (MkDocs)

项目使用 **MkDocs** (Material Theme) 生成静态文档网站。

### 7.1 结构

文档源文件位于 `docs/` 目录：

- **`docs/api/`**: 自动生成的 API 参考文档 (使用 `mkdocstrings`)。
  - 对应源码模块结构 (如 `api.md`, `struct.md`, `fields.md`)。
- **`docs/usage/`**: 用户指南、核心概念和教程。
  - `models.md`: 定义结构体。
  - `serialization.md`: 序列化与 API。
  - `cli.md`: 命令行工具使用说明。
- **`docs/index.md`**: 项目主页和快速开始。
- **`mkdocs.yml`**: MkDocs 配置文件。

### 7.2 命令

- **安装依赖**:
  ```bash
  uv add --group docs mkdocs-material mkdocstrings[python]
  # 或同步环境
  uv sync --all-groups
  ```

- **本地预览**:
  启动实时重载的本地服务器：
  ```bash
  uv run mkdocs serve
  ```
  访问: `http://127.0.0.1:8000`

- **构建静态站**:
  生成 HTML 文件到 `site/` 目录：
  ```bash
  uv run mkdocs build
  ```

### 7.3 编写规范

- **API 文档**:
  使用 `mkdocstrings` 语法引用 Python 对象：
  ```markdown
  ::: jce.JceStruct
      
  ```
- **概念文档**:
  专注于"如何做" (How-to) 和"为什么" (Why)。代码块应可运行。
- **交叉引用**:
  链接到其他文档页面或 API 定义。
