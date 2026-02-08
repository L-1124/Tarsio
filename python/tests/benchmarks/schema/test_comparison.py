"""Comparison benchmarks."""

import pytest
from tarsio import decode, decode_raw, encode, encode_raw

from .models import Medium


@pytest.mark.benchmark(group="compare_encode")
def test_bench_compare_schema_encode(benchmark, medium_struct_obj):
    """[对比] Schema 模式编码 Medium 结构."""
    benchmark(encode, medium_struct_obj)


@pytest.mark.benchmark(group="compare_encode")
def test_bench_compare_raw_encode(benchmark, medium_dict):
    """[对比] Raw 模式编码 Medium 字典."""
    benchmark(encode_raw, medium_dict)


@pytest.mark.benchmark(group="compare_decode")
def test_bench_compare_schema_decode(benchmark, medium_struct_bytes):
    """[对比] Schema 模式解码 Medium 结构."""
    benchmark(decode, Medium, medium_struct_bytes)


@pytest.mark.benchmark(group="compare_decode")
def test_bench_compare_raw_decode(benchmark, medium_struct_bytes):
    """[对比] Raw 模式解码 Medium 数据."""
    benchmark(decode_raw, medium_struct_bytes)
