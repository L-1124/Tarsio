# 序列化与 API

JceStruct 提供了简单直观的 API 用于数据的序列化和反序列化，API 设计灵感来源于 Python 标准库的 `json` 模块。

## 核心函数

### `dumps` (Serialize)

将 `JceStruct` 对象或字典序列化为 `bytes`。

```python
from jce import dumps

data = dumps(user)
# 或者序列化字典 (会被编码为 JCE Map)
map_data = dumps({"key": "value"})
```

**参数**:

*   `obj`: 要序列化的对象。
*   `option`: 序列化选项（如 `JceOption.LITTLE_ENDIAN`）。
*   `exclude_unset`: 是否排除未显式设置的字段（默认为 `True`）。
*   `context`: 传递给字段序列化钩子 (`@field_serializer`) 的上下文数据。

### `loads` (Deserialize)

将 `bytes` 反序列化为 `JceStruct` 对象。

```python
from jce import loads

user = loads(data, User, context={"db": db_connection})
```

**参数**:
*   `data`: 输入的字节数据。
*   `target`: 目标类（`JceStruct` 子类）或类型（如 `dict`）。
*   `bytes_mode`: 控制如何处理二进制数据（见下文）。
*   `context`: 传递给字段反序列化钩子 (`@field_deserializer`) 的上下文数据。

## 文件 I/O


如果你需要直接读写文件，可以使用 `dump` 和 `load`。

```python
from jce import dump, load

# 写入文件
with open("data.bin", "wb") as f:
    dump(user, f)

# 读取文件
with open("data.bin", "rb") as f:
    user = load(f, User)
```

## Bytes 处理模式

JCE 协议没有原生的 String 类型，只有 `String1` (长度<256) 和 `String4`。它们本质上都是带长度的 buffer。
`loads` 函数提供 `bytes_mode` 参数来控制如何将这些 buffer 解析为 Python 类型。

* `"auto"` (**默认**): 智能模式。尝试将 buffer 解码为 UTF-8 字符串；如果失败，则尝试递归解码为嵌套 JCE 结构；如果都失败，则保留为 `bytes`。
* `"string"`: 强制尝试解码为 UTF-8 字符串。
* `"raw"`: 始终保留为 `bytes`。

```python
# 假设 data 包含字符串 "hello"
print(loads(data, bytes_mode="auto"))   # > "hello"
print(loads(data, bytes_mode="raw"))    # > b"hello"
```

## 高级选项

### 大小端序

JCE 协议默认使用**大端序 (Big Endian)**。如果你的对端系统使用小端序，可以通过选项指定：

```python
from jce import JceOption

data = dumps(user, option=JceOption.LITTLE_ENDIAN)
```

### JceDict (动态结构)

如果你不知道数据的具体结构，或者只是想查看原始 Tag-Value 对，可以使用 `JceDict`。

```python
from jce import JceDict

# 解码为通用字典结构
raw_struct = loads(data, target=JceDict)
print(raw_struct)
# > {0: 10086, 1: 'Alice', ...}
```

!!! note "JceDict vs dict"
    *`JceDict`: 代表一个 **Struct**，编码时直接拼接字段。
    *   `dict`: 代表一个 **Map**，编码时包含 Map 头信息 (Key-Value Pairs)。

## 延伸阅读

- [定义模型](models.md): 了解如何创建 `User` 这样的 JCE 结构体。
- [流式处理](streams.md): 处理网络流中的粘包数据。
