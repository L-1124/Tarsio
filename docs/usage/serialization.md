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

Tars 协议同时支持 Map (字典) 和 Struct (结构体)。Raw 模式下，编码规则由容器类型决定：

* **Struct**: 必须使用 `TarsDict` 包装字典数据。`TarsDict` 的键必须是 `int` 类型且在 0-255 范围内。
* **Map**: 普通 Python `dict` 一律视为 Map，无论键的类型如何。

#### 示例

```python
from tarsio import encode, TarsDict

payload = TarsDict({
    0: 1001,
    # 值也是 TarsDict，表示嵌套 Struct
    1: TarsDict({0: "Alice", 1: "Bob"}),
    # 值是普通 dict，表示 Map<string, int>
    2: {"math": 90, "english": 85}
})

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
except ValueError as e:
    # ValueError 现包含精确的错误路径，如 "Error at <root>.user.id: ..."
    print(f"数据格式错误: {e}")
```

## 调试与内省

对于复杂的二进制协议分析，Tarsio 提供了专门的调试工具。

### Trace 模式

当你不确定二进制数据的结构，或者需要进行协议逆向时，可以使用 `decode_trace`。它会生成一棵包含原始 Tag、Tars 类型及值的 `TraceNode` 树。

```python
from tarsio import decode_trace

# 解析数据，生成 TraceNode
root = decode_trace(data)

print(f"Tag: {root.tag}, Type: {root.jce_type}")
for child in root.children:
    print(f"  Field Tag {child.tag}: {child.value}")
```

如果你有部分 Schema，也可以传入以增强显示效果（自动匹配字段名）：

```python
root = decode_trace(data, cls=User)
# root.children[0].name 将显示为 "uid"
```

### 结构探测

`probe_struct` 是一个启发式函数，用于快速判断一段未知的 bytes 是否像是一个有效的 Tars Struct。

```python
from tarsio import probe_struct

if probe_struct(unknown_bytes):
    print("这看起来像是一个 Tars 结构体")
```
