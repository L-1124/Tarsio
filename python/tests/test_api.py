"""测试 JCE API 层."""

import io
from collections.abc import Callable
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from jce.api import BytesMode

import pytest
from jce import (
    JceDecodeError,
    JceDict,
    JceField,
    JceOption,
    JceStruct,
    dump,
    dumps,
    load,
    loads,
)


class SimpleUser(JceStruct):
    """测试用的 JCE Struct."""

    uid: int = JceField(jce_id=0)
    name: str = JceField(jce_id=1, default="unknown")


def test_dumps_basic() -> None:
    """dumps() 应能正确序列化基本的 Struct 对象."""
    u = SimpleUser(uid=100, name="test")

    data = dumps(u)

    assert data == bytes.fromhex("0064160474657374")


def test_loads_basic() -> None:
    """loads() 应能正确反序列化基本的 Struct 对象."""
    data = bytes.fromhex("0064160474657374")

    u = loads(data, target=SimpleUser)

    assert u.uid == 100
    assert u.name == "test"


def test_loads_map_behavior() -> None:
    """loads() 默认应将 Map 数据解析为 JceDict."""
    data = {1: "a", 2: "b"}
    encoded = dumps(data)

    decoded = loads(encoded)

    assert isinstance(decoded, JceDict)
    assert decoded[0] == data
    assert isinstance(decoded[0][1], str)


def test_jce_dict_struct_behavior() -> None:
    """JceDict 对象应被序列化为 Struct 格式而非 Map."""
    data = JceDict({0: 100})

    encoded = dumps(data)

    assert encoded == b"\x00\x64"

    encoded_map = dumps({0: 100})
    assert encoded_map != b"\x00\x64"
    assert encoded_map[0] == 0x08


def test_round_trip_basic() -> None:
    """dumps() 和 loads() 应能完成基本的 dict 数据往返."""
    data = {1: "a", 2: "b"}
    encoded = dumps(data)

    decoded = loads(encoded)

    assert decoded[0] == data


def test_file_io_round_trip() -> None:
    """dump() 和 load() 应能通过文件对象进行读写."""
    data = {1: 100, 2: "test"}
    f = io.BytesIO()

    dump(data, f)
    f.seek(0)
    loaded_data = load(f)

    assert isinstance(loaded_data, JceDict)
    assert loaded_data[0] == data
    assert loaded_data[0][2] == "test"


def test_file_io_load_target_dict() -> None:
    """load(target=dict) 应返回普通 dict 类型."""
    data = JceDict({0: 100, 1: "abc"})
    f = io.BytesIO()
    dump(data, f)
    f.seek(0)

    loaded_data = load(f, target=dict)

    assert isinstance(loaded_data, dict)
    assert not isinstance(loaded_data, JceDict)
    assert loaded_data[0] == 100
    assert loaded_data[1] == "abc"


def test_auto_bytes_conversion() -> None:
    """loads() 在默认 Auto 模式下应自动将 bytes 转换为 str."""
    data = {1: "hello"}
    encoded = dumps(data)

    decoded = loads(encoded)

    assert isinstance(decoded[0][1], str)
    assert decoded[0][1] == "hello"


def test_loads_returns_jcedict() -> None:
    """loads() 默认应返回 JceDict 类型."""
    struct_data = JceDict({0: 100, 1: "test"})
    encoded = dumps(struct_data)

    decoded = loads(encoded)

    assert isinstance(decoded, JceDict)
    assert decoded[0] == 100
    assert decoded[1] == "test"


def test_loads_target_dict_returns_dict() -> None:
    """loads(target=dict) 应返回普通 dict 类型."""
    struct_data = JceDict({0: 100})
    encoded = dumps(struct_data)

    decoded = loads(encoded, target=dict)

    assert isinstance(decoded, dict)
    assert not isinstance(decoded, JceDict)
    assert decoded[0] == 100


def test_jcedict_vs_dict_encoding_difference() -> None:
    """JceDict 和 dict 应有不同的编码表现 (Struct vs Map)."""
    jce_data = JceDict({0: 100})
    jce_encoded = dumps(jce_data)
    assert jce_encoded == b"\x00\x64"

    dict_data = {0: 100}
    dict_encoded = dumps(dict_data)
    assert dict_encoded[0] == 0x08
    assert len(dict_encoded) > len(jce_encoded)


def test_jcedict_as_nested_struct() -> None:
    """JceDict 应支持嵌套使用."""
    outer = JceDict({0: JceDict({1: "inner"})})
    encoded = dumps(outer)

    decoded = loads(encoded)

    assert isinstance(decoded, JceDict)
    # 嵌套的结构体现在是普通的 dict (性能优化: Rust 直接返回 dict)
    assert isinstance(decoded[0], dict)
    assert decoded[0][1] == "inner"


BYTES_MODE_CASES = [
    ({1: b"test"}, "raw", bytes, b"test", "raw模式保持原始字节"),
    ({1: b"test"}, "string", str, "test", "string模式转换为字符串"),
    ({1: b"\xff\xfe"}, "string", bytes, b"\xff\xfe", "string模式无效UTF-8保持原样"),
    ({1: b"hello world"}, "auto", str, "hello world", "auto模式识别文本"),
    (
        {1: b"\x00\x01\x02\x03"},
        "auto",
        bytes,
        b"\x00\x01\x02\x03",
        "auto模式保留二进制",
    ),
    ({1: b""}, "auto", str, "", "空字节转为空字符串"),
]


@pytest.mark.parametrize(
    ("data", "mode", "expected_type", "expected_value", "desc"),
    BYTES_MODE_CASES,
    ids=[c[4] for c in BYTES_MODE_CASES],
)
def test_convert_bytes_mode(
    data: dict[int, bytes],
    mode: "BytesMode",
    expected_type: type,
    expected_value: Any,
    desc: str,
) -> None:
    """loads() 应根据 bytes_mode 参数正确转换字节数据."""
    encoded = dumps(data)

    decoded = loads(encoded, bytes_mode=mode)

    assert isinstance(decoded[0][1], expected_type), f"失败: {desc}"
    assert decoded[0][1] == expected_value, f"失败: {desc}"


def test_convert_bytes_in_list() -> None:
    """loads() 应递归转换列表中的字节数据."""
    data = {1: [b"item1", b"item2"]}
    encoded = dumps(data)

    decoded = loads(encoded, bytes_mode="auto")

    assert isinstance(decoded[0][1], list)
    assert decoded[0][1][0] == "item1"
    assert decoded[0][1][1] == "item2"


def test_convert_bytes_dict_key() -> None:
    """loads() 应转换字典键中的字节数据."""
    inner_dict = {b"key": "value"}
    data = {1: inner_dict}
    encoded = dumps(data)

    decoded = loads(encoded, bytes_mode="auto")

    assert "key" in decoded[0][1]
    assert decoded[0][1]["key"] == "value"


JCEDICT_TO_DICT_CASES: list[tuple[Any, Callable[[Any], bool], str]] = [
    (
        JceDict({0: JceDict({1: "inner"})}),
        lambda r: (
            isinstance(r, dict)
            and not isinstance(r, JceDict)
            and isinstance(r[0], dict)
            and not isinstance(r[0], JceDict)
            and r[0][1] == "inner"
        ),
        "嵌套JceDict转换",
    ),
    (
        JceDict({0: [JceDict({1: "item"})]}),
        lambda r: (
            isinstance(r[0], list)
            and isinstance(r[0][0], dict)
            and not isinstance(r[0][0], JceDict)
        ),
        "列表中的JceDict转换",
    ),
    (
        (JceDict({0: "test"}),),
        lambda r: (
            isinstance(r, tuple)
            and isinstance(r[0], dict)
            and not isinstance(r[0], JceDict)
        ),
        "元组中的JceDict转换",
    ),
]


def test_dumps_with_option() -> None:
    """dumps() 应支持传入 option 参数控制编码行为."""
    data = {1: 100}

    encoded_le = dumps(data, option=JceOption.LITTLE_ENDIAN)
    encoded_be = dumps(data)

    assert isinstance(encoded_le, bytes)
    assert isinstance(encoded_be, bytes)


def test_dumps_with_exclude_unset() -> None:
    """dumps(exclude_unset=True) 应排除未设置的字段."""
    user = SimpleUser(uid=100)

    encoded_all = dumps(user, exclude_unset=False)
    encoded_unset = dumps(user, exclude_unset=True)

    assert len(encoded_unset) < len(encoded_all)


def test_dumps_with_context() -> None:
    """dumps() 应能将 context 传递给序列化过程."""

    class ContextUser(JceStruct):
        uid: int = JceField(jce_id=0)
        name: str = JceField(jce_id=1)

    user = ContextUser(uid=1, name="test")
    context = {"version": "1.0"}

    encoded = dumps(user, context=context)

    assert isinstance(encoded, bytes)


def test_dump_and_load_with_bytesio() -> None:
    """dump() 和 load() 应支持 BytesIO 对象."""
    user = SimpleUser(uid=200, name="file_test")
    buffer = io.BytesIO()

    dump(user, buffer)
    buffer.seek(0)
    loaded = load(buffer, target=SimpleUser)

    assert loaded.uid == 200
    assert loaded.name == "file_test"


def test_dump_with_options() -> None:
    """dump() 应支持所有序列化选项参数."""
    user = SimpleUser(uid=300, name="option_test")
    buffer = io.BytesIO()

    dump(
        user,
        buffer,
        option=JceOption.LITTLE_ENDIAN,
        exclude_unset=False,
        context={"key": "value"},
    )

    assert buffer.tell() > 0


def test_load_with_bytes_mode() -> None:
    """load() 应支持 bytes_mode 参数."""
    data = {1: b"binary_data"}
    buffer = io.BytesIO()
    dump(data, buffer)

    buffer.seek(0)
    loaded_raw = load(buffer, bytes_mode="raw")
    assert isinstance(loaded_raw[0][1], bytes)

    buffer.seek(0)
    loaded_str = load(buffer, bytes_mode="string")
    assert isinstance(loaded_str[0][1], str)


def test_load_with_context() -> None:
    """load() 应能将 context 传递给反序列化过程."""

    class ContextStruct(JceStruct):
        value: int = JceField(jce_id=0)

    obj = ContextStruct(value=42)
    buffer = io.BytesIO()
    dump(obj, buffer)
    buffer.seek(0)

    loaded = load(buffer, target=ContextStruct, context={"decode_key": "test"})

    assert loaded.value == 42


INPUT_TYPE_CASES = [
    (memoryview, "memoryview输入"),
    (bytearray, "bytearray输入"),
]


@pytest.mark.parametrize(
    ("input_type", "desc"),
    INPUT_TYPE_CASES,
    ids=[c[1] for c in INPUT_TYPE_CASES],
)
def test_loads_with_different_input_types(
    input_type: type[memoryview] | type[bytearray], desc: str
) -> None:
    """loads() 应支持 bytes/bytearray/memoryview 等多种输入类型."""
    user = SimpleUser(uid=100, name="test")
    data = dumps(user)

    converted_data = input_type(data)
    loaded = loads(converted_data, target=SimpleUser)

    assert loaded.uid == 100, f"失败: {desc}"
    assert loaded.name == "test", f"失败: {desc}"


def test_convert_bytes_nested_jcedict() -> None:
    """loads(bytes_mode='auto') 应递归转换嵌套 JceDict 中的字节."""
    data = JceDict({0: JceDict({1: b"nested_text"})})
    encoded = dumps(data)

    decoded = loads(encoded, bytes_mode="auto")

    assert isinstance(decoded[0][1], str)
    assert decoded[0][1] == "nested_text"


def test_convert_bytes_preserves_jcedict_type() -> None:
    """字节转换过程应保持 JceDict 类型不变."""
    data = JceDict({0: b"test"})
    encoded = dumps(data)

    decoded = loads(encoded, bytes_mode="auto")

    assert isinstance(decoded, JceDict)


def test_error_handling_invalid_data() -> None:
    """loads() 应在数据无效时抛出 JceDecodeError."""
    invalid_data = b"\xff\xff\xff\xff"

    with pytest.raises(JceDecodeError):
        loads(invalid_data)
