# 序列化

介绍 Tarsio 的编解码 API，包括强类型 `Struct` 模式与无 Schema 的 `Raw` 模式。
核心函数：`encode`, `decode`, `encode_raw`, `decode_raw`。

## Struct 模式

适用于已定义 `Struct` 类的情况。这是推荐的使用方式，提供类型安全与校验。

### 编码 (Encode)

```python
from typing import Annotated
from tarsio import Struct, encode

class User(Struct):
    uid: Annotated[int, 0]

user = User(uid=123)
data = encode(user)
# data 是 bytes 类型
```

### 解码 (Decode)

```python
from tarsio import decode

# 将 bytes 还原为 User 对象
user = decode(User, data)
print(user.uid)  # 123
```

## Raw 模式

适用于没有 Schema 定义，或需要动态处理 Tars 数据的场景。
数据以 `TarsDict` (继承自 `dict` 的特殊类型) 的形式存在，Key 为 Tag ID (int)。

### Raw 编码

```python
from tarsio import TarsDict, encode_raw

# 必须使用 TarsDict 包裹以声明这是一个结构体
payload = TarsDict({
    0: 123,           # Tag 0: int
    1: "Alice",       # Tag 1: str
    2: [1, 2, 3]      # Tag 2: list
})
data = encode_raw(payload)
```

### Raw 解码

```python
from tarsio import decode_raw, TarsDict

# 还原为 TarsDict
data_dict = decode_raw(data)
# TarsDict({0: 123, 1: 'Alice', 2: [1, 2, 3]})
assert isinstance(data_dict, TarsDict)
```

> **注意**: 由于 Tars 协议不包含字段名信息，Raw 模式只能还原 Tag ID 和基本值类型。

### TarsDict 详解

在 Raw 模式下，Tars 结构体被严格表示为 **TarsDict**。

#### 结构体 vs 映射 (Struct vs Map)

Tars 协议同时支持 Map (字典) 和 Struct (结构体)。Tarsio 通过 Python 类型来严格区分二者：

* **Struct**: 必须是 `TarsDict` 实例（其 Key 为 `int` 类型的 Tag）。
* **Map**: 普通的 `dict` 实例。

这一规则消除了以往基于 Key 类型的模糊推断。

#### 示例

```python
from tarsio import TarsDict, encode_raw

payload = TarsDict({
    0: 1001,
    # 值也是 TarsDict，表示嵌套 Struct
    1: TarsDict({0: "Alice", 1: "Bob"}),
    # 值是普通 dict，表示 Map<string, int>
    2: {"math": 90, "english": 85}
})

encode_raw(payload)
```

#### 限制

* Raw 模式无法区分 `int` 的具体精度 (int8/16/32/64)，解码时会根据数值大小自动选择。
* Raw 模式解码 `SimpleList` (vector<byte>) 时，如果内容是有效 UTF-8，会还原为 `str`；否则返回 `bytes`。

## 异常处理

在反序列化过程中可能抛出以下异常：

| 异常类型 | 说明 |
| :--- | :--- |
| `tarsio.ValidationError` | **数据逻辑错误**。违反了 Meta 约束（如数值越界、正则不匹配）。 |
| `ValueError` | **数据格式错误**。二进制数据截断、畸形或递归过深。 |
| `TypeError` | **类型错误**。Schema 定义与实际数据类型不兼容。 |

### 示例

```python
from tarsio import decode, ValidationError

try:
    user = decode(User, malicious_data)
except ValidationError as e:
    print(f"校验失败: {e}")
except ValueError:
    print("数据损坏")
```
