"""Error handling benchmarks."""

import pytest
from tarsio._core import decode

from .models import Primitives


@pytest.mark.benchmark(group="error_handling")
def test_bench_decode_error(benchmark, malformed_bytes):
    """测试畸形数据解码失败的开销 (Fail Fast)."""

    def _decode_safe():
        with pytest.raises(ValueError):  # noqa: PT011
            decode(Primitives, malformed_bytes)

    benchmark(_decode_safe)
