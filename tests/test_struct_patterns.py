"""测试 JCE 字段定义的编码模式 (Nested/Blob/Any)."""

from typing import Any

import pytest

from jce import BYTES, JceDict, JceField, JceStruct, dumps


class NestedPattern(JceStruct):
    """模式 A: 标准嵌套结构体."""

    param: JceDict = JceField(jce_id=2)


class BlobPattern(JceStruct):
    """模式 B: 二进制 Blob (透传)."""

    param: JceDict = JceField(jce_id=2, jce_type=BYTES)


class AnyPattern(JceStruct):
    """模式 C: Any 类型 (动态推断)."""

    param: Any = JceField(jce_id=2)


class SafeAnyPattern(JceStruct):
    """模式 D: Any + 显式 BYTES (安全 Blob)."""

    param: Any = JceField(jce_id=2, jce_type=BYTES)


@pytest.fixture
def inner_data():
    """提供测试用的标准内部数据 {0: 100}."""
    # 代表 {0: 100}
    return JceDict({0: 100})


def test_pattern_a_nested_struct(inner_data):
    """模式 A 应该编码为 JCE Struct (Tag 2, Type 10)."""
    obj = NestedPattern(param=inner_data)
    encoded = dumps(obj)

    # 2A ... 0B
    assert encoded.startswith(b"\x2a")  # Tag 2, Type 10 (StructBegin)
    assert encoded.endswith(b"\x0b")  # Type 11 (StructEnd)

    # 应该包含内部数据 (00 64 -> Tag 0, Int 100)
    assert b"\x00\x64" in encoded


def test_pattern_b_binary_blob(inner_data):
    """模式 B 应该编码为 SimpleList (Tag 2, Type 13)."""
    obj = BlobPattern(param=inner_data)
    encoded = dumps(obj)

    # 2D ...
    assert encoded.startswith(b"\x2d")  # Tag 2, Type 13 (SimpleList)

    # Payload (inner_data 的 JceStruct 序列化结果) 应该在其中
    # inner_data 的序列化结果是 00 64 (Tag 0: 100)
    # Blob 格式: [Head] [Length] [Payload]
    # 00 64 的长度是 2 字节
    assert b"\x00\x02\x00\x64" in encoded


def test_pattern_c_any_with_jcedict(inner_data):
    """模式 C 传入 JceDict 时应该推断为 STRUCT."""
    obj = AnyPattern(param=inner_data)
    encoded = dumps(obj)

    # 行为应类似嵌套结构体
    assert encoded.startswith(b"\x2a")  # Tag 2, Type 10 (StructBegin)


def test_pattern_c_any_with_dict():
    """模式 C 传入 dict 时应该推断为 MAP."""
    # 普通 dict, 非 JceDict
    inner_dict = {0: 100}
    obj = AnyPattern(param=inner_dict)
    encoded = dumps(obj)

    # 行为应类似 Map (Tag 2, Type 8)
    # 28 ...
    assert encoded.startswith(b"\x28")  # Tag 2, Type 8 (Map)


def test_pattern_d_any_with_bytes_mode(inner_data):
    """模式 D (Any + BYTES) 应该始终作为 Blob 编码."""
    obj = SafeAnyPattern(param=inner_data)
    encoded = dumps(obj)

    # 行为应类似模式 B
    assert encoded.startswith(b"\x2d")  # Tag 2, Type 13 (SimpleList)


def test_any_field_inference_consistency():
    """验证 Any 字段的类型推断一致性."""
    # int -> INT
    # 100 适合 1 字节, 因此使用 JCE_INT1 (Type 0)
    # Tag 2 << 4 | Type 0 = 0x20
    assert dumps(AnyPattern(param=100)).startswith(b"\x20")

    # str -> STRING

    assert dumps(AnyPattern(param="a")).startswith(b"\x26")  # Tag 2, Type 6 (STRING1)

    # bytes -> SIMPLE_LIST
    assert dumps(AnyPattern(param=b"a")).startswith(b"\x2d")  # Tag 2, Type 13
