# CLI 工具

Tarsio 提供了一个功能强大的命令行工具 `tarsio`，用于快速查看、调试和转换 Tars 二进制数据。它能够自动探测嵌套结构，并以树状视图展示。

## 基础用法

直接解码十六进制字符串：

```bash
# 解码 Tag 0 (Int) = 100 (0x64)
tarsio "00 64"
```

从文件读取：

```bash
# 自动检测文件是纯二进制还是 Hex 文本
tarsio -f payload.bin
```

## 输出格式

Tarsio 支持三种输出格式，通过 `--format` 参数指定。

### 1. Pretty (默认)

打印 Python 字典的字符串表示 (`pprint`)。

```bash
tarsio "00 64" --format pretty
# 输出: {0: 100}
```

### 2. JSON

输出标准的 JSON 格式。对于无法直接映射到 JSON 的类型（如 bytes），会自动转为 Hex 字符串。

```bash
tarsio "00 64" --format json
# 输出:
# {
#   "0": 100
# }
```

### 3. Tree (推荐)

以交互式树状图展示数据结构。它不仅展示值，还会显示类型和长度信息。

```bash
tarsio -f payload.hex --format tree
```

输出示例：

```text
Struct Root
└── Struct
    ├── [0] int: 1001
    ├── [1] String(len=5): 'Alice'
    ├── [2] SimpleList(len=12): 0A 0B 0C ...
    │   └── Decoded Structure
    │       └── Struct
    │           └── [0] ...
```

## 智能探测 (Smart Probing)

CLI 内置了**递归探测**功能。

当遇到 `SimpleList` (即 `vector<byte>`) 类型时，CLI 会自动尝试将其作为 Tars 结构体进行解析。如果探测成功（数据符合 Tars 结构特征），它会在 Tree 视图中自动展开，或者在 JSON 视图中直接替换为解析后的对象。

这对于分析复杂的嵌套协议包非常有用，你不需要手动提取 bytes 再进行二次解码。

## 选项参考

```text
Usage: tarsio [OPTIONS] [ENCODED]

  Tars 编解码命令行工具.

Arguments:
  [ENCODED]  十六进制编码字符串 (如 "00 64")

Options:
  -f, --file PATH                 从文件读取数据 (支持 binary 或 hex text)
  --format [pretty|json|tree]     输出格式  [default: pretty]
  -o, --output PATH               将结果保存到文件
  -v, --verbose                   显示详细调试信息 (如 Hex Dump)
  --help                          显示此帮助信息
```
