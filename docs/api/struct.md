# Struct 与字段 API

本页汇总 `Struct` 建模相关的公共 API。
行为细节以类型注解、默认值和配置项的可观察结果为准。

## 示例代码

```python
from typing import Annotated
from tarsio import Meta, Struct, field

class User(Struct, frozen=True, omit_defaults=True):
    id: int = field(tag=0)
    name: str = field(tag=1)
    score: Annotated[int, Meta(ge=0)] = field(tag=2, default=0)
```

## 核心概念

* `Struct` 是 schema 入口，自动生成构造、比较与编码行为。
* `field` 用于声明 Tag、默认值、`default_factory` 与 `wrap_simplelist`。
* `Meta` 描述解码约束，失败时抛 `ValidationError`。
* `StructConfig` 记录类定义时启用的配置快照。
* `TarsDict` 是 Raw Struct 语义容器，不等同于普通 `dict`。
* Raw 路径下，`Struct` 可在任意嵌套位置编码，且 `StructBegin` 在任意嵌套层级统一还原为 `TarsDict`。
* 对 `Struct`/`TarsDict` 字段可使用 `field(wrap_simplelist=True)`：先按原类型编码，再将该字段包装为 `SimpleList(bytes)`。

## 注意事项

* `Struct` 配置会影响构造行为与编码结果，建议在模型层统一约定。
* `__post_init__` 中抛 `TypeError`/`ValueError` 会被视为校验失败路径。
* 协议演进中如需严格拒绝未知字段，可使用 `forbid_unknown_tags=True`。
* `wrap_simplelist=True` 仅支持 `Struct`/`TarsDict` 字段，解码严格要求 wire 为 `SimpleList(bytes)`，不做回退。

## API 参考

::: tarsio.Struct
    options:
      members:
        - encode
        - decode

::: tarsio.StructMeta
    options:
      members: false

::: tarsio.StructConfig

::: tarsio.Meta

::: tarsio.field

::: tarsio.NODEFAULT

::: tarsio.TarsDict
