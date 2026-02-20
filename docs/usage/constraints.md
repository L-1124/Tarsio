# 字段约束

Tarsio 使用 `typing.Annotated` + `Meta` 表达字段约束。
约束在解码阶段执行,用于阻断无效输入进入业务层。

## 示例代码

```python
from typing import Annotated
from tarsio import Meta, Struct, field

class Product(Struct):
    price: Annotated[int, Meta(gt=0)] = field(tag=0)
    code: Annotated[str, Meta(min_len=1, max_len=16)] = field(tag=1)
```

## 核心概念

### 约束执行时机

* 约束在 decode 过程中校验。
* 校验失败抛 `ValidationError`。
* 编码阶段主要关注类型与格式,不是业务校验主入口。

### 常见约束项

| 约束 | 含义 |
| --- | --- |
| `gt` / `ge` | 数值下界（严格/非严格）。 |
| `lt` / `le` | 数值上界（严格/非严格）。 |
| `min_len` | 字符串/容器最小长度。 |
| `max_len` | 字符串/容器最大长度。 |
| `pattern` | 字符串正则匹配。 |

### 约束设计建议

* 把协议约束与业务规则分层: 协议约束放 `Meta`,复杂业务规则放服务层。
* 错误消息建议直接返回给调用方或记录日志,便于排查输入问题。

## 注意事项

* 约束越严格,升级成本越高;调整前建议补充兼容性测试。
* `pattern` 只适合稳定规则,频繁变更规则建议改为业务层判断。
