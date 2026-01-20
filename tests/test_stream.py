"""JCE 流式处理功能测试.

覆盖 jce.stream 模块的核心特性:
1. 基础流读写 (Buffer management)
2. 长度前缀协议 (LengthPrefixedWriter/Reader)
3. 网络场景模拟 (粘包、拆包)
4. 边界条件 (Max buffer size, Length limits)
"""

import struct

import pytest

from jce import JceDict, JceField, JceStruct
from jce.stream import (
    JceStreamReader,
    JceStreamWriter,
    LengthPrefixedReader,
    LengthPrefixedWriter,
)

# --- 辅助结构体 ---


class StreamMsg(JceStruct):
    """用于流传输测试的简单消息."""

    id: int = JceField(jce_id=0)
    data: str = JceField(jce_id=1)


# --- 1. 基础流写入器测试 ---


def test_stream_writer_basic():
    """JceStreamWriter 应该能正确缓存和清空数据."""
    writer = JceStreamWriter()
    msg = StreamMsg(id=1, data="a")

    # 1. Pack 对象
    writer.write(msg)  # write is alias for pack

    # 2. Pack 原始字节
    writer.write_bytes(b"\xff")

    # 验证缓冲区
    buf = writer.get_buffer()
    assert len(buf) > 0
    assert buf.endswith(b"\xff")

    # 3. Clear
    writer.clear()
    assert len(writer.get_buffer()) == 0


# --- 2. 基础流读取器测试 ---


def test_stream_reader_basic():
    """JceStreamReader 应该能正确接收数据并执行缓冲区检查."""
    reader = JceStreamReader(target=JceDict)

    # Feed 数据
    reader.feed(b"\x00\x01")

    # JceStreamReader 本身不支持迭代 (因为没有定界符)
    with pytest.raises(NotImplementedError, match="Use LengthPrefixedUnpacker"):
        next(iter(reader))

    # 测试 Max Buffer Size
    small_reader = JceStreamReader(target=JceDict, max_buffer_size=5)
    small_reader.feed(b"123")

    with pytest.raises(BufferError, match="max size"):
        small_reader.feed(b"456")  # Total 6 > 5


def test_stream_reader_with_jcedict_target():
    """JceStreamReader 应该支持 JceDict 作为 target."""
    reader = JceStreamReader(target=JceDict)

    # Feed 完整的 Struct 数据
    reader.feed(b"\x00\x64")  # Tag 0, Int 100

    # 验证类型 (虽然 JceStreamReader 不支持迭代，但可以测试初始化)
    assert reader._target == JceDict


# --- 3. 长度前缀协议测试 (核心) ---


def test_length_prefixed_round_trip_defaults():
    """LengthPrefixedWriter/Reader 默认配置下应能正确处理多包数据 (模拟粘包)."""
    # --- Writer ---
    writer = LengthPrefixedWriter()
    msg1 = StreamMsg(id=1, data="hello")
    msg2 = StreamMsg(id=2, data="world")

    writer.pack(msg1)
    writer.pack(msg2)

    full_data = writer.get_buffer()

    # --- Reader ---
    # 模拟粘包: 一次性喂入两个包的数据
    reader = LengthPrefixedReader(target=StreamMsg)
    reader.feed(full_data)

    # 解析
    packets = list(reader)
    assert len(packets) == 2
    assert packets[0].id == 1
    assert packets[0].data == "hello"
    assert packets[1].id == 2
    assert packets[1].data == "world"


def test_length_prefixed_custom_config():
    """LengthPrefixedWriter/Reader 应支持自定义长度头 (长度/字节序/包含性)."""
    # Exclusive: 长度头的值仅表示 Body 长度，不包含头本身
    writer = LengthPrefixedWriter(
        length_type=2, little_endian_length=True, inclusive_length=False
    )

    msg = StreamMsg(id=1, data="a")  # 编码后长度很短
    writer.pack(msg)
    data = writer.get_buffer()

    # 验证二进制结构
    # Header (2 bytes, LE)
    body_len = len(data) - 2
    header_val = struct.unpack("<H", data[:2])[0]
    assert header_val == body_len  # 因为是 Exclusive，所以等于 Body 长度

    # --- Reader (配置必须匹配) ---
    reader = LengthPrefixedReader(
        target=StreamMsg,
        length_type=2,
        little_endian_length=True,
        inclusive_length=False,
    )
    reader.feed(data)
    packets = list(reader)
    assert len(packets) == 1
    assert packets[0].data == "a"


def test_length_prefixed_fragmentation():
    """LengthPrefixedReader 应能处理数据分片到达 (模拟拆包)."""
    writer = LengthPrefixedWriter()
    writer.pack(StreamMsg(id=1, data="long_message"))
    full_data = writer.get_buffer()

    reader = LengthPrefixedReader(target=StreamMsg)

    # 1. 喂入前 5 个字节 (Header + 1 byte Body)
    reader.feed(full_data[:5])
    assert list(reader) == []  # 数据不够，不应该产出任何包

    # 2. 喂入剩余字节
    reader.feed(full_data[5:])
    packets = list(reader)
    assert len(packets) == 1
    assert packets[0].data == "long_message"


def test_reader_partial_header():
    """LengthPrefixedReader 应能处理头部尚未接收完整的情况."""
    reader = LengthPrefixedReader(target=dict, length_type=4)

    # 只喂 2 个字节 (需要 4 字节头)
    reader.feed(b"\x00\x00")
    assert list(reader) == []

    # 补齐头和 Body
    # 假设 Body 长度 4 (Inclusive) -> Header value = 4
    # Big Endian: 00 00 00 04
    # 注意: Reader 内部会尝试 decode BODY，这里我们需要构造合法的 JCE Body
    # JCE: Tag 0 INT1(1) -> 00 01 (2 bytes)
    # Total Len (Inclusive) = 4 (Header) + 2 (Body) = 6

    # Reset
    reader = LengthPrefixedReader(target=dict, length_type=4, inclusive_length=True)

    # 1. 部分头 (2 bytes)
    reader.feed(b"\x00\x00")
    assert list(reader) == []

    # 2. 补齐头 (Remaining 2 bytes of header -> 00 06)
    reader.feed(b"\x00\x06")
    assert list(reader) == []  # Have Header (know len=6), but buffer only 4 bytes

    # 3. 补齐 Body (00 01)
    reader.feed(b"\x00\x01")
    # Total buffer: 00 00 00 06 00 01 (6 bytes) -> Packet Complete

    packets = list(reader)
    assert len(packets) == 1
    assert packets[0] == {0: 1}


def test_length_prefixed_reader_with_jcedict():
    """LengthPrefixedReader 应该支持 JceDict 作为 target."""
    # 构造数据
    writer = LengthPrefixedWriter()
    test_data = JceDict({0: 100, 1: "test"})
    writer.pack(test_data)

    # 使用 JceDict 作为 target
    reader = LengthPrefixedReader(target=JceDict)
    reader.feed(writer.get_buffer())

    packets = list(reader)
    assert len(packets) == 1
    assert isinstance(packets[0], JceDict)
    assert packets[0][0] == 100
    assert packets[0][1] == "test"


# --- 4. 异常边界测试 ---


def test_writer_length_limit():
    """LengthPrefixedWriter 在包大小超过 Header 表示范围时应抛出 ValueError."""
    # 1字节头，最大 255
    writer = LengthPrefixedWriter(length_type=1)

    # 构造一个 300 字节的字符串
    large_msg = StreamMsg(id=1, data="x" * 300)

    with pytest.raises(ValueError, match="Packet too large"):
        writer.pack(large_msg)


def test_invalid_length_type():
    """Stream 组件初始化时应校验 length_type 参数."""
    with pytest.raises(ValueError, match="length_type must be"):
        LengthPrefixedWriter(length_type=3)  # 只有 1, 2, 4 合法

    with pytest.raises(ValueError, match="length_type must be"):
        LengthPrefixedReader(target=dict, length_type=3)
