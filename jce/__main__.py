"""JCE命令行工具."""

import json
import sys
from pathlib import Path
from typing import TYPE_CHECKING, Any, cast

from . import BytesMode, loads
from .decoder import JceNode

if TYPE_CHECKING:
    import click as click_module
    from rich.console import Console
    from rich.syntax import Syntax
    from rich.text import Text
    from rich.tree import Tree
else:
    try:
        import click as click_module
        from rich.console import Console
        from rich.syntax import Syntax
        from rich.text import Text
        from rich.tree import Tree
    except ImportError:
        click_module = None
        Console = None
        Syntax = None
        Text = None
        Tree = None

click = click_module

# 流式读取配置
FILE_SIZE_THRESHOLD = 10 * 1024 * 1024  # 10MB
CHUNK_SIZE = 8 * 1024 * 1024  # 8MB


if not click:

    def main() -> None:
        """入口函数 (缺少 click)."""
        print("错误: 未检测到 'click' 模块,无法运行 CLI 工具。", file=sys.stderr)
        print(
            "\n该功能属于可选组件,请通过以下命令安装依赖:\n"
            "  pip install 'git+https://github.com/L-1124/JceStruct.git[cli]'\n"
            "\n或者如果您使用 uv:\n"
            "  uv add 'git+https://github.com/L-1124/JceStruct.git[cli]'",
            file=sys.stderr,
        )
        sys.exit(1)

else:

    def _read_binary_file(file_path: Path, verbose: bool) -> bytes:
        """读取二进制文件,大文件使用分块以控制内存.

        Args:
            file_path: 文件路径.
            verbose: 是否显示详细信息.

        Returns:
            文件内容的bytes.
        """
        file_size = file_path.stat().st_size

        if file_size > FILE_SIZE_THRESHOLD:
            if verbose:
                click.echo(f"[DEBUG] 文件大小 {file_size} 字节,使用分块读取", err=True)

            chunks = []
            with open(file_path, "rb") as f:
                while chunk := f.read(CHUNK_SIZE):
                    chunks.append(chunk)
            return b"".join(chunks)
        return file_path.read_bytes()

    def _read_hex_file(file_path: Path, verbose: bool) -> bytes:
        """读取并解析十六进制文本文件.

        Args:
            file_path: 文件路径.
            verbose: 是否显示详细信息.

        Returns:
            解析后的bytes.

        Raises:
            ValueError: 如果文件内容不是有效的十六进制字符串.
        """
        file_size = file_path.stat().st_size

        if file_size > FILE_SIZE_THRESHOLD:
            if verbose:
                click.echo(f"[DEBUG] 文件大小 {file_size} 字节,使用分块读取", err=True)

            hex_parts = []
            with open(file_path, encoding="utf-8") as f:
                hex_parts.extend(line.strip() for line in f)
            hex_data = "".join(hex_parts)
        else:
            hex_data = file_path.read_text(encoding="utf-8").strip()

        # 验证并清理
        cleaned = "".join(hex_data.split())
        if not all(c in "0123456789abcdefABCDEF" for c in cleaned):
            raise ValueError("不是有效的十六进制字符串")

        return bytes.fromhex(cleaned)

    def _build_rich_tree(node: "JceNode", tree: Tree, prefix: str) -> None:
        """递归构建 Rich 树.

        Args:
            node: 当前 JCE 节点.
            tree: 父级 Tree 对象.
            prefix: 节点 ID 前缀 (用于显示完整路径).
        """
        # 计算当前 ID 显示
        if node.tag is not None:
            tag_display = f"[{node.tag}]"
            current_id = f"{prefix}{node.tag}"
        else:
            tag_display = ""
            current_id = prefix

        # 类型和长度
        type_str = node.type_name
        if node.length is not None:
            type_str += f"={node.length}"

        # 样式定义
        style_tag = "bold blue"
        style_type = "cyan"
        style_value_str = "green"
        style_value_num = "magenta"

        # 根据类型构建分支或叶子
        if node.type_name == "Struct":
            label = Text()
            if tag_display:
                label.append(f"{tag_display} ", style=style_tag)
            label.append("Struct", style="bold yellow")

            branch = tree.add(label)
            for child in cast(list[JceNode], node.value):
                _build_rich_tree(child, branch, f"{current_id}.")

        elif node.type_name == "List":
            label = Text()
            if tag_display:
                label.append(f"{tag_display} ", style=style_tag)
            label.append(f"List ({type_str})", style=style_type)

            branch = tree.add(label)
            for i, child in enumerate(cast(list[JceNode], node.value)):
                # List 元素通常没有 Tag，用索引作为 ID
                child_prefix = f"{current_id}[{i}]"

                # 简单做法：为列表项创建一个分支
                idx_label = Text(f"[{i}] ", style="dim")
                idx_branch = branch.add(idx_label)
                _build_rich_tree(child, idx_branch, child_prefix)

        elif node.type_name == "Map":
            label = Text()
            if tag_display:
                label.append(f"{tag_display} ", style=style_tag)
            label.append(f"Map ({type_str})", style=style_type)

            branch = tree.add(label)
            for i, (k, v) in enumerate(cast(list[tuple[JceNode, JceNode]], node.value)):
                entry_branch = branch.add(Text(f"Entry {i}", style="dim"))

                k_branch = entry_branch.add(Text("Key", style="italic"))
                _build_rich_tree(k, k_branch, f"{current_id}[{i}].key")

                v_branch = entry_branch.add(Text("Value", style="italic"))
                _build_rich_tree(v, v_branch, f"{current_id}[{i}].val")

        elif node.type_name == "SimpleList":
            val = node.value
            if isinstance(val, list):
                # 递归解析成功的 SimpleList
                label = Text()
                if tag_display:
                    label.append(f"{tag_display} ", style=style_tag)
                label.append("SimpleList (Parsed)", style="bold yellow")

                branch = tree.add(label)
                for child in cast(list[JceNode], val):
                    _build_rich_tree(child, branch, f"{current_id}.")
            else:
                # 普通字节数组
                val_str = bytes(val).hex(" ").upper()
                label = Text()
                if tag_display:
                    label.append(f"{tag_display} ", style=style_tag)
                label.append(f"{type_str}: ", style=style_type)
                label.append(val_str, style=style_value_str)
                tree.add(label)

        else:
            # 基本类型
            val = node.value
            val_str = str(val)
            if isinstance(val, bytes | bytearray | memoryview):
                val_str = bytes(val).hex(" ").upper()
                val_style = style_value_str
            elif isinstance(val, str):
                val_style = style_value_str
            else:
                val_style = style_value_num

            label = Text()
            if tag_display:
                label.append(f"{tag_display} ", style=style_tag)
            label.append(f"{type_str}: ", style=style_type)
            label.append(val_str, style=val_style)
            tree.add(label)

    def _print_node_tree(nodes: list["JceNode"], file: Any = None) -> None:
        """打印JCE节点树 (使用 Rich).

        Args:
            nodes: JCE节点列表.
            file: 输出文件对象,默认为stdout.
        """
        if not Console:
            click.echo("错误: 未安装 rich 库,无法使用 Tree 视图.", err=True)
            return

        console = Console(file=file, force_terminal=file is None)
        root = Tree("JceStruct Root", style="bold white")

        for node in nodes:
            _build_rich_tree(node, root, "")

        console.print(root)

    def _decode_and_print(
        data: bytes,
        output_format: str,
        output_file: str | None,
        verbose: bool,
        bytes_mode: str,
    ) -> None:
        """解码并输出结果."""
        # 延迟导入以避免循环依赖
        from .decoder import DataReader, NodeDecoder

        def _validate_bytes_mode(mode: str) -> None:
            """验证 bytes-mode 参数."""
            if mode not in {"auto", "string", "raw"}:
                raise click.BadParameter("bytes-mode 只能为 auto/string/raw")

        if verbose:
            click.echo(f"[DEBUG] 数据大小: {len(data)} 字节", err=True)

        if output_format == "tree":
            try:
                reader = DataReader(data)
                decoder = NodeDecoder(reader)
                nodes = decoder.decode(suppress_log=not verbose)

                if output_file:
                    with open(output_file, "w", encoding="utf-8") as f:
                        _print_node_tree(nodes, file=f)
                    click.echo(f"结果已保存到: {output_file}", err=True)
                else:
                    _print_node_tree(nodes)
                return
            except Exception as e:
                if verbose:
                    import traceback

                    traceback.print_exc(file=sys.stderr)
                raise click.ClickException(f"Tree解码失败: {e}") from e

        # 解码
        try:
            _validate_bytes_mode(bytes_mode)
            result = loads(
                data,
                target=dict,
                bytes_mode=cast(BytesMode, bytes_mode),
            )
        except Exception as e:
            if verbose:
                import traceback

                traceback.print_exc(file=sys.stderr)
            raise click.ClickException(f"解码失败: {e}") from e

        # 格式化输出准备
        def _json_default(obj: object) -> object:
            if isinstance(obj, bytes | bytearray | memoryview):
                return bytes(obj).hex()
            return str(obj)

        output_text: str | None = None

        if output_format == "json":
            # 始终生成 JSON 字符串，用于 Syntax 高亮或文件写入
            output_text = json.dumps(
                result, indent=2, ensure_ascii=False, default=_json_default
            )
        elif output_format == "pretty":
            # 仅在需要文本输出时生成 pprint 字符串
            if output_file or not Console:
                import pprint

                output_text = pprint.pformat(result, width=100)

        # 执行输出
        if output_file:
            assert output_text is not None
            output_path = Path(output_file)
            output_path.write_text(output_text, encoding="utf-8")
            click.echo(f"结果已保存到: {output_file}", err=True)

        elif Console:
            # 使用 Rich 进行高亮输出
            console = Console()
            if output_format == "json":
                assert output_text is not None
                syntax = Syntax(output_text, "json", theme="monokai", word_wrap=True)
                console.print(syntax)
            else:  # pretty
                # Rich 直接支持 Python 对象高亮
                console.print(result)

        else:
            # 降级模式 (无 Rich)
            assert output_text is not None
            click.echo(output_text)

    @click.command(help="JCE 编解码命令行工具")
    @click.argument("encoded", required=False)
    @click.option(
        "-f",
        "--file",
        "file_path",
        type=click.Path(exists=True, dir_okay=False, path_type=Path),
        help="从文件读取十六进制编码数据",
    )
    @click.option(
        "--format",
        "output_format",
        type=click.Choice(["pretty", "json", "tree"]),
        default="pretty",
        show_default=True,
        help="输出格式",
    )
    @click.option(
        "-o",
        "--output",
        "output_file",
        type=click.Path(dir_okay=False, writable=True),
        help="将输出保存到文件 (如不指定则输出到控制台)",
    )
    @click.option(
        "-v",
        "--verbose",
        is_flag=True,
        help="显示详细的解码过程信息",
    )
    @click.option(
        "--bytes-mode",
        type=click.Choice(["auto", "string", "raw"]),
        default="auto",
        show_default=True,
        help="字节处理模式: auto/string/raw",
    )
    def cli(
        encoded: str | None,
        file_path: Path | None,
        output_format: str,
        output_file: str | None,
        verbose: bool,
        bytes_mode: str,
    ) -> None:
        """JCE 编解码命令行工具.

        Examples:
          # 直接解码十六进制数据
          jce "0a0b0c"

          # 从文件读取十六进制数据
          jce -f input.hex

          # 以 JSON 格式输出结果
          jce -f input.hex --format json

          # 以 Tree 格式输出 (需要 rich)
          jce "0C" --format tree
        """
        # 互斥参数检查
        if encoded and file_path:
            raise click.UsageError("不能同时指定 ENCODED 数据和 --file 参数")
        if not encoded and not file_path:
            raise click.UsageError("必须指定 ENCODED 数据或 --file 参数")

        # 获取二进制数据
        if file_path:
            try:
                # 尝试hex文本模式
                data = _read_hex_file(file_path, verbose)
                if verbose:
                    click.echo("[DEBUG] 从文件读取十六进制数据 (文本模式)", err=True)
            except (UnicodeDecodeError, ValueError):
                # 降级到二进制模式
                data = _read_binary_file(file_path, verbose)
                if verbose:
                    click.echo("[DEBUG] 从文件读取二进制数据 (二进制模式)", err=True)
        else:
            # 命令行参数: hex字符串
            assert encoded is not None
            try:
                data = bytes.fromhex(encoded)
            except ValueError as e:
                if verbose:
                    import traceback

                    traceback.print_exc(file=sys.stderr)
                raise click.BadParameter(f"无效的十六进制格式 - {e}") from e

        _decode_and_print(data, output_format, output_file, verbose, bytes_mode)

    def main() -> None:
        """入口函数."""
        cli()


if __name__ == "__main__":
    main()
