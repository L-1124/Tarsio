"""测试 Schema 编译期规则: Tag 与 Meta 严格互斥."""

from typing import Annotated

import pytest
from tarsio import Meta, Struct


def test_classic_mode_allows_extra_non_meta_annotations() -> None:
    """经典模式允许额外的非 Meta 注解."""

    class User(Struct):
        name: Annotated[str, 2, "doc"]

    obj = User("a")
    assert obj.name == "a"


def test_forbidden_mix_int_tag_and_meta_raises() -> None:
    """混用整数 Tag 与 Meta 时在类定义期抛出 TypeError."""
    with pytest.raises(TypeError, match="Do not mix integer tag and Meta object"):

        class User(Struct):
            uid: Annotated[int, 1, Meta(tag=1, gt=0)]


def test_forbidden_mix_meta_and_int_tag_raises() -> None:
    """混用 Meta 与整数 Tag(顺序颠倒)时抛出 TypeError."""
    with pytest.raises(TypeError, match="Do not mix integer tag and Meta object"):

        class User(Struct):
            uid: Annotated[int, Meta(tag=1, gt=0), 1]


def test_meta_missing_tag_raises() -> None:
    """Meta 缺失 tag 时在类定义期抛出 TypeError."""
    with pytest.raises(TypeError, match="must include 'tag'"):

        class User(Struct):
            name: Annotated[str, Meta(max_len=10)]


def test_missing_tag_raises() -> None:
    """缺失 Tag 时在类定义期抛出 TypeError."""
    with pytest.raises(TypeError, match="Missing tag"):

        class User(Struct):
            uid: Annotated[int, "doc"]


def test_multiple_integer_tags_raises() -> None:
    """同一字段声明多个整数 Tag 时抛出 TypeError."""
    with pytest.raises(TypeError, match="Multiple integer tags"):

        class User(Struct):
            uid: Annotated[int, 1, 2]


def test_multiple_meta_objects_raises() -> None:
    """同一字段声明多个 Meta 时抛出 TypeError."""
    with pytest.raises(TypeError, match="Multiple Meta objects"):

        class User(Struct):
            uid: Annotated[int, Meta(tag=1), Meta(tag=2)]


def test_duplicate_tag_raises() -> None:
    """两个字段重复使用同一 Tag 时抛出 TypeError."""
    with pytest.raises(TypeError, match="Duplicate tag"):

        class User(Struct):
            a: Annotated[int, 1]
            b: Annotated[int, 1]
