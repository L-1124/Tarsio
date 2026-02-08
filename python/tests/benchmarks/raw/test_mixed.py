"""Raw mixed types benchmarks."""

import pytest
from tarsio import decode_raw, encode_raw


@pytest.mark.benchmark(group="raw_mixed")
def test_bench_raw_encode_mixed(benchmark, raw_mixed_data):
    """测试 Raw 模式混合数据编码性能."""
    benchmark(encode_raw, raw_mixed_data)


@pytest.mark.benchmark(group="raw_mixed")
def test_bench_raw_decode_mixed(benchmark, raw_mixed_bytes):
    """测试 Raw 模式混合数据解码性能."""
    benchmark(decode_raw, raw_mixed_bytes)
