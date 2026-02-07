"""测试 inspect 类型内省接口."""

from typing import Annotated, Optional

import pytest
from tarsio import Meta, Struct
from tarsio import inspect as tinspect


def test_type_info_primitives() -> None:
    """type_info 应能解析基础类型."""
    assert tinspect.type_info(int).kind == "int"
    assert tinspect.type_info(str).kind == "str"
    assert tinspect.type_info(float).kind == "float"
    assert tinspect.type_info(bool).kind == "bool"
    assert tinspect.type_info(bytes).kind == "bytes"


def test_type_info_containers() -> None:
    """type_info 应能解析容器类型."""
    assert tinspect.type_info(list[int]).kind == "list"
    assert tinspect.type_info(tuple[str]).kind == "tuple"
    assert tinspect.type_info(dict[str, int]).kind == "map"


def test_type_info_optional() -> None:
    """type_info 应能解析 Optional."""
    info = tinspect.type_info(Optional[int])  # noqa: UP045
    assert info.kind == "optional"


def test_type_info_unsupported_type_raises() -> None:
    """不支持类型应抛 TypeError."""
    with pytest.raises(TypeError, match="Unsupported Tars type"):
        tinspect.type_info(set[int])  # type: ignore[arg-type]


def test_struct_info_fields_and_order() -> None:
    """struct_info 应按 Tag 顺序返回字段."""

    class Sample(Struct):
        b: Annotated[int, 1]
        a: Annotated[str, 0]

    info = tinspect.struct_info(Sample)
    assert info is not None
    assert [f.name for f in info.fields] == ["a", "b"]


def test_struct_info_optional_default() -> None:
    """Optional 字段无显式默认值时应默认 None."""

    class Sample(Struct):
        a: Annotated[int | None, 0]

    info = tinspect.struct_info(Sample)
    assert info is not None
    field = info.fields[0]
    assert field.optional is True
    assert field.has_default is True
    assert field.default is None


def test_struct_info_meta_constraints() -> None:
    """Meta 约束应被解析到 constraints."""

    class Sample(Struct):
        a: Annotated[int, Meta(tag=0, gt=0)]

    info = tinspect.struct_info(Sample)
    assert info is not None
    field = info.fields[0]
    assert field.constraints is not None
    assert field.constraints.gt == 0


def test_struct_info_missing_tag_raises() -> None:
    """缺失 Tag 应抛 TypeError."""
    with pytest.raises(TypeError, match="Missing tag"):

        class Sample(Struct):
            a: Annotated[int, "doc"]  # type: ignore


def test_struct_info_duplicate_tag_raises() -> None:
    """重复 Tag 应抛 TypeError."""
    with pytest.raises(TypeError, match="Duplicate tag"):

        class Sample(Struct):
            a: Annotated[int, 1]
            b: Annotated[int, 1]


def test_struct_info_mix_meta_and_int_tag_raises() -> None:
    """混用 Meta 与整数 Tag 应抛 TypeError."""
    with pytest.raises(TypeError, match="Do not mix integer tag and Meta object"):

        class Sample(Struct):
            a: Annotated[int, 1, Meta(tag=1)]
