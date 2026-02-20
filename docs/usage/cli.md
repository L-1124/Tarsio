# CLI 工具

`tarsio` CLI 用于解析和查看 Tars 二进制数据。
它支持 hex 字符串、文件输入，以及 `pretty/json/tree` 三种输出格式。

## 示例代码

```bash
# 直接解析 hex
tarsio "00 64"

# 从文件读取并输出 JSON
tarsio -f payload.bin --format json

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
* `-f/--file`: 文件输入。若文件是纯 hex 文本会自动识别，否则按原始二进制读取。

### 输出格式

* `pretty` (默认): 适合快速查看解码结果。
* `json`: 适合存档或与其他工具联动。
* `tree`: 基于 `decode_trace` 显示结构、tag 与类型信息。

### 常用选项

| 选项 | 作用 |
| --- | --- |
| `--format {pretty,json,tree}` | 选择输出格式。 |
| `-o, --output PATH` | 将结果写入 JSON 文件。 |
| `-v, --verbose` | 输出输入大小和 hex 摘要。 |

### 智能探测

CLI 会尝试探测 `SimpleList(bytes)` 内是否嵌套了可解析的 Struct。
在 `tree` 或 `json` 输出中，探测成功的节点会展开显示。

## 注意事项

* `ENCODED` 与 `--file` 不能同时使用。
* 输入不是合法 hex 或二进制格式时，CLI 会以非零状态码退出。
* `tree` 视图用于诊断，不代表业务层字段约束都已通过。
