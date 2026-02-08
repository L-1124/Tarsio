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
    """整数类型（JCE int 家族的抽象视图）.

    编码：`ZeroTag` 或 `Int1/Int2/Int4/Int8`。
    """

    constraints: Constraints | None
    kind: str

class StrType:
    """字符串类型.

    编码：`String1` 或 `String4`。
    """

    constraints: Constraints | None
    kind: str

class FloatType:
    """浮点类型（运行时对应 double 语义）.

    编码：`ZeroTag` 或 `Double`。
    """

    constraints: Constraints | None
    kind: str

class BoolType:
    """布尔类型（在 JCE 编码层面通常以 int 表达）.

    编码：`ZeroTag` 或 `Int1/Int2/Int4/Int8`。
    """

    constraints: Constraints | None
    kind: str

class BytesType:
    """二进制类型（运行时会被视为 byte-list 的特殊形式）.

    编码：`SimpleList`。
    """

    constraints: Constraints | None
    kind: str

class AnyType:
    """动态类型（运行时根据值推断编码）.

    编码：运行时按值类型选择具体 TarsType。
    """

    constraints: Constraints | None
    kind: str

class NoneType:
    """None 类型（通常仅出现在 Union/Optional 中）.

    编码：不能直接编码，仅用于 Optional/Union 的语义分支。
    """

    constraints: Constraints | None
    kind: str

class EnumType:
    """Enum 类型.

    编码：取 `value` 的内层类型映射。
    """

    cls: type
    value_type: TypeInfo
    constraints: Constraints | None
    kind: str

class UnionType:
    """Union 类型（非 Optional 形式）.

    编码：按变体顺序匹配实际值，直接按匹配类型编码。
    """

    variants: tuple[TypeInfo, ...]
    constraints: Constraints | None
    kind: str

class ListType:
    """列表类型：`list[T]`.

    编码：`List`（若元素类型为 int 且值为 bytes，则使用 `SimpleList`）。
    """

    item_type: TypeInfo
    constraints: Constraints | None
    kind: str

class TupleType:
    """元组类型：固定长度、固定类型 `tuple[T1, T2, ...]`.

    编码：`List`。
    """

    items: tuple[TypeInfo, ...]
    constraints: Constraints | None
    kind: str

class VarTupleType:
    """元组类型：可变长度、元素类型相同 `tuple[T, ...]`.

    编码：`List`（若元素类型为 int 且值为 bytes，则使用 `SimpleList`）。
    """

    item_type: TypeInfo
    constraints: Constraints | None
    kind: str

class MapType:
    """映射类型：`dict[K, V]`.

    编码：`Map`。
    """

    key_type: TypeInfo
    value_type: TypeInfo
    constraints: Constraints | None
    kind: str

class SetType:
    """集合类型：`set[T]` / `frozenset[T]`.

    编码：`List`，解码为 set。
    """

    item_type: TypeInfo
    constraints: Constraints | None
    kind: str

class OptionalType:
    """可选类型：`T | None` 或 `typing.Optional[T]`.

    编码：None 时不写 tag，有值时按内层类型映射。
    """

    inner_type: TypeInfo
    constraints: Constraints | None
    kind: str

class StructType:
    """Struct 类型：字段类型为另一个 `tarsio.Struct` 子类.

    编码：`StructBegin` ... `StructEnd`。
    """

    cls: type
    constraints: Constraints | None
    kind: str

TypeInfo: TypeAlias = (
    IntType
    | StrType
    | FloatType
    | BoolType
    | BytesType
    | AnyType
    | NoneType
    | EnumType
    | UnionType
    | ListType
    | TupleType
    | VarTupleType
    | MapType
    | SetType
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
