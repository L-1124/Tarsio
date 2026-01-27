"""测试 JCE 类型适配器."""

import pytest
from tarsio import Field, Struct, StructDict, dumps
from tarsio.adapter import TarsTypeAdapter


class User(Struct):
    """用户信息."""

    uid: int = Field(id=0)
    name: str = Field(id=1)


def test_adapter_struct_knowledge() -> None:
    """TarsTypeAdapter 应能自动识别 Struct 中的 Tag ID."""
    adapter = TarsTypeAdapter(User)
    user = User(uid=100, name="Alice")
    data = adapter.dump_tars(user)

    user_restored = adapter.validate_tars(data)

    assert user_restored.uid == 100
    assert user_restored.name == "Alice"


def test_adapter_primitive_default_tag() -> None:
    """TarsTypeAdapter 对于基础类型默认使用 Tag 0."""
    adapter = TarsTypeAdapter(int)
    data = adapter.dump_tars(123)

    val = adapter.validate_tars(data)

    assert val == 123


def test_adapter_primitive_custom_tag() -> None:
    """TarsTypeAdapter 应支持指定非 0 的 Tag."""
    adapter = TarsTypeAdapter(int)
    data = dumps(StructDict({5: 999}))

    with pytest.raises(ValueError, match="No data found at id 0"):
        adapter.validate_tars(data)

    val = adapter.validate_tars(data, id=5)

    assert val == 999


def test_adapter_generic_list() -> None:
    """TarsTypeAdapter 应支持泛型 list[int]."""
    adapter = TarsTypeAdapter(list[int])
    input_list = [1, 2, 3]
    data = adapter.dump_tars(input_list)

    restored = adapter.validate_tars(data)

    assert restored == input_list


def test_adapter_list_of_structs() -> None:
    """TarsTypeAdapter 应支持结构体列表 list[User]."""
    adapter = TarsTypeAdapter(list[User])
    users = [User(uid=1, name="A"), User(uid=2, name="B")]
    data = adapter.dump_tars(users)

    restored = adapter.validate_tars(data)

    assert len(restored) == 2
    assert restored[0].name == "A"
    assert restored[1].uid == 2


def test_adapter_generic_dict() -> None:
    """TarsTypeAdapter 应支持泛型 dict[int, str] (JCE Map)."""
    adapter = TarsTypeAdapter(dict[int, str])
    input_map = {100: "ok", 404: "not found"}
    data = adapter.dump_tars(input_map)

    restored = adapter.validate_tars(data)

    assert restored == input_map


def test_adapter_not_struct_type() -> None:
    """TarsTypeAdapter 应能正确区分 Struct 和非 Struct 类型."""
    adapter = TarsTypeAdapter(list[int])
    assert adapter._is_struct is False

    adapter2 = TarsTypeAdapter(User)
    assert adapter2._is_struct is True
