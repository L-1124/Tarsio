# Tarsio 文档

Tarsio 是一个 Rust 核心驱动的高性能 Python Tars (JCE) 协议库。
本文档旨在帮助你快速掌握 Tarsio 的使用方法、核心概念与 API。

## 导航

* **[使用指南](usage/models.md)**: 学习如何定义模型、编解码数据。
* **[支持类型](usage/types.md)**: 查看内置、typing 与标准库的支持范围。
* **[API 参考](api/struct.md)**: 查阅类与函数的详细说明。

## 安装

```bash
pip install tarsio
```

## 核心概念

Tarsio 的设计哲学是 **"Type Hint as Schema"**。你不需要编写额外的 IDL 文件，只需使用标准的 Python 类型注解即可定义 JCE 结构。

```python
from typing import Annotated
from tarsio import Struct, Meta, field

class Packet(Struct):
    version: int = field(tag=0, default=1)
    body: bytes
    note: Annotated[str | None, Meta(max_len=32)] = None
```

* **Struct**: 所有 Tarsio 模型的基类。
* **field(tag=...)**: 用于显式指定字段 Tag。
* **Annotated + Meta**: 用于附加约束（如长度、范围、正则）。
* **Tag ID**: 未显式指定时按字段顺序自动分配，可与显式 tag 混用。

## 性能

Tarsio 的底层编解码由 Rust 实现 (通过 PyO3 绑定)，面向性能敏感场景。具体性能与数据规模、结构复杂度和运行环境有关。
