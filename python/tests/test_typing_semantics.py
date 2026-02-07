from typing import Annotated, Any, Final, Literal, NewType, TypeAlias

from tarsio import Struct, decode, encode

UserId = NewType("UserId", int)


class NewTypeWrap(Struct):
    """NewType 包装结构体."""

    uid: Annotated[UserId, 0]


def test_newtype_unwraps_to_base() -> None:
    """验证 NewType 会被解析为底层类型."""
    obj = NewTypeWrap(UserId(123))
    data = encode(obj)
    restored = decode(NewTypeWrap, data)
    assert restored.uid == 123
    assert isinstance(restored.uid, int)


class FinalWrap(Struct):
    """Final 包装结构体."""

    value: Annotated[Final[int], 0]


def test_final_unwraps_to_base() -> None:
    """验证 Final 会被解析为底层类型."""
    obj = FinalWrap(7)
    data = encode(obj)
    restored = decode(FinalWrap, data)
    assert restored.value == 7


Name: TypeAlias = str


class TypeAliasWrap(Struct):
    """TypeAlias 包装结构体."""

    name: Annotated[Name, 0]


def test_typealias_unwraps_to_base() -> None:
    """验证 TypeAlias 会被解析为底层类型."""
    obj = TypeAliasWrap("Ada")
    data = encode(obj)
    restored = decode(TypeAliasWrap, data)
    assert restored.name == "Ada"


class LiteralWrap(Struct):
    """Literal 包装结构体."""

    flag: Annotated[Literal["ok"], 0]


def test_literal_maps_to_underlying_type() -> None:
    """验证 Literal 会被解析为底层类型."""
    obj = LiteralWrap("ok")
    data = encode(obj)
    restored = decode(LiteralWrap, data)
    assert restored.flag == "ok"


class UnionWrap(Struct):
    """Union 包装结构体."""

    value: Annotated[int | str, 0]


def test_union_roundtrip_int() -> None:
    """验证 Union[int, str] 能正确解码为 int."""
    obj = UnionWrap(123)
    data = encode(obj)
    restored = decode(UnionWrap, data)
    assert restored.value == 123


def test_union_roundtrip_str() -> None:
    """验证 Union[int, str] 能正确解码为 str."""
    obj = UnionWrap("abc")
    data = encode(obj)
    restored = decode(UnionWrap, data)
    assert restored.value == "abc"


class AnyWrap(Struct):
    """Any 包装结构体."""

    value: Annotated[Any, 0]


def test_any_roundtrip_bytes() -> None:
    """验证 Any 能往返字节数据."""
    obj = AnyWrap(b"\xff")
    data = encode(obj)
    restored = decode(AnyWrap, data)
    assert restored.value == b"\xff"
