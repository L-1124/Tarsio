"""Payloads benchmarks (Large data, Zero copy)."""

import pytest
from tarsio import decode, encode

from .models import LargeData


@pytest.mark.benchmark(group="large_data")
def test_bench_encode_large_data(benchmark, large_data_obj):
    """测试大数据块(Bytes vs List[int])编码性能."""
    benchmark(encode, large_data_obj)


@pytest.mark.benchmark(group="large_data")
def test_bench_decode_large_data(benchmark, large_data_bytes):
    """测试大数据块(Bytes vs List[int])解码性能."""
    benchmark(decode, LargeData, large_data_bytes)


@pytest.mark.benchmark(group="zero_copy")
def test_bench_decode_huge_blob(benchmark, huge_blob_bytes):
    """测试 10MB Blob 解码性能 (检测 Zero-copy)."""
    benchmark(decode, LargeData, huge_blob_bytes)
