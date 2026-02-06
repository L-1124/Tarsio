"""Tarsio 公共 Python API.

本模块导出 Tarsio 的核心类型与编解码接口，包含基类 `Struct`、元类
`StructMeta`、配置对象 `StructConfig` 以及编码/解码函数。
"""

from inspect import Signature
from typing import Any, ClassVar, TypeAlias, TypeVar

from typing_extensions import dataclass_transform

_StructT = TypeVar("_StructT")
_SM = TypeVar("_SM", bound="StructMeta")
TarsDict: TypeAlias = dict[int, Any]

__all__ = [
    "Meta",
    "Struct",
    "StructConfig",
    "StructMeta",
    "TarsDict",
    "ValidationError",
    "decode",
    "decode_raw",
    "encode",
    "encode_raw",
    "probe_struct",
]

class ValidationError(ValueError):
    """解码阶段的校验错误.

    由 `Meta` 约束或 Schema 校验失败触发。
    """

    ...

class Meta:
    """字段元数据与约束定义.

    用于在 `Annotated` 中替代纯整数 Tag, 提供额外的运行时校验.

    Examples:
        ```python
        from typing import Annotated
        from tarsio import Struct, Meta

        class Product(Struct):
            # 价格必须 > 0
            price: Annotated[int, Meta(tag=0, gt=0)]
            # 代码必须是 1-10 位大写字母
            code: Annotated[
                str, Meta(tag=1, min_len=1, max_len=10, pattern=r"^[A-Z]+$")
            ]
        ```
    """
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
    ) -> None:
        """初始化字段元数据.

        Args:
            tag: 字段 Tag, 范围 0-255。
            gt: 数值必须大于该值。
            lt: 数值必须小于该值。
            ge: 数值必须大于或等于该值。
            le: 数值必须小于或等于该值。
            min_len: 长度下限。
            max_len: 长度上限。
            pattern: 正则表达式约束。
        """
        ...

    tag: int | None
    gt: float | None
    lt: float | None
    ge: float | None
    le: float | None
    min_len: int | None
    max_len: int | None
    pattern: str | None

@dataclass_transform(
    eq_default=True,
    order_default=False,
    kw_only_default=False,
    frozen_default=False,
)
class StructMeta(type):
    """Struct 的元类.

    负责在类创建期编译 Schema、处理默认值与 `__slots__`。
    仅支持 Tars/JCE 相关配置项，以下 msgspec 配置**不支持**：
    `tag/tag_field/rename/array_like/gc/cache_hash`，传入会抛 `TypeError`。
    """

    __struct_fields__: ClassVar[tuple[str, ...]]
    @property
    def __signature__(self) -> Signature: ...
    @property
    def __struct_config__(self) -> StructConfig: ...
    def __new__(
        mcls: type[_SM],
        name: str,
        bases: tuple[type, ...],
        namespace: dict[str, Any],
        /,
        *,
        frozen: bool = ...,
        order: bool = ...,
        forbid_unknown_tags: bool = ...,
        eq: bool = ...,
        omit_defaults: bool = ...,
        repr_omit_defaults: bool = ...,
        kw_only: bool = ...,
        dict: bool = ...,
        weakref: bool = ...,
        **kwargs: Any,
    ) -> _SM:
        """创建 Struct 子类并编译 Schema.

        Args:
            mcls: 元类本身。
            name: 类名。
            bases: 基类。
            namespace: 类命名空间。
            frozen: 是否冻结实例。
            order: 是否生成排序比较方法。
            forbid_unknown_tags: 是否禁止未知 Tag。
            eq: 是否生成相等比较。
            omit_defaults: 编码时是否省略默认值字段。
            repr_omit_defaults: repr 是否省略默认值字段。
            kw_only: 是否只允许关键字参数构造。
            dict: 是否为实例保留 `__dict__`。
            weakref: 是否支持弱引用。
            **kwargs: 预留扩展配置。

        Returns:
            新创建的 Struct 子类。
        """
        ...

class StructConfig:
    """Struct 的配置对象.

    可通过 `Struct.__struct_config__` 或实例的 `__struct_config__` 访问。
    不支持的配置项会保持默认值（False/None）。
    """

    frozen: bool
    eq: bool
    order: bool
    kw_only: bool
    array_like: bool
    gc: bool
    repr_omit_defaults: bool
    omit_defaults: bool
    weakref: bool
    dict: bool
    cache_hash: bool
    tag_field: str | None
    tag: Any | None
    rename: Any | None

class Struct(metaclass=StructMeta):
    """Tarsio Struct 基类.

    使用 `typing.Annotated[T, tag]` 声明字段与 Tag。
    运行时提供 `__struct_fields__`（按 Tag 顺序）与 `__struct_config__`。

    Examples:
        ```python
        from typing import Annotated
        from tarsio import Struct

        class User(Struct):
            uid: Annotated[int, 0]
            name: Annotated[str, 1]
            score: Annotated[int, 2] = 0

        user = User(uid=1, name="Ada")
        data = user.encode()
        restored = User.decode(data)
        assert restored == user
        ```
    """

    __struct_fields__: ClassVar[tuple[str, ...]]
    __struct_config__: ClassVar[StructConfig]
    def __init__(self, *args: Any, **kwargs: Any) -> None:
        """构造 Struct 实例.

        Args:
            *args: 位置参数, 按字段 Tag 顺序。
            **kwargs: 关键字参数, 字段名匹配。

        Raises:
            TypeError: 参数数量不匹配或缺少必填字段。
        """
        ...
    def encode(self) -> bytes:
        """将当前实例编码为 Tars 二进制数据.

        Returns:
            编码后的 bytes。

        Raises:
            ValueError: 缺少必填字段或类型不匹配。
        """
        ...
    @classmethod
    def decode(cls: type[_StructT], data: bytes) -> _StructT:
        """将 Tars 二进制数据解码为当前类实例.

        Args:
            data: 待解码的 bytes。

        Returns:
            解码得到的实例。

        Raises:
            TypeError: 目标类未注册 Schema。
            ValueError: 数据格式不正确或缺少必填字段。
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
