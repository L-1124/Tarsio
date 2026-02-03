"""测试 Struct 基类的构造器和类行为."""

from typing import Annotated

import pytest
from tarsio import Struct
from tarsio._core import encode_raw

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
    data = encode_raw({2: "a@b.com"})
    obj = UserWithDefaults.decode(data)
    assert obj.name == "unknown"
    assert obj.age == 0
    assert obj.email == "a@b.com"


def test_missing_non_optional_field_without_default_raises() -> None:
    """非 Optional 且无默认值的字段缺失应抛错."""
    data = encode_raw({})
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
