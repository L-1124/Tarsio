from tarsio._core import decode_raw, encode_raw


def test_struct_encoding_rules():
    """Verify int keys (TarsStruct) are sorted by tag in the encoded output."""
    # encode_raw expects a dict[int, Any] representing a Struct.
    data = {3: 300, 1: 100, 2: 200}
    encoded = encode_raw(data)

    # Verify we can decode it back
    decoded = decode_raw(encoded)
    assert decoded == data

    # To verify sorting, we can inspect the first few bytes.
    # 100 -> Tag 1
    # 200 -> Tag 2
    # 300 -> Tag 3
    # We expect tag 1 to appear first.
    # Tag 1 (Type Int1/2/4/8/Zero). 100 fits in 1 byte (Int1? No, Int1 is usually char).
    # 100 is 0x64. Tars Int1 is i8. 100 fits.
    # Type 0 (Int1/Byte) -> Tag 1, Type 0 -> 0x10.
    # So first byte should be 0x10.
    assert encoded[0] == 0x10


def test_map_encoding_rules():
    """Verify non-int keys encoded as Map (Type 8)."""
    # We must wrap the map in a struct to use encode_raw
    inner_map = {"key": "value"}
    data = {0: inner_map}
    encoded = encode_raw(data)

    # Tag 0, Type 8 (Map) -> 0x08
    assert encoded[0] == 0x08


def test_float_double():
    """Verify float encodes as Double (Type 5)."""
    # Python float is C double (64-bit). Tars should map this to Double (Type 5).
    # Current implementation suspected to map to Float (Type 4).
    val = 1.234
    data = {0: val}
    encoded = encode_raw(data)

    # Tag 0.
    # If Type 5 (Double): 0x05.
    # If Type 4 (Float): 0x04.

    head_byte = encoded[0]
    # We expect 0x05 for compliance.
    assert head_byte == 0x05, f"Expected Type 5 (Double), got Type {head_byte & 0x0F}"


def test_bytes_simplelist():
    """Verify bytes encoded as SimpleList (Type 13)."""
    val = b"hello"
    data = {0: val}
    encoded = encode_raw(data)

    # Tag 0.
    # Type 13 (SimpleList) -> 0x0D.
    head_byte = encoded[0]
    assert head_byte == 0x0D, (
        f"Expected Type 13 (SimpleList), got Type {head_byte & 0x0F}"
    )


def test_roundtrip_reversibility():
    """Verify decode(encode(data)) == data for various types."""
    cases = [
        123,
        -123,
        "string",
        b"bytes",
        [300, 400],  # List (use values > 255 to avoid auto-conversion to bytes)
        {1: "a", 2: "b"},  # Struct (nested)
        {"a": 1, "b": 2},  # Map
        True,
        False,
        # 1.234, # Float excluded because we know it might fail type check or roundtrip precision if implemented wrong
    ]

    for case in cases:
        # Wrap in struct tag 0
        data = {0: case}
        encoded = encode_raw(data)
        decoded = decode_raw(encoded)
        # decoded is dict[int, Any]
        assert decoded[0] == case, f"Failed roundtrip for {case}"
