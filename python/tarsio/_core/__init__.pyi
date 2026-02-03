"""Tarsio 的核心 Rust 扩展模块.

本模块包含 TARS 协议处理的高性能 Rust 实现，包括 Schema 编译器、注册表和编解码接口。
    """
    ...

def encode_raw(obj: dict[int, Any]) -> bytes:
    """Raw API: encode a TarsDict (dict[int, TarsValue]) to bytes.

    Args:
        obj: A dict mapping tags (int) to Tars values.

    Returns:
        Encoded bytes.
    """
    ...

def decode_raw(data: bytes) -> dict[int, Any]:
    """Raw API: decode bytes into a TarsDict (dict[int, TarsValue]).

    Args:
        data: Tars encoded bytes.

    Returns:
        A dict mapping tags (int) to decoded Tars values.
    """
    ...


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
    """将 Tars Struct 对象序列化为 Tars 二进制格式 (codec-style API).

    Args:
        obj: 继承自 `Struct` 的类实例。

    Returns:
        包含序列化数据的 bytes 对象。

    Raises:
        TypeError: 如果对象不是有效的 Tars Struct。
    """
    ...

def decode(cls: type[_StructT], data: bytes) -> _StructT:
    """从 Tars 二进制数据反序列化为类实例 (codec-style API).

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
