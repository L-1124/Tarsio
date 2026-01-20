"""JCE 类型适配器测试.

覆盖 jce.adapter 模块的核心特性:
1. 结构体适配 (自动识别 Tag)
2. 基础类型适配 (默认 Tag 0 vs 显式指定 Tag)
3. 泛型容器适配 (List, Dict)
4. 异常处理 (Tag 不存在)
"""

import pytest

from jce import JceDict, JceField, JceStruct, dumps
from jce.adapter import JceTypeAdapter

# --- 辅助结构体 ---


class User(JceStruct):
    """用户信息."""

    uid: int = JceField(jce_id=0)
    name: str = JceField(jce_id=1)


# --- 测试用例 ---


def test_adapter_struct_knowledge():
    """JceTypeAdapter 应该能自动识别 Struct 中的 Tag ID."""
    # 结构体定义里包含了 jce_id=0/1，所以不需要在 validate 时指定
    adapter = JceTypeAdapter(User)

    user = User(uid=100, name="Alice")
    data = adapter.dump_jce(user)

    # 只需要传入 bytes，不需要 jce_id 参数
    user_restored = adapter.validate_jce(data)
    assert user_restored.uid == 100
    assert user_restored.name == "Alice"


def test_adapter_primitive_default_tag():
    """JceTypeAdapter 对于基础类型默认使用 Tag 0."""
    adapter = JceTypeAdapter(int)

    # 1. 序列化: dump_jce 对于基础类型，默认使用 Tag 0 包装
    # 等价于 dumps({0: 123})
    data = adapter.dump_jce(123)

    # 2. 反序列化: 默认去 Tag 0 找
    val = adapter.validate_jce(data)
    assert val == 123


def test_adapter_primitive_custom_tag():
    """JceTypeAdapter 应该支持指定非 0 Tag."""
    adapter = JceTypeAdapter(int)

    # 模拟场景: 服务器返回的数据里，整数放在了 Tag 5
    # 这样 encoded 就是 05 ... (Tag 5 INT)，而不是 Map
    data = dumps(JceDict({5: 999}))

    # 1. 如果不指定 jce_id (默认找 Tag 0)，应该报错
    with pytest.raises(ValueError, match="No data found at jce_id 0"):
        adapter.validate_jce(data)

    # 2. 显式告诉它去 Tag 5 找
    val = adapter.validate_jce(data, jce_id=5)
    assert val == 999


def test_adapter_generic_list():
    """JceTypeAdapter 应该支持泛型 List[int]."""
    adapter = JceTypeAdapter(list[int])

    input_list = [1, 2, 3]
    # 默认 dump 到 Tag 0
    data = adapter.dump_jce(input_list)

    restored = adapter.validate_jce(data)
    assert restored == input_list


def test_adapter_list_of_structs():
    """JceTypeAdapter 应该支持结构体列表 List[User]."""
    # 这是一个非常强大的功能: 自动处理 List 中嵌套的 Struct
    adapter = JceTypeAdapter(list[User])

    users = [User(uid=1, name="A"), User(uid=2, name="B")]
    data = adapter.dump_jce(users)

    restored = adapter.validate_jce(data)
    assert len(restored) == 2
    assert restored[0].name == "A"
    assert restored[1].uid == 2


def test_adapter_generic_dict():
    """JceTypeAdapter 应该支持泛型 Dict[int, str] (JCE Map)."""
    adapter = JceTypeAdapter(dict[int, str])

    input_map = {100: "ok", 404: "not found"}
    data = adapter.dump_jce(input_map)

    restored = adapter.validate_jce(data)
    assert restored == input_map


def test_adapter_not_struct_type():
    """JceTypeAdapter 应该能正确区分 Struct 和非 Struct 类型."""
    # List[int] 不是 JceStruct 的子类，_is_struct 应该为 False
    adapter = JceTypeAdapter(list[int])
    assert adapter._is_struct is False

    # User 是 JceStruct 的子类
    adapter2 = JceTypeAdapter(User)
    assert adapter2._is_struct is True
