# 序列化

介绍 Tarsio 的编解码 API，包括强类型 `Struct` 模式与无 Schema 的 `Raw` 模式。
核心函数：`encode`, `decode`。

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

# 将 bytes 还原为 User 对象 (data 在前)
user = decode(data, User)
print(user.uid)  # 123
```

## Raw 模式

适用于没有 Schema 定义，或需要动态处理 Tars 数据的场景。
Raw 模式用于没有 Schema 定义，或需要动态处理 Tars 数据的场景。
`encode` 会在遇到 `dict`、`list`、`tuple`、`set` 以及基本类型时自动切换到 Raw 编码。
解码时不提供目标类（只传入 bytes），则返回 Raw 结构。

### Raw 编码

```python
from tarsio import encode

payload = {
    0: 123,           # Tag 0: int
    1: "Alice",       # Tag 1: str
    2: [1, 2, 3]      # Tag 2: list
}
# 直接使用 encode，它会自动进入 Raw 模式
data = encode(payload)
```

### Raw 解码

```python
from tarsio import decode

# 还原为 Raw 结构 (单参数调用)
data_dict = decode(data)
# {0: 123, 1: 'Alice', 2: [1, 2, 3]}
```

> **注意**: `encode` 会根据输入类型自动选择 Schema 或 Raw。`decode` 的调用方式决定输出：提供目标类即 Schema 模式，不提供则走 Raw 模式。

### 结构体 vs 映射 (Struct vs Map)

#### 结构体 vs 映射 (Struct vs Map)

Tars 协议同时支持 Map (字典) 和 Struct (结构体)。Raw 模式下，编码规则由字典的 Key 决定：

* **Struct**: 字典的 Key 全部为 `int` 且在 0-255 范围内。
* **Map**: Key 含非 `int` 类型，或 `int` 超出 Tag 范围，或为空字典（默认为 Map）。

#### 示例

```python
from tarsio import encode

payload = {
    0: 1001,
    # 值也是 int-key dict，表示嵌套 Struct
    1: {0: "Alice", 1: "Bob"},
    # 值是普通 dict，表示 Map<string, int>
    2: {"math": 90, "english": 85}
}

encode(payload)
```

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
