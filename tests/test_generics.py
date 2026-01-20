"""JCE 泛型支持测试."""

from typing import Generic, TypeVar

import pytest

from jce import JceField, JceStruct, dumps, loads

T = TypeVar("T")
K = TypeVar("K")
V = TypeVar("V")


class Box(JceStruct, Generic[T]):
    """基础泛型容器."""

    value: T = JceField(jce_id=0)


class Pair(JceStruct, Generic[K, V]):
    """双类型泛型容器."""

    key: K = JceField(jce_id=0)
    value: V = JceField(jce_id=1)


class User(JceStruct):
    """测试用的普通结构体."""

    uid: int = JceField(jce_id=0)
    name: str = JceField(jce_id=1)


def test_generic_primitive():
    """Box[int] 应该能正确处理基础类型的泛型参数."""
    # 1. Box[int]
    box = Box[int](value=100)
    data = dumps(box)

    # 验证二进制 (Tag 0, Int 100)
    assert data.hex().upper() == "0064"

    # 反序列化
    restored = loads(data, Box[int])
    assert restored.value == 100
    assert isinstance(restored.value, int)


def test_generic_struct():
    """Box[User] 应该能正确处理嵌套结构体的泛型参数."""
    user = User(uid=1, name="A")
    box = Box[User](value=user)

    data = dumps(box)

    # 反序列化
    restored = loads(data, Box[User])
    assert restored.value.uid == 1
    assert restored.value.name == "A"
    assert isinstance(restored.value, User)


def test_generic_multi_type():
    """Pair[int, str] 应该能处理多个泛型参数."""
    pair = Pair[int, str](key=1, value="test")
    data = dumps(pair)

    restored = loads(data, Pair[int, str])
    assert restored.key == 1
    assert restored.value == "test"


def test_generic_inheritance():
    """继承自泛型类的具体子类应该能正确解析类型."""

    class IntBox(Box[int]):
        pass

    box = IntBox(value=999)
    data = dumps(box)

    restored = loads(data, IntBox)
    assert restored.value == 999

    # 尝试用 IntBox 解析错误类型数据 (比如 String)
    # 构造 Tag 0 = String "abc" -> 06 03 616263
    bad_data = bytes.fromhex("0603616263")

    from pydantic import ValidationError

    # 因为 IntBox 明确 value 是 int，这里应该报错或强转失败
    with pytest.raises(ValidationError):
        loads(bad_data, IntBox)


def test_nested_generics():
    """Box[Box[int]] 应该能处理多层嵌套泛型."""
    inner = Box[int](value=42)
    outer = Box[Box[int]](value=inner)

    data = dumps(outer)

    # 反序列化
    restored = loads(data, Box[Box[int]])
    assert restored.value.value == 42
    assert isinstance(restored.value, Box)


def test_generic_list_field():
    """Box[list[int]] 应该能处理泛型列表字段."""
    box = Box[list[int]](value=[1, 2, 3])
    data = dumps(box)

    restored = loads(data, Box[list[int]])
    assert restored.value == [1, 2, 3]
