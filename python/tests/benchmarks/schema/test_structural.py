"""Structural benchmarks (Recursion, Inheritance)."""

from typing import Annotated

import pytest
from tarsio._core import Struct, decode, encode

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


def _make_struct(size: int) -> tuple[type[Struct], dict[str, int]]:
    annotations = {f"f{i}": Annotated[int, i] for i in range(size)}
    payload = {f"f{i}": i for i in range(size)}

    cls = type(
        f"Init{size}",
        (Struct,),
        {"__annotations__": annotations, "__module__": __name__},
    )
    return cls, payload


INIT_8, PAYLOAD_8 = _make_struct(8)
INIT_24, PAYLOAD_24 = _make_struct(24)
INIT_64, PAYLOAD_64 = _make_struct(64)


@pytest.mark.benchmark(group="init_small")
def test_bench_init_small(benchmark):
    """测试小结构体 __init__ 性能."""
    benchmark(INIT_8, **PAYLOAD_8)


@pytest.mark.benchmark(group="init_medium")
def test_bench_init_medium(benchmark):
    """测试中结构体 __init__ 性能."""
    benchmark(INIT_24, **PAYLOAD_24)


@pytest.mark.benchmark(group="init_large")
def test_bench_init_large(benchmark):
    """测试大结构体 __init__ 性能."""
    benchmark(INIT_64, **PAYLOAD_64)
