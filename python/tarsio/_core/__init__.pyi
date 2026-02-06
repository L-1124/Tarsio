"""Tarsio 的核心 Rust 扩展模块.

本模块包含 TARS 协议处理的高性能 Rust 实现，包括 Schema 编译器、注册表和编解码接口。
"""

from typing import TypeAlias, TypeVar

from typing_extensions import Any, Self, dataclass_transform

_StructT = TypeVar("_StructT", bound="Struct")
TarsDict: TypeAlias = dict[int, Any]

__all__ = [
    "Meta",
    "Struct",
    "TarsDict",
    "ValidationError",
    "decode",
    "decode_raw",
    "encode",
    "encode_raw",
    "probe_struct",
]

class ValidationError(ValueError): ...

class Meta:
    def __init__(
        self,
        tag: int | None = ...,
        gt: float | None = ...,
        lt: float | None = ...,
        ge: float | None = ...,
        le: float | None = ...,
        min_len: int | None = ...,
        max_len: int | None = ...,
        pattern: str | None = ...,
    ) -> None: ...

    tag: int | None
    gt: float | None
    lt: float | None
    ge: float | None
    le: float | None
    min_len: int | None
    max_len: int | None
    pattern: str | None

@dataclass_transform()
class Struct:
    """由 Rust Schema 编译器驱动的 Tarsio Struct 基类.

    继承此类将触发静态 Schema 编译过程。编译器会检查 `Annotated[T, Tag]` 注解，
    并将结构体布局注册到全局 Rust 注册表中。

    此类支持：
    - 静态 Schema 编译（在类定义时）
    - 泛型 TypeVar 解析（例如 `Box[int]`）
    - 向前引用（Forward References，使用字符串注解 `"User"`）
    - 强大的类型检查（由 `dataclass_transform` 支持）

    示例:
        >>> from typing import Annotated, Generic, TypeVar
        >>> class User(Struct):
        ...     id: Annotated[int, 1]
        ...     name: Annotated[str, 2]

        >>> # 同时也支持泛型:
        >>> T = TypeVar("T")
        >>> class Response(Struct, Generic[T]):
        ...     code: Annotated[int, 0]
        ...     data: Annotated[T, 1]

        >>> MyResp = Response[User]  # 为 Response[User] 注册专用的 Schema

        >>> # 支持向前引用 (Forward Reference):
        >>> class Node(Struct):
        ...     val: Annotated[int, 0]
        ...     next: Annotated["Node", 1]
    """
    def __new__(cls) -> Struct: ...
    def __init_subclass__(
        cls,
        frozen: bool = False,
        forbid_unknown_tags: bool = False,
        **kwargs: Any,
    ) -> None:
        """根据类型注解编译 Schema 并将其注册到 Rust 后端.

        Args:
            frozen: 如果为 True,则实例不可变且可哈希。
            forbid_unknown_tags: 如果为 True,反序列化时遇到未知 Tag 会报错。
            **kwargs: 其他传递给基类的参数。
        """
        ...

    def __eq__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...
    def encode(self) -> bytes:
        """将当前实例序列化为 Tars 二进制格式.

        Returns:
            包含序列化数据的 bytes 对象。
        """
        ...

    @classmethod
    def decode(cls, data: bytes) -> Self:
        """从 Tars 二进制数据反序列化为类实例.

        Args:
            data: 包含 Tars 编码数据的 bytes 对象。

        Returns:
            反序列化的类实例。
        """
        ...

def encode(obj: Any) -> bytes:
    """将 Tars Struct 对象序列化为 Tars 二进制格式.

    Args:
        obj: 继承自 `Struct` 的类实例。

    Returns:
        包含序列化数据的 bytes 对象。

    Raises:
        TypeError: 如果对象不是有效的 Tars Struct。
    """
    ...

def decode(cls: type[_StructT], data: bytes) -> _StructT:
    """从 Tars 二进制数据反序列化为类实例.

    Args:
        cls: 目标类（继承自 `Struct`）。
        data: 包含 Tars 编码数据的 bytes 对象。

    Returns:
        反序列化的类实例。

    Raises:
        TypeError: 如果类未注册 Schema。
        ValueError: 如果数据格式不正确。
    """
    ...

def encode_raw(obj: TarsDict) -> bytes:
    """将 TarsDict 编码为 Tars 二进制格式.

    Args:
        obj: 一个字典，映射 tag (int) 到 Tars 值。

    Returns:
        编码后的字节对象。
    """
    ...

def decode_raw(data: bytes) -> TarsDict:
    """将字节解码为 TarsDict.

    Args:
        data: 包含 Tars 编码数据的 bytes 对象。

    Returns:
        解码后的 TarsDict。
    """
    ...

def probe_struct(data: bytes) -> TarsDict | None:
    """尝试将字节数据递归解析为 Tars 结构.

    这是一个启发式工具，用于探测一段二进制数据是否恰好是有效的 Tars 序列化结构。
    它不仅检查格式，还会验证是否完全消费了数据。

    Args:
        data: 可能包含 Tars 结构的二进制数据。

    Returns:
        如果解析成功且数据完整，返回 TarsDict；否则返回 None。
    """
    ...
