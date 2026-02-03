"""测试 Tars 协议编解码的核心功能.

此文件是库的**根本性测试**，必须保证 100% 通过。
它定义了 Tars 协议实现的基准，任何破坏此文件测试用例的修改均被视为破坏性变更。
"""

from typing import Annotated, Optional

import pytest
from tarsio import Struct, decode, encode

# ==========================================
# 协议测试专用结构体
# ==========================================


class SimpleInt(Struct):
    """仅包含单个整数字段的结构体."""

    val: Annotated[int, 0]


class SimpleString(Struct):
    """仅包含单个字符串字段的结构体."""

    val: Annotated[str, 0]


class TwoFields(Struct):
    """包含两个字段的结构体."""

    id: Annotated[int, 0]
    name: Annotated[str, 1]


class NestedStruct(Struct):
    """嵌套结构体."""

    val: Annotated[int, 0]
    next: Annotated[Optional["NestedStruct"], 1]


# ==========================================
# 编码测试 - 整数
# ==========================================


@pytest.mark.parametrize(
    ("val", "expected_hex"),
    [
        (0, "0c"),  # ZeroTag
        (1, "0001"),  # Int1
        (127, "007f"),  # Int1
        (255, "0100ff"),  # Int2 (255 > 127, needs Int2)
        (256, "010100"),  # Int2
        (0x7FFF, "017fff"),  # Int2
        (-1, "00ff"),  # Int1 (signed -1 = unsigned 0xff)
    ],
    ids=[
        "zero",
        "one",
        "max_int1_signed",
        "int2_255",
        "int2_256",
        "max_int2",
        "negative_one",
    ],
)
def test_encode_int_values(val: int, expected_hex: str) -> None:
    """不同范围整数应该编码为正确的字节序列."""
    # Arrange
    obj = SimpleInt(val)

    # Act
    result = encode(obj)

    # Assert
    assert result.hex() == expected_hex


# ==========================================
# 编码测试 - 字符串
# ==========================================


@pytest.mark.parametrize(
    ("val", "expected_hex"),
    [
        ("", "0600"),
        ("A", "060141"),
        ("Hello", "060548656c6c6f"),
        ("你好", "0606e4bda0e5a5bd"),
    ],
    ids=[
        "empty_string",
        "single_char",
        "ascii_hello",
        "chinese_nihao",
    ],
)
def test_encode_string_values(val: str, expected_hex: str) -> None:
    """字符串应该正确编码为 UTF-8 字节序列."""
    # Arrange
    obj = SimpleString(val)

    # Act
    result = encode(obj)

    # Assert
    assert result.hex() == expected_hex


# ==========================================
# 编码测试 - 复合结构
# ==========================================


def test_encode_two_fields_struct() -> None:
    """多字段结构体应该按照 tag 顺序正确编码."""
    # Arrange
    obj = TwoFields(10, "Alice")
    # Tag 0: Int1(10) -> 00 0a
    # Tag 1: String1(5, "Alice") -> 16 05 416c696365
    expected_hex = "000a1605416c696365"

    # Act
    result = encode(obj)

    # Assert
    assert result.hex() == expected_hex


def test_encode_nested_struct() -> None:
    """嵌套结构体应该正确编码 StructBegin/StructEnd."""
    # Arrange
    obj = NestedStruct(1, NestedStruct(2, None))
    # Tag 0: Int1(1) -> 00 01
    # Tag 1: StructBegin -> 1a
    #   Tag 0: Int1(2) -> 00 02
    #   StructEnd -> 0b
    # Result: 00 01 1a 00 02 0b
    expected_hex = "00011a00020b"

    # Act
    result = encode(obj)

    # Assert
    assert result.hex() == expected_hex


def test_encode_deeply_nested_struct() -> None:
    """深层嵌套结构体应该正确编码."""
    # Arrange
    obj = NestedStruct(1, NestedStruct(2, NestedStruct(3, None)))
    # Tag 0: Int1(1) -> 00 01
    # Tag 1: StructBegin -> 1a
    #   Tag 0: Int1(2) -> 00 02
    #   Tag 1: StructBegin -> 1a
    #     Tag 0: Int1(3) -> 00 03
    #     StructEnd -> 0b
    #   StructEnd -> 0b
    expected_hex = "00011a00021a00030b0b"

    # Act
    result = encode(obj)

    # Assert
    assert result.hex() == expected_hex


# ==========================================
# 解码测试 - 整数
# ==========================================


@pytest.mark.parametrize(
    ("hex_data", "expected_val"),
    [
        ("0c", 0),  # ZeroTag
        ("0001", 1),  # Int1
        ("007f", 127),  # Int1
        ("0100ff", 255),  # Int2
        ("017fff", 0x7FFF),  # Int2
    ],
    ids=[
        "zero",
        "one",
        "max_int1_signed",
        "int2_255",
        "max_int2",
    ],
)
def test_decode_int_values(hex_data: str, expected_val: int) -> None:
    """字节序列应该正确解码为整数值."""
    # Arrange
    data = bytes.fromhex(hex_data)

    # Act
    result = decode(SimpleInt, data)

    # Assert
    assert result.val == expected_val


# ==========================================
# 解码测试 - 字符串
# ==========================================


@pytest.mark.parametrize(
    ("hex_data", "expected_val"),
    [
        ("0600", ""),
        ("060141", "A"),
        ("060548656c6c6f", "Hello"),
        ("0606e4bda0e5a5bd", "你好"),
    ],
    ids=[
        "empty_string",
        "single_char",
        "ascii_hello",
        "chinese_nihao",
    ],
)
def test_decode_string_values(hex_data: str, expected_val: str) -> None:
    """字节序列应该正确解码为字符串."""
    # Arrange
    data = bytes.fromhex(hex_data)

    # Act
    result = decode(SimpleString, data)

    # Assert
    assert result.val == expected_val


# ==========================================
# 解码测试 - 复合结构
# ==========================================


def test_decode_two_fields_struct() -> None:
    """多字段结构体应该正确解码."""
    # Arrange
    hex_data = "000a1605416c696365"
    data = bytes.fromhex(hex_data)

    # Act
    result = decode(TwoFields, data)

    # Assert
    assert result.id == 10
    assert result.name == "Alice"


def test_decode_nested_struct() -> None:
    """嵌套结构体应该正确解码."""
    # Arrange
    hex_data = "00011a00020b"
    data = bytes.fromhex(hex_data)

    # Act
    result = decode(NestedStruct, data)

    # Assert
    assert result.val == 1
    assert result.next is not None
    assert result.next.val == 2
    assert result.next.next is None


def test_decode_skips_unknown_tag() -> None:
    """未知 tag 应被跳过而不影响已知字段."""
    # Arrange
    # TwoFields(10, "Alice") + unknown tag 2 (Int1 = 7)
    hex_data = "000a1605416c6963652007"
    data = bytes.fromhex(hex_data)

    # Act
    result = decode(TwoFields, data)

    # Assert
    assert result.id == 10
    assert result.name == "Alice"


def test_decode_with_type_mismatch_raises_value_error() -> None:
    """类型不匹配应抛出 ValueError."""
    # Arrange
    # Tag 0 as String1 "A" while schema expects int
    data = bytes.fromhex("060141")

    # Act / Assert
    with pytest.raises(ValueError, match="Failed to read int"):
        decode(SimpleInt, data)
