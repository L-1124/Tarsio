from typing import Annotated

import pytest
from tarsio import Struct, decode


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
