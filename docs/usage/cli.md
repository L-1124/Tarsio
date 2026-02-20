# CLI 工具

`tarsio` CLI 用于解析和查看 Tars 二进制数据。
它支持 hex 字符串、文件输入，以及 `pretty/json/tree` 三种输出格式。

## 示例代码

```bash
# 直接解析 hex
tarsio "00 64"

# 从二进制文件读取并输出 JSON
tarsio -f payload.bin --file-format bin --format json

# 从 hex 文本文件读取
tarsio -f payload.hex --file-format hex

# 树形追踪视图
tarsio -f payload.bin --format tree
```

## 核心概念

### 安装

CLI 依赖 `click` 与 `rich`，建议安装扩展依赖:

```bash
pip install "tarsio[cli]"
```

### 输入来源

* 位置参数 `ENCODED`: hex 字符串，可包含空格或 `0x` 前缀。
* `-f/--file`: 文件输入。
* `--file-format {bin,hex}`: 显式声明文件输入格式，默认 `bin`。

### 输出格式

* `pretty` (默认): 适合快速查看解码结果。
* `json`: 适合存档或与其他工具联动。
* `tree`: 基于 `decode_trace` 显示结构、tag 与类型信息。

### 常用选项

| 选项 | 作用 |
| --- | --- |
| `--file-format {bin,hex}` | 声明文件输入格式，避免启发式误判。 |
| `--probe {off,auto,on}` | 控制 `SimpleList(bytes)` 的嵌套探测策略。 |
| `--probe-max-bytes N` | `probe=auto` 时，单个 bytes 允许探测的最大长度。 |
| `--probe-max-depth N` | 探测最大递归深度。 |
| `--probe-max-nodes N` | 单次执行最多探测的 bytes 节点数量。 |
| `--format {pretty,json,tree}` | 选择输出格式。 |
| `-o, --output PATH` | 将结果写入 JSON 文件。 |
| `-v, --verbose` | 输出输入大小和 hex 摘要。 |

### 嵌套探测策略

`SimpleList(bytes)` 可能承载嵌套 Struct。CLI 提供三种策略:

* `off`: 不进行嵌套探测。
* `auto` (默认): 在长度/深度/节点预算内探测。
* `on`: 强制探测，仍受深度和节点上限保护。

在 `tree` 或 `json` 输出中，探测成功的节点会展开显示。

## 注意事项

* `ENCODED` 与 `--file` 不能同时使用。
* `--file-format hex` 要求输入为 UTF-8 文本且内容是合法 hex。
* 输入不是合法 hex 或二进制格式时，CLI 会以非零状态码退出。
* `tree` 视图用于诊断，不代表业务层字段约束都已通过。
