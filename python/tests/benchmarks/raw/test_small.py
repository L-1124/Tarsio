"""Raw simple structures benchmarks."""

import pytest
from tarsio import decode_raw, encode_raw


@pytest.mark.benchmark(group="raw_small")
def test_bench_raw_encode_small(benchmark, raw_data_small):
    """测试 Raw 模式编码简单字典."""
    benchmark(encode_raw, raw_data_small)


@pytest.mark.benchmark(group="raw_small")
def test_bench_raw_decode_small(benchmark, raw_bytes_small):
    """测试 Raw 模式解码简单字典."""
    benchmark(decode_raw, raw_bytes_small)
