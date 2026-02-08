"""测试 typing/collections/generics 相关行为.

覆盖 Tarsio 支持的所有 Python 类型映射。
"""

import sys
from collections.abc import (
    Collection,
    Mapping,
    MutableMapping,
    MutableSequence,
    MutableSet,
    Sequence,
    Set,
)
from dataclasses import dataclass
from enum import Enum, IntEnum
from typing import (
    Annotated,
    Any,
    Final,
    Literal,
    NewType,
    Optional,
    TypeAlias,
    Union,
    cast,
)

from tarsio._core import Meta, Struct, decode, encode, inspect
from typing_extensions import (
    NamedTuple,
    NotRequired,
    Required,
    TypeAliasType,
    TypedDict,
)

AliasInt: TypeAlias = int

# ==========================================
# 1. Primitives (基础类型)
# ==========================================


def test_primitives_roundtrip() -> None:
    """验证基础类型 (int, float, bool, str, bytes) 的编解码."""

    class PrimitiveStruct(Struct):
        i: Annotated[int, 0]
        f: Annotated[float, 1]
        b: Annotated[bool, 2]
        s: Annotated[str, 3]
        by: Annotated[bytes, 4]

    obj = PrimitiveStruct(123, 1.23, True, "hello", b"\x01\x02")
    encoded = encode(obj)
    decoded = decode(PrimitiveStruct, encoded)

    assert decoded.i == 123
    assert decoded.f == 1.23
    assert decoded.b is True
    assert decoded.s == "hello"
    assert decoded.by == b"\x01\x02"


# ==========================================
# 2. Containers (容器类型)
# ==========================================


def test_list_roundtrip() -> None:
    """验证 list[T] 的编解码."""

    class ListStruct(Struct):
        l1: Annotated[list[int], 0]
        l2: Annotated[list[str], 1]

    obj = ListStruct([1, 2], ["a", "b"])
    decoded = decode(ListStruct, encode(obj))
    assert decoded.l1 == [1, 2]
    assert decoded.l2 == ["a", "b"]


def test_tuple_fixed_roundtrip() -> None:
    """验证定长 tuple[T1, T2] 的编解码."""

    class TupleStruct(Struct):
        t: Annotated[tuple[int, str, float], 0]

    obj = TupleStruct((1, "a", 1.1))
    decoded = decode(TupleStruct, encode(obj))
    assert decoded.t == (1, "a", 1.1)


def test_tuple_variable_roundtrip() -> None:
    """验证变长 tuple[T, ...] 的编解码."""

    class VarTupleStruct(Struct):
        t: Annotated[tuple[int, ...], 0]

    obj = VarTupleStruct((1, 2, 3))
    decoded = decode(VarTupleStruct, encode(obj))
    assert decoded.t == (1, 2, 3)


def test_set_roundtrip() -> None:
    """验证 set[T] / frozenset[T] 的编解码."""

    class SetStruct(Struct):
        s: Annotated[set[int], 0]
        fs: Annotated[frozenset[int], 1]

    obj = SetStruct({1, 2}, frozenset({3, 4}))
    decoded = decode(SetStruct, encode(obj))
    assert decoded.s == {1, 2}
    assert decoded.fs == frozenset({3, 4})


def test_dict_roundtrip() -> None:
    """验证 dict[K, V] 的编解码."""

    class DictStruct(Struct):
        d1: Annotated[dict[str, int], 0]
        d2: Annotated[dict[int, str], 1]

    obj = DictStruct({"a": 1}, {2: "b"})
    decoded = decode(DictStruct, encode(obj))
    assert decoded.d1 == {"a": 1}
    assert decoded.d2 == {2: "b"}


# ==========================================
# 3. Abstract Base Classes (抽象基类)
# ==========================================


def test_abc_collection_list() -> None:
    """验证 Collection/Sequence 映射为 list."""

    class AbcListStruct(Struct):
        c: Annotated[Collection[int], 0]
        s: Annotated[Sequence[int], 1]
        ms: Annotated[MutableSequence[int], 2]
        ac: Annotated[Collection[int], 3]
        as_: Annotated[Sequence[int], 4]
        ams: Annotated[MutableSequence[int], 5]

    data = [1, 2]
    # cast necessary because Collection is abstract in typing but we pass list
    obj = AbcListStruct(data, data, data, data, data, data)  # type: ignore
    decoded = decode(AbcListStruct, encode(obj))

    for field in ["c", "s", "ms", "ac", "as_", "ams"]:
        assert getattr(decoded, field) == data
        assert isinstance(getattr(decoded, field), list)


def test_abc_collection_set() -> None:
    """验证 Set/MutableSet 映射为 set."""

    class AbcSetStruct(Struct):
        s: Annotated[Set[int], 0]
        ms: Annotated[MutableSet[int], 1]
        as_: Annotated[Set[int], 2]
        ams: Annotated[MutableSet[int], 3]

    data = {1, 2}
    obj = AbcSetStruct(data, data, data, data)  # type: ignore
    decoded = decode(AbcSetStruct, encode(obj))

    for field in ["s", "ms", "as_", "ams"]:
        assert getattr(decoded, field) == data
        assert isinstance(getattr(decoded, field), set)


def test_abc_collection_map() -> None:
    """验证 Mapping/MutableMapping 映射为 dict."""

    class AbcMapStruct(Struct):
        m: Annotated[Mapping[str, int], 0]
        mm: Annotated[MutableMapping[str, int], 1]
        am: Annotated[Mapping[str, int], 2]
        amm: Annotated[MutableMapping[str, int], 3]

    data = {"a": 1}
    obj = AbcMapStruct(data, data, data, data)  # type: ignore
    decoded = decode(AbcMapStruct, encode(obj))

    for field in ["m", "mm", "am", "amm"]:
        assert getattr(decoded, field) == data
        assert isinstance(getattr(decoded, field), dict)


# ==========================================
# 4. Structural Types (结构化类型)
# ==========================================


def test_dataclass_support() -> None:
    """验证标准库 dataclass 支持."""

    @dataclass
    class User:
        id: int
        name: str

    class Wrap(Struct):
        u: Annotated[User, 0]

    obj = Wrap(User(1, "dc"))
    decoded = decode(Wrap, encode(obj))
    assert decoded.u == User(1, "dc")


def test_namedtuple_support() -> None:
    """验证 NamedTuple 支持."""

    class Point(NamedTuple):
        x: int
        y: int

    class Wrap(Struct):
        p: Annotated[Point, 0]

    obj = Wrap(Point(10, 20))
    decoded = decode(Wrap, encode(obj))
    assert decoded.p == Point(10, 20)


def test_typeddict_support() -> None:
    """验证 TypedDict 支持 (含 Required/NotRequired)."""

    class UserDict(TypedDict):
        id: int
        name: NotRequired[str]

    class StrictDict(TypedDict):
        id: Required[int]

    class Wrap(Struct):
        u: Annotated[UserDict, 0]
        s: Annotated[StrictDict, 1]

    obj = Wrap({"id": 1, "name": "td"}, {"id": 2})
    decoded = decode(Wrap, encode(obj))
    assert decoded.u == {"id": 1, "name": "td"}
    assert decoded.s == {"id": 2}

    # Test NotRequired missing
    obj2 = Wrap({"id": 1}, {"id": 2})  # name missing
    decoded2 = decode(Wrap, encode(obj2))
    assert decoded2.u.get("name") is None


# ==========================================
# 5. Logic & Markers (逻辑与标记)
# ==========================================


def test_optional_union() -> None:
    """验证 Optional 和 Union."""

    class Logic(Struct):
        uni: Annotated[int | str, 0]
        opt: Annotated[int | None, 1]
        uni_opt: Annotated[int | str | None, 2]

    # Case 1: Values present
    obj1 = Logic("s", 1, 2)
    dec1 = decode(Logic, encode(obj1))
    assert dec1.uni == "s"
    assert dec1.opt == 1
    assert dec1.uni_opt == 2

    # Case 2: None
    obj2 = Logic(10, None, None)
    dec2 = decode(Logic, encode(obj2))
    assert dec2.uni == 10
    assert dec2.opt is None
    assert dec2.uni_opt is None


def test_any_type() -> None:
    """验证 Any 类型 (动态推断)."""

    class AnyStruct(Struct):
        val: Annotated[Any, 0]

    # Note: b"\x00" decodes to "\x00" because Tarsio decodes SimpleList into str if valid UTF-8
    cases = [
        (1, 1),
        ("s", "s"),
        (b"\x00", "\x00"),
        ([1, 2], [1, 2]),
        ({"a": 1}, {"a": 1}),
    ]
    for inp, exp in cases:
        obj = AnyStruct(inp)
        dec = decode(AnyStruct, encode(obj))
        assert dec.val == exp


def test_literal_type() -> None:
    """验证 Literal 类型."""

    class Lit(Struct):
        status: Annotated[Literal["ok", "err"], 0]
        code: Annotated[Literal[1, 2], 1]

    obj = Lit("ok", 1)
    dec = decode(Lit, encode(obj))
    assert dec.status == "ok"
    assert dec.code == 1


def test_newtype_final_alias() -> None:
    """验证 NewType, Final, TypeAlias."""
    MyInt = NewType("MyInt", int)

    class Wrapper(Struct):
        n: Annotated[MyInt, 0]
        f: Annotated[Final[int], 1]
        a: Annotated[AliasInt, 2]

    obj = Wrapper(MyInt(1), 2, 3)
    dec = decode(Wrapper, encode(obj))
    assert dec.n == 1
    assert dec.f == 2
    assert dec.a == 3


# ==========================================
# 6. Enum Types (枚举)
# ==========================================


def test_enum_types() -> None:
    """验证 Enum, IntEnum."""

    class Color(Enum):
        RED = 1
        BLUE = "blue"

    class Level(IntEnum):
        LOW = 10
        HIGH = 20

    class EnumStruct(Struct):
        c: Annotated[Color, 0]
        lvl: Annotated[Level, 1]

    obj = EnumStruct(Color.RED, Level.HIGH)
    dec = decode(EnumStruct, encode(obj))
    assert dec.c == Color.RED
    assert dec.lvl == Level.HIGH

    obj2 = EnumStruct(Color.BLUE, Level.LOW)
    dec2 = decode(EnumStruct, encode(obj2))
    assert dec2.c == Color.BLUE
    assert dec2.lvl == Level.LOW


if sys.version_info >= (3, 11):
    from enum import StrEnum

    def test_strenum() -> None:
        """验证 StrEnum (Py3.11+)."""

        class State(StrEnum):
            ON = "on"
            OFF = "off"

        class S(Struct):
            s: Annotated[State, 0]

        obj = S(State.ON)
        dec = decode(S, encode(obj))
        assert dec.s == State.ON


# ==========================================
# 7. Inspect API (类型内省)
# ==========================================


def test_inspect_type_info_supported_kinds() -> None:
    """验证 type_info 覆盖所有支持类型."""

    class Inner(Struct):
        v: Annotated[int, 0]

    class Level(IntEnum):
        LOW = 1
        HIGH = 2

    cases = {
        "int": inspect.type_info(int),
        "str": inspect.type_info(str),
        "float": inspect.type_info(float),
        "bool": inspect.type_info(bool),
        "bytes": inspect.type_info(bytes),
        "any": inspect.type_info(Any),
        "none": inspect.type_info(type(None)),
        "enum": inspect.type_info(Level),
        "union": inspect.type_info(Union[int, str]),  # noqa: UP007
        "list": inspect.type_info(list[int]),
        "tuple": inspect.type_info(tuple[int, str]),
        "var_tuple": inspect.type_info(tuple[int, ...]),
        "map": inspect.type_info(dict[str, int]),
        "set": inspect.type_info(set[int]),
        "optional": inspect.type_info(Optional[int]),  # noqa: UP045
        "struct": inspect.type_info(Inner),
    }

    assert cases["int"].kind == "int"
    assert cases["str"].kind == "str"
    assert cases["float"].kind == "float"
    assert cases["bool"].kind == "bool"
    assert cases["bytes"].kind == "bytes"
    assert cases["any"].kind == "any"
    assert cases["none"].kind == "none"
    assert cases["enum"].kind == "enum"
    assert cases["union"].kind == "union"
    assert cases["list"].kind == "list"
    assert cases["tuple"].kind == "tuple"
    assert cases["var_tuple"].kind == "var_tuple"
    assert cases["map"].kind == "map"
    assert cases["set"].kind == "set"
    assert cases["optional"].kind == "optional"
    assert cases["struct"].kind == "struct"

    enum_info = cast(inspect.EnumType, cases["enum"])
    struct_info = cast(inspect.StructType, cases["struct"])
    assert enum_info.value_type.kind == "int"
    assert struct_info.cls is Inner


def test_inspect_struct_info_fields() -> None:
    """验证 struct_info 的字段结构与属性."""

    class Sample(Struct):
        a: Annotated[int, 0]
        b: Annotated[str, 1] = "x"
        c: Annotated[int | None, 2] = None

    info = inspect.struct_info(Sample)
    assert info is not None
    assert info.cls is Sample
    assert [f.name for f in info.fields] == ["a", "b", "c"]

    field_a, field_b, field_c = info.fields
    assert field_a.tag == 0
    assert field_a.type.kind == "int"
    assert field_a.required is True

    assert field_b.tag == 1
    assert field_b.type.kind == "str"
    assert field_b.has_default is True

    assert field_c.tag == 2
    assert field_c.type.kind == "optional"
    assert field_c.optional is True


def test_inspect_constraints_from_meta() -> None:
    """验证 Meta 约束进入 constraints."""

    class Limited(Struct):
        v: Annotated[int, Meta(tag=0, gt=1, le=10)]

    info = inspect.struct_info(Limited)
    assert info is not None
    field = info.fields[0]
    assert field.constraints is not None
    assert field.constraints.gt == 1
    assert field.constraints.le == 10


AliasInt2 = TypeAliasType("AliasInt2", int)


def test_inspect_type_alias_type() -> None:
    """验证 TypeAliasType 能被解析."""
    info = inspect.type_info(AliasInt2)
    assert info.kind == "int"
