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
数据以 `dict[int, Any]` (即 TarsDict) 的形式存在，Key 为 Tag ID。

### Raw 编码

```python
from tarsio import encode_raw

payload = {
    0: 123,           # Tag 0: int
    1: "Alice",       # Tag 1: str
    2: [1, 2, 3]      # Tag 2: list
}
data = encode_raw(payload)
```

### Raw 解码

```python
from tarsio import decode_raw

# 还原为字典
data_dict = decode_raw(data)
# {0: 123, 1: 'Alice', 2: [1, 2, 3]}
```

> **注意**: 由于 Tars 协议不包含字段名信息，Raw 模式只能还原 Tag ID 和基本值类型。

### TarsDict 详解

在 Raw 模式下，Tars 结构体被表示为 `dict[int, Any]`，通常称为 **TarsDict**。

#### 结构体 vs 映射 (Struct vs Map)

Tars 协议同时支持 Map (字典) 和 Struct (结构体)。
在 **Schema 模式** 下，这通过类型定义严格区分：

* `class User(Struct)` -> Struct
* `Annotated[dict[K, V], Tag]` -> Map

但在 **Raw 模式** 下，输入都是 Python `dict`。Tarsio 通过 Key 的类型来推断意图：

* **Struct**: 字典的 Key **全部**为 `int` (且在 0-255 范围内)。
* **Map**: 字典的 Key 包含非 `int` 类型，或 `int` 超出 Tag 范围，或为空字典（默认为 Map）。

#### 嵌套结构处理

这种推断是递归的。当一个字典的值也是字典时：

```python
payload = {
    0: 1001,
    # Key 为 int (0, 1)，被推断为嵌套 Struct
    1: {0: "Alice", 1: "Bob"},
    # Key 为 str，被推断为 Map<string, int>
    2: {"math": 90, "english": 85}
}
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
