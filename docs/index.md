# Tarsio

**Tarsio** 是一个面向 Python 的 Tars (JCE) 协议序列化库。
它提供 `Struct` 模型、二进制编解码与调试工具。

## 核心特性

* **性能**: 适合高频编解码场景。
* **模型**: 使用 Python 标准库 `Annotated` 定义 Tag。
* **约束**: 支持 `Meta` 声明字段约束, 解码阶段执行校验。
* **工具**: 提供 CLI 与 `probe_struct`，用于分析二进制数据。
* **依赖**: 核心功能不依赖第三方 Python 包（仅需 `typing-extensions`）。

## 快速开始

### 安装

```bash
pip install tarsio
```

或者使用 `uv`:

```bash
uv add tarsio
```

### 定义模型

使用 `Annotated[T, Tag]` 语法定义 Tars 结构体：

```python
from typing import Annotated
from tarsio import Struct

class User(Struct):
    id: Annotated[int, 0]
    name: Annotated[str, 1]
    email: Annotated[str | None, 2] = None  # 可选字段

# 实例化
user = User(id=1001, name="Alice")
print(user)
```

### 序列化与反序列化

```python
# 编码
data = user.encode()
print(f"Hex: {data.hex()}")

# 解码
user_decoded = User.decode(data)
assert user_decoded.id == 1001
```

## 许可证

MIT License
