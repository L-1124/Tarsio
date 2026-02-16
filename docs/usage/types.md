# 支持类型

本页汇总 Tarsio 在 Schema API 中支持的 Python 类型。这些类型会先被 `introspect` 解析为语义中间表示，再映射到 Tars 协议编码。

所有示例都假设以下导入：

```python
from dataclasses import dataclass
from enum import Enum
from typing import Annotated, Any, Final, Literal, NewType, Optional, TypeAlias, TypedDict
from typing_extensions import NotRequired, Required, TypeAliasType
from tarsio import Struct, Meta, TarsDict, decode, encode, field
```

## 标量类型 (Scalar Types)

### [`int`][int]

编码：`ZeroTag` 或 `Int1/Int2/Int4/Int8`。

```python
class S(Struct):
    value: Annotated[int, 0]

encoded = encode(S(123))
restored = decode(S, encoded)
assert restored.value == 123
```

### [`float`][float]

编码：`ZeroTag` 或 `Double`。

```python
class S(Struct):
    value: Annotated[float, 0]

encoded = encode(S(1.5))
restored = decode(S, encoded)
assert restored.value == 1.5
```

### [`bool`][bool]

编码：`ZeroTag` 或 `Int1/Int2/Int4/Int8`。

```python
class S(Struct):
    value: Annotated[bool, 0]

encoded = encode(S(True))
restored = decode(S, encoded)
assert restored.value is True
```

### [`str`][str]

编码：`String1` 或 `String4`。

```python
class S(Struct):
    value: Annotated[str, 0]

encoded = encode(S("hello"))
restored = decode(S, encoded)
assert restored.value == "hello"
```

### [`bytes`][bytes]

编码：`SimpleList`。

```python
class S(Struct):
    value: Annotated[bytes, 0]

encoded = encode(S(b"\x01\x02"))
restored = decode(S, encoded)
assert restored.value == b"\x01\x02"
```

## 容器类型 (Container Types)

### [`list[T]`][list]

编码：`List`（若元素类型为 int 且值为 bytes，则使用 `SimpleList`）。

```python
class S(Struct):
    value: Annotated[list[int], 0]

encoded = encode(S([1, 2, 3]))
restored = decode(S, encoded)
assert restored.value == [1, 2, 3]
```

### [`tuple[T]`][tuple]

编码：`List`（若元素类型为 int 且值为 bytes，则使用 `SimpleList`）。

支持定长元组 `tuple[T1, T2, ...]` 和变长元组 `tuple[T, ...]`。

```python
class S(Struct):
    fixed: Annotated[tuple[str, int], 0]
    variable: Annotated[tuple[int, ...], 1]

obj = S(("a", 1), (1, 2, 3))
encoded = encode(obj)
restored = decode(S, encoded)
assert restored.fixed == ("a", 1)
assert restored.variable == (1, 2, 3)
```

### [`set[T]`][set] / [`frozenset[T]`][frozenset]

编码：`List`。

```python
class S(Struct):
    value: Annotated[set[int], 0]

encoded = encode(S({1, 2}))
restored = decode(S, encoded)
assert restored.value == {1, 2}
```

### [`dict[K, V]`][dict]

编码：`Map`。

```python
class S(Struct):
    value: Annotated[dict[str, int], 0]

encoded = encode(S({"a": 1}))
restored = decode(S, encoded)
assert restored.value == {"a": 1}
```

## 抽象基类 (Abstract Base Classes)

Tarsio 支持 `collections.abc` 中的常见抽象类型，它们会被映射到最接近的 Tars 容器类型。

### [`Collection[T]`][collections.abc.Collection] / [`Sequence[T]`][collections.abc.Sequence]

编码：`List`。解码时默认转换为 `list`。

```python
from collections.abc import Sequence

class S(Struct):
    value: Annotated[Sequence[int], 0]

encoded = encode(S((1, 2, 3)))
restored = decode(S, encoded)
assert restored.value == [1, 2, 3]  # Decodes to list
```

### [`Set[T]`][collections.abc.Set] / [`MutableSet[T]`][collections.abc.MutableSet]

编码：`List`。解码时默认转换为 `set`。

```python
from collections.abc import Set

class S(Struct):
    value: Annotated[Set[int], 0]

encoded = encode(S({1, 2}))
restored = decode(S, encoded)
assert restored.value == {1, 2}  # Decodes to set
```

### [`Mapping[K, V]`][collections.abc.Mapping] / [`MutableMapping[K, V]`][collections.abc.MutableMapping]

编码：`Map`。解码时默认转换为 `dict`。

```python
from collections.abc import Mapping

class S(Struct):
    value: Annotated[Mapping[str, int], 0]

encoded = encode(S({"a": 1}))
restored = decode(S, encoded)
assert restored.value == {"a": 1}  # Decodes to dict
```

## 结构化类型 (Structural Types)

### [`Struct`][tarsio.Struct]

编码：`StructBegin` ... `StructEnd`。

Tarsio 的核心类型，提供了最强的性能与最丰富的功能。

* **构造器**：支持按 Tag 顺序的 **位置参数** 以及按字段名的 **关键字参数**。
* **高性能解码**：**关键优化**。为了追求极致性能，从二进制解码 `Struct` 实例时会**绕过 `__init__` 方法**，直接将数据写入对象内存。这意味着你在 `__init__` 中定义的衍生逻辑在解码对象上不会执行。
* **配置项**：支持 `frozen`（不可变）、`order`（可排序）、`omit_defaults`（编码省略默认值）等高级配置。

```python
class User(Struct, frozen=True):
    id: Annotated[int, 0]
    name: Annotated[str, 1]
    # 支持 Meta 约束
    score: Annotated[int, Meta(ge=0)] = field(tag=2, default=0)

# 支持位置参数构造 (按 Tag 顺序)
user1 = User(1, "Ada")
# 也支持关键字参数
user2 = User(name="Bob", id=2)

encoded = encode(user1)
# 解码产生的对象不会调用 User.__init__
restored = decode(User, encoded)
```

### [`dataclass`][dataclasses.dataclass]

编码：`StructBegin` ... `StructEnd`。

```python
@dataclass
class User:
    id: int
    name: str

obj = User(1, "Ada")
encoded = encode(obj)
restored = decode(User, encoded)
assert restored == obj
```

### [`NamedTuple`][typing.NamedTuple]

编码：`StructBegin` ... `StructEnd`。

```python
class Point(NamedTuple):
    x: int
    y: int

obj = Point(1, 2)
encoded = encode(obj)
restored = decode(Point, encoded)
assert restored == obj
```

### [`TypedDict`][typing.TypedDict]

编码：`StructBegin` ... `StructEnd`。

```python
class Payload(TypedDict):
    a: int
    b: str

class S(Struct):
    value: Annotated[Payload, 0]

obj = S({"a": 1, "b": "x"})
encoded = encode(obj)
restored = decode(S, encoded)
assert restored.value == {"a": 1, "b": "x"}
```

### [`TarsDict`][tarsio.TarsDict]

编码：`StructBegin` ... `StructEnd`。

一种特殊的字典类型，继承自 `dict`，专门用于表示 Tars 结构中的 **Tag-Value 映射**。它的键必须是整数 Tag（0-255），值可以是任意支持的 Tars 类型。

`TarsDict` 是 Raw 模式（无 Schema 模式）下的核心数据容器：

* 在 `decode` (单参数) 中，所有的 Tars 结构体都会被还原为 `TarsDict`。
* 在 `encode` 中，只有 `TarsDict` 会被编码为结构体字段序列，普通 `dict` 则一律视为 Map。

```python

from tarsio import TarsDict, encode



# 1. 作为 Raw 模式的输入

raw_data = TarsDict({

    0: 123,           # Tag 0

    1: "hello",       # Tag 1

    2: TarsDict({     # 嵌套结构体

        0: True

    })

})

encoded = encode(raw_data)



# 2. 在 Struct 中用于保留原始结构

class DynamicMsg(Struct):

    header: Annotated[int, 0]

    # 明确标注该字段为一个原始结构体容器

    payload: Annotated[TarsDict, 1]



msg = DynamicMsg(header=1, payload=TarsDict({10: "raw_content"}))

```

## 联合与可选类型 (Union Types)

### [`Optional[T]`][typing.Optional] / `T | None`

编码：None 时不写 tag，有值时按内层类型映射。

```python
class S(Struct):
    value: Annotated[int | None, 0] = None

encoded = encode(S())
restored = decode(S, encoded)
assert restored.value is None
```

### [`Union[A, B, ...]`][typing.Union] / `A | B`

编码：按变体顺序匹配实际值，直接按匹配类型编码。

```python
class S(Struct):
    value: Annotated[int | str, 0]

encoded = encode(S("x"))
restored = decode(S, encoded)
assert restored.value == "x"
```

## 枚举类型 (Enum Types)

### [`Enum`][enum.Enum]

编码：取 `value` 的内层类型映射。

```python
class Color(Enum):
    RED = 1
    BLUE = 2

class S(Struct):
    value: Annotated[Color, 0]

obj = S(Color.RED)
encoded = encode(obj)
restored = decode(S, encoded)
assert restored.value == Color.RED
```

## 特殊标记类型 (Marker Types)

字段 Tag 支持显式与隐式混合：

* 显式 tag：通过 `field(tag=...)` 指定。
* 隐式 tag：未显式指定时按字段定义顺序自动分配。
* 隐式字段位于显式字段之后时，会从该显式 tag 继续递增分配。

### [`Annotated[T, tag]`][typing.Annotated]

编码：按 `T` 的类型映射。`Annotated` 主要用于附加约束/标记元数据。
显式 Tag 建议使用 `field(tag=...)`；`Annotated[T, tag]` 作为兼容语法仍可使用。

```python
class S(Struct):
    value: Annotated[int, Literal["important"]] = field(tag=7)

encoded = encode(S(42))
restored = decode(S, encoded)
assert restored.value == 42
```

### [`Literal[...]`][typing.Literal]

编码：按其基础类型映射。

```python
class S(Struct):
    value: Annotated[Literal["ok"], 0]

encoded = encode(S("ok"))
restored = decode(S, encoded)
assert restored.value == "ok"
```

### [`NewType`][typing.NewType]

编码：按底层类型映射。

```python
UserId = NewType("UserId", int)

class S(Struct):
    value: Annotated[UserId, 0]

encoded = encode(S(UserId(1)))
restored = decode(S, encoded)
assert restored.value == 1
```

### [`Final[T]`][typing.Final]

编码：按 `T` 的类型映射。

```python
class S(Struct):
    value: Annotated[Final[int], 0]

encoded = encode(S(9))
restored = decode(S, encoded)
assert restored.value == 9
```

### [`TypeAlias`][typing.TypeAlias] / [`TypeAliasType`][typing.TypeAliasType]

编码：按别名展开后的类型映射。

```python
Name: TypeAlias = str

class S(Struct):
    value: Annotated[Name, 0]

encoded = encode(S("Ada"))
restored = decode(S, encoded)
assert restored.value == "Ada"

Alias = TypeAliasType("Alias", str)

class S2(Struct):
    value: Annotated[Alias, 0]
```

### [`Required`][typing.Required] / [`NotRequired`][typing.NotRequired]

编码：按字段的实际类型映射。

```python
class Payload(TypedDict):
    id: Required[int]
    name: NotRequired[str]

class S(Struct):
    value: Annotated[Payload, 0]

encoded = encode(S({"id": 1}))
restored = decode(S, encoded)
assert restored.value["id"] == 1
```

## 其他类型

### [`Any`][typing.Any]

编码：运行时按值类型选择具体 TarsType。

```python
class S(Struct):
    value: Annotated[Any, 0]

encoded = encode(S(b"\xff"))
restored = decode(S, encoded)
assert restored.value == b"\xff"

# 仅在 Any 解码路径中，SimpleList 若为有效 UTF-8 会返回 str，否则返回 bytes。
```
