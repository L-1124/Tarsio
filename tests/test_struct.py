"""JCE 结构体功能测试.

覆盖 jce.struct 模块的核心特性:
1. JceStruct 定义与字段配置 (default, default_factory)
2. 显式反序列化 (model_validate_jce)
3. 自动解包机制 (_auto_unpack_bytes_field)
4. 对象方法 (encode, decode)
"""

import pytest
from pydantic import ValidationError

from jce import JceDict, JceField, JceStruct

# --- 辅助模型 ---


class SimpleUser(JceStruct):
    """基础测试用户结构体."""

    uid: int = JceField(jce_id=0)
    name: str = JceField(jce_id=1, default="unknown")


class FactoryUser(JceStruct):
    """测试 default_factory 的结构体."""

    tags: list[str] = JceField(jce_id=0, default_factory=list)
    scores: list[int] = JceField(jce_id=1, default_factory=lambda: [100])


class NestedUser(JceStruct):
    """测试嵌套的结构体."""

    info: SimpleUser = JceField(jce_id=0)


class BlobContainer(JceStruct):
    """测试自动解包: 字段声明为 Struct, 但实际数据可能是 bytes."""

    inner: SimpleUser = JceField(jce_id=0)


# --- 测试用例 ---


def test_struct_defaults():
    """JceStruct 应正确处理字段默认值和 factory."""
    # 标准 Pydantic 用法: 关键字参数
    u1 = SimpleUser(uid=1)
    assert u1.uid == 1
    assert u1.name == "unknown"

    u2 = FactoryUser()
    assert u2.tags == []
    assert u2.scores == [100]


def test_struct_validate_from_bytes():
    """model_validate_jce() 应支持从 bytes 直接反序列化."""
    # SimpleUser(uid=100, name="test") -> 00 64 ...
    raw_bytes = bytes.fromhex("0064160474657374")

    u = SimpleUser.model_validate_jce(raw_bytes)

    assert u.uid == 100
    assert u.name == "test"


def test_struct_validate_from_tag_dict():
    """model_validate_jce() 应支持从 JceDict (Tag-Map) 反序列化."""
    # 模拟 JCE 解码器的中间产物: JceDict({0: 100, ...})
    raw_dict = JceDict({0: 100, 1: "test"})

    u = SimpleUser.model_validate_jce(raw_dict)

    assert u.uid == 100
    assert u.name == "test"


def test_auto_unpack_nested_bytes():
    """当字段定义为 Struct 但数据是 bytes 时应自动尝试解包."""
    inner_bytes = bytes.fromhex("0063")  # uid=99

    # 构造包含 bytes 的 Tag-Dict
    raw_data = JceDict({0: inner_bytes})

    # 通过 model_validate_jce 解析
    container = BlobContainer.model_validate_jce(raw_data)

    assert isinstance(container.inner, SimpleUser)
    assert container.inner.uid == 99


def test_struct_methods():
    """model_dump_jce() 和 model_validate_jce() 应该互为逆操作."""
    u = SimpleUser(uid=123)
    data = u.model_dump_jce()

    # 验证新旧方法是等价的
    u2 = SimpleUser.model_validate_jce(data)

    assert u2 == u
    assert u.model_dump_jce() == data


def test_from_bytes_shortcut():
    """from_bytes() 应该返回解码后的字典和消耗长度."""
    data = bytes.fromhex("0001")
    res_dict, _ = SimpleUser.from_bytes(data)

    assert isinstance(res_dict, JceDict)
    assert res_dict == {0: 1}


def test_validation_error():
    """缺失必填字段时应抛出 ValidationError."""
    with pytest.raises(ValidationError):
        SimpleUser()  # type: ignore[call-arg]


def test_jcedict_key_validation_init():
    """JceDict 初始化时应拒绝非 int 类型的键."""
    # 正常情况: 所有键都是 int
    d1 = JceDict({0: 100, 1: "test"})
    assert d1[0] == 100
    assert d1[1] == "test"

    # 异常情况: 包含非 int 键
    with pytest.raises(TypeError, match="keys must be int"):
        JceDict({"name": "value"})

    with pytest.raises(TypeError, match="keys must be int"):
        JceDict({0: 100, "key": "value"})


def test_jcedict_key_validation_setitem():
    """JceDict 赋值时应拒绝非 int 类型的键."""
    d = JceDict()

    # 正常: int 键
    d[0] = 100
    assert d[0] == 100

    # 异常: str 键
    with pytest.raises(TypeError, match="keys must be int"):
        d["name"] = "value"  # type: ignore[arg]

    # 异常: float 键
    with pytest.raises(TypeError, match="keys must be int"):
        d[1.5] = "value"  # type: ignore[arg]
