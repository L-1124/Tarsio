"""Tarsio Raw 基准测试共享 Fixtures."""

import pytest
from tarsio._core import TarsDict, encode_raw


@pytest.fixture
def raw_data_small() -> TarsDict:
    """生成 Small 对应的 raw dict."""
    return TarsDict({0: 123, 1: "benchmark", 2: True, 3: 3.14159})


@pytest.fixture
def raw_bytes_small(raw_data_small):
    """生成 raw dict 编码后的 bytes."""
    return encode_raw(raw_data_small)


@pytest.fixture
def raw_data_nested() -> TarsDict:
    """生成 Parent 对应的 raw dict (复杂嵌套)."""
    children = [TarsDict({0: i, 1: f"child_{i}"}) for i in range(50)]
    return TarsDict({0: children, 1: {"env": "production", "version": "1.0.0"}})


@pytest.fixture
def raw_bytes_nested(raw_data_nested):
    """生成 raw nested dict 编码后的 bytes."""
    return encode_raw(raw_data_nested)


@pytest.fixture
def raw_mixed_data() -> TarsDict:
    """生成混合类型字典."""
    return TarsDict(
        {0: 123, 1: "hello", 2: [1, 2, 3], 3: TarsDict({10: "nest"}), 200: 999}
    )


@pytest.fixture
def raw_mixed_bytes(raw_mixed_data):
    """生成混合类型编码数据."""
    return encode_raw(raw_mixed_data)


@pytest.fixture
def raw_huge_blob_struct() -> TarsDict:
    """生成包含 10MB Blob 的 Raw Struct (字典)."""
    return TarsDict({0: b"\x00" * (10 * 1024 * 1024)})


@pytest.fixture
def raw_huge_blob_bytes(raw_huge_blob_struct):
    """生成包含 10MB Blob 的 Raw Struct 编码数据."""
    return encode_raw(raw_huge_blob_struct)


@pytest.fixture
def raw_direct_list():
    """生成直接列表 (非 Struct)."""
    return [1, 2, 3, 4, 5]


@pytest.fixture
def raw_map_str_key():
    """生成字符串 Key 的字典 (Map, 非 Struct)."""
    return {"a": 1, "b": 2}


@pytest.fixture
def raw_sparse_tag() -> TarsDict:
    """生成稀疏 Tag 的 Struct."""
    return TarsDict({250: "sparse"})


@pytest.fixture
def raw_truncated_bytes():
    """生成截断的数据."""
    return b"\x02"  # Tag 0, Int4 (needs 4 bytes)... but EOF immediately
