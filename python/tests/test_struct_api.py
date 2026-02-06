"""测试 msgspec 风格 Struct API 扩展."""

from __future__ import annotations

import copy
import inspect
from typing import Annotated

import pytest
from tarsio import Struct, StructConfig


def test_struct_fields_in_tag_order() -> None:
    """__struct_fields__ 应按 tag 顺序排列."""

    class Sample(Struct):
        b: Annotated[int, 1]
        a: Annotated[str, 0]

    assert Sample.__struct_fields__ == ("a", "b")


def test_struct_config_exposed_on_class_and_instance() -> None:
    """__struct_config__ 应可通过类与实例访问."""

    class Configured(Struct, frozen=True, order=True, kw_only=True):
        a: Annotated[int, 0]

    conf = Configured.__struct_config__
    assert isinstance(conf, StructConfig)
    assert conf.frozen is True
    assert conf.order is True
    assert conf.kw_only is True

    obj = Configured(a=1)
    assert obj.__struct_config__ is conf


def test_order_comparisons_when_enabled() -> None:
    """order=True 时应生成比较方法."""

    class Point(Struct, order=True):
        x: Annotated[int, 0]
        y: Annotated[int, 1]

    assert Point(1, 2) < Point(2, 0)
    assert Point(1, 2) <= Point(1, 2)
    assert Point(2, 0) > Point(1, 2)


def test_order_comparisons_not_enabled_raise() -> None:
    """order=False 时比较应回退为 TypeError."""

    class Point(Struct):
        x: Annotated[int, 0]
        y: Annotated[int, 1]

    with pytest.raises(TypeError):
        _ = Point(1, 2) < Point(2, 0)  # type: ignore


def test_omit_defaults_skips_encoding() -> None:
    """omit_defaults=True 时应跳过默认值字段."""

    class Sample(Struct, omit_defaults=True):
        b: Annotated[int, 1]
        a: Annotated[int, 0] = 1

    data = Sample(b=2).encode()
    assert data.hex() == "1002"


def test_signature_tag_order_and_kwonly_handling() -> None:
    """__signature__ 应按 tag 顺序生成并在必要时切换为 kw-only."""

    class Sample(Struct):
        b: Annotated[int, 1]
        a: Annotated[int, 0] = 1

    sig = inspect.signature(Sample)
    params = list(sig.parameters.values())
    assert [p.name for p in params] == ["a", "b"]
    assert params[0].kind is inspect.Parameter.POSITIONAL_OR_KEYWORD
    assert params[1].kind is inspect.Parameter.KEYWORD_ONLY


def test_dataclass_transform_runtime_attribute_present() -> None:
    """StructMeta 应有 __dataclass_transform__ 运行时属性."""
    assert hasattr(type(Struct), "__dataclass_transform__")


def test_copy_returns_new_instance() -> None:
    """__copy__ 应返回新对象且字段一致."""

    class Sample(Struct):
        a: Annotated[int, 0]
        b: Annotated[str, 1]

    original = Sample(1, "x")
    cloned = copy.copy(original)
    assert cloned is not original
    assert cloned.a == original.a
    assert cloned.b == original.b
