# 定义模型

使用 `Struct` 与 `Annotated` 定义 Tars 结构体。
本页介绍如何定义字段类型、添加元数据约束、配置默认值以及处理嵌套结构。

> **实现说明**: `Struct` 基类由 Rust 扩展实现，类创建期由 `StructMeta` 编译 Schema 与处理 `__slots__`。
> `tarsio.Struct` 由 Rust 侧创建并从 `tarsio._core` 公开导出。

## 基础定义

使用 Python 标准库 `typing.Annotated` 将 JCE Tag 绑定到类型上。

### 语法结构

```python
from typing import Annotated
from tarsio import Struct

class User(Struct):
    uid: Annotated[int, 0]
    name: Annotated[str, 1]
```

* **Annotated[T, N]**: 定义一个类型为 `T`，Tag 为 `N` 的字段。
* **N**: 必须是 0-255 之间的整数。

## 字段类型

Tarsio 支持以下标准 Python 类型与 Tars 类型的映射：

| Python 类型 | Tars 类型 | 说明 |
| :--- | :--- | :--- |
| `int` | `int8` / `int16` / `int32` / `int64` | 根据数值大小自动选择最紧凑的编码 |
| `float` | `float` / `double` | 对应 JCE 的浮点数 |
| `bool` | `int8` (0 或 1) | Tars 无原生 bool，映射为 0/1 |
| `str` | `String1` / `String4` | 自动处理长度前缀 |
| `bytes` | `SimpleList` | 对应 `vector<byte>` |
| `list[T]` | `List<T>` | 列表 |
| `dict[K, V]` | `Map<K, V>` | 映射 |

### 容器类型示例

```python
from typing import Annotated
from tarsio import Struct

class Group(Struct):
    # 列表: vector<string>
    tags: Annotated[list[str], 0]

    # 映射: map<string, int>
    scores: Annotated[dict[str, int], 1]
```

## 元数据与校验 (Meta)

当需要对字段值进行约束时，使用 `tarsio.Meta` 替代纯整数 Tag。
校验逻辑在 **反序列化 (decode)** 时执行，失败抛出 `ValidationError`。

```python
from typing import Annotated
from tarsio import Struct, Meta

class Product(Struct):
    # 必须大于 0
    price: Annotated[int, Meta(tag=0, gt=0)]

    # 长度必须在 1-50 之间，且匹配正则
    code: Annotated[str, Meta(tag=1, min_len=1, max_len=50, pattern=r"^[A-Z]+$")]
```

> **注意**: `Meta(tag=N, ...)` 包含了 Tag 信息。同一字段必须二选一：要么用整数 `N`，要么用 `Meta(tag=N, ...)`。

## 默认值与可选字段

建议为所有字段提供默认值，通常是 `None`。

### 必填字段 (Required)

不提供默认值的字段为必填项。如果在解码数据中找不到对应的 Tag，且该字段没有默认值，将抛出异常。

```python
class Request(Struct):
    # 必填: 数据中必须包含 Tag 0
    token: Annotated[str, 0]
```

### 可选字段 (Optional)

使用 `Optional[T]` 或 `T | None` 并赋值 `= None`。

```python
from typing import Annotated, Optional
from tarsio import Struct

class Response(Struct):
    # 必填
    code: Annotated[int, 0]

    # 可选: 若数据中无 Tag 1，则 message 为 None
    message: Annotated[Optional[str], 1] = None
```

## 嵌套结构体

将另一个 `Struct` 子类作为类型注解即可实现嵌套。

```python
class Address(Struct):
    city: Annotated[str, 0]

class User(Struct):
    id: Annotated[int, 0]
    address: Annotated[Address, 1]  # 嵌套
```

## 模型配置

通过在类定义时传递参数来配置模型行为。

```python
class Config(Struct, frozen=True, forbid_unknown_tags=True):
    ...
```

* **frozen**: `bool` (默认 `False`)。
    * 设为 `True` 后实例变为不可变 (Immutable)，禁止修改属性。
    * 同时会生成 `__hash__` 方法，使实例可哈希（可作为 `dict` 的 Key 或放入 `set`）。
* **forbid_unknown_tags**: `bool` (默认 `False`)。
    * 设为 `True` 时，若解码数据中包含 Schema 未定义的 Tag，将抛出 `ValueError`。
    * 设为 `False` 时，会自动忽略未知 Tag（向前兼容）。
* **eq**: `bool` (默认 `True`)。
    * 设为 `True` 时，会生成 `__eq__` 方法，基于字段内容进行相等性比较。
    * 设为 `False` 时，回退到 Python 默认的对象身份比较 (Identity Comparison, `id(a) == id(b)`)。
* **order**: `bool` (默认 `False`)。
    * 设为 `True` 时，会生成 `__lt__`/`__le__`/`__gt__`/`__ge__` 方法。
    * 比较顺序按 **Tag 顺序** 逐字段进行。
* **omit_defaults**: `bool` (默认 `False`)。
    * 设为 `True` 时，编码会省略值等于默认值的字段以减少体积。
* **repr_omit_defaults**: `bool` (默认 `False`)。
    * 设为 `True` 时，`__repr__` 输出将自动省略那些值等于默认值的字段，使日志输出更简洁。
* **kw_only**: `bool` (默认 `False`)。
    * 设为 `True` 时，构造函数仅接受关键字参数（禁止位置参数），降低误用概率。
* **dict**: `bool` (默认 `False`)。
    * 设为 `False` 时，实例默认使用 `__slots__` 存储字段，不包含 `__dict__`，更省内存。
    * 设为 `True` 时，实例会包含 `__dict__`，允许动态挂载未声明的属性（适合保存私有运行时状态）。
* **weakref**: `bool` (默认 `False`)。
    * 设为 `True` 时，实例支持 `weakref.ref(obj)`。

> **注意**: `tag/tag_field/rename/array_like/gc/cache_hash` 等 msgspec 扩展配置当前不支持，
> 使用时将抛出 `TypeError`。

## 元数据与签名

`Struct` 类会暴露以下运行时元数据：

* ****struct_fields****: `tuple[str, ...]`
    * 字段名列表，按 **Tag 顺序** 排列。
* ****struct_config****: `StructConfig`
    * 当前结构体的配置对象，可从 `tarsio.StructConfig` 获取类型。
* ****signature****: `inspect.Signature`
    * 构造函数签名，按 **Tag 顺序** 生成；当必填字段出现在默认值字段之后时，后续字段会被标记为仅关键字参数。

### 示例

```python
class Point(Struct, frozen=True, eq=True):
    x: Annotated[int, 0]
    y: Annotated[int, 1]

p1 = Point(1, 2)
p2 = Point(1, 2)
assert p1 == p2
assert hash(p1) == hash(p2)
```

```python
class Config(Struct, repr_omit_defaults=True):
    host: Annotated[str, 0] = "localhost"
    port: Annotated[int, 1] = 8080
    debug: Annotated[bool, 2] = False

# 仅输出非默认值字段
conf = Config(port=9090)
print(conf)
# > Config(port=9090)
```

```python
class RuntimeState(Struct, dict=True, kw_only=True):
    id: Annotated[int, 0]

obj = RuntimeState(id=1)
obj._cache = {"k": "v"}  # dict=True 允许动态属性
```
