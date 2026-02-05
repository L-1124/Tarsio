# 定义模型

在 Tarsio 中，定义模型（Tars 结构体）的方式非常直观。您只需要创建一个继承自 `tarsio.Struct` 的类，并使用标准的 Python 类型注解定义字段即可。

## 基础定义

Tarsio 使用 Python 标准库 `typing.Annotated` 来指定 JCE Tag。这使得模型定义既保持了 Python 的原生风格，又提供了清晰的协议元数据。

### 语法结构

```python
Annotated[Type, Tag]
```

* **Type**: 字段的 Python 类型（如 `int`, `str`）。
* **Tag**: JCE 协议中的 Tag ID（整数，0-255）。

### 元数据模式 (Meta)

当你希望为字段增加约束校验（如数值范围、长度、正则）时，可以使用 `tarsio.Meta` 作为 Tag 的替代形式:

```python
from typing import Annotated
from tarsio import Meta, Struct

class User(Struct):
    uid: Annotated[int, Meta(tag=0, gt=0)]
    name: Annotated[str, Meta(tag=1, min_len=1, max_len=20)]
```

约束校验在 Rust 侧的反序列化阶段执行, 校验失败会抛出 `tarsio.ValidationError`。

> **重要**: Tag 的写法遵循"单一来源"策略, 同一字段必须二选一:
> `Annotated[T, 1]` 或 `Annotated[T, Meta(tag=1, ...)]`, 禁止混用。

### 示例

```python
from typing import Annotated
from tarsio import Struct

class User(Struct):
    uid: Annotated[int, 0]
    username: Annotated[str, 1]
    is_active: Annotated[bool, 2]
```

## 可选字段与默认值

如果字段是可选的（允许为 None），可以使用 `Optional` 或 `| None`。建议显式提供默认值（通常是 `None`）；未提供时，Tarsio 会将其视为默认 `None`。

```python
class Response(Struct):
    code: Annotated[int, 0]
    message: Annotated[str | None, 1] = None  # 可选字段
```

对于非 Optional 字段，如果您希望它有默认值，也可以直接赋值：

```python
class Config(Struct):
    retry_count: Annotated[int, 0] = 3
    debug_mode: Annotated[bool, 1] = False
```

### 解码语义（有 Schema）

下面用一个例子说明“字段缺失”时的行为：

```python
from typing import Annotated, Optional
from tarsio import Struct


class User(Struct):
    name: Annotated[str, 0] = "unknown"
    email: Annotated[str, 1]
    phone: Annotated[Optional[str], 2]
```

* 当 wire 缺失 `name` 时：解码结果使用默认值 `"unknown"`。
* 当 wire 缺失 `phone` 且未显式提供默认值时：解码结果为 `None`。
* 当 wire 缺失 `email`（非 Optional 且无默认值）时：解码会抛出错误。

### 编码策略（有 Schema）

* 编码端仅省略值为 None 的字段；不会因为“值等于默认值”而省略。

## 嵌套结构体

Tarsio 支持结构体的嵌套。只需将被嵌套的结构体类作为类型即可。

```python
class Address(Struct):
    city: Annotated[str, 0]
    street: Annotated[str, 1]

class UserProfile(Struct):
    uid: Annotated[int, 0]
    address: Annotated[Address, 1]  # 嵌套 Address
```

## 容器类型

支持标准的 `list`/`tuple` 以及 `dict`。

* `list[T]`: 对应 Tars 的 `List<T>`。
* `tuple[T, ...]`: 对应 Tars 的 `List<T>`。
* `dict[K, V]`: 对应 Tars 的 `Map<K, V>`。

```python
class Group(Struct):
    group_id: Annotated[int, 0]
    members: Annotated[list[str], 1]      # 字符串列表
    metadata: Annotated[dict[str, int], 2] # Map<String, Integer>
```

> **注意**: `bytes` 类型在 Tarsio 中对应 Tars 的 `SimpleList` (即 `vector<byte>`)，这是一种针对二进制数据的优化存储格式。

## 泛型支持

Tarsio 完美支持泛型结构体。这对于定义通用的包装器（如 `Response<T>`）非常有用。

```python
from typing import TypeVar, Generic

T = TypeVar("T")

class Box(Struct, Generic[T]):
    value: Annotated[T, 0]

# 使用具体类型实例化泛型
# Tarsio 会自动为 Box[int] 生成专门的 Schema
int_box = Box[int](value=42)
str_box = Box[str](value="Hello")
```

## 向前引用 (Forward References)

如果需要定义递归结构（例如链表节点），可以使用字符串形式的类型注解。

```python
class Node(Struct):
    value: Annotated[int, 0]
    next: Annotated["Node", 1] = None
```

## 最佳实践

1. **Tag 连续性**: 虽然 Tars 协议不强制 Tag 连续，但建议从 0 开始连续编号，以节省空间（Tag < 15 时有头部压缩优化）。
2. **类型注解**: 始终使用 `Annotated` 包裹类型和 Tag。未注解 Tag 的字段将被视为普通类属性，**不会**被序列化。
3. **继承**: 目前 Tarsio 尚不支持结构体继承（即从另一个 Struct 子类继承）。每个 Struct 应该是独立的定义。
