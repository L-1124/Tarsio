"""测试 Schema/Meta/Inspect 相关行为."""

from typing import Annotated, Generic, Optional, TypeVar, cast

import pytest
from tarsio import inspect as tinspect
from tarsio._core import (
    NODEFAULT,
    Meta,
    Struct,
    TarsDict,
    ValidationError,
    encode_raw,
    field,
)


def test_classic_mode_allows_extra_non_meta_annotations() -> None:
    """经典模式允许额外的非 Meta 注解."""

    class User(Struct):
        name: Annotated[str, 2, "doc"]

    obj = User("a")
    assert obj.name == "a"


def test_mix_int_tag_and_meta_constraints_allowed() -> None:
    """整数 Tag 与 Meta 约束可组合使用."""

    class User(Struct):
        uid: Annotated[int, 1, Meta(gt=0)]

    data = encode_raw(TarsDict({1: 7}))
    obj = User.decode(data)
    assert obj.uid == 7


def test_meta_without_tag_is_allowed() -> None:
    """Meta 不再承载 tag，缺省由自动分配生效."""

    class User(Struct):
        name: Annotated[str, Meta(max_len=10)]

    info = tinspect.struct_info(User)
    assert info is not None
    assert info.fields[0].tag == 0


def test_annotated_without_explicit_tag_auto_assigns() -> None:
    """Annotated 字段缺省 tag 时应自动分配."""

    class User(Struct):
        uid: Annotated[int, "doc"]

    info = tinspect.struct_info(User)
    assert info is not None
    assert info.fields[0].tag == 0


def test_plain_annotations_auto_assign_tags() -> None:
    """全普通注解字段应按定义顺序自动分配 Tag."""

    class User(Struct):
        uid: int
        name: str

    info = tinspect.struct_info(User)
    assert info is not None
    assert [f.tag for f in info.fields] == [0, 1]
    assert [f.name for f in info.fields] == ["uid", "name"]


def test_plain_annotations_roundtrip_succeeds() -> None:
    """全普通注解字段应可正常编解码."""

    class User(Struct):
        uid: int
        name: str

    obj = User(uid=7, name="alice")
    restored = User.decode(obj.encode())
    assert restored.uid == 7
    assert restored.name == "alice"


def test_mix_plain_and_annotated_is_allowed() -> None:
    """普通注解与 Annotated 可混用并自动分配 tag."""

    class User(Struct):
        uid: int
        name: Annotated[str, Meta(min_len=1)]

    info = tinspect.struct_info(User)
    assert info is not None
    assert [f.tag for f in info.fields] == [0, 1]


def test_field_tag_supports_explicit_and_implicit_mix() -> None:
    """field(tag=...) 与自动分配可混合使用."""

    class User(Struct):
        a: int = field(tag=0)
        b: Annotated[int, Meta(gt=0)]
        c: str = field(tag=5)
        d: bytes

    info = tinspect.struct_info(User)
    assert info is not None
    assert [(f.name, f.tag) for f in info.fields] == [
        ("a", 0),
        ("b", 1),
        ("c", 5),
        ("d", 6),
    ]


def test_multiple_integer_tags_raises() -> None:
    """同一字段声明多个整数 Tag 时抛出 TypeError."""
    with pytest.raises(TypeError, match="Multiple integer tags"):

        class User(Struct):
            uid: Annotated[int, 1, 2]


def test_multiple_meta_objects_raises() -> None:
    """同一字段声明多个 Meta 时抛出 TypeError."""
    with pytest.raises(TypeError, match="Multiple Meta objects"):

        class User(Struct):
            uid: Annotated[int, Meta(gt=0), Meta(le=10)]


def test_duplicate_tag_raises() -> None:
    """两个字段重复使用同一 Tag 时抛出 TypeError."""
    with pytest.raises(TypeError, match="Duplicate tag"):

        class User(Struct):
            a: Annotated[int, 1]
            b: Annotated[int, 1]


def test_tag_upper_limit_raises_error() -> None:
    """Tag 超过上限 255 时抛出 ValueError / TypeError."""
    # 根据目前的具体实现可能抛出 TypeError (Schema构建时) 或者是 ValueError
    with pytest.raises((TypeError, ValueError)):

        class User(Struct):
            a: Annotated[int, 256]


def test_tag_negative_raises_error() -> None:
    """负数 Tag 时抛出 ValueError / TypeError."""
    with pytest.raises((TypeError, ValueError)):

        class User(Struct):
            a: Annotated[int, -1]


def test_meta_constraints_with_field_tag_is_allowed() -> None:
    """Meta 约束与 field(tag=...) 组合应允许正常解码."""

    class User(Struct):
        uid: Annotated[int, Meta(ge=0)] = field(tag=1)

    data = encode_raw(TarsDict({1: 123}))
    obj = User.decode(data)
    assert obj.uid == 123


def test_numeric_gt_validation_raises() -> None:
    """数值 gt 约束不满足时抛出 ValidationError."""

    class User(Struct):
        uid: Annotated[int, Meta(gt=0)] = field(tag=1)

    data = encode_raw(TarsDict({1: 0}))
    with pytest.raises(ValidationError, match="must be >"):
        User.decode(data)


@pytest.mark.parametrize("value", [0, 10], ids=["lower", "upper"])
def test_numeric_ge_le_validation_passes(value: int) -> None:
    """数值区间约束满足时正常解码."""

    class User(Struct):
        uid: Annotated[int, Meta(ge=0, le=10)] = field(tag=1)

    ok = User.decode(encode_raw(TarsDict({1: value})))
    assert ok.uid == value


def test_numeric_ge_validation_raises() -> None:
    """数值 ge 约束不满足时抛出 ValidationError."""

    class User(Struct):
        uid: Annotated[int, Meta(ge=1)] = field(tag=1)

    with pytest.raises(ValidationError, match="must be >="):
        User.decode(encode_raw(TarsDict({1: 0})))


def test_string_min_len_validation_raises() -> None:
    """字符串 min_len 约束不满足时抛出 ValidationError."""

    class User(Struct):
        name: Annotated[str, Meta(min_len=1, max_len=3)] = field(tag=1)

    with pytest.raises(ValidationError, match="length must be >="):
        User.decode(encode_raw(TarsDict({1: ""})))


def test_string_max_len_validation_raises() -> None:
    """字符串 max_len 约束不满足时抛出 ValidationError."""

    class User(Struct):
        name: Annotated[str, Meta(min_len=1, max_len=3)] = field(tag=1)

    with pytest.raises(ValidationError, match="length must be <="):
        User.decode(encode_raw(TarsDict({1: "abcd"})))


def test_string_pattern_validation_passes() -> None:
    """字符串满足 pattern 约束时正常解码."""

    class User(Struct):
        code: Annotated[str, Meta(pattern=r"^[A-Z]{2}\d{2}$")] = field(tag=1)

    ok = User.decode(encode_raw(TarsDict({1: "AA12"})))
    assert ok.code == "AA12"


def test_string_pattern_validation_raises() -> None:
    """字符串不满足 pattern 约束时抛出 ValidationError."""

    class User(Struct):
        code: Annotated[str, Meta(pattern=r"^[A-Z]{2}\d{2}$")] = field(tag=1)

    with pytest.raises(ValidationError, match="does not match pattern"):
        User.decode(encode_raw(TarsDict({1: "aa12"})))


def test_tuple_length_constraints_validation_raises() -> None:
    """Tuple 字段应应用 min_len/max_len 约束."""

    class User(Struct):
        data: Annotated[tuple[int, int], Meta(min_len=3)] = field(tag=1)

    with pytest.raises(ValidationError, match="length must be >="):
        User.decode(encode_raw(TarsDict({1: (1, 2)})))


def test_incompatible_constraints_type_raises() -> None:
    """约束与值类型不兼容时应抛出 ValidationError."""

    class User(Struct):
        uid: Annotated[int, Meta(min_len=1)] = field(tag=1)

    with pytest.raises(ValidationError, match="must have length"):
        User.decode(encode_raw(TarsDict({1: 10})))


def test_init_decode_constraint_error_type_consistent() -> None:
    """同一约束在构造与解码阶段应保持同类异常."""

    class User(Struct):
        uid: Annotated[int, Meta(gt=0)] = field(tag=1)

    with pytest.raises(ValidationError, match="must be >"):
        User(uid=0)

    with pytest.raises(ValidationError, match="must be >"):
        User.decode(encode_raw(TarsDict({1: 0})))


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


def test_struct_info_generic_template_returns_fields() -> None:
    """未具体化泛型模板的 struct_info 应返回字段信息."""
    t_type = TypeVar("t_type")

    class Box(Struct, Generic[t_type]):
        value: Annotated[t_type, 0]

    info = tinspect.struct_info(Box)
    assert info is not None
    assert info.fields[0].name == "value"


def test_struct_info_generic_template_and_concrete_diff() -> None:
    """模板与具体化泛型的字段类型应反映差异."""
    t_type = TypeVar("t_type")

    class Box(Struct, Generic[t_type]):
        value: Annotated[t_type, 0]

    template_info = tinspect.struct_info(Box)
    assert template_info is not None
    assert template_info.fields[0].type.kind == "any"

    concrete_info = tinspect.struct_info(Box[int])
    assert concrete_info is not None
    assert concrete_info.fields[0].type.kind == "int"


def test_struct_info_meta_constraints() -> None:
    """Meta 约束应被解析到 constraints."""

    class Sample(Struct):
        a: Annotated[int, Meta(gt=0)] = field(tag=0)

    info = tinspect.struct_info(Sample)
    assert info is not None
    field_info = info.fields[0]
    int_type = cast(tinspect.IntType, field_info.type)
    assert int_type.gt == 0


def test_fieldinfo_alias_to_field() -> None:
    """FieldInfo 应作为 Field 的兼容别名可用."""

    class Sample(Struct):
        a: Annotated[int, 0]

    info = tinspect.struct_info(Sample)
    assert info is not None
    field = info.fields[0]
    assert isinstance(field, tinspect.Field)
    assert tinspect.FieldInfo is tinspect.Field


def test_struct_info_field_default_factory_visible() -> None:
    """struct_info 字段应暴露 default_factory 并与 default 区分."""

    class Sample(Struct):
        a: Annotated[list[int], 0] = field(default_factory=list)

    info = tinspect.struct_info(Sample)
    assert info is not None
    f = info.fields[0]
    assert f.has_default is True
    assert f.default is NODEFAULT
    assert f.default_factory is list


def test_struct_info_annotated_without_tag_auto_assigns() -> None:
    """Annotated 字段未显式指定 tag 时应自动分配."""

    class Sample(Struct):
        a: Annotated[int, "doc"]  # type: ignore

    info = tinspect.struct_info(Sample)
    assert info is not None
    assert info.fields[0].tag == 0


def test_struct_info_duplicate_tag_raises() -> None:
    """重复 Tag 应抛 TypeError."""
    with pytest.raises(TypeError, match="Duplicate tag"):

        class Sample(Struct):
            a: Annotated[int, 1]
            b: Annotated[int, 1]


def test_struct_info_mix_annotated_tag_and_field_tag_raises() -> None:
    """Annotated 整数 tag 与 field(tag=...) 同时设置应抛 TypeError."""
    with pytest.raises(
        TypeError,
        match="cannot mix Annotated integer tag with field\\(tag=\\.\\.\\.\\)",
    ):

        class Sample(Struct):
            a: Annotated[int, 1] = field(tag=1)
