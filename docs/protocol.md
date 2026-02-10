# Tars 协议规范

Tars 协议（原 JCE 协议）是一种高效的二进制序列化协议，采用 **TLV (Tag-Type-Value)** 格式存储数据。它支持 15 种基本数据类型，具有高度紧凑、编解码快、支持接口演进（Forward/Backward Compatibility）等特点。

## 1. 核心结构 (TLV)

Tars 数据的基本存储单元为 `Head` + `Value`。

* **Head**: 包含 **Tag** (字段 ID) 和 **Type** (数据类型)。
* **Value**: 实际数据内容（部分类型如 `ZeroTag` 无 Value）。

### Head 编码

Head 负责存储 Tag 和 Type，根据 Tag 的大小分为两种格式：

1. **Tag < 15 (单字节)**:

    ```text
    |  Tag (4 bits)  |  Type (4 bits) |
    ```

    * 高 4 位存储 Tag，低 4 位存储 Type。

2. **Tag >= 15 (双字节)**:

    ```text
    |  1111 (4 bits) |  Type (4 bits) |   Tag (8 bits)   |
    ```

    * 第一个字节的高 4 位固定为 `15` (`0xF`)，低 4 位存储 Type。
    * 第二个字节存储实际的 Tag 值 (0-255)。

> **注意**: Tars 协议限制 Tag 最大值为 255。

---

## 2. 数据类型 (Type ID)

Tars 定义了 4 位（0-15）的类型标识符。

| Type ID | 名称          | 说明                    | 载荷 (Value)              |
|:--------|:--------------|:------------------------|:--------------------------|
| `0`     | `Int1`        | `byte`, `short`, `bool` | 1 字节                    |
| `1`     | `Int2`        | `short`                 | 2 字节 (Big-Endian)       |
| `2`     | `Int4`        | `int`                   | 4 字节 (Big-Endian)       |
| `3`     | `Int8`        | `long`                  | 8 字节 (Big-Endian)       |
| `4`     | `Float`       | `float`                 | 4 字节 (IEEE 754)         |
| `5`     | `Double`      | `double`                | 8 字节 (IEEE 754)         |
| `6`     | `String1`     | 短字符串/二进制         | 1 字节长度 + 内容         |
| `7`     | `String4`     | 长字符串/二进制         | 4 字节长度 + 内容         |
| `8`     | `Map`         | 键值对映射              | 长度 + 键值对序列         |
| `9`     | `List`        | 列表/数组               | 长度 + 元素序列           |
| `10`    | `StructBegin` | 结构体开始              | 无                        |
| `11`    | `StructEnd`   | 结构体结束              | 无                        |
| `12`    | `ZeroTag`     | 数字 0                  | **无** (值包含在 Type 中) |
| `13`    | `SimpleList`  | 字节数组 (`byte[]`)     | Type + 长度 + 内容        |
| `14`    | -             | *Unused*                | -                         |
| `15`    | -             | *Unused*                | -                         |

---

## 3. 编码规则

### 3.1 整数 (Integers)

Tars 对整数采用**紧凑编码**。编码器应根据数值的实际大小选择最小的 Type ID，而**不是**根据数据定义的类型。

* **ZeroTag (12)**: 当值为 `0` 时，直接写入 Type `12`，不占用 Value 空间。
* **Int1 (0)**: `Value` 在 `[-128, 127]` 范围内。
* **Int2 (1)**: `Value` 在 `[-32768, 32767]` 范围内。
* **Int4 (2)**: `Value` 在 `[-2^31, 2^31-1]` 范围内。
* **Int8 (3)**: 其他情况。

**示例**:

* `int a = 0;`  -> `[Tag, Type=12]`
* `long b = 10;` -> `[Tag, Type=0] [0x0A]`

### 3.2 浮点数 (Float/Double)

* **Float (4)**: 4 字节，大端序 (Big-Endian)。
* **Double (5)**: 8 字节，大端序 (Big-Endian)。
* **0.0 优化**: 浮点数 `0.0` 同样使用 `ZeroTag (12)` 进行优化。

### 3.3 字符串 (String)

根据字符串字节长度 `len` 选择类型：

* **String1 (6)**: 当 `len <= 255`。

    ```text
    [Head] [Length (1 byte)] [Bytes...]
    ```

* **String4 (7)**: 当 `len > 255`。

    ```text
    [Head] [Length (4 bytes, BE)] [Bytes...]
    ```

### 3.4 列表 (List - Type 9)

用于存储通用列表（如 `vector<int>`, `list<string>`）。

```text
[Head (List)] [Length (Integer)] [Item 0] [Item 1] ...
```

* **Length**: 作为一个 Tars 整数编码（通常使用 Tag 0）。
* **Items**: 依次编码每个元素。

### 3.5 映射 (Map - Type 8)

用于存储键值对（`map<K, V>`）。

```text
[Head (Map)] [Length (Integer)] [Key 0] [Val 0] [Key 1] [Val 1] ...
```

* **Length**: 键值对的数量（Tars 整数，Tag 0）。
* **Items**: 键值对交替存储。
    * **Key**: 使用 **Tag 0**。
    * **Value**: 使用 **Tag 1**。

### 3.6 结构体 (Struct - Type 10)

用于嵌套结构。

```text
[Head (StructBegin)] [Field 1] [Field 2] ... [Head (StructEnd)]
```

* **StructBegin (10)**: 标识结构开始。
* **StructEnd (11)**: 标识结构结束（通常 Tag 为 0）。
* 字段顺序：通常按 Tag 从小到大排序写入，但解码器应支持乱序。

### 3.7 简单列表 (SimpleList - Type 13)

这是对 `vector<byte>` (即字节数组) 的专用优化，避免了每个字节都带有 Head 的开销。

```text
[Head (SimpleList)] [Type (Head=0, Type=byte(0))] [Length (Integer)] [Bytes...]
```

* **Type**: 紧跟一个 Head 为 0、Type 为 `Int1(0)` 的字节（即 `0x00`），表示元素类型为 byte。
* **Length**: 数组长度（Tars 整数，Tag 0）。
* **Bytes**: 原始字节流。

---

## 4. 字节序

Tars 协议中所有多字节数值（Int2, Int4, Int8, Float, Double, Length）均采用 **Big-Endian (网络字节序)**。

---

## 5. 可选字段 (Optional)

Tars 协议支持字段的可选性，这对于版本兼容性至关重要。

### 5.1 编码行为

* **Optional 字段**: 如果字段的值等于其默认值（或为空/None），编码器**可以选择**跳过该字段，不写入任何数据。
* **Require 字段**: 必须写入数据，即使是默认值。

### 5.2 解码行为

* **遇到未知 Tag**: 解码器应跳过该字段（读取 Head 和 Length 后跳过 Value），继续解析后续字段。这保证了**向前兼容性**（旧代码读取新数据）。
* **缺失 Optional Tag**: 解码器应使用该字段定义的默认值。这保证了**向后兼容性**（新代码读取旧数据）。
* **缺失 Require Tag**: 解码器应抛出异常，视为数据损坏或版本不匹配。
