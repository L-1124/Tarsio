"""测试 Meta 模式的运行时校验(反序列化路径)."""

from typing import Annotated

import pytest
from tarsio import Meta, Struct, ValidationError, encode_raw


def test_meta_tag_only_is_allowed() -> None:
    """Meta 仅提供 tag 时允许正常解码."""

    class User(Struct):
        uid: Annotated[int, Meta(tag=1)]

    data = encode_raw({1: 123})
    obj = User.decode(data)
    assert obj.uid == 123


def test_numeric_gt_validation_raises() -> None:
    """数值 gt 约束不满足时抛出 ValidationError."""

    class User(Struct):
        uid: Annotated[int, Meta(tag=1, gt=0)]

    data = encode_raw({1: 0})
    with pytest.raises(ValidationError, match="must be >"):
        User.decode(data)


@pytest.mark.parametrize("value", [0, 10], ids=["lower", "upper"])
def test_numeric_ge_le_validation_passes(value: int) -> None:
    """数值区间约束满足时正常解码."""

    class User(Struct):
        uid: Annotated[int, Meta(tag=1, ge=0, le=10)]

    ok = User.decode(encode_raw({1: value}))
    assert ok.uid == value


def test_numeric_ge_validation_raises() -> None:
    """数值 ge 约束不满足时抛出 ValidationError."""

    class User(Struct):
        uid: Annotated[int, Meta(tag=1, ge=1)]

    with pytest.raises(ValidationError, match="must be >="):
        User.decode(encode_raw({1: 0}))


def test_string_min_len_validation_raises() -> None:
    """字符串 min_len 约束不满足时抛出 ValidationError."""

    class User(Struct):
        name: Annotated[str, Meta(tag=1, min_len=1, max_len=3)]

    with pytest.raises(ValidationError, match="length must be >="):
        User.decode(encode_raw({1: ""}))


def test_string_max_len_validation_raises() -> None:
    """字符串 max_len 约束不满足时抛出 ValidationError."""

    class User(Struct):
        name: Annotated[str, Meta(tag=1, min_len=1, max_len=3)]

    with pytest.raises(ValidationError, match="length must be <="):
        User.decode(encode_raw({1: "abcd"}))


def test_string_pattern_validation_passes() -> None:
    """字符串满足 pattern 约束时正常解码."""

    class User(Struct):
        code: Annotated[str, Meta(tag=1, pattern=r"^[A-Z]{2}\d{2}$")]

    ok = User.decode(encode_raw({1: "AA12"}))
    assert ok.code == "AA12"


def test_string_pattern_validation_raises() -> None:
    """字符串不满足 pattern 约束时抛出 ValidationError."""

    class User(Struct):
        code: Annotated[str, Meta(tag=1, pattern=r"^[A-Z]{2}\d{2}$")]

    with pytest.raises(ValidationError, match="does not match pattern"):
        User.decode(encode_raw({1: "aa12"}))
