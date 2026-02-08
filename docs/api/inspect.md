# Inspect API

`tarsio.inspect` 相关定义。
本页展示类型内省与解析的 API 参考。

## 概览

`tarsio.inspect` 提供两类入口：

* `type_info(tp)`：解析类型标注，返回 `TypeInfo` 分支对象
* `struct_info(cls)`：解析结构体类型，返回 `StructInfo`

返回对象都包含 `kind` 字段，可用于分支判断。

::: tarsio.inspect
    options:
      members:
        - type_info
        - struct_info
        - TypeInfo
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
        - MapType
        - OptionalType
        - StructType
        - FieldInfo
        - StructInfo
        - Constraints
