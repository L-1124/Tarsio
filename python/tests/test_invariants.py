"""验证 Rust Codec 核心不变量.

涵盖 checklist:
- [x] encode ∘ decode 可逆性 (Roundtrip)
- [x] WireType 覆盖率 (Int, Long, Float, Double, String, Struct, List, Map, SimpleList)
- [x] Unknown tag 行为 (Skipping)
- [x] 错误路径 (Error Handling)
"""

from typing import Annotated, Optional

import pytest
from tarsio import Struct, decode, encode

# ==========================================
# 综合测试结构体 (覆盖所有 WireType)
# ==========================================


class AllTypes(Struct):
    """包含所有基本 WireType 的结构体."""

    i8: Annotated[int, 0]
    i16: Annotated[int, 1]
    i32: Annotated[int, 2]
    i64: Annotated[int, 3]
    f32: Annotated[float, 4]
    f64: Annotated[float, 5]
    s1: Annotated[str, 6]  # Short string
    s4: Annotated[str, 7]  # Long string (logic driven by length)
    lst: Annotated[list[int], 8]
    mp: Annotated[dict[str, int], 9]
    sl: Annotated[bytes, 10]  # SimpleList
    nst: Annotated[Optional["AllTypes"], 11] = None  # Struct


# ==========================================
# Invariant: Encode ∘ Decode = Identity
# ==========================================


def test_invariant_roundtrip_all_types() -> None:
    """验证 encode 后 decode 能精确还原所有类型的字段."""
    # Arrange
    original = AllTypes(
        i8=100,
        i16=30000,
        i32=1000000,
        i64=2**60,
        f32=1.234,
        f64=3.1415926535,
        s1="Short",
        s4="Long " * 100,
        lst=[1, 2, 3],
        mp={"key": 1, "val": 2},
        sl=b"\x01\x02\x03",
        nst=None,
    )

    # Act
    encoded = encode(original)
    restored = decode(AllTypes, encoded)

    # Assert
    assert restored.i8 == original.i8
    assert restored.i16 == original.i16
    assert restored.i32 == original.i32
    assert restored.i64 == original.i64
    assert restored.f32 == pytest.approx(original.f32, rel=1e-5)
    assert restored.f64 == original.f64
    assert restored.s1 == original.s1
    assert restored.s4 == original.s4
    assert restored.lst == original.lst
    assert restored.mp == original.mp
    assert restored.sl == original.sl
    assert restored.nst is None


# ==========================================
# Invariant: Unknown Tags are Skipped
# ==========================================


def test_invariant_unknown_tags_skipped() -> None:
    """验证解码器能安全跳过未知的 Tag，不影响后续字段读取."""

    class V1(Struct):
        a: Annotated[int, 0]
        c: Annotated[int, 2]

    class V2(Struct):
        a: Annotated[int, 0]
        b: Annotated[dict[str, int], 1]  # V1 未知 Tag 1（复杂类型）
        c: Annotated[int, 2]

    # Arrange: 用 V2 编码
    v2_obj = V2(10, {"x": 1, "y": 2}, 20)
    data = encode(v2_obj)

    # Act: 用 V1 解码
    v1_obj = decode(V1, data)

    # Assert: 已知字段正确，未知字段被忽略
    assert v1_obj.a == 10
    assert v1_obj.c == 20
    assert not hasattr(v1_obj, "b")


# ==========================================
# 边界：空容器与 SimpleList
# ==========================================


def test_encode_decode_empty_list() -> None:
    """空列表应能正确往返还原."""

    class IntList(Struct):
        val: Annotated[list[int], 0]

    encoded = encode(IntList([]))
    decoded = decode(IntList, encoded)
    assert decoded.val == []


def test_encode_decode_empty_map() -> None:
    """空字典应能正确往返还原."""

    class StrIntMap(Struct):
        val: Annotated[dict[str, int], 0]

    encoded = encode(StrIntMap({}))
    decoded = decode(StrIntMap, encoded)
    assert decoded.val == {}


def test_simple_list_wire_format() -> None:
    """Bytes 应被编码为 SimpleList 且能正确解码为 bytes."""

    class SimpleListStruct(Struct):
        val: Annotated[bytes, 0]

    data = b"\x01\x02"
    encoded = encode(SimpleListStruct(data))
    assert encoded[0] == 0x0D

    decoded = decode(SimpleListStruct, encoded)
    assert decoded.val == data


# ==========================================
# Schema Evolution（演进）
# ==========================================


def test_evolution_forward_compatibility_optional_field_defaults_none() -> None:
    """旧数据应能被新 Schema 解码且新增字段为 None."""

    class UserV1(Struct):
        uid: Annotated[int, 0]
        name: Annotated[str, 1]

    class UserV2(Struct):
        uid: Annotated[int, 0]
        name: Annotated[str, 1]
        email: Annotated[str | None, 2] = None

    data = encode(UserV1(1, "Alice"))
    result = decode(UserV2, data)
    assert result.uid == 1
    assert result.name == "Alice"
    assert result.email is None


def test_evolution_backward_compatibility_skips_unknown_bytes_field() -> None:
    """旧 Schema 解码新数据时应跳过未知 bytes 字段."""

    class UserV1Old(Struct):
        uid: Annotated[int, 0]
        name: Annotated[str, 1]

    class UserV2New(Struct):
        uid: Annotated[int, 0]
        name: Annotated[str, 1]
        payload: Annotated[bytes, 2]

    data = encode(UserV2New(1, "Alice", b"\x01\x02"))
    result = decode(UserV1Old, data)
    assert result.uid == 1
    assert result.name == "Alice"


# ==========================================
# Invariant: Error Handling
# ==========================================


def test_invariant_missing_required_raises_value_error() -> None:
    """验证必需字段缺失时抛出 ValueError."""

    class Strict(Struct):
        req: Annotated[int, 0]

    # Decode to Strict (expects Tag 0)
    with pytest.raises(ValueError, match="Missing required field"):
        decode(Strict, b"")


def test_invariant_truncated_data_raises_value_error() -> None:
    """验证数据截断时抛出 ValueError (Rust codec error)."""

    class Simple(Struct):
        val: Annotated[int, 0]

    # Tag 0 (Head) + Int4 (Type 2) but no data
    truncated_data = b"\x02"

    with pytest.raises(ValueError, match=r"Read head error|Unexpected end of buffer"):
        decode(Simple, truncated_data)
