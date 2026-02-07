"""大规模序列编码行为测试."""

from tarsio import decode_raw, encode_raw


def test_encode_raw_large_range_roundtrip() -> None:
    """Range 作为序列输入时应可编码并正确解码."""
    size = 2000
    payload = {0: range(size)}
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
    payload = {0: seq}

    encoded = encode_raw(payload)
    decoded = decode_raw(encoded)

    values = decoded[0]
    assert isinstance(values, list)
    assert len(values) == size
    assert values[0] == 0
    assert values[-1] == size - 1
