# 定义模型

使用 `Struct` 与类型注解定义 Tars 结构体。
本页介绍如何定义字段类型、添加元数据约束、配置默认值以及处理嵌套结构。

## 基础定义

字段 Tag 支持显式与隐式混合：

* **显式 tag**：使用 `field(tag=...)`。
* **隐式 tag**：未显式声明时按字段定义顺序自动分配。
* **混合使用**：允许在同一模型中混合显式与隐式字段。若隐式字段位于显式字段之后，会从该显式 tag 继续递增分配。

### 语法结构

```python
from tarsio import Struct, field

class User(Struct):
    uid: int = field(tag=0)  # 显式
    name: str                # 隐式 -> tag 1
```

* **field(tag=N)**: 定义显式 Tag（0-255）。
* **普通注解字段**: 由系统自动分配 Tag。
* **Annotated**: 仅用于附加 `Meta` 约束，不负责声明 Tag。

## 字段类型

Tarsio 支持以下标准 Python 类型与 Tars 类型的映射：

| Python 类型  | Tars 类型                            |               说明               |
|:-------------|:-------------------------------------|:--------------------------------:|
| `int`        | `int8` / `int16` / `int32` / `int64` | 根据数值大小自动选择最紧凑的编码 |
| `float`      | `float` / `double`                   |         对应 JCE 的浮点数        |
| `bool`       | `int8` (0 或 1)                      |   Tars 无原生 bool，映射为 0/1   |
| `str`        | `String1` / `String4`                |         自动处理长度前缀         |
| `bytes`      | `SimpleList`                         |        对应 `vector<byte>`       |
| `list[T]`    | `List<T>`                            |               列表               |
| `dict[K, V]` | `Map<K, V>`                          |               映射               |

### 容器类型示例

```python
from tarsio import Struct, field

class Group(Struct):
    # 列表: vector<string>
    tags: list[str] = field(tag=0)

    # 映射: map<string, int>
    scores: dict[str, int]
```

## 元数据与校验 (Meta)

当需要对字段值进行约束时，使用 `tarsio.Meta`。
校验逻辑在 **反序列化 (decode)** 时执行，失败抛出 `ValidationError`。

```python
from typing import Annotated
from tarsio import Struct, Meta, field

class Product(Struct):
    # 必须大于 0
    price: Annotated[int, Meta(gt=0)] = field(tag=0)

    # 长度必须在 1-50 之间，且匹配正则
    code: Annotated[str, Meta(min_len=1, max_len=50, pattern=r"^[A-Z]+$")] = field(tag=1)
```

## 默认值与可选字段

建议为所有字段提供默认值，通常是 `None`。

### 使用 `field` 指定默认值工厂

`field` 用于声明 `tag`、默认值与默认值工厂（`default_factory`）。

```python
from tarsio import Struct, field

class Cache(Struct):
    # 每个实例都会得到新的 list
    items: list[int] = field(tag=0, default_factory=list)
```

### 可变默认值规则（严格模式）

* 空可变字面量（`[]`、`{}`、`set()`、`bytearray()`）会自动转为 `default_factory` 语义。
* 非空可变默认值（如 `[1]`、`{"k": 1}`）会在类定义阶段抛出 `TypeError`。

### 必填字段 (Required)

不提供默认值的字段为必填项。如果在解码数据中找不到对应的 Tag，且该字段没有默认值，将抛出异常。

```python
class Request(Struct):
    # 必填: 数据中必须包含 Tag 0
    token: str = field(tag=0)
```

### 可选字段 (Optional)

使用 `Optional[T]` 或 `T | None` 并赋值 `= None`。

```python
from typing import Optional
from tarsio import Struct, field

class Response(Struct):
    # 必填
    code: int = field(tag=0)

    # 可选: 若数据中无 Tag 1，则 message 为 None
    message: Optional[str] = field(tag=1, default=None)
```

## 嵌套结构体

将另一个 `Struct` 子类作为类型注解即可实现嵌套。

```python
class Address(Struct):
    city: str = field(tag=0)

class User(Struct):
    id: int = field(tag=0)
    address: Address = field(tag=1)  # 嵌套
```

## 模型配置

通过在类定义时传递参数来配置模型行为。

```python
class Config(Struct, frozen=True, forbid_unknown_tags=True):
    ...
```

### 示例

```python
class Point(Struct, frozen=True, eq=True):
    x: int = field(tag=0)
    y: int = field(tag=1)

p1 = Point(1, 2)
p2 = Point(1, 2)
assert p1 == p2
assert hash(p1) == hash(p2)
```

```python
class Config(Struct, repr_omit_defaults=True):
    host: str = field(tag=0, default="localhost")
    port: int = field(tag=1, default=8080)
    debug: bool = field(tag=2, default=False)

# 仅输出非默认值字段
conf = Config(port=9090)
print(conf)
# > Config(port=9090)
```

```python
class RuntimeState(Struct, dict=True, kw_only=True):
    id: int = field(tag=0)

obj = RuntimeState(id=1)
obj._cache = {"k": "v"}  # dict=True 允许动态属性
```
