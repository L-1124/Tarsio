from typing import Annotated, Any

from tarsio._core import (
    Struct,
    TarsDict,
    decode,
    decode_raw,
    encode,
    encode_raw,
    probe_struct,
)
from typing_extensions import assert_type


class User(Struct):
    """用户结构体示例."""

    uid: Annotated[int, 0]
    name: Annotated[str, 1]


def test_type_decode() -> None:
    """验证 decode 与 Struct.decode 的返回类型推导."""
    # 使用有效的编码数据进行 User 结构体测试

    user_obj = User(uid=123, name="test_user")

    data = encode(user_obj)

    # 测试顶层 decode 函数

    user = decode(User, data)

    assert_type(user, User)

    # 测试 Struct.decode 类方法

    user2 = User.decode(data)

    assert_type(user2, User)


def test_type_encode() -> None:
    """验证 encode 与 Struct.encode 的返回类型推导."""
    user = User(uid=1, name="test")

    # 测试顶层 encode 函数

    data = encode(user)

    assert_type(data, bytes)

    # 测试实例的 encode 方法

    data2 = user.encode()

    assert_type(data2, bytes)


def test_type_raw() -> None:
    """验证原始编解码接口的 TarsDict 类型一致性."""
    raw_data: TarsDict = TarsDict({0: 123, 1: "hello"})

    # 编码为字节

    encoded = encode_raw(raw_data)

    assert_type(encoded, bytes)

    # 解码回 TarsDict

    decoded = decode_raw(encoded)

    assert_type(decoded, TarsDict)

    decoded_auto = decode_raw(encoded, auto_simplelist=True)

    assert_type(decoded_auto, TarsDict)


def test_type_tars_dict_usage() -> None:
    """验证 TarsDict 的基本使用类型."""
    d = TarsDict({0: 1, 1: "s"})

    assert_type(d, TarsDict)

    assert_type(d[0], Any)


def test_type_containers() -> None:
    """验证容器类型的编解码类型推导."""
    # 列表测试 (Raw 模式)

    lst = [1, 2, 3]

    enc_lst = encode_raw(lst)

    assert_type(enc_lst, bytes)

    # 字典 (Map) 测试 (Raw 模式)

    mp = {"a": 1}

    enc_mp = encode_raw(mp)

    assert_type(enc_mp, bytes)


def test_type_probe_struct() -> None:
    """验证 probe_struct 返回类型."""
    data = b"..."

    res = probe_struct(data)

    # probe_struct 返回 TarsDict | None

    assert_type(res, TarsDict | None)
