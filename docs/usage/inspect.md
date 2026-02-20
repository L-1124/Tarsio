# 类型内省

`tarsio.inspect` 用于在运行时查看类型被解析后的结果。
它适合做 schema 调试、自动文档生成和动态校验前置检查。

## 示例代码

```python
from tarsio import Struct, inspect as tinspect, field

class User(Struct):
    id: int = field(tag=0)
    name: str = field(tag=1)

info = tinspect.struct_info(User)
assert info is not None
assert [f.name for f in info.fields] == ["id", "name"]
```

## 核心概念

### `type_info(tp)`

返回 `TypeInfo` 分支对象，可通过 `kind` 做分支判断。

```python
from tarsio import inspect as tinspect

assert tinspect.type_info(int).kind == "int"
assert tinspect.type_info(list[str]).kind == "list"
```

### `struct_info(cls)`

返回 `StructInfo`，包含字段名、tag、默认值和 required/optional 信息。

```python
from tarsio import Struct, inspect as tinspect, field

class User(Struct):
    id: int = field(tag=0)

schema = tinspect.struct_info(User)
assert schema is not None
assert schema.fields[0].tag == 0
```

### 递归结构

递归结构会通过 `RefType` 表达引用关系，避免无限展开。

```python
from typing import Optional
from tarsio import Struct, inspect as tinspect, field

class Node(Struct):
    val: int = field(tag=0)
    next: Optional["Node"] = field(tag=1, default=None)

node_type = tinspect.type_info(Node)
assert node_type.kind == "struct"
```

## 注意事项

* 不支持的类型会抛 `TypeError`，建议在应用启动阶段提前检查。
* `struct_info` 对未具体化的泛型模板可能返回 `None`。
* 内省输出用于分析和工具集成，不应替代业务输入校验。
