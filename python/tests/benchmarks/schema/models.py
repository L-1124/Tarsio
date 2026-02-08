"""Tarsio 基准测试共享模型定义."""

from enum import Enum, IntEnum
from typing import Annotated, Optional

from tarsio import Struct


class Status(IntEnum):
    """整数枚举."""

    INIT = 0
    RUNNING = 1
    DONE = 2
    ERROR = 3


class Mode(str, Enum):
    """字符串枚举."""

    FAST = "fast"
    SAFE = "safe"


class Primitives(Struct):
    """覆盖所有基础类型 (ZeroTag 优化路径 + 正常值)."""

    i8: Annotated[int, 0]
    i16: Annotated[int, 1]
    i32: Annotated[int, 2]
    i64: Annotated[int, 3]
    f32: Annotated[float, 4]
    f64: Annotated[float, 5]
    s1: Annotated[str, 6]
    s4: Annotated[str, 7]
    b: Annotated[bool, 8]


class Containers(Struct):
    """覆盖所有容器类型."""

    lst_int: Annotated[list[int], 0]
    lst_struct: Annotated[list[Primitives], 1]
    mp_str_int: Annotated[dict[str, int], 2]
    mp_int_str: Annotated[dict[int, str], 3]
    st_int: Annotated[set[int], 4]
    tup: Annotated[tuple[int, str, float], 5]  # Fixed tuple
    vtup: Annotated[tuple[int, ...], 6]  # Var tuple


class SpecialTypes(Struct):
    """覆盖特殊类型 (Union, Optional, Enum)."""

    opt_none: Annotated[int | None, 0]
    opt_val: Annotated[int | None, 1]
    u_int: Annotated[int | str, 2]
    u_str: Annotated[int | str, 3]
    e_int: Annotated[Status, 4]
    e_str: Annotated[Mode, 5]


class HighTagStruct(Struct):
    """测试高 Tag (>15) 的头部编码."""

    val_low: Annotated[int, 0]
    val_high: Annotated[int, 200]


class Medium(Struct):
    """中等规模结构体 (容器)."""

    ids: Annotated[list[int], 0]
    tags: Annotated[list[str], 1]
    props: Annotated[dict[str, int], 2]
    note: Annotated[str | None, 3] = None


class DeepNest(Struct):
    """测试递归深度 (轻量级，只测开销)."""

    val: Annotated[int, 0]
    next: Annotated[Optional["DeepNest"], 1] = None


class LargeData(Struct):
    """大数据测试."""

    blob: Annotated[bytes, 0]  # SimpleList
    ints: Annotated[list[int], 1]  # Standard List


# 动态创建 SparseStruct 以包含 50 个可选字段
_sparse_annotations = {f"f{i}": Annotated[int | None, i] for i in range(50)}
_sparse_defaults = {f"f{i}": None for i in range(50)}
SparseStruct = type(
    "SparseStruct",
    (Struct,),
    {
        "__annotations__": _sparse_annotations,
        "__module__": __name__,
        **_sparse_defaults,
    },
)


class MapEnumStruct(Struct):
    """测试 Enum 作为 Map Key."""

    mapping: Annotated[dict[Status, int], 0]


class BaseStruct(Struct):
    """基类."""

    base_val: Annotated[int, 0]


class DerivedStruct(BaseStruct):
    """派生类."""

    derived_val: Annotated[int, 1]
