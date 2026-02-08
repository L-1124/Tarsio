"""Special types benchmarks."""

import pytest
from tarsio import decode, encode

from .models import SpecialTypes


@pytest.mark.benchmark(group="special")
def test_bench_encode_special(benchmark, special_obj):
    """测试特殊类型(Union/Optional/Enum)编码性能."""
    benchmark(encode, special_obj)


@pytest.mark.benchmark(group="special")
def test_bench_decode_special(benchmark, special_bytes):
    """测试特殊类型(Union/Optional/Enum)解码性能."""
    benchmark(decode, SpecialTypes, special_bytes)
