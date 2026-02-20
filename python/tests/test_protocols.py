"""Tars 协议基线测试.

验证编解码规则、边界情况与 Raw 模式行为。
此文件定义的 hex 字节流必须与 Rust 核心层实现完全匹配 (Big Endian, Tagged Lengths).
"""

import pytest
from tarsio._core import TarsDict, decode, decode_raw, encode_raw, probe_struct


@pytest.mark.parametrize(
    ("val", "expected_hex"),
    [
        (0, "1C"),  # Tag 1, ZeroTag(12) -> 1C
        (1, "1001"),  # Tag 1, Int1(0) -> 10, Val 01
        (-128, "1080"),  # Tag 1, Int1, Val 80 (i8)
        (127, "107F"),  # Tag 1, Int1, Val 7F
        (255, "1100FF"),  # Tag 1, Int2(1) -> 11, Val 00FF (Big Endian)
        (32767, "117FFF"),  # Tag 1, Int2, Val 7FFF
        (-32768, "118000"),  # Tag 1, Int2, Val 8000
        (65535, "120000FFFF"),  # Tag 1, Int4(2) -> 12, Val 0000FFFF
        (2147483647, "127FFFFFFF"),  # Tag 1, Int4, Val 7FFFFFFF
    ],
    ids=[
        "zero",
        "one",
        "int1_min",
        "int1_max",
        "int2_255",
        "int2_max",
        "int2_min",
        "int4_65535",
        "int4_max",
    ],
)
def test_encode_int_values(val: int, expected_hex: str) -> None:
    """测试不同范围整数的编码 (Raw Struct {1: val})."""
    data = encode_raw(TarsDict({1: val}))
    assert data.hex().upper() == expected_hex.replace(" ", "")


@pytest.mark.parametrize(
    ("val", "expected_hex"),
    [
        ("", "1600"),  # Tag 1, String1(6) -> 16, Len 0
        ("a", "160161"),  # Tag 1, String1, Len 1, 'a'
        ("hello", "160568656C6C6F"),
        ("你好", "1606E4BDA0E5A5BD"),
    ],
    ids=["empty_string", "single_char", "ascii_hello", "chinese_nihao"],
)
def test_encode_string_values(val: str, expected_hex: str) -> None:
    """测试字符串编码 (Raw Struct {1: val})."""
    data = encode_raw(TarsDict({1: val}))
    assert data.hex().upper() == expected_hex


def test_encode_two_fields_struct() -> None:
    """测试多字段结构体编码顺序与 Tag."""
    # {0: 1, 1: "a"}
    # Tag 0, Int1(0): 00 01
    # Tag 1, String1: 16 01 61
    data = encode_raw(TarsDict({0: 1, 1: "a"}))
    assert data.hex().upper() == "0001160161"


def test_encode_nested_struct() -> None:
    """测试嵌套结构体编码."""
    # {0: {1: 100}}
    # Tag 0, StructBegin(10): 0A
    #   Tag 1, Int1(0): 10 64 (100)
    # StructEnd(11): 0B
    inner = TarsDict({1: 100})
    data = encode_raw(TarsDict({0: inner}))
    assert data.hex().upper() == "0A10640B"


def test_encode_deeply_nested_struct() -> None:
    """测试深层嵌套."""
    # {0: {0: {0: 1}}}
    # 0A -> 0A -> 00 01 -> 0B -> 0B
    # Inner: Tag 0, Int1(0), Val 1 -> 00 01
    val = TarsDict({0: TarsDict({0: TarsDict({0: 1})})})
    data = encode_raw(val)
    assert data.hex().upper() == "0A0A00010B0B"


# --- Decode Tests ---


def test_decode_int_roundtrip() -> None:
    """测试整数解码回原值."""
    for val in [0, 1, -1, 127, -128, 255, 65535, 2147483647]:
        data = encode_raw(TarsDict({0: val}))
        decoded = decode_raw(data)
        assert decoded[0] == val


def test_decode_invalid_utf8_raises_value_error() -> None:
    """无效 UTF-8 字节串解码应抛出 ValueError."""
    # Tag 1, String1, len 1, 0xFF
    data = bytes.fromhex("1601FF")
    with pytest.raises(ValueError, match="Invalid UTF-8"):
        decode_raw(data)


def test_decode_two_fields_struct() -> None:
    """测试多字段解码."""
    # 00 01 (Tag 0: 1), 16 01 61 (Tag 1: "a")
    data = bytes.fromhex("0001160161")
    res = decode_raw(data)
    assert res[0] == 1
    assert res[1] == "a"


def test_decode_nested_struct() -> None:
    """测试嵌套结构解码."""
    # 0A 10 64 0B
    data = bytes.fromhex("0A10640B")
    res = decode_raw(data)
    assert isinstance(res[0], TarsDict)
    assert res[0][1] == 100


def test_decode_skips_unknown_tag() -> None:
    """测试跳过未知 Tag (Schema 模式)."""
    from typing import Annotated

    from tarsio import Struct

    class User(Struct):
        uid: Annotated[int, 0]

    # Data: Tag 0=1, Tag 1="a" -> 00 01 16 01 61
    data = bytes.fromhex("0001160161")
    user = decode(User, data)
    assert user.uid == 1


def test_decode_with_type_mismatch_raises_value_error() -> None:
    """类型不匹配抛出异常."""
    from typing import Annotated

    from tarsio import Struct

    class User(Struct):
        uid: Annotated[int, 0]

    # Data has tag 0 as String "a" -> 06 01 61
    data = bytes.fromhex("060161")
    with pytest.raises(ValueError, match="Failed to read int"):
        decode(User, data)


def test_encode_raw_map_complex() -> None:
    """测试 Raw 模式 Map 编码."""
    # {0: {"a": 1}} -> Tag 0 Map(8) -> 08
    # Size 1 (Int1 Tag 0) -> 00 01
    # Key "a" (Tag 0 String): 06 01 61
    # Val 1 (Tag 1 Int1): 10 01
    val = TarsDict({0: {"a": 1}})
    data = encode_raw(val)
    assert data.hex().upper() == "0800010601611001"


def test_encode_raw_list_complex() -> None:
    """测试 Raw 模式 List 编码."""
    # {0: [1]} -> Tag 0 List(9) -> 09
    # Size 1 (Int1 Tag 0) -> 00 01
    # Item 1 (Tag 0 Int1): 00 01  <-- List items usually use implicit tags or Tag 0?
    # Tars List: Head (Tag 0, Type 9), Length (Tag 0 Int), Items...
    # Items in list don't have tags in strict Tars?
    # However, any_codec.rs calls `serialize_any(writer, 0, &item...)`.
    # So items are encoded with Tag 0.
    val = TarsDict({0: [1]})
    data = encode_raw(val)
    assert data.hex().upper() == "0900010001"


def test_decode_raw_max_depth_exceeded() -> None:
    """测试 Raw 解码深度限制."""
    data = bytes.fromhex("0A" * 101 + "0B" * 101)
    with pytest.raises(ValueError, match="Recursion limit"):
        decode_raw(data)


def test_float_double() -> None:
    """测试浮点数编码."""
    d = TarsDict({0: 1.5})
    data = encode_raw(d)
    # Tag 0, Double(5) -> 05
    # 1.5 Double Big Endian: 3FF8000000000000
    assert data.hex().upper() == "053FF8000000000000"


def test_bytes_simplelist() -> None:
    """测试 bytes (SimpleList)."""
    val = b"\x01\x02"
    d = TarsDict({0: val})
    data = encode_raw(d)
    # Tag 0, SimpleList(13) -> 0D
    # Subtype 0 (Byte) -> 00
    # Len 2 (Int1 Tag 0) -> 00 02
    # Data -> 01 02
    assert data.hex().upper() == "0D0000020102"


def test_raw_accepts_bytearray_and_memoryview_as_simplelist() -> None:
    """Raw 编码应将 bytearray/memoryview 统一按 SimpleList(bytes) 编码."""
    for value in (bytearray(b"\x01\x02"), memoryview(b"\x01\x02")):
        data = encode_raw(TarsDict({0: value}))
        assert data.hex().upper() == "0D0000020102"


def test_raw_accepts_non_contiguous_memoryview_as_simplelist() -> None:
    """Raw 编码应支持非连续 memoryview 并自动拷贝."""
    view = memoryview(bytearray(b"abcdef"))[::2]
    data = encode_raw(TarsDict({0: view}))
    decoded = decode_raw(data)
    assert decoded[0] == b"ace"
    assert isinstance(decoded[0], bytes)


def test_simplelist_always_returns_bytes() -> None:
    """验证 SimpleList 始终返回 bytes."""
    # Construct manually: 0D 00 00 02 11 01
    # Tag 0 SimpleList, Byte type, Len 2, Data 11 01
    data = bytes.fromhex("0D0000021101")
    decoded = decode_raw(data)
    assert isinstance(decoded[0], bytes)
    assert decoded[0] == b"\x11\x01"


def test_decode_raw_accepts_buffer_protocol_input() -> None:
    """decode_raw 应接受 bytearray 和 memoryview 输入."""
    payload = bytes.fromhex("0D0000021101")

    decoded_ba = decode_raw(bytearray(payload))
    decoded_mv = decode_raw(memoryview(payload))

    assert decoded_ba[0] == b"\x11\x01"
    assert decoded_mv[0] == b"\x11\x01"


def test_raw_recursion_limit() -> None:
    """验证递归深度限制."""
    d = TarsDict({})
    curr = d
    for _ in range(101):
        curr[0] = TarsDict({})
        curr = curr[0]
    with pytest.raises(ValueError, match="Recursion limit"):
        encode_raw(d)


def test_probe_struct_valid() -> None:
    """测试 probe_struct 有效性."""
    # {0: 1, 1: "s"} -> 00 01 16 01 73
    data = bytes.fromhex("0001160173")
    assert probe_struct(data) == {0: 1, 1: "s"}

    # Invalid
    assert probe_struct(bytes.fromhex("0F")) is None
    # Truncated
    assert probe_struct(bytes.fromhex("0A11")) is None


def test_decode_schema_accepts_buffer_protocol_input() -> None:
    """Schema decode 应接受 bytearray 和 memoryview 输入."""
    from tarsio import Struct

    class User(Struct):
        uid: int

    data = encode_raw(TarsDict({0: 7}))
    u1 = decode(User, bytearray(data))
    u2 = decode(User, memoryview(data))

    assert u1.uid == 7
    assert u2.uid == 7
