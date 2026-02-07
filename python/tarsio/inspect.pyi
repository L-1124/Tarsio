"""Tarsio 类型内省.

主要用途：

- 在开发阶段对 `typing.Annotated` 字段标注进行静态建模
- 提供 `type_info()` / `struct_info()` 的返回对象结构（`kind` 分支 + 关联字段）
"""

from typing import Any, TypeAlias, TypeVar

T = TypeVar("T")

class Constraints:
    """字段约束信息.

    这些约束通常来自 `tarsio.Meta(...)`（如 `gt/min_len/pattern`），用于在解码时进行校验。
    """

    gt: float | None
    lt: float | None
    ge: float | None
    le: float | None
    min_len: int | None
    max_len: int | None
    pattern: str | None

class IntType:
    """整数类型（JCE int 家族的抽象视图）."""

    constraints: Constraints | None
    kind: str

class StrType:
    """字符串类型."""

    constraints: Constraints | None
    kind: str

class FloatType:
    """浮点类型（运行时对应 double 语义）."""

    constraints: Constraints | None
    kind: str

class BoolType:
    """布尔类型（在 JCE 编码层面通常以 int 表达）."""

    constraints: Constraints | None
    kind: str

class BytesType:
    """二进制类型（运行时会被视为 byte-list 的特殊形式）."""

    constraints: Constraints | None
    kind: str

class ListType:
    """列表类型：`list[T]`."""

    item_type: TypeInfo
    constraints: Constraints | None
    kind: str

class TupleType:
    """元组类型：仅支持同构 `tuple[T]`（运行时会按 list 处理）."""

    item_type: TypeInfo
    constraints: Constraints | None
    kind: str

class MapType:
    """映射类型：`dict[K, V]`."""

    key_type: TypeInfo
    value_type: TypeInfo
    constraints: Constraints | None
    kind: str

class OptionalType:
    """可选类型：`T | None` 或 `typing.Optional[T]`."""

    inner_type: TypeInfo
    constraints: Constraints | None
    kind: str

class StructType:
    """Struct 类型：字段类型为另一个 `tarsio.Struct` 子类."""

    cls: type
    constraints: Constraints | None
    kind: str

TypeInfo: TypeAlias = (
    IntType
    | StrType
    | FloatType
    | BoolType
    | BytesType
    | ListType
    | TupleType
    | MapType
    | OptionalType
    | StructType
)

class FieldInfo:
    """结构体字段信息."""

    name: str
    tag: int
    type: TypeInfo
    default: Any
    has_default: bool
    optional: bool
    required: bool
    constraints: Constraints | None

class StructInfo:
    """结构体信息（类级 Schema 视图）."""

    cls: type
    fields: tuple[FieldInfo, ...]

def type_info(tp: Any) -> TypeInfo:
    """将类型标注解析为 Tarsio 的类型内省结果.

    Args:
        tp: 需要解析的类型标注，支持内置类型（如 `int/str/bytes`）、容器类型
            （如 `list[T]`、`tuple[T]`、`dict[K, V]`）、Optional/Union 形式，
            以及 `typing.Annotated[T, ...]`（会解析其中的 `Meta` 约束信息）。

    Returns:
        解析后的 `TypeInfo` 实例，可通过其 `kind` 字段区分具体分支，并读取对应属性。

    Raises:
        TypeError: 当类型标注不受支持或包含未支持的前向引用时抛出。
    """

def struct_info(cls: type) -> StructInfo | None:
    """解析 Struct 类并返回字段定义信息.

    Args:
        cls: 需要解析的 `tarsio.Struct` 子类。

    Returns:
        `StructInfo` 对象，包含字段列表（按 tag 升序）；如果该类没有可用字段，
        或该类是未具体化的泛型模板，则返回 `None`。

    Raises:
        TypeError: 当字段缺少 tag、tag 重复、混用整数 tag 与 `Meta`，或字段类型不受支持时抛出。
    """
