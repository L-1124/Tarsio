# 类型内省

`tarsio.inspect` 提供运行时类型内省与解析能力。
它会将 Python 类型标注翻译为可编程的中间表示,用于 Schema 编译与调试。

## 基本用法

```python
from typing import Annotated, Optional
from tarsio import Struct, field
from tarsio import inspect as tinspect


class User(Struct):
    uid: int = field(tag=0)
    name: str  # 自动分配 tag=1
    score: Annotated[Optional[int], "nullable"] = field(tag=2, default=None)


info = tinspect.struct_info(User)
assert info is not None
assert [f.name for f in info.fields] == ["uid", "name", "score"]

typ = tinspect.type_info(User)
assert typ.kind == "struct"
assert [f.name for f in typ.fields] == ["uid", "name", "score"]
```

## 类型解析

```python
from tarsio import inspect as tinspect

assert tinspect.type_info(int).kind == "int"
assert tinspect.type_info(list[str]).kind == "list"
```

## 递归结构

```python
from typing import Optional
from tarsio import Struct, field
from tarsio import inspect as tinspect


class Node(Struct):
    val: int = field(tag=0)
    child: Optional["Node"] = field(tag=1, default=None)


info = tinspect.type_info(Node)
child_type = info.fields[1].type
assert child_type.kind == "optional"
assert child_type.inner_type.kind == "ref"
assert child_type.inner_type.cls is Node
```

## 注意事项

* 支持的类型与 `tarsio.inspect.TypeInfo` 分支一致，包含：
    * 标量：`int`、`float`、`bool`、`str`、`bytes`、`Any`、`None`。
    * 容器：`list`、`tuple`、`tuple[T, ...]`、`dict`、`set`/`frozenset`。
    * 组合：`Optional`、`Union`。
    * 结构：`Struct` 子类。
    * 循环引用：`RefType`（仅在递归结构中出现）。
    * 枚举：`Enum`（按 `value` 的内层类型解析）。
* `typing.Annotated` 中的 `Meta` 约束会被解析到 `constraints`。
* 字段 tag 可来自 `field(tag=...)`、自动分配或 `Annotated[T, tag]` 兼容语法。
* 不支持的类型或未支持的前向引用会抛出 `TypeError`。
