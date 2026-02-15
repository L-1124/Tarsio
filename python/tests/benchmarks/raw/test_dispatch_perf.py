"""Benchmark for API dispatch overhead."""

from typing import Annotated

import pytest
import tarsio
import tarsio._core
from tarsio import Struct


class SimpleStruct(Struct):
    """基准测试使用的最小结构体."""

    id: Annotated[int, 0]


@pytest.fixture(scope="module")
def struct_obj():
    """提供 Struct 编码基准对象."""
    return SimpleStruct(id=123)


@pytest.fixture(scope="module")
def dict_obj():
    """提供 dict 编码基准对象."""
    return {1: 100, 2: "test"}


@pytest.fixture(scope="module")
def list_obj():
    """提供 list 编码基准对象."""
    return [1, 2, 3, 4, 5]


@pytest.fixture(scope="module")
def int_obj():
    """提供 int 编码基准对象."""
    return 12345


@pytest.mark.benchmark(group="dispatch_struct")
def test_encode_dispatch_struct(benchmark, struct_obj):
    """基准测试：tarsio.encode 对 Struct 的分发开销."""
    benchmark(tarsio.encode, struct_obj)


@pytest.mark.benchmark(group="dispatch_struct")
def test_encode_core_struct(benchmark, struct_obj):
    """基准测试：直接调用 tarsio._core.encode 处理 Struct."""
    benchmark(tarsio._core.encode, struct_obj)


@pytest.mark.benchmark(group="dispatch_dict")
def test_encode_dispatch_dict(benchmark, dict_obj):
    """基准测试：tarsio.encode 对 dict 的分发开销."""
    benchmark(tarsio.encode, dict_obj)


@pytest.mark.benchmark(group="dispatch_dict")
def test_encode_raw_dict(benchmark, dict_obj):
    """基准测试：直接调用 tarsio._core.encode_raw 处理 dict."""
    benchmark(tarsio._core.encode_raw, dict_obj)


@pytest.mark.benchmark(group="dispatch_list")
def test_encode_dispatch_list(benchmark, list_obj):
    """基准测试：tarsio.encode 对 list 的分发开销."""
    benchmark(tarsio.encode, list_obj)


@pytest.mark.benchmark(group="dispatch_list")
def test_encode_raw_list(benchmark, list_obj):
    """基准测试：直接调用 tarsio._core.encode_raw 处理 list."""
    benchmark(tarsio._core.encode_raw, list_obj)


@pytest.mark.benchmark(group="dispatch_int")
def test_encode_dispatch_int(benchmark, int_obj):
    """基准测试：tarsio.encode 对 int 的分发开销."""
    benchmark(tarsio.encode, int_obj)


@pytest.mark.benchmark(group="dispatch_int")
def test_encode_raw_int(benchmark, int_obj):
    """基准测试：直接调用 tarsio._core.encode_raw 处理 int."""
    benchmark(tarsio._core.encode_raw, int_obj)
