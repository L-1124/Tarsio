"""测试 Tars 协议编解码的核心功能.

此文件是库的**根本性测试**，必须保证 100% 通过。
它定义了 Tars 协议实现的基准，任何破坏此文件测试用例的修改均被视为破坏性变更。
"""

from typing import Annotated, Optional

import pytest
from tarsio import (
    Struct,
    TarsDict,
    decode,
    decode_raw,
    encode,
    encode_raw,
    probe_struct,
)
from tarsio._core import decode_raw as core_decode_raw
from tarsio._core import encode_raw as core_encode_raw

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


def test_decode_invalid_utf8_string_raises_value_error() -> None:
    """无效 UTF-8 字符串应抛出 ValueError."""
    # Arrange
    data = bytes.fromhex("0601ff")

    # Act / Assert
    with pytest.raises(ValueError, match="Invalid UTF-8 string"):
        decode(SimpleString, data)


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


# ==========================================
# encode_raw/decode_raw 协议级测试
# ==========================================


def test_encode_raw_with_map_writes_map_type_id() -> None:
    """Map 类型 (非 struct dict) 编码后首字节为 0x08."""
    inner = {"a": 1}
    outer = TarsDict({0: inner})

    encoded = encode_raw(outer)

    assert encoded[0] == 0x08


def test_encode_raw_with_list_writes_list_type_id() -> None:
    """List 类型编码后首字节为 0x09."""
    lst = [1, 2]
    outer = TarsDict({0: lst})

    encoded = encode_raw(outer)

    assert encoded[0] == 0x09


def test_encode_raw_with_nested_struct_writes_struct_begin_end() -> None:
    """嵌套 Struct (int 键 dict) 编码后首字节为 0x0A，末字节为 0x0B."""
    inner = TarsDict({1: 1})
    outer = TarsDict({0: inner})

    encoded = encode_raw(outer)

    assert encoded[0] == 0x0A
    assert encoded[-1] == 0x0B


def test_decode_raw_max_depth_exceeded() -> None:
    """递归深度超过 MAX_DEPTH (100) 时抛出 ValueError."""
    data = bytearray()
    for _ in range(102):
        data.append(0x09)  # tag=0, type=List
        data.append(0x00)  # tag=0, type=ZeroTag
        data.append(0x01)  # size=1

    data.append(0x00)  # 最内层值

    with pytest.raises(ValueError, match="Recursion limit exceeded"):
        decode_raw(bytes(data))


def test_struct_encoding_rules() -> None:
    """验证结构体字段按 tag 排序编码."""
    data = TarsDict({3: 300, 1: 100, 2: 200})
    encoded = core_encode_raw(data)
    assert encoded[0] == 0x10


def test_map_encoding_rules() -> None:
    """验证非整数键按 Map 类型编码."""
    inner_map = {"key": "value"}
    data = TarsDict({0: inner_map})
    encoded = core_encode_raw(data)

    assert encoded[0] == 0x08


def test_float_double() -> None:
    """验证浮点数编码为 Double 类型."""
    val = 1.234
    data = TarsDict({0: val})
    encoded = core_encode_raw(data)

    head_byte = encoded[0]
    assert head_byte == 0x05, f"Expected Type 5 (Double), got Type {head_byte & 0x0F}"


def test_bytes_simplelist() -> None:
    """验证 bytes 编码为 SimpleList 类型."""
    val = b"hello"
    data = TarsDict({0: val})
    encoded = core_encode_raw(data)

    head_byte = encoded[0]
    assert head_byte == 0x0D, (
        f"Expected Type 13 (SimpleList), got Type {head_byte & 0x0F}"
    )


def test_roundtrip_reversibility() -> None:
    """验证多类型 encode/decode 往返一致."""
    cases = [
        123,
        -123,
        "string",
        b"\xff",
        [300, 400],
        TarsDict({1: "a", 2: "b"}),
        {"a": 1, "b": 2},
        True,
        False,
    ]

    for case in cases:
        data = TarsDict({0: case})
        encoded = core_encode_raw(data)
        decoded = core_decode_raw(encoded)
        assert decoded[0] == case, f"Failed roundtrip for {case}"


def test_simplelist_utf8_bytes_decodes_to_str() -> None:
    """验证 SimpleList 的 UTF-8 bytes 解码为字符串."""
    data = TarsDict({0: b"hello"})
    encoded = core_encode_raw(data)
    decoded = core_decode_raw(encoded)
    assert decoded[0] == "hello"


def test_simplelist_invalid_utf8_decodes_to_bytes() -> None:
    """验证 SimpleList 的无效 UTF-8 回退为 bytes."""
    data = TarsDict({0: b"\xff"})
    encoded = core_encode_raw(data)
    decoded = core_decode_raw(encoded)
    assert decoded[0] == b"\xff"


def test_raw_string_invalid_utf8_raises_value_error() -> None:
    """验证原始字符串无效 UTF-8 时抛出 ValueError."""
    data = bytes.fromhex("0601ff")
    with pytest.raises(ValueError, match="Invalid UTF-8 string"):
        core_decode_raw(data)


def test_raw_simplelist_invalid_subtype_raises_value_error() -> None:
    """验证 SimpleList 子类型非法时抛出 ValueError."""
    data = bytes([0x0D, 0x01, 0x00, 0x01, 0x00])
    with pytest.raises(ValueError, match="SimpleList must contain Byte"):
        core_decode_raw(data)


def test_raw_simplelist_negative_size_raises_value_error() -> None:
    """验证 SimpleList 负长度时抛出 ValueError."""
    data = bytes([0x0D, 0x00, 0x00, 0xFF])
    with pytest.raises(ValueError, match="Invalid SimpleList size"):
        core_decode_raw(data)


def test_raw_list_negative_size_raises_value_error() -> None:
    """验证 List 负长度时抛出 ValueError."""
    data = bytes([0x09, 0x00, 0xFF])
    with pytest.raises(ValueError, match="Invalid list size"):
        core_decode_raw(data)


def test_raw_map_negative_size_raises_value_error() -> None:
    """验证 Map 负长度时抛出 ValueError."""
    data = bytes([0x08, 0x00, 0xFF])
    with pytest.raises(ValueError, match="Invalid map size"):
        core_decode_raw(data)


def _nest_list(depth: int) -> object:
    """构造指定深度的嵌套 list,用于触发递归保护.

    Args:
        depth: 嵌套深度。

    Returns:
        object: 形如 [[...[1]...]] 的嵌套结构。
    """
    val: object = 1
    for _ in range(depth):
        val = [val]
    return val


def test_raw_encode_max_depth_exceeded() -> None:
    """Raw 编码递归超限时抛出 ValueError."""
    data = TarsDict({0: _nest_list(101)})
    with pytest.raises(ValueError, match="Recursion limit exceeded"):
        core_encode_raw(data)


def test_decode_simplelist_invalid_subtype_raises_value_error() -> None:
    """验证 SimpleList 子类型非法时抛出 ValueError."""

    class Blob(Struct):
        val: Annotated[bytes, 0]

    data = bytes([0x0D, 0x01, 0x00, 0x01, 0x00])
    with pytest.raises(ValueError, match="SimpleList must contain Byte"):
        decode(Blob, data)


def test_decode_simplelist_negative_size_raises_value_error() -> None:
    """验证 SimpleList 负长度时抛出 ValueError."""

    class Blob(Struct):
        val: Annotated[bytes, 0]

    data = bytes([0x0D, 0x00, 0x00, 0xFF])
    with pytest.raises(ValueError, match="Invalid SimpleList size"):
        decode(Blob, data)


def test_decode_list_negative_size_raises_value_error() -> None:
    """验证 List 负长度时抛出 ValueError."""

    class IntList(Struct):
        val: Annotated[list[int], 0]

    data = bytes([0x09, 0x00, 0xFF])
    with pytest.raises(ValueError, match="Invalid list size"):
        decode(IntList, data)


def test_decode_map_negative_size_raises_value_error() -> None:
    """验证 Map 负长度时抛出 ValueError."""

    class StrIntMap(Struct):
        val: Annotated[dict[str, int], 0]

    data = bytes([0x08, 0x00, 0xFF])
    with pytest.raises(ValueError, match="Invalid map size"):
        decode(StrIntMap, data)


# ==========================================
# 大规模序列编码行为测试
# ==========================================


def test_encode_raw_large_range_roundtrip() -> None:
    """Range 作为序列输入时应可编码并正确解码."""
    size = 2000
    payload = TarsDict({0: range(size)})
    encoded = encode_raw(payload)
    decoded = decode_raw(encoded)

    values = decoded[0]
    assert isinstance(values, list)
    assert len(values) == size
    assert values[0] == 0
    assert values[-1] == size - 1


_LargeList = type("_LargeList", (list,), {})


def test_encode_raw_large_custom_sequence() -> None:
    """自定义序列应可编码并正确解码."""
    size = 3000
    seq = _LargeList(range(size))
    payload = TarsDict({0: seq})

    encoded = encode_raw(payload)
    decoded = decode_raw(encoded)

    values = decoded[0]
    assert isinstance(values, list)
    assert len(values) == size
    assert values[0] == 0
    assert values[-1] == size - 1


def test_decode_raw_returns_tars_dict() -> None:
    """验证 decode_raw 返回 TarsDict 实例."""
    data = encode_raw(TarsDict({0: 123, 1: "test"}))
    decoded = decode_raw(data)

    assert isinstance(decoded, TarsDict)
    assert isinstance(decoded, dict)
    assert decoded[0] == 123
    assert decoded[1] == "test"


def test_probe_struct_returns_tars_dict() -> None:
    """验证 probe_struct 返回 TarsDict 实例."""
    data = encode_raw(TarsDict({0: 1}))
    probed = probe_struct(data)

    assert probed is not None
    assert isinstance(probed, TarsDict)
    assert probed[0] == 1


def test_raw_recursion_limit() -> None:
    """验证原始接口的递归深度限制."""

    def create_deep_dict(depth: int) -> TarsDict:
        if depth <= 0:
            return TarsDict({0: "end"})
        return TarsDict({0: create_deep_dict(depth - 1)})

    # 101 层嵌套，超过默认 100 限制
    deep_dict = create_deep_dict(101)

    with pytest.raises(ValueError, match="limit exceeded"):
        encode_raw(deep_dict)


def test_probe_struct_valid() -> None:
    """验证 probe_struct 探测有效结构."""
    data = encode_raw(TarsDict({0: 1, 1: "s"}))
    assert probe_struct(data) == {0: 1, 1: "s"}
