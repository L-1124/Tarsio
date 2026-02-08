"""Containers benchmarks."""

import pytest
from tarsio._core import decode, encode

from .models import Containers, MapEnumStruct, SparseStruct


@pytest.mark.benchmark(group="containers")
def test_bench_encode_containers(benchmark, containers_obj):
    """测试容器类型编码性能."""
    benchmark(encode, containers_obj)


@pytest.mark.benchmark(group="containers")
def test_bench_decode_containers(benchmark, containers_bytes):
    """测试容器类型解码性能."""
    benchmark(decode, Containers, containers_bytes)


@pytest.mark.benchmark(group="sparse")
def test_bench_encode_sparse(benchmark, sparse_obj):
    """测试稀疏字段结构体编码."""
    benchmark(encode, sparse_obj)


@pytest.mark.benchmark(group="sparse")
def test_bench_decode_sparse(benchmark, sparse_bytes):
    """测试稀疏字段结构体解码."""
    benchmark(decode, SparseStruct, sparse_bytes)


@pytest.mark.benchmark(group="map_complex")
def test_bench_encode_map_enum(benchmark, map_enum_obj):
    """测试 Enum 作为 Map Key 的编码性能."""
    benchmark(encode, map_enum_obj)


@pytest.mark.benchmark(group="map_complex")
def test_bench_decode_map_enum(benchmark, map_enum_bytes):
    """测试 Enum 作为 Map Key 的解码性能."""
    benchmark(decode, MapEnumStruct, map_enum_bytes)
