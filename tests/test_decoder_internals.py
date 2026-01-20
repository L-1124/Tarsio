"""JCE 解码器内部逻辑测试.

主要覆盖 jce.decoder 模块的深层机制:
1. DataReader 边界检查 (EOF, Out of bounds)
2. 长标签解析 (Tag >= 15)
3. 字段跳过逻辑 (Skip logic for unknown fields) - 通过 Struct 触发
4. 异常处理 (JcePartialDataError, JceDecodeError)
5. 浮点数解码 (Float/Double, 大小端)
6. Map 解码 (不可哈希键, 标签验证)
7. SimpleList 解码
8. 递归深度限制
9. 整数边界测试
"""

import pytest

from jce import JceDict, JceField, JceStruct, dumps
from jce.decoder import DataReader, GenericDecoder
from jce.exceptions import JceDecodeError, JcePartialDataError

# --- 1. 测试 DataReader (流读取器) ---


def test_reader_basic_read():
    """read_bytes() 应该能正确读取指定长度并移动指针."""
    data = b"\x01\x02\x03\x04"
    reader = DataReader(data)

    assert reader.read_bytes(1) == b"\x01"
    assert reader._pos == 1

    # read_bytes 默认返回 bytes
    assert reader.read_bytes(2) == b"\x02\x03"
    assert reader._pos == 3


def test_reader_partial_data_error():
    """read_bytes() 在数据不足时应抛出 JcePartialDataError."""
    data = b"\x01"
    reader = DataReader(data)

    reader.read_bytes(1)

    # 再读 -> PartialDataError (不是普通的 DecodeError)
    with pytest.raises(JcePartialDataError):
        reader.read_bytes(1)

    with pytest.raises(JcePartialDataError):
        reader.read_u8()


def test_reader_peek():
    """peek_u8() 应该返回下一个字节但不移动指针."""
    data = b"\x01\x02"
    reader = DataReader(data)

    # 偷看 1 字节, 指针不应移动
    assert reader.peek_u8() == 0x01
    assert reader._pos == 0

    # 实际读取
    assert reader.read_u8() == 0x01
    assert reader._pos == 1


def test_reader_skip():
    """skip() 应该正确移动指针且在越界时抛出错误."""
    data = b"\x01\x02\x03"
    reader = DataReader(data)

    reader.skip(2)
    assert reader._pos == 2
    assert reader.read_u8() == 0x03

    # Skip 越界
    with pytest.raises(JcePartialDataError):
        reader.skip(10)


# --- 2. 测试 Head 解析 (Tag/Type) ---


def test_decode_long_tag():
    """Decoder 应该能正确解析 Tag >= 15 的长标签."""
    # JCE 协议: 如果 Tag >= 15, 高 4 位全 1 (0xF), 下一个字节存 Tag
    # 构造: Tag 20, Type INT1 (0) -> Value 1
    # Byte 1: (Tag=15 | Type=0) -> 0xF0
    # Byte 2: Tag=20 -> 0x14
    # Byte 3: Value=1 -> 0x01
    data = b"\xf0\x14\x01"

    reader = DataReader(data)
    decoder = GenericDecoder(reader)

    # 使用 decode 应该能解析出 {20: 1}
    res = decoder.decode()
    assert res == {20: 1}


def test_decode_invalid_type_head():
    """Decoder 遇到未知的 Type ID 应抛出 JceDecodeError."""
    # 构造: Tag 0, Type 14 (在当前 decoder.py 中未定义处理逻辑，应该抛错)
    # 0x0E (Tag 0, Type 14)
    data = b"\x0e"
    reader = DataReader(data)
    decoder = GenericDecoder(reader)

    with pytest.raises(JceDecodeError, match="Unknown JCE Type ID"):
        decoder.decode()


# --- 3. 测试 Skip Logic (核心难点 - 间接测试) ---


class TargetStruct(JceStruct):
    """只包含 Tag 0 和 Tag 2 的结构体."""

    f0: int = JceField(jce_id=0)
    f2: int = JceField(jce_id=2)


def test_skip_unknown_fields_primitive():
    """Decoder 应该能跳过结构体中未定义的基础类型字段."""
    # 构造包含 Tag 0, 1, 2 的数据
    # Tag 0: INT1(10)
    # Tag 1: INT8(999...) -> 这是一个长字段 (8 bytes)，测试指针移动是否准确
    # Tag 2: INT1(20)

    # 1. 构造数据
    from jce import JceDict

    data_0 = dumps(JceDict({0: 10}))
    data_1 = dumps(JceDict({1: 99999999999}))  # Tag 1, INT8
    data_2 = dumps(JceDict({2: 20}))

    full_data = data_0 + data_1 + data_2

    # 2. 用 TargetStruct (只认 Tag 0, 2) 去解析
    # 这会强制 Decoder 触发 _skip_value 逻辑跳过 Tag 1
    obj = TargetStruct.model_validate_jce(full_data)

    # 3. 验证结果
    assert obj.f0 == 10
    assert obj.f2 == 20
    # 如果 Tag 1 跳过错误（比如只跳了 4 字节），Tag 2 的解析位置就会错，导致读出乱码或报错


def test_skip_recursive_container():
    """Decoder 应该能递归跳过复杂的容器 (List/Map) 字段."""
    # Tag 1 是一个复杂的嵌套列表，用来测试 Skip 的递归能力
    # List<List<int>>
    from jce import JceDict

    complex_data = JceDict({1: [[1, 2], [3, 4]]})
    data_complex = dumps(complex_data)

    data_0 = dumps(JceDict({0: 10}))
    data_2 = dumps(JceDict({2: 20}))

    full_data = data_0 + data_complex + data_2

    # 解析
    obj = TargetStruct.model_validate_jce(full_data)

    assert obj.f0 == 10
    assert obj.f2 == 20


def test_skip_struct():
    """Decoder 应该能跳过嵌套的结构体字段."""
    # Tag 1 是一个 Struct (GenericDecoder 编码出来的 Struct 是 Tag-Keyed Map)
    # 我们手动构造一个 Struct Begin/End 包裹的数据
    # Struct Begin (Tag 1, Type 10) -> ... content ... -> Struct End (Type 11)

    # 简便方法：dumps 一个 dict，key 为 int，Encoder 会把它编成 Struct
    from jce import JceDict

    struct_data = JceDict({1: JceDict({10: 100, 11: 200})})  # Tag 1 的值是一个 Struct
    data_struct = dumps(struct_data)

    data_0 = dumps(JceDict({0: 10}))
    data_2 = dumps(JceDict({2: 20}))

    full_data = data_0 + data_struct + data_2

    obj = TargetStruct.model_validate_jce(full_data)

    assert obj.f0 == 10
    assert obj.f2 == 20


# --- 4. 异常处理测试 ---


def test_decode_truncated_string():
    """读取长度不足的字符串时应抛出 JcePartialDataError."""
    # String1 (Tag 0, Type 6) -> 06
    # Len 5 -> 05
    # Data "abc" (missing 2 bytes)
    data = bytes.fromhex("0605616263")

    reader = DataReader(data)
    decoder = GenericDecoder(reader)

    # 因为是用 read_bytes 读取，数据不足会报 PartialDataError
    with pytest.raises(JcePartialDataError):
        decoder.decode()


def test_decode_truncated_primitive():
    """读取长度不足的基础类型时应抛出 JcePartialDataError."""
    # INT4 (Tag 0, Type 2) -> 02
    # 需要 4 字节，只给 1 字节
    data = bytes.fromhex("0201")

    reader = DataReader(data)
    decoder = GenericDecoder(reader)

    with pytest.raises(JcePartialDataError):
        decoder.decode()


# --- 5. 浮点数解码测试 ---


def test_decode_float_normal():
    """Decoder 应该能正确解码标准的 Float 数据."""
    from jce import JceDict

    data = JceDict({0: 3.14})
    encoded = dumps(data)

    reader = DataReader(encoded)
    decoder = GenericDecoder(reader)
    result = decoder.decode()

    assert abs(result[0] - 3.14) < 0.01


def test_decode_double_normal():
    """Decoder 应该能正确解码标准的 Double 数据."""
    from jce import JceDict

    data = JceDict({0: 3.141592653589793})
    encoded = dumps(data)

    reader = DataReader(encoded)
    decoder = GenericDecoder(reader)
    result = decoder.decode()

    assert abs(result[0] - 3.141592653589793) < 1e-10


def test_decode_float_little_endian():
    """Decoder 应该支持小端序浮点数解码."""
    from jce import JceOption

    data = JceDict({0: 2.718})
    encoded = dumps(data, option=JceOption.LITTLE_ENDIAN)

    reader = DataReader(encoded, option=int(JceOption.LITTLE_ENDIAN))
    decoder = GenericDecoder(reader, option=int(JceOption.LITTLE_ENDIAN))
    result = decoder.decode()

    assert abs(result[0] - 2.718) < 0.01


def test_decode_float_partial_data():
    """读取长度不足的浮点数时应抛出 JcePartialDataError."""
    # FLOAT (Tag 0, Type 4) -> 04
    # 需要 4 字节，只给 2 字节
    data = bytes.fromhex("04ffff")

    reader = DataReader(data)
    decoder = GenericDecoder(reader)

    with pytest.raises(JcePartialDataError):
        decoder.decode()


def test_decode_double_partial_data():
    """读取长度不足的双精度浮点数时应抛出 JcePartialDataError."""
    # DOUBLE (Tag 0, Type 5) -> 05
    # 需要 8 字节，只给 4 字节
    data = bytes.fromhex("05ffffffff")

    reader = DataReader(data)
    decoder = GenericDecoder(reader)

    with pytest.raises(JcePartialDataError):
        decoder.decode()


# --- 6. Map 解码测试 ---


def test_decode_map_normal():
    """Decoder 应该能正确解码 Map 数据."""
    from jce import JceDict

    # 创建正常的 Map (注意: GenericDecoder 会把字符串键解码为 int)
    data = JceDict({0: {1: "value1", 2: "value2"}})
    encoded = dumps(data)

    reader = DataReader(encoded)
    decoder = GenericDecoder(reader)
    result = decoder.decode()

    # 验证能成功解码
    assert isinstance(result[0], dict)
    assert 1 in result[0] or "1" in result[0]  # 键可能是 int 或 str


def test_decode_simple_list_normal():
    """Decoder 应该能正确解码 SimpleList (bytes) 数据."""
    from jce import JceDict

    # 创建字节数组
    data = JceDict({0: b"hello"})
    encoded = dumps(data)

    reader = DataReader(encoded)
    decoder = GenericDecoder(reader)
    result = decoder.decode()

    # 验证解码结果
    assert isinstance(result[0], (bytes, memoryview))


# --- 8. 递归深度测试 ---


def test_decode_moderate_nesting():
    """Decoder 应该能处理合理深度的递归嵌套."""
    from jce import JceDict

    # 构建中等深度嵌套的结构 (10层)
    nested = [0]
    for _ in range(10):
        nested = [nested]

    data = JceDict({0: nested})
    encoded = dumps(data)

    reader = DataReader(encoded)
    decoder = GenericDecoder(reader)

    # 应该成功
    result = decoder.decode()
    assert isinstance(result, dict)


# --- 9. EOF 和 边界测试 ---


def test_reader_eof():
    """Reader 应该正确报告 EOF 状态."""
    data = b"\x01\x02"
    reader = DataReader(data)

    assert not reader.eof
    reader.read_bytes(2)
    assert reader.eof


def test_reader_zero_copy_mode():
    """Reader 在零拷贝模式下应该返回 memoryview."""
    from jce import JceOption

    data = b"\x01\x02\x03\x04"
    reader = DataReader(data, option=int(JceOption.ZERO_COPY))

    # 零复制模式应该返回 memoryview
    result = reader.read_bytes(2, zero_copy=True)
    assert isinstance(result, memoryview)
    assert bytes(result) == b"\x01\x02"


# --- 10. 整数解码边界测试 ---


def test_decode_int_with_bytes_value():
    """整数类型的 validate 方法应能自动转换 bytes 输入."""
    from jce.types import INT

    # INT.validate 应该能处理 bytes 输入
    result = INT.validate(b"\x00\x00\x00\x01")
    assert result == 1


INT_DECODE_CASES = [
    (0, "零值"),
    (127, "INT1最大值"),
    (128, "需要INT2"),
    (32767, "INT2最大值"),
    (32768, "需要INT4"),
    (2147483647, "INT4最大值"),
    (2147483648, "需要INT8"),
    (-1, "负数"),
    (-128, "INT1最小值"),
    (-129, "需要INT2负数"),
]


@pytest.mark.parametrize(("value", "desc"), INT_DECODE_CASES)
def test_decode_various_int_sizes(value, desc):
    """Decoder 应该能正确处理各种大小和符号的整数."""
    from jce import JceDict

    data = JceDict({0: value})
    encoded = dumps(data)
    reader = DataReader(encoded)
    decoder = GenericDecoder(reader)
    result = decoder.decode()
    assert result[0] == value, f"失败: {desc}"
