"""测试 Schema/Meta/Inspect 相关行为."""

from typing import Annotated, Optional

import pytest
from tarsio import Meta, Struct, TarsDict, ValidationError, encode_raw
from tarsio import inspect as tinspect


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


def test_meta_tag_only_is_allowed() -> None:
    """Meta 仅提供 tag 时允许正常解码."""

    class User(Struct):
        uid: Annotated[int, Meta(tag=1)]

    data = encode_raw(TarsDict({1: 123}))
    obj = User.decode(data)
    assert obj.uid == 123


def test_numeric_gt_validation_raises() -> None:
    """数值 gt 约束不满足时抛出 ValidationError."""

    class User(Struct):
        uid: Annotated[int, Meta(tag=1, gt=0)]

    data = encode_raw(TarsDict({1: 0}))
    with pytest.raises(ValidationError, match="must be >"):
        User.decode(data)


@pytest.mark.parametrize("value", [0, 10], ids=["lower", "upper"])
def test_numeric_ge_le_validation_passes(value: int) -> None:
    """数值区间约束满足时正常解码."""

    class User(Struct):
        uid: Annotated[int, Meta(tag=1, ge=0, le=10)]

    ok = User.decode(encode_raw(TarsDict({1: value})))
    assert ok.uid == value


def test_numeric_ge_validation_raises() -> None:
    """数值 ge 约束不满足时抛出 ValidationError."""

    class User(Struct):
        uid: Annotated[int, Meta(tag=1, ge=1)]

    with pytest.raises(ValidationError, match="must be >="):
        User.decode(encode_raw(TarsDict({1: 0})))


def test_string_min_len_validation_raises() -> None:
    """字符串 min_len 约束不满足时抛出 ValidationError."""

    class User(Struct):
        name: Annotated[str, Meta(tag=1, min_len=1, max_len=3)]

    with pytest.raises(ValidationError, match="length must be >="):
        User.decode(encode_raw(TarsDict({1: ""})))


def test_string_max_len_validation_raises() -> None:
    """字符串 max_len 约束不满足时抛出 ValidationError."""

    class User(Struct):
        name: Annotated[str, Meta(tag=1, min_len=1, max_len=3)]

    with pytest.raises(ValidationError, match="length must be <="):
        User.decode(encode_raw(TarsDict({1: "abcd"})))


def test_string_pattern_validation_passes() -> None:
    """字符串满足 pattern 约束时正常解码."""

    class User(Struct):
        code: Annotated[str, Meta(tag=1, pattern=r"^[A-Z]{2}\d{2}$")]

    ok = User.decode(encode_raw(TarsDict({1: "AA12"})))
    assert ok.code == "AA12"


def test_string_pattern_validation_raises() -> None:
    """字符串不满足 pattern 约束时抛出 ValidationError."""

    class User(Struct):
        code: Annotated[str, Meta(tag=1, pattern=r"^[A-Z]{2}\d{2}$")]

    with pytest.raises(ValidationError, match="does not match pattern"):
        User.decode(encode_raw(TarsDict({1: "aa12"})))


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
        tinspect.type_info(complex)  # type: ignore[arg-type]


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
