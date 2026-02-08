"""Structural benchmarks (Recursion, Inheritance)."""

import pytest
from tarsio._core import decode, encode

from .models import DeepNest, DerivedStruct


@pytest.mark.benchmark(group="recursion")
def test_bench_encode_deep_nest(benchmark, deep_nest_obj):
    """测试深度递归结构编码."""
    benchmark(encode, deep_nest_obj)


@pytest.mark.benchmark(group="recursion")
def test_bench_decode_deep_nest(benchmark, deep_nest_bytes):
    """测试深度递归结构解码."""
    benchmark(decode, DeepNest, deep_nest_bytes)


@pytest.mark.benchmark(group="inheritance")
def test_bench_encode_inheritance(benchmark, derived_obj):
    """测试继承结构体编码性能."""
    benchmark(encode, derived_obj)


@pytest.mark.benchmark(group="inheritance")
def test_bench_decode_inheritance(benchmark, derived_bytes):
    """测试继承结构体解码性能."""
    benchmark(decode, DerivedStruct, derived_bytes)
