# 🧪 Tarsio Python 集成测试规范

**适用范围**：
本规范适用于 `tarsio` Python 包的所有集成测试，用于验证 Python API 与 Rust 扩展模块（`_core`）之间的协议级不变量。

**目标**：
测试用于**证明协议成立**，而非追求覆盖率或验证实现细节。

---

## 一、测试风格

* **测试框架**：强制使用 `pytest`（v9.0.2+）。
* **测试形式**：
    * 仅允许**函数式测试**（`def test_xxx()`）。
    * **禁止**使用类式测试（`class TestXxx`）。
* **测试位置**：
    * 所有测试文件必须位于 `python/tests/` 根目录。
    * **禁止创建任何子目录**。
* **原子性原则**：
    * 一个测试函数只验证**一个行为或一个不变量**。
    * **禁止**在单个测试中同时验证 encode + decode + schema 行为。
* **确定性原则**：
    * 测试必须是确定性的。
    * 禁止依赖随机数、时间、系统状态或环境变量。
    * 如需随机数据，必须显式固定 seed。

---

## 二、去重原则（Single Source of Truth）

* **同一不变量只保留一个权威测试**：避免跨文件重复 round-trip 或重复 unknown tag 场景。
* **按职责分层**：
    * **协议层（字节级）**：允许断言具体字节布局（hex），用于固定协议实现基线。
    * **API 层（可观察行为）**：只断言输入/输出/异常，不断言字节布局。
    * **综合不变量**：用少量“跨类型组合”的 round-trip 证明整体协议成立，而非重复覆盖每个类型。

---

## 三、文件与命名约定

### 3.1 测试文件

* 命名规则：`test_<被测模块>.py`
* 示例：
    * `struct.py` → `test_struct.py`
    * 协议字节基线（Rust codec）→ `test_protocol.py`

> 测试文件可以按**语义分类**拆分为多个 `test_*.py`，
> 但必须全部位于 `python/tests/` 根目录，
> **禁止镜像 Rust 模块结构或创建子目录**。

#### 推荐文件职责（用于减少重复）

| 文件 | 职责（只测这一类） |
|---|---|
| `test_protocol.py` | 字节级协议基线（hex/布局/skip_field 相关） |
| `test_struct.py` | Struct 构造器与 Python API 行为（encode/decode 路由、异常模型） |
| `test_invariants.py` | 综合不变量（跨 WireType 组合的 round-trip + 错误路径） |
| `test_generics.py` | 泛型具体化/模板限制/泛型场景行为 |
| `test_evolution.py`（可选） | schema 演进（forward/back compatibility） |

---

### 3.2 测试函数

* 命名规则：`test_<被测函数>_<预期行为>`
* 示例：
    * `test_encode_missing_required_field_raises_value_error`
    * `test_decode_unknown_tag_is_skipped`

---

### 3.3 Fixtures

* 使用 `snake_case`
* 名称必须体现**语义角色**，而非实现细节
* 示例：
    * `simple_struct`
    * `generic_box_int`
    * `encoded_bytes_with_unknown_tag`

---

## 四、文档字符串与注释规范

### 4.1 测试函数

* **必须包含 docstring**
* **单行中文**
* **只描述预期行为，不描述实现**
* 示例：

```python
def test_decode_missing_required_field_raises_error() -> None:
    """反序列化缺失必填字段时抛出 ValueError。"""
```

---

### 4.2 Fixtures 与辅助函数

* 必须使用 **Google Style** 注释规范
* 必须包含 `Args` / `Returns` / `Yields`
* 示例：

```python
@pytest.fixture
def simple_struct() -> MyStruct:
    """提供一个包含最小字段集的 Struct 实例。

    Returns:
        MyStruct: 一个合法的 Struct 实例。
    """
```

---

## 五、类型注解约定

* **测试函数**：统一标注 `-> None`
* **Fixtures**：必须标注返回类型
* **辅助函数**：必须标注参数与返回类型
* **禁止**使用 `Any`，除非测试目标本身是类型不确定性

---

## 六、Python / Rust 边界约定（核心原则）

### 6.1 Python 测试的职责

Python 测试**只验证可观察行为**，包括：

* encode / decode 的输入输出
* 异常类型与异常信息
* Optional / Required 语义
* Generic 行为
* schema 演进行为（unknown tag）

**禁止在 Python 测试中：**

* 假设 Rust 内部结构（如 `WireType`、tag 顺序）
* 断言具体字节布局（除非测试目标本身是字节协议）

---

### 6.2 Rust 行为的验证方式

* Rust codec 的正确性通过 **Python round‑trip** 间接验证
* Python 测试是 Rust codec 的**黑盒验证层**
* Rust 单元测试仅用于：
    * reader / writer 边界
    * skip_field
    * SimpleList 的低层行为

---

## 七、泛型与类型测试约定

### 7.1 泛型测试

* 每个 Generic Struct 至少测试：
    * 一个 primitive 实例（如 `Box[int]`）
    * 一个 Struct 实例（如 `Box[User]`）
* **禁止**对 Generic Template 本身调用 encode / decode
* 必须验证：
    * template 不注册 schema
    * concrete specialization 正常工作

---

### 7.2 类型错误测试

必须覆盖以下场景，并断言异常类型与信息：

* encode 非 Struct → `TypeError`
* decode 无 schema → `TypeError`
* required 缺失 → `ValueError`
* 非法 bytes → `ValueError`

---

## 八、边界与 Schema 演进测试

* 整个测试套件至少包含：
    * 一个**空值边界测试**
    * 一个 **unknown tag 测试**
* 推荐：每个“语义域测试文件”（Protocol / API / Invariants / Generics / Evolution）各自至少包含一个边界或 unknown tag 用例，但不强制要求每个文件都重复覆盖两者。
* Schema 演进测试必须：
    * 明确区分 v1 / v2
    * **禁止复用同一个 class 名称**

---

## 九、明确禁止的测试行为

* ❌ 调用私有 Rust API
* ❌ monkey‑patch Rust 行为
* ❌ 在一个测试中验证多个不变量
* ❌ 为覆盖率而编写测试

---

## 十、最佳实践

### 9.1 参数化测试

* 多输入场景必须使用 `pytest.mark.parametrize`
* 必须提供 `ids` 提升可读性

---

### 9.2 异常捕获

* 必须使用 `pytest.raises`
* 禁止捕获宽泛异常（如 `Exception`）
* 建议使用 `match` 校验异常信息关键词

---

### 9.3 Warnings

* 使用 `pytest.warns`
* 禁止忽略或手动捕获警告

---

### 9.4 I/O

* 优先使用 `io.BytesIO` / `io.StringIO`
* 必须使用物理文件时，使用 `tmp_path`

---

### 9.5 显式断言

* 禁止 `assert result`
* 使用明确比较（`==`、`in`、`pytest.approx`）

---

## 🧊 冻结声明

> 本测试规范用于验证 Tarsio 协议级不变量。
> 所有测试必须遵循本规范。
> 违反本规范的测试视为无效。
