"""测试 Struct 基类的构造器和类行为."""

from typing import Annotated, Optional

import pytest
from tarsio import Struct

# ==========================================
# 测试专用结构体
# ==========================================


class BasicUser(Struct):
    """基础用户结构体."""

    id: Annotated[int, 0]
    name: Annotated[str, 1]


class OptionalFields(Struct):
    """带可选字段的结构体."""

    required_id: Annotated[int, 0]
    optional_name: Annotated[str | None, 1] = None
    optional_age: Annotated[int | None, 2] = None


class RecursiveNode(Struct):
    """递归链表节点."""

    val: Annotated[int, 0]
    next: Annotated[Optional["RecursiveNode"], 1] = None


class OptionalOnly(Struct):
    """仅包含可选字段的结构体."""

    optional: Annotated[int | None, 0] = None


# ==========================================
# 构造器测试 - 位置参数
# ==========================================


def test_init_with_positional_args() -> None:
    """位置参数应该按照字段顺序正确赋值."""
    # Act
    user = BasicUser(42, "Alice")

    # Assert
    assert user.id == 42
    assert user.name == "Alice"


def test_init_with_keyword_args() -> None:
    """关键字参数应该正确赋值."""
    # Act
    user = BasicUser(id=100, name="Bob")

    # Assert
    assert user.id == 100
    assert user.name == "Bob"


def test_init_with_mixed_args() -> None:
    """混合位置和关键字参数应该正确赋值."""
    # Act
    user = BasicUser(200, name="Charlie")

    # Assert
    assert user.id == 200
    assert user.name == "Charlie"


# ==========================================
# 构造器测试 - 错误处理
# ==========================================


def test_init_missing_required_arg_raises_type_error() -> None:
    """缺少必需参数时应该抛出 TypeError."""
    with pytest.raises(TypeError, match="missing 1 required"):
        BasicUser(1)  # type: ignore


def test_init_duplicate_arg_raises_type_error() -> None:
    """参数重复赋值时应该抛出 TypeError."""
    with pytest.raises(TypeError, match="got multiple values"):
        BasicUser(1, id=2)  # type: ignore


def test_init_extra_positional_arg_raises_type_error() -> None:
    """多余的位置参数应该抛出 TypeError."""
    with pytest.raises(TypeError, match="takes"):
        BasicUser(1, "Alice", "Extra")  # type: ignore


# ==========================================
# 可选字段测试
# ==========================================


def test_optional_field_defaults_to_none() -> None:
    """可选字段未提供时应该默认为 None."""
    # Act
    obj = OptionalFields(1, "Name")

    # Assert
    assert obj.required_id == 1
    assert obj.optional_name == "Name"
    assert obj.optional_age is None


def test_optional_field_accepts_none() -> None:
    """可选字段应该接受显式 None."""
    # Act
    obj = OptionalFields(1, None, None)

    # Assert
    assert obj.required_id == 1
    assert obj.optional_name is None
    assert obj.optional_age is None


def test_optional_field_accepts_value() -> None:
    """可选字段应该接受有效值."""
    # Act
    obj = OptionalFields(1, "Name", 25)

    # Assert
    assert obj.required_id == 1
    assert obj.optional_name == "Name"
    assert obj.optional_age == 25


# ==========================================
# 递归结构测试
# ==========================================


def test_recursive_struct_leaf_node() -> None:
    """叶子节点的 next 应该为 None."""
    # Act
    node = RecursiveNode(1)

    # Assert
    assert node.val == 1
    assert node.next is None


def test_recursive_struct_linked_nodes() -> None:
    """链表节点应该正确链接."""
    # Arrange
    leaf = RecursiveNode(2)

    # Act
    root = RecursiveNode(1, leaf)

    # Assert
    assert root.val == 1
    assert root.next is leaf
    assert root.next is not None
    assert root.next.val == 2


def test_recursive_struct_deep_nesting() -> None:
    """深层嵌套结构应该正确构造."""
    # Act
    node = RecursiveNode(1, RecursiveNode(2, RecursiveNode(3, None)))

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
