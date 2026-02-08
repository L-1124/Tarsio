"""Tarsio Raw benchmark payloads."""

import pytest
from tarsio._core import decode_raw, encode_raw


@pytest.mark.benchmark(group="raw_payloads")
def test_bench_raw_encode_huge_struct_blob(benchmark, raw_huge_blob_struct):
    """测试 Raw 模式编码 10MB Blob (Struct)."""
    benchmark(encode_raw, raw_huge_blob_struct)


@pytest.mark.benchmark(group="raw_payloads")
def test_bench_raw_decode_huge_struct_blob(benchmark, raw_huge_blob_bytes):
    """测试 Raw 模式解码 10MB Blob (Struct)."""
    benchmark(decode_raw, raw_huge_blob_bytes)
