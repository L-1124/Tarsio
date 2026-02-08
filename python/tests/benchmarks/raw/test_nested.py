"""Raw nested structures benchmarks."""

import pytest
from tarsio import decode_raw, encode_raw


@pytest.mark.benchmark(group="raw_nested")
def test_bench_raw_encode_nested(benchmark, raw_data_nested):
    """测试 Raw 模式编码嵌套字典."""
    benchmark(encode_raw, raw_data_nested)


@pytest.mark.benchmark(group="raw_nested")
def test_bench_raw_decode_nested(benchmark, raw_bytes_nested):
    """测试 Raw 模式解码嵌套字典."""
    benchmark(decode_raw, raw_bytes_nested)
