# 支持类型

本页汇总 Tarsio 在 Schema API 中支持的 Python 类型。
类型会先被 `introspect` 解析为语义中间表示，再映射到协议编码。

所有示例都假设以下导入：

```python
from dataclasses import dataclass
from datetime import date, datetime, time, timedelta, timezone
from decimal import Decimal
from enum import Enum
from typing import Annotated, Any, Final, Literal, NewType, NotRequired, Optional, TypeAlias, TypedDict
from uuid import UUID
from tarsio import Struct, encode, decode
```

示例中涉及 [tarsio.Struct][tarsio.Struct]、[tarsio.encode][tarsio.encode]、[tarsio.decode][tarsio.decode]。

## [`int`][int]

```python
class S(Struct):
    value: Annotated[int, 0]

encoded = encode(S(123))
restored = decode(S, encoded)
assert restored.value == 123
```

## [`float`][float]

```python
class S(Struct):
    value: Annotated[float, 0]

encoded = encode(S(1.5))
restored = decode(S, encoded)
assert restored.value == 1.5
```

## [`bool`][bool]

```python
class S(Struct):
    value: Annotated[bool, 0]

encoded = encode(S(True))
restored = decode(S, encoded)
assert restored.value is True
```

## [`str`][str]

```python
class S(Struct):
    value: Annotated[str, 0]

encoded = encode(S("hello"))
restored = decode(S, encoded)
assert restored.value == "hello"
```

## [`bytes`][bytes]

```python
class S(Struct):
    value: Annotated[bytes, 0]

encoded = encode(S(b"\x01\x02"))
restored = decode(S, encoded)
assert restored.value == b"\x01\x02"
```

## [`list[T]`][list]

```python
class S(Struct):
    value: Annotated[list[int], 0]

encoded = encode(S([1, 2, 3]))
restored = decode(S, encoded)
assert restored.value == [1, 2, 3]
```

## [`tuple[T]`][tuple]

```python
class S(Struct):
    value: Annotated[tuple[str], 0]

encoded = encode(S(("a", "b")))
restored = decode(S, encoded)
assert restored.value == ("a", "b")
```

## [`dict[K, V]`][dict]

```python
class S(Struct):
    value: Annotated[dict[str, int], 0]

encoded = encode(S({"a": 1}))
restored = decode(S, encoded)
assert restored.value == {"a": 1}
```

## [`Annotated[T, tag]`][typing.Annotated]

```python
class S(Struct):
    value: Annotated[int, 7]

encoded = encode(S(42))
restored = decode(S, encoded)
assert restored.value == 42
```

## [`Optional[T]`][typing.Optional] / `T | None`

```python
class S(Struct):
    value: Annotated[Optional[int], 0] = None

encoded = encode(S())
restored = decode(S, encoded)
assert restored.value is None
```

## [`Union[A, B, ...]`][typing.Union]

```python
class S(Struct):
    value: Annotated[int | str, 0]

encoded = encode(S("x"))
restored = decode(S, encoded)
assert restored.value == "x"
```

## [`Literal[...]`][typing.Literal]

```python
class S(Struct):
    value: Annotated[Literal["ok"], 0]

encoded = encode(S("ok"))
restored = decode(S, encoded)
assert restored.value == "ok"
```

## [`NewType`][typing.NewType]

```python
UserId = NewType("UserId", int)

class S(Struct):
    value: Annotated[UserId, 0]

encoded = encode(S(UserId(1)))
restored = decode(S, encoded)
assert restored.value == 1
```

## [`Final[T]`][typing.Final]

```python
class S(Struct):
    value: Annotated[Final[int], 0]

encoded = encode(S(9))
restored = decode(S, encoded)
assert restored.value == 9
```

## [`TypeAlias`][typing.TypeAlias] / [`TypeAliasType`][typing.TypeAliasType]

```python
Name: TypeAlias = str

class S(Struct):
    value: Annotated[Name, 0]

encoded = encode(S("Ada"))
restored = decode(S, encoded)
assert restored.value == "Ada"
```

## [`Required`][typing.Required] / [`NotRequired`][typing.NotRequired]

```python
class Payload(TypedDict):
    id: int
    name: NotRequired[str]

class S(Struct):
    value: Annotated[Payload, 0]

encoded = encode(S({"id": 1}))
restored = decode(S, encoded)
assert restored.value["id"] == 1
```

## [`Any`][typing.Any]

```python
class S(Struct):
    value: Annotated[Any, 0]

encoded = encode(S(b"\xff"))
restored = decode(S, encoded)
assert restored.value == b"\xff"
```

## [`datetime`][datetime.datetime]

```python
class S(Struct):
    value: Annotated[datetime, 0]

obj = S(datetime(2020, 1, 2, tzinfo=timezone.utc))
encoded = encode(obj)
restored = decode(S, encoded)
assert restored.value == obj.value
```

## [`date`][datetime.date]

```python
class S(Struct):
    value: Annotated[date, 0]

obj = S(date(2020, 1, 2))
encoded = encode(obj)
restored = decode(S, encoded)
assert restored.value == obj.value
```

## [`time`][datetime.time]

```python
class S(Struct):
    value: Annotated[time, 0]

obj = S(time(3, 4, 5, 6))
encoded = encode(obj)
restored = decode(S, encoded)
assert restored.value == obj.value
```

## [`timedelta`][datetime.timedelta]

```python
class S(Struct):
    value: Annotated[timedelta, 0]

obj = S(timedelta(days=1, seconds=2, microseconds=3))
encoded = encode(obj)
restored = decode(S, encoded)
assert restored.value == obj.value
```

## [`UUID`][uuid.UUID]

```python
class S(Struct):
    value: Annotated[UUID, 0]

obj = S(UUID("12345678-1234-5678-1234-567812345678"))
encoded = encode(obj)
restored = decode(S, encoded)
assert restored.value == obj.value
```

## [`Decimal`][decimal.Decimal]

```python
class S(Struct):
    value: Annotated[Decimal, 0]

obj = S(Decimal("12.34"))
encoded = encode(obj)
restored = decode(S, encoded)
assert restored.value == obj.value
```

## [`Enum`][enum.Enum]

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

## [`dataclass`][dataclasses.dataclass]

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

## [`NamedTuple`][typing.NamedTuple]

```python
class Point(NamedTuple):
    x: int
    y: int

obj = Point(1, 2)
encoded = encode(obj)
restored = decode(Point, encoded)
assert restored == obj
```

## [`TypedDict`][typing.TypedDict]

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
