# Tarsio 文档

Tarsio 是一个由 Rust 核心驱动的 Python Tars (JCE) 协议库。

## 安装

```bash
pip install tarsio
```

## 示例代码

```python
from typing import Annotated
from tarsio import Meta, Struct, encode, field

class User(Struct):
    id: int = field(tag=0)
    name: str = field(tag=1)
    score: Annotated[int, Meta(ge=0)] = field(tag=2, default=0)

user = User(id=1, name="Ada")
data = encode(user)
restored = User.decode(data)
assert restored == user
```

## 核心概念

* `Struct` 是推荐入口。你声明类型注解，Tarsio 负责 schema 编译与编解码。
* `field(tag=...)` 用于显式指定 Tag；未指定时按字段顺序自动分配。
* `Annotated[T, Meta(...)]` 用于约束校验，解码阶段失败会抛出 `ValidationError`。
* 除了 schema 模式，Tarsio 也支持 Raw 模式用于动态协议分析。

## 注意事项

* 业务层优先使用稳定 schema，不建议在高频路径无界动态创建 `Struct` 子类。
* 调试未知二进制时，优先使用 `decode_trace` 与 CLI 的 `--format tree`。
* 约束与默认值会影响演进兼容性，升级前建议增加跨版本回归测试。
