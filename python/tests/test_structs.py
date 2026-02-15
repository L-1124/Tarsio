"""Tars Struct API 行为测试.

验证 Struct 构造、配置、默认值、演进兼容性等 API 契约.
"""

from typing import Annotated, Any, Optional

import pytest
from tarsio._core import (
    Struct,
    TarsDict,
    decode,
    decode_raw,
    encode,
    encode_raw,
)


class User(Struct, order=True):
    """标准用户结构体."""

    uid: Annotated[int, 0]
    name: Annotated[str, 1]


class Point(Struct):
    """坐标点."""

    x: Annotated[int, 0] = 0
    y: Annotated[int, 1] = 0


class Config(Struct):
    """带默认值的配置."""

    timeout: Annotated[int, 0] = 30
    debug: Annotated[bool, 1] = False


class Node(Struct):
    """递归结构体."""

    val: Annotated[int, 0]
    next: Annotated[Optional["Node"], 1] = None


# ==========================================
# 构造函数行为测试
# ==========================================


def test_init_with_positional_args() -> None:
    """支持按 tag 顺序的位置参数构造."""
    u = User(1001, "Alice")
    assert u.uid == 1001
    assert u.name == "Alice"


def test_init_with_keyword_args() -> None:
    """支持关键字参数构造."""
    u = User(name="Bob", uid=1002)
    assert u.uid == 1002
    assert u.name == "Bob"


def test_init_with_mixed_args() -> None:
    """支持位置参数与关键字参数混用."""
    u = User(1003, name="Charlie")
    assert u.uid == 1003
    assert u.name == "Charlie"


def test_init_missing_required_arg_raises_type_error() -> None:
    """缺少必填字段应抛出 TypeError."""
    with pytest.raises(TypeError, match=r"missing .* argument"):
        User(uid=1)  # pyright: ignore[reportCallIssue]


def test_init_duplicate_arg_raises_type_error() -> None:
    """重复传递参数应抛出 TypeError."""
    with pytest.raises(TypeError, match="multiple values"):
        User(1, uid=1)  # pyright: ignore[reportCallIssue]


def test_init_extra_positional_arg_raises_type_error() -> None:
    """多余的位置参数应抛出 TypeError."""
    with pytest.raises(TypeError, match=r"takes .* arguments"):
        User(1, "a", 3)  # pyright: ignore[reportCallIssue]


def test_init_unexpected_keyword_raises_type_error() -> None:
    """未知关键字参数应抛出 TypeError."""
    with pytest.raises(TypeError, match="unexpected keyword argument"):
        User(uid=1, name="a", unknown=1)  # pyright: ignore[reportCallIssue]


# ==========================================
# 默认值与 Optional 行为测试
# ==========================================


def test_optional_field_without_default_raises_type_error() -> None:
    """Optional 字段若无显式默认值，不应有默认值."""

    class Opt(Struct):
        val: Annotated[int | None, 0]

    with pytest.raises(TypeError, match=r"missing .* argument"):
        Opt()  # pyright: ignore[reportCallIssue]

    o2 = Opt(None)
    assert o2.val is None


def test_optional_field_accepts_value() -> None:
    """Optional 字段传入有效值应允许."""

    class Opt(Struct):
        val: Annotated[int | None, 0]

    o = Opt(123)
    assert o.val == 123


def test_default_value_used_when_field_missing() -> None:
    """字段缺失时应使用默认值."""
    p = Point(x=10)
    assert p.x == 10
    assert p.y == 0  # Default


def test_missing_non_optional_field_without_default_raises() -> None:
    """非 Optional 且无默认值的字段在解码缺失时应抛出异常."""

    class Required(Struct):
        val: Annotated[int, 0]

    # Empty payload
    data = encode_raw(TarsDict({}))
    with pytest.raises(ValueError, match="Missing required field"):
        decode(Required, data)


# ==========================================
# 递归结构体测试
# ==========================================


def test_recursive_struct_leaf_node() -> None:
    """递归结构体的叶子节点 (next=None)."""
    n = Node(1)
    data = encode(n)
    restored = decode(Node, data)
    assert restored.val == 1
    assert restored.next is None


def test_recursive_struct_linked_nodes() -> None:
    """递归结构体的链表形式."""
    n2 = Node(2)
    n1 = Node(1, n2)
    data = encode(n1)
    restored = decode(Node, data)
    assert restored.val == 1
    assert restored.next is not None
    assert restored.next.val == 2
    assert restored.next.next is None


def test_recursive_struct_deep_nesting() -> None:
    """较深层的递归结构体."""
    head = Node(0)
    curr = head
    for i in range(1, 10):
        new_node = Node(i)
        curr.next = new_node
        curr = new_node

    data = encode(head)
    restored = decode(Node, data)

    curr_r = restored
    count = 0
    while curr_r:
        assert curr_r.val == count
        curr_r = curr_r.next
        count += 1
    assert count == 10


# ==========================================
# 协议容错性测试 (Unknown Tags / Empty)
# ==========================================


def test_decode_empty_bytes_with_all_optional_fields() -> None:
    """全可选字段结构体可以从空 bytes 解码."""
    # Encode with defaults -> if omit_defaults=False (default), it writes 0s.
    # Let's use omit_defaults=True behavior simulation or raw empty bytes.
    data = b""
    # Decode empty bytes -> fields use defaults
    restored = decode(Point, data)
    assert restored.x == 0
    assert restored.y == 0


def test_decode_skips_unknown_tag() -> None:
    """解码应跳过未知 tag."""
    # Data has tag 0, 1, 2. User only knows 0, 1.
    data = encode_raw(TarsDict({0: 1, 1: "a", 2: "secret"}))
    u = decode(User, data)
    assert u.uid == 1
    assert u.name == "a"
    # Tag 2 ignored


# ==========================================
# Struct 配置项测试 (frozen, order, eq...)
# ==========================================


def test_frozen_struct_is_immutable() -> None:
    """frozen=True 时实例不可变."""

    class FrozenUser(Struct, frozen=True):
        id: Annotated[int, 0]

    u = FrozenUser(1)
    # AttributeError: can't set attribute
    with pytest.raises(AttributeError):
        u.id = 2  # pyright: ignore[reportAttributeAccessIssue]


def test_frozen_struct_is_hashable() -> None:
    """frozen=True 时实例可哈希."""

    class FrozenUser(Struct, frozen=True):
        id: Annotated[int, 0]

    u = FrozenUser(1)
    assert hash(u) is not None
    s = {u}
    assert u in s


def test_non_frozen_struct_is_not_hashable() -> None:
    """默认 (frozen=False) 实例不可哈希."""
    u = User(1, "a")
    with pytest.raises(TypeError, match="unhashable"):
        hash(u)


def test_struct_equality() -> None:
    """默认生成 __eq__."""
    u1 = User(1, "a")
    u2 = User(1, "a")
    u3 = User(2, "b")
    assert u1 == u2
    assert u1 != u3


def test_decode_raises_on_unknown_tag_if_forbidden() -> None:
    """forbid_unknown_tags=True 时遇到未知 tag 抛错."""

    class Strict(Struct, forbid_unknown_tags=True):
        val: Annotated[int, 0]

    data = encode_raw(TarsDict({0: 1, 1: 2}))
    with pytest.raises(ValueError, match="Unknown tag"):
        decode(Strict, data)


def test_struct_eq_option() -> None:
    """eq=False 不生成 __eq__ (使用 object 默认同一性比较)."""

    class NoEq(Struct, eq=False):
        val: Annotated[int, 0]

    a = NoEq(1)
    b = NoEq(1)
    assert a != b  # Different instances


def test_struct_repr_omit_defaults_option() -> None:
    """repr_omit_defaults=True 时 repr 不显示默认值."""

    class C(Struct, repr_omit_defaults=True):
        a: Annotated[int, 0] = 1
        b: Annotated[int, 1] = 2

    c = C(b=3)
    assert "a=1" not in repr(c)
    assert "b=3" in repr(c)


def test_struct_kw_only_option() -> None:
    """kw_only=True 时构造函数仅接受关键字参数."""

    class Kw(Struct, kw_only=True):
        val: Annotated[int, 0]

    with pytest.raises(TypeError):
        Kw(1)  # pyright: ignore[reportCallIssue]
    k = Kw(val=1)
    assert k.val == 1


def test_struct_fields_in_tag_order() -> None:
    """__struct_fields__ 应按 tag 排序."""

    class Unordered(Struct):
        b: Annotated[int, 10]
        a: Annotated[int, 1]

    assert Unordered.__struct_fields__ == ("a", "b")


def test_order_comparisons_when_enabled() -> None:
    """order=True 时支持排序比较."""

    class Ordered(Struct, order=True):
        val: Annotated[int, 0]

    a = Ordered(1)
    b = Ordered(2)
    assert a < b
    assert b > a


def test_order_comparisons_not_enabled_raise() -> None:
    """默认 order=False 不支持排序."""

    class NoOrderUser(Struct):
        uid: Annotated[int, 0]
        name: Annotated[str, 1]

    u1 = NoOrderUser(1, "a")
    u2 = NoOrderUser(2, "b")
    with pytest.raises(TypeError):
        _ = u1 < u2  # pyright: ignore[reportOperatorIssue]


def test_omit_defaults_skips_encoding() -> None:
    """omit_defaults=True 时编码跳过默认值字段."""

    class Compact(Struct, omit_defaults=True):
        a: Annotated[int, 0] = 0
        b: Annotated[int, 1] = 1

    c = Compact(b=2)  # a is default, b is not
    data = encode(c)
    # Should only contain tag 1
    # raw dict to verify
    raw = decode_raw(data)
    assert 0 not in raw
    assert raw[1] == 2


def test_signature_tag_order_and_kwonly_handling() -> None:
    """Signature 应反映字段顺序和 kw_only 设置."""
    import inspect

    class S(Struct, kw_only=True):
        b: Annotated[int, 2]
        a: Annotated[int, 1]

    sig = inspect.signature(S)
    params = list(sig.parameters.values())
    # Order by tag: a, b
    assert params[0].name == "a"
    assert params[1].name == "b"
    # All keyword-only
    assert params[0].kind == inspect.Parameter.KEYWORD_ONLY


# ==========================================
# 辅助特性测试 (Dataclass 兼容, Copy 等)
# ==========================================


def test_dataclass_transform_runtime_attribute_present() -> None:
    """Struct 应标记为 dataclass_transform 兼容."""
    # This is mostly for static analysis, but we can check if PyO3 metaclass works
    assert isinstance(User, type)


def test_copy_returns_new_instance() -> None:
    """__copy__ 应返回新实例."""
    import copy

    u = User(1, "a")
    u2 = copy.copy(u)
    assert u == u2
    assert u is not u2


# ==========================================
# 不变量测试 (Invariants)
# ==========================================


def test_invariant_roundtrip_all_types() -> None:
    """所有支持类型字段的 Roundtrip."""

    class AllTypes(Struct):
        i: Annotated[int, 0]
        s: Annotated[str, 1]
        f: Annotated[float, 2]
        b: Annotated[bool, 3]
        by: Annotated[bytes, 4]
        L: Annotated[list[int], 5]
        m: Annotated[dict[str, int], 6]
        nest: Annotated[User, 7]

    obj = AllTypes(
        i=1,
        s="s",
        f=1.5,
        b=True,
        by=b"bin",
        L=[1, 2],
        m={"k": 1},
        nest=User(1, "u"),
    )
    restored = decode(AllTypes, encode(obj))
    assert restored == obj


def test_invariant_unknown_tags_skipped() -> None:
    """未知 Tag 必须被安全跳过 (Forward Compatibility)."""

    # Version 1
    class V1(Struct):
        a: Annotated[int, 0]

    # Version 2 adds field
    class V2(Struct):
        a: Annotated[int, 0]
        b: Annotated[int, 1]

    v2 = V2(1, 2)
    data = encode(v2)

    # Decode as V1
    v1 = decode(V1, data)
    assert v1.a == 1
    # b is skipped/ignored


def test_encode_decode_empty_list() -> None:
    """空列表应正常处理."""

    class L(Struct):
        L: Annotated[list[int], 0]

    obj = L([])
    restored = decode(L, encode(obj))
    assert restored.L == []


def test_encode_decode_empty_map() -> None:
    """空字典应正常处理."""

    class M(Struct):
        m: Annotated[dict[int, int], 0]

    obj = M({})
    restored = decode(M, encode(obj))
    assert restored.m == {}


def test_simple_list_wire_format() -> None:
    """bytes/List[int]/List[bytes] 在特定条件下的 SimpleList 优化行为验证."""

    # Case 1: bytes -> SimpleList(13)
    class B(Struct):
        v: Annotated[bytes, 0]

    data = encode(B(b"123"))
    # Tag 0 (SimpleList 13) -> 0D
    # Head 00 (Byte), Len 3 (Int1), Data 313233
    # 0D 00 00 03 31 32 33
    assert data.hex().upper().startswith("0D000003")


def test_any_simplelist_auto_utf8_and_passthrough() -> None:
    """Any 字段在遇到 SimpleList 时应保留为 bytes (不再自动转 str)."""

    class AnyBox(Struct):
        val: Annotated[Any, 0]

    utf8_payload = encode_raw(TarsDict({0: b"hello"}))
    utf8_decoded = decode(AnyBox, utf8_payload)

    # 修改: auto_simplelist 移除后，应始终返回 bytes
    assert utf8_decoded.val == b"hello"


def test_evolution_forward_compatibility_optional_field_defaults_none() -> None:
    """新版本增加 Optional 字段，旧数据解码应为 None/Default."""

    class V1(Struct):
        a: Annotated[int, 0]

    class V2(Struct):
        a: Annotated[int, 0]
        b: Annotated[int | None, 1] = None

    data = encode(V1(1))
    v2 = decode(V2, data)
    assert v2.a == 1
    assert v2.b is None


def test_evolution_backward_compatibility_skips_unknown_bytes_field() -> None:
    """旧版本增加字段，新版本(移除字段)读取应跳过."""
    # Same as test_invariant_unknown_tags_skipped but explicitly about 'bytes' skipping
    pass


def test_reentrant_encode_error_message_mentions_common_triggers() -> None:
    """递归编码错误应提示常见原因."""
    # Construct self-referencing dict
    d = {}
    d[0] = d
    # TarsDict(d) would stack overflow in python before encode?
    # No, TarsDict constructor shallow copies.
    # We need cycle in TarsDict
    td = TarsDict({})
    td[0] = td

    with pytest.raises(ValueError, match="Recursion limit"):
        encode_raw(td)


def test_invariant_missing_required_raises_value_error() -> None:
    """缺少必填字段抛 ValueError."""

    class S(Struct):
        a: Annotated[int, 0]

    with pytest.raises(ValueError, match="Missing required field"):
        decode(S, b"")


def test_invariant_truncated_data_raises_value_error() -> None:
    """数据截断抛 ValueError."""

    class S(Struct):
        a: Annotated[int, 0]

    # Tag 0 Head Int1(1) -> 00 01. Truncate last byte.
    data = bytes.fromhex("00")
    with pytest.raises(ValueError, match="Unexpected end of buffer"):
        decode(S, data)


def test_invariant_trailing_bytes_raises_value_error() -> None:
    """尾随无关字节应抛出 ValueError."""

    class S(Struct):
        a: Annotated[int, 0]

    data = bytes.fromhex("0001FF")
    with pytest.raises(ValueError, match="Trailing bytes after decode"):
        decode(S, data)
