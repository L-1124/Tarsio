"""Tarsio 基准测试共享 Fixtures."""

from typing import Any, cast

import pytest
from tarsio import TarsDict, encode

from tests.benchmarks.schema.models import (
    Containers,
    DeepNest,
    DerivedStruct,
    HighTagStruct,
    LargeData,
    MapEnumStruct,
    Medium,
    Mode,
    Primitives,
    SparseStruct,
    SpecialTypes,
    Status,
)


@pytest.fixture
def primitives_obj():
    """生成基础类型测试对象."""
    return Primitives(
        i8=10,
        i16=3000,
        i32=100000,
        i64=5000000000,
        f32=3.14,
        f64=2.718281828,
        s1="hello",
        s4="world" * 50,  # s4 > 255 length
        b=True,
    )


@pytest.fixture
def primitives_bytes(primitives_obj):
    """生成基础类型编码数据."""
    return encode(primitives_obj)


@pytest.fixture
def primitives_zeros():
    """全零值，测试 ZeroTag 压缩优化."""
    return Primitives(0, 0, 0, 0, 0.0, 0.0, "", "", False)


@pytest.fixture
def containers_obj(primitives_obj):
    """生成容器类型测试对象."""
    return Containers(
        lst_int=list(range(100)),
        lst_struct=[primitives_obj] * 10,
        mp_str_int={f"k{i}": i for i in range(50)},
        mp_int_str={i: f"v{i}" for i in range(50)},
        st_int=set(range(50)),
        tup=(1, "tuple", 2.0),
        vtup=(1, 2, 3, 4, 5),
    )


@pytest.fixture
def containers_bytes(containers_obj):
    """生成容器类型编码数据."""
    return encode(containers_obj)


@pytest.fixture
def medium_dict() -> TarsDict:
    """生成 Medium 对象的字典形式 (用于 Raw 对比)."""
    return TarsDict(
        {
            0: list(range(100)),
            1: [f"tag_{i}" for i in range(20)],
            2: {f"key_{i}": i for i in range(20)},
            3: None,
        }
    )


@pytest.fixture
def medium_struct_obj():
    """生成 Medium 结构体实例."""
    return Medium(
        ids=list(range(100)),
        tags=[f"tag_{i}" for i in range(20)],
        props={f"key_{i}": i for i in range(20)},
    )


@pytest.fixture
def medium_struct_bytes(medium_struct_obj):
    """生成 Medium 结构体编码数据."""
    return encode(medium_struct_obj)


@pytest.fixture
def special_obj():
    """生成特殊类型测试对象."""
    return SpecialTypes(
        opt_none=None,
        opt_val=123,
        u_int=456,
        u_str="union",
        e_int=Status.DONE,
        e_str=Mode.SAFE,
    )


@pytest.fixture
def special_bytes(special_obj):
    """生成特殊类型编码数据."""
    return encode(special_obj)


@pytest.fixture
def high_tag_obj():
    """生成高 Tag 测试对象."""
    return HighTagStruct(1, 2)


@pytest.fixture
def high_tag_bytes(high_tag_obj):
    """生成高 Tag 编码数据."""
    return encode(high_tag_obj)


@pytest.fixture
def large_data_obj():
    """生成大数据测试对象."""
    size = 10000
    return LargeData(blob=b"\x01" * size, ints=[1] * size)


@pytest.fixture
def large_data_bytes(large_data_obj):
    """生成大数据编码数据."""
    return encode(large_data_obj)


@pytest.fixture
def huge_blob_obj():
    """生成超大 Blob (10MB) 用于 Zero-copy 测试."""
    return LargeData(blob=b"\xff" * (10 * 1024 * 1024), ints=[])


@pytest.fixture
def huge_blob_bytes(huge_blob_obj):
    """生成超大 Blob 编码数据."""
    return encode(huge_blob_obj)


@pytest.fixture
def sparse_obj():
    """生成稀疏对象 (50个字段只填首尾)."""
    obj = cast(Any, SparseStruct())
    obj.f0 = 100
    obj.f49 = 200
    return obj


@pytest.fixture
def sparse_bytes(sparse_obj):
    """生成稀疏对象编码数据."""
    return encode(sparse_obj)


@pytest.fixture
def map_enum_obj():
    """生成 Map Enum Key 对象."""
    return MapEnumStruct(mapping={Status.INIT: 1, Status.DONE: 2, Status.ERROR: -1})


@pytest.fixture
def map_enum_bytes(map_enum_obj):
    """生成 Map Enum Key 编码数据."""
    return encode(map_enum_obj)


@pytest.fixture
def derived_obj():
    """生成继承结构对象."""
    return DerivedStruct(base_val=10, derived_val=20)


@pytest.fixture
def derived_bytes(derived_obj):
    """生成继承结构编码数据."""
    return encode(derived_obj)


@pytest.fixture
def malformed_bytes():
    """生成畸形数据 (截断)."""
    return b"\x00\x01\x02"


@pytest.fixture
def deep_nest_obj():
    """生成深度嵌套对象 (Depth 30)."""
    root = DeepNest(val=0)
    curr = root
    for i in range(1, 30):
        new_node = DeepNest(val=i)
        curr.next = new_node
        curr = new_node
    return root


@pytest.fixture
def deep_nest_bytes(deep_nest_obj):
    """生成深度嵌套编码数据."""
    return encode(deep_nest_obj)
