"""encode_raw/decode_raw 协议级测试."""

import pytest
from tarsio import decode_raw, encode_raw


def test_encode_raw_with_map_writes_map_type_id() -> None:
    """Map 类型 (非 struct dict) 编码后首字节为 0x08."""
    inner = {"a": 1}
    outer = {0: inner}

    encoded = encode_raw(outer)

    assert encoded[0] == 0x08


def test_encode_raw_with_list_writes_list_type_id() -> None:
    """List 类型编码后首字节为 0x09."""
    lst = [1, 2]
    outer = {0: lst}

    encoded = encode_raw(outer)

    assert encoded[0] == 0x09


def test_encode_raw_with_nested_struct_writes_struct_begin_end() -> None:
    """嵌套 Struct (int 键 dict) 编码后首字节为 0x0A，末字节为 0x0B."""
    inner = {1: 1}
    outer = {0: inner}

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
