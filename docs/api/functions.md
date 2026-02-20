# 函数与异常 API

本页汇总模块级函数与调试能力。
包括序列化入口、追踪工具以及异常类型。

## 示例代码

```python
from tarsio import Struct, decode_trace, encode, field

class User(Struct):
    id: int = field(tag=0)

payload = encode(User(1))
obj = User.decode(payload)
trace = decode_trace(payload)
assert obj.id == 1
assert trace.jce_type == "ROOT"
```

## 核心概念

* `encode`/`decode` 是统一入口，按输入决定 schema 或 Raw 路径。
* `decode_trace` 适合协议调试，可输出树状追踪信息。
* `probe_struct` 可快速判断 bytes 是否像完整 Struct。
* `ValidationError` 表示约束校验失败，不等同于二进制损坏。

## 注意事项

* 调试函数返回的是诊断视图，不保证和业务模型一一对应。
* 面向外部输入时应限制包体大小并做好异常分类处理。
* `decode` 的目标类型应明确，避免运行时分派歧义。

## API 参考

::: tarsio.encode

::: tarsio.decode

::: tarsio.probe_struct

::: tarsio.decode_trace

::: tarsio.TraceNode

::: tarsio.ValidationError
