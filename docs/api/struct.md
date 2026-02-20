# Struct API

`tarsio.Struct` 及其相关定义。

## 使用提示

* [`Struct`][tarsio.Struct] 的构造参数、默认值语义与配置选项说明以其 docstring 为准。
* [`StructConfig`][tarsio.StructConfig] 用于查看类定义后生效的配置快照（`__struct_config__`）。
* 涉及约束与校验失败时，异常语义请参考 [`ValidationError`][tarsio.ValidationError]。
* `Struct` 自动生成 `__replace__`、`__match_args__` 与 `__rich_repr__`，可用于不可变风格更新、模式匹配与 rich pretty-print。

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
