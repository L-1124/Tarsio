# 使用总览

本页给出 Tarsio 的推荐使用路径: 先定义 `Struct`, 再进行编解码。
若数据结构未知,再切换到 Raw 模式和调试工具。

## 示例代码

```python
from tarsio import Struct, TarsDict, decode, encode, field

class User(Struct):
    id: int = field(tag=0)
    name: str = field(tag=1)

data = encode(User(id=7, name="Ada"))
restored = User.decode(data)
raw = decode(data)  # TarsDict
assert restored.name == "Ada"
assert isinstance(raw, TarsDict)
```

## 核心概念

### Schema 模式优先

* 业务对象优先使用 `Struct`。
* `User.decode(data)` 返回类型稳定,适合服务主链路。
* 字段约束失败时抛 `ValidationError`。
* `bytes` 字段可直接接收 `bytearray`、`memoryview`，统一按 `SimpleList(bytes)` 编码。
* `Struct`/`TarsDict` 字段可通过 `field(wrap_simplelist=True)` 按“先正常编码，再包装为 `SimpleList(bytes)`”输出。
* 解码输入支持 bytes-like（`bytes`、`bytearray`、`memoryview`）。

### Raw 模式用于边界输入

* `decode(data)` 返回 `TarsDict`,适合动态协议或网关透传。
* Raw 模式下普通 `dict` 按 `Map` 语义处理,`TarsDict` 按 `Struct` 语义处理。
* Raw/Any 路径中 `bytearray`、`memoryview` 也按 `bytes` 语义编码。

### 调试与可视化

* `decode_trace(data)` 可查看 tag、类型与路径。
* `probe_struct(data)` 可快速探测 bytes 是否可解析为完整 Struct。
* CLI 可直接读取 hex 或文件并输出 tree/json。

## 注意事项

* 对外部输入建议先做大小限制,再进入解码。
* 业务逻辑不要依赖 `decode_trace` 输出格式。
* 当模型稳定后,优先走 schema 模式,减少运行时分派。
* 非连续 `memoryview` 会先拷贝为连续 `bytes` 再编码。
* `wrap_simplelist=True` 字段在解码时是严格模式：若 wire 不是 `SimpleList(bytes)` 会直接报错。
