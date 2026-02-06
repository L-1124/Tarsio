"""Tarsio CLI - Tars 编解码命令行工具."""

import json
import sys
from pathlib import Path
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    import click as click_module
    from rich.console import Console as ConsoleType
    from rich.syntax import Syntax as SyntaxType
    from rich.tree import Tree as TreeType
else:
    try:
        import click as click_module
        from rich.console import Console as ConsoleType
        from rich.syntax import Syntax as SyntaxType
        from rich.tree import Tree as TreeType
    except ImportError:
        click_module = None
        ConsoleType = None
        SyntaxType = None
        TreeType = None

from tarsio import decode_raw, probe_struct

click = click_module


def _check_cli_deps() -> None:
    """检查 CLI 依赖是否安装."""
    if not click:
        print(
            "错误: CLI 依赖未安装\n请运行: pip install tarsio[cli]",
            file=sys.stderr,
        )
        sys.exit(1)


def parse_hex_string(hex_str: str) -> bytes:
    """解析 hex 字符串为字节.

    Args:
        hex_str: hex 编码字符串.

    Returns:
        解析后的字节数据.
    """
    # 移除空格和换行
    cleaned = "".join(hex_str.split())
    # 移除可能的 0x 前缀
    if cleaned.lower().startswith("0x"):
        cleaned = cleaned[2:]
    return bytes.fromhex(cleaned)


def is_hex_file(content: bytes) -> bool:
    """检测文件内容是否为 hex 文本.

    Args:
        content: 文件原始内容.

    Returns:
        是否为有效的 hex 文本.
    """
    try:
        text = content.decode("ascii")
        cleaned = "".join(text.split())
        if cleaned.lower().startswith("0x"):
            cleaned = cleaned[2:]
        return all(c in "0123456789abcdefABCDEF" for c in cleaned) and len(cleaned) > 0
    except (UnicodeDecodeError, ValueError):
        return False


def deep_probe(data: Any) -> Any:
    """递归探测并解码 bytes 中的 Struct."""
    if isinstance(data, dict):
        return {k: deep_probe(v) for k, v in data.items()}
    elif isinstance(data, list):
        return [deep_probe(item) for item in data]
    elif isinstance(data, bytes):
        struct = probe_struct(data)
        if struct:
            # 递归处理解码后的结构(因为里面可能还有嵌套)
            return deep_probe(struct)
        return data
    return data


def build_tree(
    data: Any, parent: TreeType | None = None, key: str = "root"
) -> TreeType:
    """递归构建 rich Tree (移植自 0.3.1 风格).

    Args:
        data: 要展示的数据.
        parent: 父节点.
        key: 当前节点的键名/标签.

    Returns:
        构建的 Tree 对象.
    """
    from rich.text import Text
    from rich.tree import Tree

    # 样式定义
    style_tag = "bold blue"
    style_type = "cyan"
    style_value_str = "green"
    style_value_num = "magenta"

    if parent is None:
        # 根节点
        tree = Tree(f"[bold white]{key}[/]")
    else:
        # 子节点,通常 key 是 "[0]" 或 "Key" 等前缀
        tree = parent

    # 1. Struct (Dict with int keys)
    if isinstance(data, dict) and all(isinstance(k, int) for k in data.keys()):
        label = Text()
        if key != "root" and parent is not None:
            label.append(f"{key} ", style=style_tag)
        label.append("Struct", style="bold yellow")

        branch = tree.add(label)
        for k, v in sorted(data.items()):
            build_tree(v, branch, f"[{k}]")

    # 2. Map (Dict with mixed/other keys)
    elif isinstance(data, dict):
        label = Text()
        if key != "root" and parent is not None:
            label.append(f"{key} ", style=style_tag)
        label.append(f"Map (len={len(data)})", style=style_type)

        branch = tree.add(label)
        for k, v in data.items():
            item_branch = branch.add(Text("Item", style="dim"))
            # Key
            build_tree(k, item_branch, "Key")
            # Value
            build_tree(v, item_branch, "Value")

    # 3. List
    elif isinstance(data, list):
        label = Text()
        if key != "root" and parent is not None:
            label.append(f"{key} ", style=style_tag)
        label.append(f"List (len={len(data)})", style=style_type)

        branch = tree.add(label)
        for i, item in enumerate(data):
            build_tree(item, branch, f"[{i}]")

    # 4. Bytes (SimpleList)
    elif isinstance(data, bytes):
        label = Text()
        if key != "root" and parent is not None:
            label.append(f"{key} ", style=style_tag)
        # 显示 SimpleList=长度
        label.append(f"SimpleList(len={len(data)}): ", style=style_type)

        # Hex display
        hex_str = data.hex(" ").upper()
        if len(hex_str) > 50:
            display_hex = hex_str[:50] + "..."
        else:
            display_hex = hex_str
        label.append(display_hex, style="dim")

        tree.add(label)

        # 尝试探测内部结构
        if struct := probe_struct(data):
            build_tree(struct, tree, "Decoded Structure")

    # 5. String
    elif isinstance(data, str):
        label = Text()
        if key != "root" and parent is not None:
            label.append(f"{key} ", style=style_tag)
        # 显示 String=长度
        label.append(f"String(len={len(data)}): ", style=style_type)
        label.append(repr(data), style=style_value_str)
        tree.add(label)

    # 6. Primitives (int, float, etc.)
    else:
        label = Text()
        if key != "root" and parent is not None:
            label.append(f"{key} ", style=style_tag)
        label.append(f"{type(data).__name__}: ", style=style_type)
        label.append(str(data), style=style_value_num)
        tree.add(label)

    return tree


class BytesEncoder(json.JSONEncoder):
    """自定义 JSON encoder, 处理 bytes.

    将 bytes 转换为 hex 字符串或尝试 UTF-8 解码.
    """

    def default(self, o: Any) -> Any:
        """将 bytes 转换为可序列化格式."""
        if isinstance(o, bytes):
            try:
                return o.decode("utf-8")
            except UnicodeDecodeError:
                return f"0x{o.hex()}"
        return super().default(o)


def format_output(data: dict, fmt: str) -> str | None:
    """格式化输出数据.

    Args:
        data: 解码后的数据.
        fmt: 输出格式 (pretty/json/tree).

    Returns:
        格式化后的字符串, 或 None (如果直接输出到 console).
    """
    from rich.console import Console

    console = Console()

    if fmt == "json":
        json_str = json.dumps(data, cls=BytesEncoder, indent=2, ensure_ascii=False)
        # 使用 Syntax 高亮
        from rich.syntax import Syntax

        syntax = Syntax(json_str, "json", theme="monokai", word_wrap=True)
        console.print(syntax)
        return None
    elif fmt == "tree":
        tree = build_tree(data, key="Tars Data")
        console.print(tree)
        return None

    else:  # pretty
        console.print(data)
        return None


def _create_cli() -> Any:
    """创建 CLI 命令."""
    _check_cli_deps()

    from rich.console import Console

    console = Console()
    error_console = Console(stderr=True)

    @click.command()
    @click.argument("encoded", required=False)
    @click.option(
        "-f",
        "--file",
        type=click.Path(exists=True, path_type=Path),
        help="从文件读取十六进制编码数据",
    )
    @click.option(
        "--format",
        "fmt",
        type=click.Choice(["pretty", "json", "tree"]),
        default="pretty",
        show_default=True,
        help="输出格式",
    )
    @click.option(
        "-o",
        "--output",
        type=click.Path(path_type=Path),
        help="将输出保存到文件",
    )
    @click.option(
        "-v",
        "--verbose",
        is_flag=True,
        help="显示详细的解码过程信息",
    )
    def cli(
        encoded: str | None,
        file: Path | None,
        fmt: str,
        output: Path | None,
        verbose: bool,
    ) -> None:
        """Tars 编解码命令行工具.

        Examples:
            tarsio "00 64"
            tarsio -f payload.bin --format json
        """
        # 检查输入
        if encoded is None and file is None:
            error_console.print("[red]Error:[/] 必须提供 ENCODED 参数或 --file 选项")
            raise SystemExit(1)

        if encoded is not None and file is not None:
            error_console.print(
                "[red]Error:[/] 不能同时使用 ENCODED 参数和 --file 选项"
            )
            raise SystemExit(1)

        # 读取数据
        try:
            if file is not None:
                raw_content = file.read_bytes()
                if is_hex_file(raw_content):
                    data = parse_hex_string(raw_content.decode("ascii"))
                    if verbose:
                        console.print("[dim][INFO] 文件格式: hex 文本[/]")
                else:
                    data = raw_content
                    if verbose:
                        console.print("[dim][INFO] 文件格式: 二进制[/]")
            else:
                assert encoded is not None
                data = parse_hex_string(encoded)
        except ValueError as e:
            error_console.print(f"[red]Error:[/] 无效的 hex 数据: {e}")
            raise SystemExit(1) from e

        # Verbose 输出
        if verbose:
            console.print(f"[dim][INFO] 输入大小: {len(data)} bytes[/]")
            hex_str = data.hex()
            formatted_hex = " ".join(
                hex_str[i : i + 2] for i in range(0, len(hex_str), 2)
            )
            if len(formatted_hex) > 100:
                formatted_hex = formatted_hex[:100] + "..."
            console.print(f"[dim][DEBUG] Hex: {formatted_hex}[/]")

        # 解码
        try:
            raw_decoded = decode_raw(data)
            # 对于 JSON/Pretty 格式,我们希望直接替换 bytes 为解码后的结构
            # 对于 Tree 格式,我们在 build_tree 中动态探测并展示为子节点(保留 SimpleList 标签)
            if fmt != "tree":
                decoded = deep_probe(raw_decoded)
            else:
                decoded = raw_decoded
        except Exception as e:
            error_console.print(f"[red]Error:[/] 解码失败: {e}")
            raise SystemExit(1) from e

        # 输出
        if output is not None:
            # 保存到文件
            if fmt == "json":
                result = json.dumps(
                    decoded, cls=BytesEncoder, indent=2, ensure_ascii=False
                )
            else:
                import pprint as pp

                result = pp.pformat(decoded, width=100)
            output.write_text(result, encoding="utf-8")
            console.print(f"[green]输出已保存到:[/] {output}")
        else:
            format_output(decoded, fmt)

    return cli


def main() -> None:
    """入口函数."""
    cli = _create_cli()
    cli()


if __name__ == "__main__":
    main()
