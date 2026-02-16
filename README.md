# Tarsio

[![Python](https://img.shields.io/badge/python-3.10+-blue.svg)](https://www.python.org/downloads/)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Ruff](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/astral-sh/ruff/main/assets/badge/v2.json)](https://github.com/astral-sh/ruff)
[![Documentation](https://img.shields.io/badge/docs-mkdocs-blue)](https://L-1124.github.io/Tarsio/)

**Tarsio** 是一个高性能的 Python Tars (JCE) 协议库，由 Rust 核心驱动。

## 核心特性

* 🚀 **高性能**: 核心编解码由 Rust 实现，比纯 Python 实现快 10-50 倍。
* 🛡️ **类型安全**: 使用标准 Python 类型注解定义 Schema，支持显式/隐式 Tag。
* ✨ **声明式校验**: 支持 `Meta` 元数据约束，在反序列化时自动校验。
* 🧩 **灵活模式**: 支持强类型 `Struct` 与无 Schema 的 `dict` (Raw) 模式。

## 快速上手

```python
from typing import Annotated
from tarsio import Struct, field, Meta, encode, decode

# 1. 定义 Schema
class User(Struct):
    # 显式 tag
    id: int = field(tag=0)
    # 未显式 tag, 按顺序自动分配
    name: str
    # Annotated 用于约束, tag 仍由 field 指定
    groups: Annotated[list[str], Meta(min_len=1)] = field(tag=2, default_factory=list)

# 2. 创建对象
alice = User(id=1001, name="Alice", groups=["admin", "dev"])
print(alice)
# > User(id=1001, name='Alice', groups=['admin', 'dev'])

# 3. 编码 (Encode)
data = encode(alice)
print(data.hex())

# 4. 解码 (Decode)
user = decode(User, data)
assert user == alice
```

## 文档

完整文档请访问 [https://L-1124.github.io/Tarsio/](https://L-1124.github.io/Tarsio/)。

## License

MIT
