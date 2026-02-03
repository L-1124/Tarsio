# Tarsio

**Tarsio** 是一个高性能的 Tars (JCE) 协议序列化库，由 Rust 核心驱动，专为 Python 设计。

它利用 PyO3 和 Rust 的极致性能，结合 Python 的灵活性，提供了现代化的开发体验。

## 核心特性

* **🚀 极致性能**: 核心编解码逻辑完全由 Rust 实现，零拷贝读取，SIMD 加速字符串校验。
* **✨ 现代 API**: 使用 Python 标准库 `Annotated` 定义 Tag，告别繁琐的 `Field` 函数。
* **🛡️ 类型安全**: 在类定义时进行静态 Schema 编译和检查。
* **🔧 强大的工具**: 内置功能丰富的 CLI 工具，支持递归探测二进制数据结构。
* **📦 零依赖**: 核心库不依赖任何第三方 Python 包（仅需 `typing-extensions`）。

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
