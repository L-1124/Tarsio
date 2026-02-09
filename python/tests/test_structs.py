"""测试 Struct 相关 API 与不变量."""

import copy
import inspect
from typing import Annotated, Any, Optional

import pytest
from tarsio._core import Struct, StructConfig, TarsDict, decode, encode, encode_raw

# ==========================================
# 测试专用结构体
# ==========================================


class OptionalOnly(Struct):
    """仅包含可选字段的结构体."""

    optional: Annotated[int | None, 0] = None


class UserWithDefaults(Struct):
    """带默认值字段的结构体."""

    email: Annotated[str, 2]
    name: Annotated[str, 0] = "unknown"
    age: Annotated[int, 1] = 0


# ==========================================
# 构造器测试 - 位置参数
# ==========================================


def test_init_with_positional_args(sample_user) -> None:
    """位置参数应该按照字段顺序正确赋值."""
    # Act
    cls = type(sample_user)
    user = cls(42, "Alice")

    # Assert
    assert user.id == 42
    assert user.name == "Alice"


def test_init_with_keyword_args(sample_user) -> None:
    """关键字参数应该正确赋值."""
    # Act
    cls = type(sample_user)
    user = cls(id=100, name="Bob")

    # Assert
    assert user.id == 100
    assert user.name == "Bob"


def test_init_with_mixed_args(sample_user) -> None:
    """混合位置和关键字参数应该正确赋值."""
    # Act
    cls = type(sample_user)
    user = cls(200, name="Charlie")

    # Assert
    assert user.id == 200
    assert user.name == "Charlie"


# ==========================================
# 构造器测试 - 错误处理
# ==========================================


def test_init_missing_required_arg_raises_type_error(sample_user) -> None:
    """缺少必需参数时应该抛出 TypeError."""
    cls = type(sample_user)
    with pytest.raises(TypeError, match="missing 1 required"):
        cls(1)  # type: ignore


def test_init_duplicate_arg_raises_type_error(sample_user) -> None:
    """参数重复赋值时应该抛出 TypeError."""
    cls = type(sample_user)
    with pytest.raises(TypeError, match="got multiple values"):
        cls(1, id=2)  # type: ignore


def test_init_extra_positional_arg_raises_type_error(sample_user) -> None:
    """多余的位置参数应该抛出 TypeError."""
    cls = type(sample_user)
    with pytest.raises(TypeError, match="takes"):
        cls(1, "Alice", "Extra")  # type: ignore


# ==========================================
# 可选字段测试
# ==========================================


def test_optional_field_defaults_to_none(sample_optional_user) -> None:
    """可选字段未提供时应该默认为 None."""
    # Act
    cls = type(sample_optional_user)
    obj = cls(1, "Name")

    # Assert
    assert obj.id == 1
    assert obj.name == "Name"
    assert obj.age is None


def test_optional_field_accepts_none(sample_optional_user) -> None:
    """可选字段应该接受显式 None."""
    # Act
    cls = type(sample_optional_user)
    obj = cls(1, None, None)

    # Assert
    assert obj.id == 1
    assert obj.name is None
    assert obj.age is None


def test_optional_field_accepts_value(sample_optional_user) -> None:
    """可选字段应该接受有效值."""
    # Act
    cls = type(sample_optional_user)
    obj = cls(1, "Name", 25)

    # Assert
    assert obj.id == 1
    assert obj.name == "Name"
    assert obj.age == 25


# ==========================================
# 默认值反序列化测试
# ==========================================


def test_default_value_used_when_field_missing() -> None:
    """Wire 缺失字段时应使用模型默认值."""
    data = encode_raw(TarsDict({2: "a@b.com"}))
    obj = UserWithDefaults.decode(data)
    assert obj.name == "unknown"
    assert obj.age == 0
    assert obj.email == "a@b.com"


def test_missing_non_optional_field_without_default_raises() -> None:
    """非 Optional 且无默认值的字段缺失应抛错."""
    data = encode_raw(TarsDict({}))
    with pytest.raises(ValueError, match="Missing required field 'email'"):
        UserWithDefaults.decode(data)


# ==========================================
# 递归结构测试
# ==========================================


def test_recursive_struct_leaf_node(sample_node) -> None:
    """叶子节点的 next 应该为 None."""
    # Act
    cls = type(sample_node)
    node = cls(1)

    # Assert
    assert node.val == 1
    assert node.next is None


def test_recursive_struct_linked_nodes(sample_node) -> None:
    """链表节点应该正确链接."""
    # Arrange
    cls = type(sample_node)
    leaf = cls(2)

    # Act
    root = cls(1, leaf)

    # Assert
    assert root.val == 1
    assert root.next is leaf
    assert root.next is not None
    assert root.next.val == 2


def test_recursive_struct_deep_nesting(sample_node) -> None:
    """深层嵌套结构应该正确构造."""
    # Act
    cls = type(sample_node)
    node = cls(1, cls(2, cls(3, None)))

    # Assert
    assert node.val == 1
    assert node.next is not None
    assert node.next.val == 2
    assert node.next.next is not None
    assert node.next.next.val == 3
    assert node.next.next.next is None


def test_decode_empty_bytes_with_all_optional_fields() -> None:
    """空数据应解析为全 None 的可选字段."""
    # Act
    result = OptionalOnly.decode(b"")

    # Assert
    assert result.optional is None


def test_decode_skips_unknown_tag() -> None:
    """解码时应安全跳过未知字段而不影响已知字段."""

    class V1(Struct):
        id: Annotated[int, 0]
        name: Annotated[str, 1]

    class V2(Struct):
        id: Annotated[int, 0]
        name: Annotated[str, 1]
        extra: Annotated[int, 2]

    # Arrange
    data = V2(1, "Alice", 999).encode()

    # Act
    result = V1.decode(data)

    # Assert
    assert result.id == 1
    assert result.name == "Alice"


# ==========================================
# Frozen 和 Hash 测试
# ==========================================


def test_frozen_struct_is_immutable() -> None:
    """Frozen 结构体应禁止修改属性."""

    class FrozenPoint(Struct, frozen=True):
        x: Annotated[int, 0]
        y: Annotated[int, 1]

    p = FrozenPoint(1, 2)

    with pytest.raises(AttributeError):
        p.x = 3  # type: ignore

    with pytest.raises(AttributeError):
        p.z = 4  # type: ignore # New attribute


def test_frozen_struct_is_hashable() -> None:
    """Frozen 结构体应支持 hash."""

    class FrozenPoint(Struct, frozen=True):
        x: Annotated[int, 0]
        y: Annotated[int, 1]

    p1 = FrozenPoint(1, 2)
    p2 = FrozenPoint(1, 2)

    assert hash(p1) == hash(p2)
    assert isinstance(hash(p1), int)

    # Test usage in set/dict
    s = {p1}
    assert p2 in s


def test_non_frozen_struct_is_not_hashable() -> None:
    """非 Frozen 结构体不应支持 hash."""

    class Point(Struct):
        x: Annotated[int, 0]
        y: Annotated[int, 1]

    p = Point(1, 2)

    # mutable types are not hashable by default in Python if __eq__ is defined
    # checking if it raises TypeError
    with pytest.raises(TypeError):
        hash(p)


# ==========================================
# Equality 测试
# ==========================================


def test_struct_equality() -> None:
    """结构体应支持基于内容的相等性比较."""

    class Point(Struct):
        x: Annotated[int, 0]
        y: Annotated[int, 1]

    p1 = Point(1, 2)
    p2 = Point(1, 2)
    p3 = Point(1, 3)

    assert p1 == p2
    assert p1 != p3
    assert p1 != "string"
    assert p1 != 123


# ==========================================
# forbid_unknown_tags 测试
# ==========================================


def test_decode_raises_on_unknown_tag_if_forbidden() -> None:
    """当启用 forbid_unknown_tags 时，遇到未知 tag 应抛错."""

    class V1Strict(Struct, forbid_unknown_tags=True):
        id: Annotated[int, 0]
        name: Annotated[str, 1]

    class V2(Struct):
        id: Annotated[int, 0]
        name: Annotated[str, 1]
        extra: Annotated[int, 2]

    # Arrange
    data = V2(1, "Alice", 999).encode()

    # Act & Assert
    # Usually unknown tags raise ValueError or similar when forbidden
    with pytest.raises(ValueError, match="Unknown tag"):
        V1Strict.decode(data)


# ==========================================
# eq, repr_omit_defaults
# ==========================================


def test_struct_eq_option() -> None:
    """eq=False 时应回退到对象身份比较."""

    class NoEq(Struct, eq=False):
        id: Annotated[int, 0]

    class YesEq(Struct, eq=True):
        id: Annotated[int, 0]

    # eq=False
    a = NoEq(1)
    b = NoEq(1)
    assert a != b

    # eq=True (default)
    c = YesEq(1)
    d = YesEq(1)
    assert c == d


def test_struct_repr_omit_defaults_option() -> None:
    """repr_omit_defaults=True 时 repr 应省略默认值字段."""

    class CleanRepr(Struct, repr_omit_defaults=True):
        id: Annotated[int, 0]
        name: Annotated[str, 1] = "default"
        opt: Annotated[str | None, 2] = None

    # Case 1: All defaults
    obj1 = CleanRepr(100)
    assert "id=100" in repr(obj1)
    assert "name=" not in repr(obj1)
    assert "opt=" not in repr(obj1)

    # Case 2: Partial defaults
    obj2 = CleanRepr(100, name="custom")
    assert "id=100" in repr(obj2)
    assert "name='custom'" in repr(obj2)
    assert "opt=" not in repr(obj2)

    # Case 3: No defaults omitted (control group)
    class FullRepr(Struct, repr_omit_defaults=False):
        id: Annotated[int, 0]
        name: Annotated[str, 1] = "default"

    obj3 = FullRepr(100)
    assert "name='default'" in repr(obj3)


def test_struct_kw_only_option() -> None:
    """kw_only=True 时强制使用关键字参数."""

    class KwOnly(Struct, kw_only=True):
        id: Annotated[int, 0]
        name: Annotated[str, 1]

    # Success
    obj = KwOnly(id=1, name="A")
    assert obj.id == 1

    # Failure: Positional args
    with pytest.raises(TypeError, match="takes 0 positional arguments"):
        KwOnly(1, "A")  # type: ignore


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


# ==========================================
# 综合测试结构体 (覆盖所有 WireType)
# ==========================================


class AllTypes(Struct):
    """包含所有基本 WireType 的结构体."""

    i8: Annotated[int, 0]
    i16: Annotated[int, 1]
    i32: Annotated[int, 2]
    i64: Annotated[int, 3]
    f32: Annotated[float, 4]
    f64: Annotated[float, 5]
    s1: Annotated[str, 6]  # Short string
    s4: Annotated[str, 7]  # Long string (logic driven by length)
    lst: Annotated[list[int], 8]
    mp: Annotated[dict[str, int], 9]
    sl: Annotated[bytes, 10]  # SimpleList
    nst: Annotated[Optional["AllTypes"], 11] = None  # Struct


# ==========================================
# Invariant: Encode ∘ Decode = Identity
# ==========================================


def test_invariant_roundtrip_all_types() -> None:
    """验证 encode 后 decode 能精确还原所有类型的字段."""
    # Arrange
    original = AllTypes(
        i8=100,
        i16=30000,
        i32=1000000,
        i64=2**60,
        f32=1.234,
        f64=3.1415926535,
        s1="Short",
        s4="Long " * 100,
        lst=[1, 2, 3],
        mp={"key": 1, "val": 2},
        sl=b"\x01\x02\x03",
        nst=None,
    )

    # Act
    encoded = encode(original)
    restored = decode(AllTypes, encoded)

    # Assert
    assert restored.i8 == original.i8
    assert restored.i16 == original.i16
    assert restored.i32 == original.i32
    assert restored.i64 == original.i64
    assert restored.f32 == pytest.approx(original.f32, rel=1e-5)
    assert restored.f64 == original.f64
    assert restored.s1 == original.s1
    assert restored.s4 == original.s4
    assert restored.lst == original.lst
    assert restored.mp == original.mp
    assert restored.sl == original.sl
    assert restored.nst is None


# ==========================================
# Invariant: Unknown Tags are Skipped
# ==========================================


def test_invariant_unknown_tags_skipped() -> None:
    """验证解码器能安全跳过未知的 Tag，不影响后续字段读取."""

    class V1(Struct):
        a: Annotated[int, 0]
        c: Annotated[int, 2]

    class V2(Struct):
        a: Annotated[int, 0]
        b: Annotated[dict[str, int], 1]  # V1 未知 Tag 1(复杂类型)
        c: Annotated[int, 2]

    # Arrange: 用 V2 编码
    v2_obj = V2(10, {"x": 1, "y": 2}, 20)
    data = encode(v2_obj)

    # Act: 用 V1 解码
    v1_obj = decode(V1, data)

    # Assert: 已知字段正确,未知字段被忽略
    assert v1_obj.a == 10
    assert v1_obj.c == 20
    assert not hasattr(v1_obj, "b")


# ==========================================
# 边界:空容器与 SimpleList
# ==========================================


def test_encode_decode_empty_list() -> None:
    """空列表应能正确往返还原."""

    class IntList(Struct):
        val: Annotated[list[int], 0]

    encoded = encode(IntList([]))
    decoded = decode(IntList, encoded)
    assert decoded.val == []


def test_encode_decode_empty_map() -> None:
    """空字典应能正确往返还原."""

    class StrIntMap(Struct):
        val: Annotated[dict[str, int], 0]

    encoded = encode(StrIntMap({}))
    decoded = decode(StrIntMap, encoded)
    assert decoded.val == {}


def test_simple_list_wire_format() -> None:
    """Bytes 应被编码为 SimpleList 且能正确解码为 bytes."""

    class SimpleListStruct(Struct):
        val: Annotated[bytes, 0]

    data = b"\x01\x02"
    encoded = encode(SimpleListStruct(data))
    assert encoded[0] == 0x0D

    decoded = decode(SimpleListStruct, encoded)
    assert decoded.val == data


def test_any_simplelist_auto_utf8_and_passthrough() -> None:
    """Any 字段应自动解析 UTF-8 并保留 Tars 透传 bytes."""

    class AnyBox(Struct):
        val: Annotated[Any, 0]

    utf8_payload = encode_raw(TarsDict({0: b"hello"}))
    utf8_decoded = decode(AnyBox, utf8_payload)
    assert utf8_decoded.val == "hello"

    passthrough = encode_raw(TarsDict({0: 1}))
    passthrough_payload = encode_raw(TarsDict({0: passthrough}))
    passthrough_decoded = decode(AnyBox, passthrough_payload)
    assert passthrough_decoded.val == passthrough


# ==========================================
# Schema Evolution(演进)
# ==========================================


def test_evolution_forward_compatibility_optional_field_defaults_none() -> None:
    """旧数据应能被新 Schema 解码且新增字段为 None."""

    class UserV1(Struct):
        uid: Annotated[int, 0]
        name: Annotated[str, 1]

    class UserV2(Struct):
        uid: Annotated[int, 0]
        name: Annotated[str, 1]
        email: Annotated[str | None, 2] = None

    data = encode(UserV1(1, "Alice"))
    result = decode(UserV2, data)
    assert result.uid == 1
    assert result.name == "Alice"
    assert result.email is None


def test_evolution_backward_compatibility_skips_unknown_bytes_field() -> None:
    """旧 Schema 解码新数据时应跳过未知 bytes 字段."""

    class UserV1Old(Struct):
        uid: Annotated[int, 0]
        name: Annotated[str, 1]

    class UserV2New(Struct):
        uid: Annotated[int, 0]
        name: Annotated[str, 1]
        payload: Annotated[bytes, 2]

    data = encode(UserV2New(1, "Alice", b"\x01\x02"))
    result = decode(UserV1Old, data)
    assert result.uid == 1
    assert result.name == "Alice"


def test_reentrant_encode_error_message_mentions_common_triggers() -> None:
    """编码过程中若 __eq__/__repr__ 意外触发 encode，应给出可理解的提示."""

    class Inner(Struct):
        x: Annotated[int, 0]

        __hash__ = Struct.__hash__

        def __eq__(self, other: object) -> bool:  # type: ignore[override]
            _ = self.encode()
            return False

    class Outer(Struct, omit_defaults=True):
        inner: Annotated[Inner, 0] = Inner(1)

    with pytest.raises(
        RuntimeError, match=r"Re-entrant encode detected.*(__repr__|__eq__|__str__)"
    ):
        _ = Outer(Inner(1)).encode()


# ==========================================
# Invariant: Error Handling
# ==========================================


def test_invariant_missing_required_raises_value_error() -> None:
    """验证必需字段缺失时抛出 ValueError."""

    class Strict(Struct):
        req: Annotated[int, 0]

    # Decode to Strict (expects Tag 0)
    with pytest.raises(ValueError, match="Missing required field"):
        decode(Strict, b"")


def test_invariant_truncated_data_raises_value_error() -> None:
    """验证数据截断时抛出 ValueError (Rust codec error)."""

    class Simple(Struct):
        val: Annotated[int, 0]

    # Tag 0 (Head) + Int4 (Type 2) but no data
    truncated_data = b"\x02"

    with pytest.raises(ValueError, match=r"Read head error|Unexpected end of buffer"):
        decode(Simple, truncated_data)
