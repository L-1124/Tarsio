"""JCE 基础类型功能测试.

覆盖 jce.types 模块的核心特性:
1. 类型验证 (validate)
2. 反序列化逻辑 (from_bytes)
3. 字符串解码回退机制 (UTF-8 vs Bytes)
4. 便捷序列化入口 (to_bytes)
"""

import pytest

from jce import JceDict, types

# --- 测试数据: 类型验证 ---

VALIDATE_CASES = [
    # (TypeClass, value, should_pass)
    (types.INT, 123, True),
    (types.INT, "123", False),  # INT 不接受字符串
    (types.BOOL, True, True),
    (types.BOOL, False, True),
    (types.BOOL, 1, True),  # 1 视为 True
    (types.BOOL, 0, True),  # 0 视为 False
    (types.BOOL, 2, False),  # 非 0/1 整数 -> False
    (types.BOOL, "True", False),
    (types.LIST, [1, 2], True),
    (types.LIST, (1, 2), False),  # 严格要求 list
    (types.MAP, {1: 2}, True),
    (types.MAP, [1, 2], False),
]

# --- 测试数据: 字符串解码 ---

STRING_DECODE_CASES = [
    # (TypeClass, input_bytes, expected_val, expected_len)
    # STRING1: 1字节长度 + 内容
    # 正常 UTF-8
    (types.STRING1, b"\x05hello", "hello", 6),
    # 包含非 UTF-8 序列 (0xFF) -> 回退为 bytes
    (types.STRING1, b"\x04\xff\xff\xff\xff", b"\xff\xff\xff\xff", 5),
    # STRING4: 4字节长度 + 内容
    # 正常 UTF-8 (len=5 -> 00 00 00 05)
    (types.STRING4, b"\x00\x00\x00\x05hello", "hello", 9),
    # 非 UTF-8
    (types.STRING4, b"\x00\x00\x00\x01\xff", b"\xff", 5),
]

# --- 测试函数 ---


@pytest.mark.parametrize(("cls", "value", "should_pass"), VALIDATE_CASES)
def test_types_validate(cls, value, should_pass):
    """JceType.validate() 应正确接受有效输入并拒绝无效输入."""
    if should_pass:
        assert cls.validate(value) == value
    else:
        with pytest.raises((TypeError, ValueError)):
            cls.validate(value)


@pytest.mark.parametrize(
    ("cls", "data", "expected_val", "expected_len"), STRING_DECODE_CASES
)
def test_string_decoding(cls, data, expected_val, expected_len):
    """字符串类型应支持自动回退到 bytes (当 UTF-8 解码失败时)."""
    val, consumed = cls.from_bytes(data)
    assert val == expected_val
    assert consumed == expected_len


def test_int_from_bytes_variants():
    """不同整数类型应能正确解析其对应的字节表示."""
    # INT8 (1 byte)
    assert types.INT8.from_bytes(b"\x01") == (1, 1)

    # INT16 (2 bytes, Big Endian) -> 0x0100 = 256
    assert types.INT16.from_bytes(b"\x01\x00") == (256, 2)

    # INT32 (4 bytes) -> 1
    assert types.INT32.from_bytes(b"\x00\x00\x00\x01") == (1, 4)

    # INT64 (8 bytes) -> 1
    assert types.INT64.from_bytes(b"\x00" * 7 + b"\x01") == (1, 8)


def test_float_double_from_bytes():
    """浮点数类型应能正确解析 IEEE 754 格式数据."""
    # FLOAT (4 bytes) - 1.5 -> 3F C0 00 00
    val, length = types.FLOAT.from_bytes(bytes.fromhex("3FC00000"))
    assert val == 1.5
    assert length == 4

    # DOUBLE (8 bytes) - 1.5 -> 3F F8 00 00 ...
    val, length = types.DOUBLE.from_bytes(bytes.fromhex("3FF8000000000000"))
    assert val == 1.5
    assert length == 8


def test_bool_from_bytes():
    """BOOL 类型应能从 1/0 字节正确解析."""
    # 0x01 -> True
    assert types.BOOL.from_bytes(b"\x01") == (True, 1)
    # 0x00 -> False
    assert types.BOOL.from_bytes(b"\x00") == (False, 1)
    # 0x02 -> True (非0即真)
    assert types.BOOL.from_bytes(b"\x02") == (True, 1)


def test_to_bytes_shortcut():
    """JceType.to_bytes() 应该生成带 Tag 的正确 JCE 编码."""
    # 这是一个集成测试，实际上它会调用 Encoder
    # 验证 INT8(1) -> Tag 0 -> 00 01
    data = types.INT8.to_bytes(tag=0, value=1)
    assert data.hex().upper() == "0001"

    # 验证 STRING -> Tag 1 -> 16 ...
    data = types.STRING.to_bytes(tag=1, value="a")
    # Tag 1 Type 6 (String1) -> 16
    # Len 1 -> 01
    # 'a' -> 61
    assert data.hex().upper() == "160161"


def test_bytes_type():
    """BYTES.from_bytes() 应该作为占位符仅消耗头部 (实际解码由 decoder 接管)."""
    # BYTES 类型 from_bytes 比较特殊，它通常只读取头部？
    # 查看 types.py 源码: return data[:1], 1
    # 这意味着它是一个占位符，实际逻辑由 decoder 处理
    assert types.BYTES.from_bytes(b"123") == (b"1", 1)


def test_int_validate_bytes_auto_convert():
    """INT.validate() 应尝试自动转换 bytes 输入."""
    # 某些场景下(如 Tag Key)，INT 可能接收到 bytes
    # INT.validate 应该尝试转换
    assert types.INT.validate(b"\x01") == 1
    assert types.INT.validate(b"\x00\xff") == 255


def test_guess_jce_type_basic():
    """guess_jce_type() 应能正确推断基础 Python 类型对应的 JCE 类型."""
    assert types.guess_jce_type(123) == types.INT
    assert types.guess_jce_type(True) == types.INT  # bool 视为 INT
    assert types.guess_jce_type(1.5) == types.DOUBLE
    assert types.guess_jce_type("hello") == types.STRING
    assert types.guess_jce_type(b"bytes") == types.BYTES
    assert types.guess_jce_type([1, 2, 3]) == types.LIST
    assert types.guess_jce_type({1: "a", 2: "b"}) == types.MAP


def test_guess_jce_type_jcedict():
    """guess_jce_type() 对于 JceDict 输入应返回 JceDict 类型本身."""
    # JceDict 应该返回 JceDict 类型本身（而不是 MAP）
    jce_data = JceDict({0: 100, 1: "test"})
    result = types.guess_jce_type(jce_data)
    assert result is JceDict

    # 对比: 普通 dict 返回 MAP
    plain_dict = {1: "a"}
    result = types.guess_jce_type(plain_dict)
    assert result == types.MAP
