"""测试 JCE 基础类型.

覆盖 jce.types 模块的核心特性:
1. 类型验证 (validate)
2. 反序列化逻辑 (from_bytes)
3. 字符串解码回退机制 (UTF-8 vs Bytes)
4. 便捷序列化入口 (to_bytes)
"""

import pytest

from jce import JceDict, types

VALIDATE_CASES = [
    (types.INT, 123, True),
    (types.INT, "123", False),
    (types.BOOL, True, True),
    (types.BOOL, False, True),
    (types.BOOL, 1, True),
    (types.BOOL, 0, True),
    (types.BOOL, 2, False),
    (types.BOOL, "True", False),
    (types.LIST, [1, 2], True),
    (types.LIST, (1, 2), False),
    (types.MAP, {1: 2}, True),
    (types.MAP, [1, 2], False),
]

STRING_DECODE_CASES = [
    (types.STRING1, b"\x05hello", "hello", 6),
    (types.STRING1, b"\x04\xff\xff\xff\xff", b"\xff\xff\xff\xff", 5),
    (types.STRING4, b"\x00\x00\x00\x05hello", "hello", 9),
    (types.STRING4, b"\x00\x00\x00\x01\xff", b"\xff", 5),
]


@pytest.mark.parametrize(
    ("cls", "value", "should_pass"),
    VALIDATE_CASES,
    ids=[f"{c[0].__name__}_{c[1]}" for c in VALIDATE_CASES],
)
def test_types_validate(cls: type, value: object, should_pass: bool) -> None:
    """JceType.validate() 应正确接受有效输入并拒绝无效输入."""
    if should_pass:
        assert cls.validate(value) == value
    else:
        with pytest.raises((TypeError, ValueError)):
            cls.validate(value)


@pytest.mark.parametrize(
    ("cls", "data", "expected_val", "expected_len"),
    STRING_DECODE_CASES,
    ids=["string1_utf8", "string1_bytes", "string4_utf8", "string4_bytes"],
)
def test_string_decoding(
    cls: type, data: bytes, expected_val: str | bytes, expected_len: int
) -> None:
    """字符串类型应支持自动回退到 bytes (当 UTF-8 解码失败时)."""
    val, consumed = cls.from_bytes(data)
    assert val == expected_val
    assert consumed == expected_len


def test_int_from_bytes_variants() -> None:
    """不同整数类型应能正确解析其对应的字节表示."""
    assert types.INT8.from_bytes(b"\x01") == (1, 1)
    assert types.INT16.from_bytes(b"\x01\x00") == (256, 2)
    assert types.INT32.from_bytes(b"\x00\x00\x00\x01") == (1, 4)
    assert types.INT64.from_bytes(b"\x00" * 7 + b"\x01") == (1, 8)


def test_float_double_from_bytes() -> None:
    """浮点数类型应能正确解析 IEEE 754 格式数据."""
    val, length = types.FLOAT.from_bytes(bytes.fromhex("3FC00000"))
    assert val == 1.5
    assert length == 4

    val, length = types.DOUBLE.from_bytes(bytes.fromhex("3FF8000000000000"))
    assert val == 1.5
    assert length == 8


def test_bool_from_bytes() -> None:
    """BOOL 类型应能从 1/0 字节正确解析."""
    assert types.BOOL.from_bytes(b"\x01") == (True, 1)
    assert types.BOOL.from_bytes(b"\x00") == (False, 1)
    assert types.BOOL.from_bytes(b"\x02") == (True, 1)


def test_to_bytes_shortcut() -> None:
    """JceType.to_bytes() 应生成带 Tag 的正确 JCE 编码."""
    data = types.INT8.to_bytes(tag=0, value=1)
    assert data.hex().upper() == "0001"

    data = types.STRING.to_bytes(tag=1, value="a")
    assert data.hex().upper() == "160161"


def test_bytes_type() -> None:
    """BYTES.from_bytes() 应作为占位符仅消耗头部 (实际解码由 decoder 接管)."""
    assert types.BYTES.from_bytes(b"123") == (b"1", 1)


def test_int_validate_bytes_auto_convert() -> None:
    """INT.validate() 应尝试自动转换 bytes 输入."""
    assert types.INT.validate(b"\x01") == 1
    assert types.INT.validate(b"\x00\xff") == 255


def test_guess_jce_type_basic() -> None:
    """guess_jce_type() 应能正确推断基础 Python 类型对应的 JCE 类型."""
    assert types.guess_jce_type(123) == types.INT
    assert types.guess_jce_type(True) == types.INT
    assert types.guess_jce_type(1.5) == types.DOUBLE
    assert types.guess_jce_type("hello") == types.STRING
    assert types.guess_jce_type(b"bytes") == types.BYTES
    assert types.guess_jce_type([1, 2, 3]) == types.LIST
    assert types.guess_jce_type({1: "a", 2: "b"}) == types.MAP


def test_guess_jce_type_jcedict() -> None:
    """guess_jce_type() 对于 JceDict 输入应返回 JceDict 类型本身."""
    jce_data = JceDict({0: 100, 1: "test"})
    result = types.guess_jce_type(jce_data)
    assert result is JceDict

    plain_dict = {1: "a"}
    result = types.guess_jce_type(plain_dict)
    assert result == types.MAP
