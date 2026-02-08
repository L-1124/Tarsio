"""Raw error handling benchmarks."""

import pytest
from tarsio import decode_raw


@pytest.mark.benchmark(group="raw_error")
def test_bench_raw_decode_truncated(benchmark, raw_truncated_bytes):
    """测试 Raw 模式解码截断数据."""

    def _decode():
        with pytest.raises(ValueError):  # noqa: PT011
            decode_raw(raw_truncated_bytes)

    benchmark(_decode)
