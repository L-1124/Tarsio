# 类型内省

`tarsio.inspect` 提供运行时类型内省与解析能力。
它会将 Python 类型标注翻译为可编程的中间表示,用于 Schema 编译与调试。

## 基本用法

```python
from typing import Annotated, Optional
from tarsio import Struct
from tarsio import inspect as tinspect


class User(Struct):
    uid: Annotated[int, 0]
    name: Annotated[str, 1]
    score: Annotated[Optional[int], 2] = None


info = tinspect.struct_info(User)
assert info is not None
assert [f.name for f in info.fields] == ["uid", "name", "score"]
```

## 类型解析

```python
from tarsio import inspect as tinspect

assert tinspect.type_info(int).kind == "int"
assert tinspect.type_info(list[str]).kind == "list"
```

## 注意事项

* 支持的类型与 `tarsio.inspect.TypeInfo` 分支一致，包含：
    * 标量：`int`、`float`、`bool`、`str`、`bytes`、`Any`、`None`。
    * 容器：`list`、`tuple`、`tuple[T, ...]`、`dict`、`set`/`frozenset`。
    * 组合：`Optional`、`Union`。
    * 结构：`Struct` 子类。
    * 枚举：`Enum`（按 `value` 的内层类型解析）。
* `typing.Annotated` 中的 `Meta` 约束会被解析到 `constraints`。
* 不支持的类型或未支持的前向引用会抛出 `TypeError`。
