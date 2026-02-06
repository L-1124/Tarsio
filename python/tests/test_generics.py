"""测试 Generic Struct (泛型结构体) 的支持.

涵盖 checklist:
- [x] Box[int], Box[str], Box[User]
- [x] Generic template 本身不可 encode/decode (应抛出错误)
"""

from typing import Annotated, Generic, TypeVar

import pytest
from tarsio import Struct, decode, encode

T = TypeVar("T")


# ==========================================
# 泛型结构体定义
# ==========================================


class Box(Struct, Generic[T]):
    """通用泛型容器."""

    val: Annotated[T, 0]


class User(Struct):
    """普通结构体，用作泛型参数."""

    id: Annotated[int, 0]
    name: Annotated[str, 1]


# ==========================================
# 泛型实例化测试
# ==========================================


def test_generic_box_int() -> None:
    """Box[int] 应该正确编解码."""
    # Arrange
    # 注意:Python 运行时构造 Box[int] 不会创建新类,但 Tarsio 的元类/init_subclass
    # 可能会在具体化时(如果用户显式继承)或运行时处理.
    # 根据 AGENTS.md,Tarsio 支持泛型,但通常需要具体化类型或运行时推导.
    # 这里我们测试最直接的用法:直接实例化泛型类(如果支持运行时推导)
    # 或者定义具体子类.

    # 假设 Tarsio 支持运行时根据 __orig_class__ 或手动指定类型.
    # 如果不支持直接 Box[int](1),则用户必须定义 class IntBox(Box[int]): pass

    # 既然 AGENTS.md 提到 "Generic Struct",我们先尝试定义具体子类,这是最稳妥的方式.
    class IntBox(Box[int]):
        pass

    original = IntBox(100)

    # Act
    encoded = encode(original)
    decoded = decode(IntBox, encoded)

    # Assert
    assert decoded.val == 100


def test_generic_box_str() -> None:
    """Box[str] 应该正确编解码."""

    class StrBox(Box[str]):
        pass

    original = StrBox("hello")

    # Act
    encoded = encode(original)
    decoded = decode(StrBox, encoded)

    # Assert
    assert decoded.val == "hello"


def test_generic_box_str_empty_string_roundtrip() -> None:
    """Box[str] 的空字符串值应能往返还原."""

    class StrBox(Box[str]):
        pass

    # Arrange
    original = StrBox("")

    # Act
    encoded = encode(original)
    decoded = decode(StrBox, encoded)

    # Assert
    assert decoded.val == ""


def test_generic_box_user() -> None:
    """Box[User] (嵌套结构体泛型) 应该正确编解码."""

    class UserBox(Box[User]):
        pass

    user = User(1, "Alice")
    original = UserBox(user)

    # Act
    encoded = encode(original)
    decoded = decode(UserBox, encoded)

    # Assert
    assert isinstance(decoded.val, User)
    assert decoded.val.id == 1
    assert decoded.val.name == "Alice"


# ==========================================
# 泛型模板测试 (不变量)
# ==========================================


def test_generic_template_cannot_be_encoded() -> None:
    """Generic template (未具体化) 不应被允许编码."""
    with pytest.raises(TypeError, match="abstract schema class"):
        Box(1)  # type: ignore


def test_generic_template_cannot_be_decoded() -> None:
    """Generic template (未具体化) 不应被允许解码."""
    with pytest.raises(TypeError, match="No schema found"):
        decode(Box, b"\x00\x01")  # type: ignore


def test_generic_decode_skips_unknown_tag() -> None:
    """泛型具体化类型解码时应能跳过未知字段."""

    class BoxV1(Struct, Generic[T]):
        val: Annotated[T, 0]

    class BoxV2(Struct, Generic[T]):
        val: Annotated[T, 0]
        meta: Annotated[str | None, 1] = None

    class IntBoxV1(BoxV1[int]):
        pass

    class IntBoxV2(BoxV2[int]):
        pass

    # Arrange
    data = encode(IntBoxV2(123, "ignored"))

    # Act
    result = decode(IntBoxV1, data)

    # Assert
    assert result.val == 123
