# Structs

`Struct` 是 Tarsio 推荐的数据建模方式。
本页仅覆盖当前版本已支持的结构体能力。

## 默认值

支持静态默认值和 `default_factory`。
缺失字段在解码时也会使用默认值。

```python
from tarsio import Struct, field

class Cache(Struct):
    owner: str = field(tag=0, default="system")
    items: list[int] = field(tag=1, default_factory=list)
```

## 初始化后处理

可定义 `__post_init__` 做初始化后校验。
在解码路径中，`TypeError`/`ValueError` 会转换为 `ValidationError`。

```python
from tarsio import Struct, field

class Interval(Struct):
    low: int = field(tag=0)
    high: int = field(tag=1)

    def __post_init__(self) -> None:
        if self.low > self.high:
            raise ValueError("`low` 不能大于 `high`")
```

## 字段顺序

* 构造签名和 `__match_args__` 按 Tag 顺序排列。
* 显式 Tag 使用 `field(tag=...)`，未显式时自动分配。
* 建议稳定模型使用显式 Tag。

## 类型校验

* 构造对象时不会做完整强校验。
* 解码到 `Struct` 时会按类型注解和 `Meta` 约束校验。

## 模式匹配

`Struct` 提供 `__match_args__`，可用于 `match/case`。

```python
from typing import Annotated
from tarsio import Struct

class Point(Struct):
    y: Annotated[int, 1]
    x: Annotated[int, 0]

p = Point(x=1, y=2)
match p:
    case Point(1, 2):
        pass
```

## 相等与排序

* 默认启用 `eq=True`，按字段值比较。
* `order=True` 时生成排序比较方法。

## 冻结实例

`frozen=True` 时实例不可变，且可哈希。

```python
from typing import Annotated
from tarsio import Struct

class User(Struct, frozen=True):
    id: Annotated[int, 0]
```

## 省略默认值

`omit_defaults=True` 时，编码会跳过值等于默认值的字段。
`repr_omit_defaults=True` 只影响显示，不影响编码。

## 禁止未知字段

`forbid_unknown_tags=True` 时，解码遇到未知 Tag 会报错。

## 运行时定义

支持运行时动态定义 `Struct` 子类,但不建议在无界循环中持续创建新类型。
这类模式可能触发类型对象长期存活并导致内存增长。

推荐做法:

* 优先使用静态定义的 `Struct` 类。
* 必须动态生成时,请缓存并复用已生成类型。
* 高风险场景建议放在独立 worker 进程并周期性重启。

## 元类

`Struct` 的元类是 `StructMeta`，负责在类创建期编译 schema 并生成签名/配置。
一般业务代码直接继承 `Struct` 即可。
