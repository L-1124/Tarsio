# Schema 演进

本页总结 Tarsio 在版本演进中的稳定实践。
目标是在升级字段时保持新旧版本互通,并降低回滚风险。

## 示例代码

```python
from tarsio import Struct, encode, field

class V1(Struct):
    id: int = field(tag=0)

class V2(Struct):
    id: int = field(tag=0)
    name: str | None = field(tag=1, default=None)

data = encode(V2(id=1, name="Ada"))
old = V1.decode(data)
assert old.id == 1
```

## 核心概念

### 演进规则

* 不复用旧 Tag 给新语义。
* 新增字段优先使用可选字段并提供默认值。
* 删除字段时保留 Tag 记录,避免后续误用。

### 兼容行为

* 旧模型读取新数据时,未知 Tag 会被跳过。
* 新模型读取旧数据时,缺失字段使用默认值。
* 必填字段缺失会触发解码失败。

### 严格模式

`forbid_unknown_tags=True` 会把未知字段视为错误。
这适合强约束内部协议,但会降低跨版本容错能力。

## 注意事项

* 版本发布前建议做双向回归: `V1 -> V2` 与 `V2 -> V1`。
* 改动默认值会影响业务语义,需要单独评审。
* 协议变更建议记录在 changelog 并附 Tag 变更表。
