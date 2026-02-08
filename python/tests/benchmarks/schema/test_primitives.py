"""Primitives benchmarks."""

import pytest
from tarsio._core import decode, encode

from .models import HighTagStruct, Primitives


@pytest.mark.benchmark(group="primitives")
def test_bench_encode_primitives(benchmark, primitives_obj):
    """测试基础类型编码性能."""
    benchmark(encode, primitives_obj)


@pytest.mark.benchmark(group="primitives")
def test_bench_decode_primitives(benchmark, primitives_bytes):
    """测试基础类型解码性能."""
    benchmark(decode, Primitives, primitives_bytes)


@pytest.mark.benchmark(group="primitives")
def test_bench_encode_primitives_zeros(benchmark, primitives_zeros):
    """测试 ZeroTag 优化路径编码性能."""
    benchmark(encode, primitives_zeros)


@pytest.mark.benchmark(group="high_tag")
def test_bench_encode_high_tag(benchmark, high_tag_obj):
    """测试高 Tag 字段编码性能."""
    benchmark(encode, high_tag_obj)


@pytest.mark.benchmark(group="high_tag")
def test_bench_decode_high_tag(benchmark, high_tag_bytes):
    """测试高 Tag 字段解码性能."""
    benchmark(decode, HighTagStruct, high_tag_bytes)
