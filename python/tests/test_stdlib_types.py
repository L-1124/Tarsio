import uuid
from dataclasses import dataclass
from datetime import date, datetime, time, timedelta, timezone
from decimal import Decimal
from enum import Enum
from typing import Annotated, NamedTuple, TypedDict

from tarsio import Struct, decode, encode


class StdTypes(Struct):
    """标准库类型结构体."""

    dt: Annotated[datetime, 0]
    d: Annotated[date, 1]
    t: Annotated[time, 2]
    td: Annotated[timedelta, 3]
    uid: Annotated[uuid.UUID, 4]
    dec: Annotated[Decimal, 5]


def test_stdlib_roundtrip() -> None:
    """验证标准库类型能正确往返还原."""
    obj = StdTypes(
        dt=datetime(2020, 1, 2, 3, 4, 5, 6, tzinfo=timezone.utc),
        d=date(2020, 1, 2),
        t=time(3, 4, 5, 6),
        td=timedelta(days=1, seconds=2, microseconds=3),
        uid=uuid.UUID("12345678-1234-5678-1234-567812345678"),
        dec=Decimal("12.34"),
    )
    data = encode(obj)
    restored = decode(StdTypes, data)

    assert restored.dt == obj.dt
    assert restored.d == obj.d
    assert restored.t == obj.t
    assert restored.td == obj.td
    assert restored.uid == obj.uid
    assert restored.dec == obj.dec


class Color(Enum):
    """示例枚举."""

    RED = 1
    BLUE = 2


class EnumWrap(Struct):
    """Enum 包装结构体."""

    color: Annotated[Color, 0]


def test_enum_roundtrip() -> None:
    """验证 Enum 类型能正确往返还原."""
    obj = EnumWrap(Color.RED)
    data = encode(obj)
    restored = decode(EnumWrap, data)
    assert restored.color == Color.RED


@dataclass
class User:
    """示例数据类."""

    uid: int
    name: str


def test_dataclass_roundtrip() -> None:
    """验证 dataclass 可直接编码与解码."""
    obj = User(1, "Ada")
    data = encode(obj)
    restored = decode(User, data)
    assert restored == obj


class Point(NamedTuple):
    """示例 NamedTuple."""

    x: int
    y: int


def test_namedtuple_roundtrip() -> None:
    """验证 NamedTuple 可直接编码与解码."""
    obj = Point(1, 2)
    data = encode(obj)
    restored = decode(Point, data)
    assert restored == obj


class Payload(TypedDict):
    """示例 TypedDict."""

    a: int
    b: str


class TypedDictWrap(Struct):
    """TypedDict 包装结构体."""

    payload: Annotated[Payload, 0]


def test_typeddict_roundtrip() -> None:
    """验证 TypedDict 可作为结构体字段往返."""
    obj = TypedDictWrap({"a": 1, "b": "x"})
    data = encode(obj)
    restored = decode(TypedDictWrap, data)
    assert restored.payload == {"a": 1, "b": "x"}
