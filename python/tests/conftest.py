"""提供 Tarsio 测试的公共 Fixtures 和配置."""

from collections.abc import Generator
from io import BytesIO
from typing import Annotated, Optional

import pytest
from tarsio import Struct

# ==========================================
# 基础测试结构体
# ==========================================


class User(Struct):
    """用于测试的简单用户结构体."""

    id: Annotated[int, 0]
    name: Annotated[str, 1]


class OptionalUser(Struct):
    """带可选字段的用户结构体."""

    id: Annotated[int, 0]
    name: Annotated[str | None, 1] = None
    age: Annotated[int | None, 2] = None


class Node(Struct):
    """用于测试递归结构的链表节点."""

    val: Annotated[int, 0]
    next: Annotated[Optional["Node"], 1] = None


# ==========================================
# Fixtures
# ==========================================


@pytest.fixture
def sample_user() -> User:
    """提供一个预置的 User 实例.

    Returns:
        User: 初始化后的 User 对象，id=42, name="Alice"。
    """
    return User(42, "Alice")


@pytest.fixture
def sample_optional_user() -> OptionalUser:
    """提供一个带可选字段的 OptionalUser 实例.

    Returns:
        OptionalUser: 初始化后的 OptionalUser 对象，id=1, name="Bob", age=None。
    """
    return OptionalUser(1, "Bob", None)


@pytest.fixture
def sample_node() -> Node:
    """提供一个链表节点实例.

    Returns:
        Node: 初始化后的 Node 对象，val=100, next=Node(200, None)。
    """
    return Node(100, Node(200, None))


@pytest.fixture
def mock_stream() -> Generator[BytesIO, None, None]:
    """提供内存字节流环境.

    Yields:
        BytesIO: 一个已打开的内存字节流实例。
    """
    stream = BytesIO()
    yield stream
    stream.close()
