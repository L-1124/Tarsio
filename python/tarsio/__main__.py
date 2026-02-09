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

from tarsio._core import TraceNode, decode_raw, decode_trace, probe_struct

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


def build_trace_tree(node: TraceNode, parent: TreeType | None = None) -> TreeType:
    """基于 TraceNode 构建 rich Tree.

    Args:
        node: TraceNode 根节点.
        parent: 父 rich 节点.

    Returns:
        构建的 Tree 对象.
    """
    from rich.text import Text
    from rich.tree import Tree

    # 样式定义
    style_tag = "bold blue"
    style_type = "cyan"
    style_name = "yellow"
    style_value = "green"

    # 构建当前节点的标签
    label = Text()

    # Tag (ROOT 不显示 Tag 0)
    if node.jce_type != "ROOT":
        label.append(f"Tag {node.tag} ", style=style_tag)

    # Type & Name
    type_desc = node.jce_type
    if node.type_name:
        type_desc = f"{node.type_name}"  # 优先显示语义类型名

    label.append(f"({type_desc})", style=style_type)

    if node.name:
        label.append(f" [{node.name}]", style=style_name)

    # Value
    if node.value is not None:
        val_str = str(node.value)
        if isinstance(node.value, str):
            val_str = repr(node.value)
        elif isinstance(node.value, bytes):
            hex_val = node.value.hex().upper()
            if len(hex_val) > 20:
                val_str = f"<{len(node.value)} bytes> {hex_val[:20]}..."
            else:
                val_str = f"<{len(node.value)} bytes> {hex_val}"

        label.append(": ", style="white")
        label.append(val_str, style=style_value)

    # Path (optional, maybe too verbose for default view)
    # label.append(f"  {node.path}", style=style_path)

    if parent is None:
        tree = Tree(label)
    else:
        tree = parent.add(label)

    # Children
    for child in node.children:
        build_trace_tree(child, tree)

    # Special handling for SimpleList (bytes) -> probe inner struct
    if node.jce_type == "SimpleList" and isinstance(node.value, bytes):
        struct = probe_struct(node.value)
        if struct:
            # 如果探测成功，我们这里为了展示方便，再次调用 decode_trace
            # 但 decode_trace 需要 schema，这里没有，所以只能是 Raw Trace
            # 这是一个递归调用新解析树的过程
            inner_trace = decode_trace(node.value)
            # 添加一个特殊的节点表示 "Decoded Inner"
            inner_branch = tree.add(
                Text(">>> Probed Structure >>>", style="bold magenta")
            )
            # 递归添加子树的 children (跳过 ROOT)
            for child in inner_trace.children:
                build_trace_tree(child, inner_branch)

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


def format_output(data: Any, fmt: str) -> None:
    """格式化输出数据.

    Args:
        data: 解码后的数据 (TraceNode 或 dict).
        fmt: 输出格式 (pretty/json/tree).
    """
    from rich.console import Console

    console = Console()

    if fmt == "json":
        # 如果是 TraceNode，先转 dict
        output_data = data
        if hasattr(data, "to_dict"):
            output_data = data.to_dict()

        # 深度探测 Raw 数据中的 Struct (仅针对 Raw dict 模式)
        if isinstance(output_data, dict):
            output_data = deep_probe(output_data)

        json_str = json.dumps(
            output_data, cls=BytesEncoder, indent=2, ensure_ascii=False
        )
        from rich.syntax import Syntax

        syntax = Syntax(json_str, "json", theme="monokai", word_wrap=True)
        console.print(syntax)

    elif fmt == "tree":
        # 此时 data 必须是 TraceNode
        if not isinstance(data, TraceNode):
            console.print("[red]Internal Error: Tree format requires TraceNode[/]")
            return

        tree = build_trace_tree(data)
        console.print(tree)

    else:  # pretty
        # 如果是 TraceNode，转为 dict 再打印，或者打印 repr？
        # 用户选 pretty 通常是想看 Python 对象结构
        output_data = data
        if isinstance(data, TraceNode):
            # TraceNode 的 repr 不太好看，转为 dict 结构展示 value
            # 但为了 pretty，我们可能更想要 deep_probe 后的 raw dict
            # 这里有点歧义。如果用户选 pretty，通常意味着 decode_raw 的结果。
            # 所以在 main 里如果不选 tree，我们还是调 decode_raw。
            pass

        if isinstance(output_data, dict):
            output_data = deep_probe(output_data)

        console.print(output_data)


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
            if fmt == "tree":
                # Tree 模式：使用 Trace 引擎
                decoded = decode_trace(data)
            else:
                # 其他模式：使用 Raw 引擎
                decoded = decode_raw(data)

        except Exception as e:
            error_console.print(f"[red]Error:[/] 解码失败: {e}")
            raise SystemExit(1) from e

        # 输出
        if output is not None:
            save_data = decoded
            to_dict_method = getattr(decoded, "to_dict", None)
            if to_dict_method:
                save_data = to_dict_method()

            if isinstance(save_data, dict):
                save_data = deep_probe(save_data)

            with open(output, "w", encoding="utf-8") as f:
                json.dump(
                    save_data,
                    f,
                    cls=BytesEncoder,
                    indent=2,
                    ensure_ascii=False,
                )
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
