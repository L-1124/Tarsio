"""Tarsio Raw benchmark sparse tag."""

import pytest
from tarsio._core import encode_raw


@pytest.mark.benchmark(group="raw_sparse")
def test_bench_raw_encode_sparse_tag(benchmark, raw_sparse_tag):
    """测试 Raw 模式编码稀疏 Tag (Tag 250)."""
    benchmark(encode_raw, raw_sparse_tag)
