# Inspect API

本页提供 `tarsio.inspect` 的参考文档。
可用于理解类型解析结果、字段布局和递归引用结构。

## 示例代码

```python
from tarsio import Struct, inspect as tinspect, field

class User(Struct):
    id: int = field(tag=0)
    name: str = field(tag=1)

info = tinspect.struct_info(User)
assert info is not None
assert info.fields[0].name == "id"
```

## 核心概念

* `type_info(tp)`: 解析任意支持类型，返回带 `kind` 的 `TypeInfo`。
* `struct_info(cls)`: 返回 `StructInfo`，描述字段、tag 与默认值语义。
* `FieldInfo` 是 `Field` 的兼容别名，适合渐进迁移。

## 注意事项

* 内省 API 面向开发与诊断，不替代业务解码逻辑。
* 不支持的标注会抛 `TypeError`，建议在启动阶段完成类型预检。
* 递归结构会出现 `RefType` 分支，需要在工具层显式处理。

## API 参考

::: tarsio.inspect
    options:
      members:
        - type_info
        - struct_info
        - TypeInfo
        - Type
        - BasicType
        - CompoundType
        - IntType
        - StrType
        - FloatType
        - BoolType
        - BytesType
        - AnyType
        - NoneType
        - EnumType
        - UnionType
        - ListType
        - TupleType
        - VarTupleType
        - MapType
        - SetType
        - OptionalType
        - StructType
        - RefType
        - Field
        - FieldInfo
        - StructInfo
