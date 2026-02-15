"""Benchmark for raw map decoding performance."""

import pytest
from tarsio._core import decode_raw, encode_raw


@pytest.fixture(scope="module")
def map_int_keys_data():
    """提供 10k 整数键 map 的编码数据."""
    # 10,000 integer keys
    data = {i: i for i in range(10000)}
    return encode_raw(data)


@pytest.fixture(scope="module")
def map_str_keys_data():
    """提供 10k 字符串键 map 的编码数据."""
    # 10,000 string keys
    data = {str(i): i for i in range(10000)}
    return encode_raw(data)


@pytest.mark.benchmark(group="raw_map_decode")
def test_decode_map_int_keys_10k(benchmark, map_int_keys_data):
    """基准测试：解码 10k 整数键的原始 map."""
    benchmark(decode_raw, map_int_keys_data)


@pytest.mark.benchmark(group="raw_map_decode")
def test_decode_map_str_keys_10k(benchmark, map_str_keys_data):
    """基准测试：解码 10k 字符串键的原始 map."""
    benchmark(decode_raw, map_str_keys_data)
