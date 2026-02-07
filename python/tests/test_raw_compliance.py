import pytest
from tarsio._core import decode_raw, encode_raw


def test_struct_encoding_rules() -> None:
    """验证结构体字段按 tag 排序编码."""
    data = {3: 300, 1: 100, 2: 200}
    encoded = encode_raw(data)
    assert encoded[0] == 0x10


def test_map_encoding_rules() -> None:
    """验证非整数键按 Map 类型编码."""
    inner_map = {"key": "value"}
    data = {0: inner_map}
    encoded = encode_raw(data)

    assert encoded[0] == 0x08


def test_float_double() -> None:
    """验证浮点数编码为 Double 类型."""
    val = 1.234
    data = {0: val}
    encoded = encode_raw(data)

    head_byte = encoded[0]
    assert head_byte == 0x05, f"Expected Type 5 (Double), got Type {head_byte & 0x0F}"


def test_bytes_simplelist() -> None:
    """验证 bytes 编码为 SimpleList 类型."""
    val = b"hello"
    data = {0: val}
    encoded = encode_raw(data)

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
        {1: "a", 2: "b"},
        {"a": 1, "b": 2},
        True,
        False,
    ]

    for case in cases:
        data = {0: case}
        encoded = encode_raw(data)
        decoded = decode_raw(encoded)
        assert decoded[0] == case, f"Failed roundtrip for {case}"


def test_simplelist_utf8_bytes_decodes_to_str() -> None:
    """验证 SimpleList 的 UTF-8 bytes 解码为字符串."""
    data = {0: b"hello"}
    encoded = encode_raw(data)
    decoded = decode_raw(encoded)
    assert decoded[0] == "hello"


def test_simplelist_invalid_utf8_decodes_to_bytes() -> None:
    """验证 SimpleList 的无效 UTF-8 回退为 bytes."""
    data = {0: b"\xff"}
    encoded = encode_raw(data)
    decoded = decode_raw(encoded)
    assert decoded[0] == b"\xff"


def test_raw_string_invalid_utf8_raises_value_error() -> None:
    """验证原始字符串无效 UTF-8 时抛出 ValueError."""
    data = bytes.fromhex("0601ff")
    with pytest.raises(ValueError, match="Invalid UTF-8 string"):
        decode_raw(data)


def test_raw_simplelist_invalid_subtype_raises_value_error() -> None:
    """验证 SimpleList 子类型非法时抛出 ValueError."""
    data = bytes([0x0D, 0x01, 0x00, 0x01, 0x00])
    with pytest.raises(ValueError, match="SimpleList must contain Byte"):
        decode_raw(data)


def test_raw_simplelist_negative_size_raises_value_error() -> None:
    """验证 SimpleList 负长度时抛出 ValueError."""
    data = bytes([0x0D, 0x00, 0x00, 0xFF])
    with pytest.raises(ValueError, match="Invalid SimpleList size"):
        decode_raw(data)


def test_raw_list_negative_size_raises_value_error() -> None:
    """验证 List 负长度时抛出 ValueError."""
    data = bytes([0x09, 0x00, 0xFF])
    with pytest.raises(ValueError, match="Invalid list size"):
        decode_raw(data)


def test_raw_map_negative_size_raises_value_error() -> None:
    """验证 Map 负长度时抛出 ValueError."""
    data = bytes([0x08, 0x00, 0xFF])
    with pytest.raises(ValueError, match="Invalid map size"):
        decode_raw(data)


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
    data = {0: _nest_list(101)}
    with pytest.raises(ValueError, match="Recursion limit exceeded"):
        encode_raw(data)
