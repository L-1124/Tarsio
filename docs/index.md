# Tarsio 文档

Tarsio 是一个 Rust 核心驱动的高性能 Python Tars (JCE) 协议库。
本文档旨在帮助你快速掌握 Tarsio 的使用方法、核心概念与 API。

## 导航

* **[使用指南](usage/models.md)**: 学习如何定义模型、编解码数据。
* **[API 参考](api/struct.md)**: 查阅类与函数的详细说明。

## 安装

```bash
pip install tarsio
```

## 核心概念

Tarsio 的设计哲学是 **"Type Hint as Schema"**。你不需要编写额外的 IDL 文件，只需使用标准的 Python 类型注解即可定义 JCE 结构。

```python
from typing import Annotated
from tarsio import Struct

class Packet(Struct):
    version: Annotated[int, 0] = 1
    body: Annotated[bytes, 1]
```

* **Struct**: 所有 Tarsio 模型的基类。
* **Annotated**: 用于附加元数据（如 Tag ID）。
* **Tag ID**: 直接使用整数 (0-255) 指定字段在 JCE 协议中的编号。

## 性能

Tarsio 的底层编解码完全由 Rust 实现 (通过 PyO3 绑定)，在处理大量数据时比纯 Python 实现快 10-50 倍。
