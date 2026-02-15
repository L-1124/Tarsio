"""Benchmark for large struct decoding performance."""

from typing import Annotated

import pytest
from tarsio._core import Struct, decode, encode


# Define a struct with 20 fields of mixed types
class LargeStruct(Struct):
    """基准测试使用的大字段结构体."""

    f0: Annotated[int, 0]
    f1: Annotated[str, 1]
    f2: Annotated[bool, 2]
    f3: Annotated[int, 3]
    f4: Annotated[str, 4]
    f5: Annotated[bool, 5]
    f6: Annotated[int, 6]
    f7: Annotated[str, 7]
    f8: Annotated[bool, 8]
    f9: Annotated[int, 9]
    f10: Annotated[str, 10]
    f11: Annotated[bool, 11]
    f12: Annotated[int, 12]
    f13: Annotated[str, 13]
    f14: Annotated[bool, 14]
    f15: Annotated[int, 15]
    f16: Annotated[str, 16]
    f17: Annotated[bool, 17]
    f18: Annotated[int, 18]
    f19: Annotated[str, 19]


@pytest.fixture(scope="module")
def large_struct_data():
    """提供 LargeStruct 解码基准数据."""
    obj = LargeStruct(
        f0=100,
        f1="test_string_1",
        f2=True,
        f3=200,
        f4="test_string_2",
        f5=False,
        f6=300,
        f7="test_string_3",
        f8=True,
        f9=400,
        f10="test_string_4",
        f11=False,
        f12=500,
        f13="test_string_5",
        f14=True,
        f15=600,
        f16="test_string_6",
        f17=False,
        f18=700,
        f19="test_string_7",
    )
    return encode(obj)


@pytest.mark.benchmark(group="struct_decode")
def test_decode_large_struct(benchmark, large_struct_data):
    """基准测试：解码包含 20 个字段的结构体."""
    benchmark(decode, LargeStruct, large_struct_data)
