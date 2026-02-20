# 支持类型

本页汇总 Tarsio 在 schema 中可直接使用的常见类型。
表格描述的是外部可观察行为，不依赖 Rust 内部实现细节。

## 示例代码

```python
from typing import Annotated
from tarsio import Struct, TarsDict, encode, field

class Packet(Struct):
    id: Annotated[int, 0]
    tags: Annotated[list[str], 1]
    extra: Annotated[TarsDict | None, 2] = None

payload = Packet(7, ["a", "b"], None)
restored = Packet.decode(encode(payload))
assert restored.id == 7
```

## 核心概念

### 标量类型

| Python 类型 | 编码语义 | 说明 |
| --- | --- | --- |
| `int` | `ZeroTag` 或 `Int1/2/4/8` | 按值范围做紧凑编码。 |
| `float` | `ZeroTag` 或浮点类型 | `0.0` 可走零值优化。 |
| `bool` | 整型语义 | 在协议层按数值处理。 |
| `str` | `String1` / `String4` | 按 UTF-8 字节长度选择。 |
| `bytes` | `SimpleList` | 对应 `vector<byte>`。 |
| `Any` | 运行时分派 | 根据实际值决定编码分支。 |

`bytes` 语义同时接受实现 buffer protocol 的输入（如 `bytearray`、`memoryview`），编码结果与 `bytes` 一致。

### 容器类型

| Python 类型 | 编码语义 | 解码结果 |
| --- | --- | --- |
| `list[T]` | `List` | `list` |
| `tuple[T1, T2, ...]` | `List` | `tuple` |
| `tuple[T, ...]` | `List` | `tuple` |
| `set[T]` / `frozenset[T]` | `List` | `set` / `frozenset` |
| `dict[K, V]` | `Map` | `dict` |
| `TarsDict` | `Struct` 语义 | `TarsDict` |

### 结构化类型

* `Struct` 子类: 推荐的建模方式。
* `Enum`: 按 `value` 的底层类型编码。
* `Optional[T]` 或 `T | None`: None 时不写该字段。
* `Union[A, B, ...]`: 按变体顺序匹配并编码。

### typing 标记

* `Annotated[T, Meta(...)]`: 为 `T` 增加约束。
* `Literal`, `NewType`, `Final`, 类型别名: 按展开后的底层类型处理。
* `Required` / `NotRequired`: 主要用于 `TypedDict` 字段语义。

## 注意事项

* 容器中的嵌套类型也必须是受支持类型。
* `Any` 适合边界输入，不建议在核心业务模型滥用。
* 协议演进时，优先让新增字段可选并提供默认值。
