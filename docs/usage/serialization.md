# 序列化

Tarsio 提供了简洁而高效的 API 来进行 Tars 对象的序列化（编码）和反序列化（解码）。

## 基础用法

### 编码 (Serialization)

将 Python 对象转换为二进制数据 (`bytes`)。

```python
# 方式 1: 使用实例方法 (推荐)
data = user.encode()

# 方式 2: 使用 tarsio.encode 函数
from tarsio import encode
data = encode(user)
```

### 解码 (Deserialization)

将二进制数据还原为 Python 对象。

```python
# 方式 1: 使用类方法 (推荐)
user = User.decode(data)

# 方式 2: 使用 tarsio.decode 函数
from tarsio import decode
user = decode(User, data)
```

## 无 Schema 模式 (Raw API)

有时你可能需要处理未知的 Tars 数据，或者不想定义对应的 `Struct` 类。Tarsio 提供了 Raw API 来直接操作字典。

### `tarsio.encode_raw`

将符合特定结构的字典编码为 Tars 二进制。

```python
from tarsio import encode_raw

# 构造一个 TarsDict (dict[int, Any])
# Key 必须是整数 Tag
payload = {
    0: 1001,           # Tag 0: int
    1: "Alice",        # Tag 1: str
    2: ["admin", "dev"] # Tag 2: List<str>
}

data = encode_raw(payload)
```

### `tarsio.decode_raw`

将任意 Tars 二进制数据解码为字典。

```python
from tarsio import decode_raw

# data 是上面生成的二进制
result = decode_raw(data)
print(result)
# 输出: {0: 1001, 1: 'Alice', 2: ['admin', 'dev']}
```

> **注意**: `decode_raw` 会尽可能还原数据类型，但由于 Tars 协议不包含字段名，所以 Key 只能是整数 Tag。

## 高级特性

### 探测结构 (`probe_struct`)

Tars 协议中的 `SimpleList` (Type 13) 实际上是 `vector<byte>`。但在很多场景下，这个字节数组内部可能包含了另一个序列化的 Tars 结构体。

Rust 核心不应该猜测数据的含义，但在调试或分析时，我们经常需要“透视”这些数据。为此，Tarsio 提供了 `probe_struct` 工具。

```python
from tarsio import probe_struct

# 假设 data 是一个 SimpleList 的内容
maybe_struct = probe_struct(data)

if maybe_struct:
    print("这是一个嵌套的结构体:", maybe_struct)
else:
    print("这是一段普通的二进制数据")
```

**工作原理**:

1. **快速失败**: 检查首字节是否像合法的 Tars 头部。
2. **试探解码**: 尝试将其作为 Struct 解析。
3. **终局校验**: 只有当解析成功且**消耗了所有字节**时，才认定为 Struct。

这一机制被内置在 Tarsio CLI 工具中，用于自动展示嵌套结构。
