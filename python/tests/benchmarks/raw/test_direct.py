"""Raw direct encoding benchmarks."""

import pytest
from tarsio import encode_raw


@pytest.mark.benchmark(group="raw_direct")
def test_bench_raw_encode_direct_list(benchmark, raw_direct_list):
    """测试 Raw 模式直接编码 List (非 Struct)."""
    benchmark(encode_raw, raw_direct_list)


@pytest.mark.benchmark(group="raw_direct")
def test_bench_raw_encode_map_str_key(benchmark, raw_map_str_key):
    """测试 Raw 模式直接编码 Map (Str Key)."""
    benchmark(encode_raw, raw_map_str_key)
