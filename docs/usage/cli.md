# CLI 工具

`tarsio` 是一个命令行工具，用于查看、调试与转换 Tars 二进制数据。
支持 Hex 字符串与二进制文件输入，提供多种可视化输出格式。

## 基础用法

### 解码 Hex 字符串

```bash
# 解码 Tag 0 (Int) = 100
$ tarsio "00 64"
{0: 100}
```

### 读取文件

使用 `-f` 参数读取文件（自动检测纯二进制或 Hex 文本）：

```bash
tarsio -f payload.bin
```

### 输出到文件

使用 `-o/--output` 保存输出结果：

```bash
tarsio "00 64" --format json -o out.json
```

### 详细模式

使用 `-v/--verbose` 输出输入大小与 Hex 摘要：

```bash
tarsio "00 64" --verbose
```

## 输出格式

通过 `--format` 参数控制输出。

### 1. Pretty (默认)

打印 Python 字典风格的字符串。

```text
{0: 100, 1: 'hello'}
```

### 2. JSON

输出标准 JSON。无法映射的类型（如 bytes）会转换为 Hex 字符串。

```bash
tarsio "..." --format json
```

### 3. Tree (推荐)

以树状图展示结构、类型与长度信息。适合分析复杂包结构。

```bash
tarsio "..." --format tree
```

输出示例：

```text
Struct Root
└── Struct
    ├── [0] int: 1001
    ├── [1] String(len=5): 'Alice'
    └── [2] SimpleList(len=12): ...
```

## 智能探测 (Smart Probing)

CLI 会自动尝试解析 `SimpleList` (vector<byte>) 内部的数据。
如果字节数组内部包含完整的 Tars 结构，Tree 视图会自动展开它，无需手动二次解码。

## 帮助信息

```bash
tarsio --help
```
