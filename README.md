# Tarsio

[![Python](https://img.shields.io/badge/python-3.10+-blue.svg)](https://www.python.org/downloads/)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Ruff](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/astral-sh/ruff/main/assets/badge/v2.json)](https://github.com/astral-sh/ruff)
[![Documentation](https://img.shields.io/badge/docs-mkdocs-blue)](https://L-1124.github.io/Tarsio/)

`Tarsio` 是一个高性能的 Python Tars (JCE) 协议库，由 Rust 核心驱动。它提供：

* 🚀 面向 JCE 的高性能编解码实现
* 🎉 丰富的 Python 类型支持，可扩展
* 🔍 基于 `typing.Annotated` 的 Schema 校验与约束
* ✨ 轻量且快速的 `Struct` 类型用于结构化数据
* 🧩 支持 Schema (Struct) 与无 Schema (dict) 两种模式
* 🛡️ 递归深度与容器大小限制，提升解码安全性

* * *

`Tarsio` 既可以作为纯编解码库使用，也可以覆盖完整的“定义 Schema -> 编码 -> 解码”流程：

定义你的消息 Schema（使用标准 Python 类型注解）：

```python
>>> from typing import Annotated
>>> from tarsio import Struct, encode, decode
>>>
>>> class User(Struct):
...     uid: Annotated[int, 0]
...     name: Annotated[str, 1]
...     tags: Annotated[list[str], 2] = []
```

编码数据为 JCE 二进制：

```python
>>> alice = User(uid=1, name="alice", tags=["admin"])
>>> payload = encode(alice)
```

解码并进行 Schema 校验：

```python
>>> decode(User, payload)
User(uid=1, name='alice', tags=['admin'])
```

更多使用方式请查看文档。

## LICENSE

MIT. See the LICENSE file.
