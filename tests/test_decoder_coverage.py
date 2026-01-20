"""Coverage tests for decoder.py."""

import struct

import pytest

from jce import (
    JceDecodeError,
    JceField,
    JceStruct,
    dumps,
    jce_field_deserializer,
    loads,
    types,
)
from jce.decoder import MAX_STRING_LENGTH, DataReader, GenericDecoder


def test_float_heuristic_infinite_primary():
    """read_float() 在大端序为 Inf 但小端序正常时应自动选择小端序."""
    # Big Endian: 7F 80 00 00 (Inf)
    inf_be = struct.pack(">f", float("inf"))

    # Check what this is in LE
    val_le = struct.unpack("<f", inf_be)[0]

    reader = DataReader(inf_be)
    # Should detect LE is better (finite) and return it
    assert reader.read_float() == val_le


def test_float_heuristic_magnitude():
    """read_float() 在大端序值过大但小端序合理时应自动选择小端序."""
    # BE: 0x50 00 00 00 (approx 8.5e9)
    # LE: 00 00 00 50 (approx 1.1e-43) -> <= 1e6.
    data = b"\x50\x00\x00\x00"
    val_le = struct.unpack("<f", data)[0]

    reader = DataReader(data)
    # Should pick LE because BE is too large (> 1e9) and LE is small
    assert reader.read_float() == val_le


def test_double_heuristic_magnitude():
    """read_double() 在大端序值过大但小端序合理时应自动选择小端序."""
    # BE > 1e18, LE <= 1e12
    # 0x60 00 ... 00
    data = b"\x60" + b"\x00" * 7
    val_le = struct.unpack("<d", data)[0]

    reader = DataReader(data)
    assert reader.read_double() == val_le


def test_string4_negative_length():
    """解码 String4 时若长度为负数应抛出 JceDecodeError."""
    # Tag 0, Type STRING4 (7)
    head = (0 << 4) | 7  # JCE_STRING4 is 7
    length = struct.pack(">i", -1)
    data = bytes([head]) + length

    with pytest.raises(JceDecodeError, match="negative"):
        loads(data, target=dict)


def test_string4_max_length_exceeded():
    """解码 String4 时若长度超过限制应抛出 JceDecodeError."""
    head = (0 << 4) | 7
    length = struct.pack(">i", MAX_STRING_LENGTH + 1)
    data = bytes([head]) + length

    with pytest.raises(JceDecodeError, match="exceeds max limit"):
        loads(data, target=dict)


def test_freeze_key_nested():
    """_freeze_key() 应该能冻结嵌套的 list 和 dict 以作为字典键."""
    reader = DataReader(b"")
    decoder = GenericDecoder(reader)

    # Dict with list and dict inside
    mutable_key = {1: [2, 3], 4: {5: 6}}
    frozen = decoder._freeze_key(mutable_key)

    assert isinstance(frozen, tuple)
    # Should be sortable and hashable
    d = {frozen: "value"}
    assert d[frozen] == "value"

    # Verify structure: items sorted by str(key)
    # 1 vs 4.
    # ( (1, (2, 3)), (4, ((5, 6),)) )
    assert frozen[0][0] == 1
    assert frozen[0][1] == (2, 3)
    assert frozen[1][0] == 4
    # Dict inside is also converted to tuple of items
    assert frozen[1][1] == ((5, 6),)


def test_struct_fallback_in_list():
    """SchemaDecoder 在列表元素类型不匹配时应尝试回退到字典模式."""

    class Item(JceStruct):
        a: int = JceField(jce_id=0, jce_type=types.INT)

    class Container(JceStruct):
        items: list[Item] = JceField(jce_id=1, jce_type=types.LIST)

    # Construct JCE data:
    # Tag 1 | LIST(9) = 0x19
    list_tag_head = bytes([(1 << 4) | 9])  # Tag 1, LIST

    # List Length: 1. Encoded as Tag 0, INT1(0), Val 1 -> 0x00 0x01
    list_len = b"\x00\x01"

    # List Item 1:
    # Tag 0 (arbitrary for list item), Type MAP(8) -> 0x08
    item_head = b"\x08"

    # Map Value:
    # Map Length: 1. Tag 0, INT1(0), Val 1 -> 0x00 0x01
    map_len = b"\x00\x01"

    # Pair 1:
    # Key: Tag 0, INT1(0) -> Key is 0.
    # Tag 0, INT1 -> 0x00. Value 0 -> 0x00.
    key_part = b"\x00\x00"

    # Value: Tag 1, INT1(100).
    # Tag 1, INT1 -> 0x10. Value 100 -> 0x64.
    val_part = b"\x10\x64"

    # Full data
    payload = list_tag_head + list_len + item_head + map_len + key_part + val_part

    # Decode
    container = loads(payload, Container)
    assert len(container.items) == 1
    assert container.items[0].a == 100


def test_deserializer_missing_cls():
    """字段反序列化器如果没有声明为 @classmethod 应抛出 TypeError."""

    class BadStruct(JceStruct):
        f: int = JceField(jce_id=0, jce_type=types.INT)

        @jce_field_deserializer("f")  # type: ignore
        def bad_deserializer(self, value, _):  # noqa: PLR6301
            # Instance method (missing @classmethod)
            return value

    # Data: Tag 0, INT(123)
    data = dumps({0: 123})

    with pytest.raises(TypeError, match="must be a @classmethod"):
        loads(data, BadStruct)
