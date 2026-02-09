"""CLI 命令行工具集成测试.

验证 tarsio CLI 的核心功能:
- hex 字符串解码
- 文件输入处理
- 输出格式化
- 错误处理
"""

import json
from pathlib import Path

import pytest
from click.testing import CliRunner
from tarsio.__main__ import _create_cli


@pytest.fixture
def cli_runner() -> CliRunner:
    """提供 Click CLI 测试 runner.

    Returns:
        CliRunner: Click 测试 runner 实例.
    """
    return CliRunner()


@pytest.fixture
def cli():
    """提供 CLI Click Command 对象.

    Returns:
        Click Command.
    """
    return _create_cli()


# ==========================================
# 基本解码功能
# ==========================================


def test_cli_decode_hex_with_spaces(cli_runner: CliRunner, cli) -> None:
    """CLI 正确解码带空格的 hex 字符串."""
    result = cli_runner.invoke(cli, ["00 64"])
    assert result.exit_code == 0
    assert "100" in result.output


def test_cli_decode_hex_without_spaces(cli_runner: CliRunner, cli) -> None:
    """CLI 正确解码无空格的 hex 字符串."""
    result = cli_runner.invoke(cli, ["0064"])
    assert result.exit_code == 0
    assert "100" in result.output


def test_cli_decode_hex_with_0x_prefix(cli_runner: CliRunner, cli) -> None:
    """CLI 正确解码带 0x 前缀的 hex 字符串."""
    result = cli_runner.invoke(cli, ["0x0064"])
    assert result.exit_code == 0
    assert "100" in result.output


# ==========================================
# 输出格式
# ==========================================


def test_cli_format_json_outputs_valid_json(cli_runner: CliRunner, cli) -> None:
    """CLI --format json 输出有效 JSON."""
    result = cli_runner.invoke(cli, ["00 64", "--format", "json"])
    assert result.exit_code == 0
    # 输出包含 JSON 内容
    assert "100" in result.output


def test_cli_format_tree_outputs_tree_structure(cli_runner: CliRunner, cli) -> None:
    """CLI --format tree 输出树状结构."""
    result = cli_runner.invoke(cli, ["00 64", "--format", "tree"])
    assert result.exit_code == 0
    assert "(ROOT)" in result.output


def test_cli_format_pretty_is_default(cli_runner: CliRunner, cli) -> None:
    """CLI 默认使用 pretty 格式输出."""
    result = cli_runner.invoke(cli, ["00 64"])
    assert result.exit_code == 0


# ==========================================
# Verbose 模式
# ==========================================


def test_cli_verbose_shows_input_size(cli_runner: CliRunner, cli) -> None:
    """CLI -v 显示输入字节大小."""
    result = cli_runner.invoke(cli, ["00 64", "-v"])
    assert result.exit_code == 0
    assert "bytes" in result.output


def test_cli_verbose_shows_hex_dump(cli_runner: CliRunner, cli) -> None:
    """CLI --verbose 显示 hex dump."""
    result = cli_runner.invoke(cli, ["00 64", "--verbose"])
    assert result.exit_code == 0
    assert "00 64" in result.output


# ==========================================
# 文件输入
# ==========================================


def test_cli_read_binary_file(cli_runner: CliRunner, cli, tmp_path: Path) -> None:
    """CLI -f 正确读取二进制文件."""
    test_file = tmp_path / "test.bin"
    test_file.write_bytes(bytes.fromhex("0064"))

    result = cli_runner.invoke(cli, ["-f", str(test_file)])
    assert result.exit_code == 0
    assert "100" in result.output


def test_cli_read_hex_text_file(cli_runner: CliRunner, cli, tmp_path: Path) -> None:
    """CLI -f 正确读取 hex 文本文件."""
    test_file = tmp_path / "test.hex"
    test_file.write_text("00 64")

    result = cli_runner.invoke(cli, ["-f", str(test_file)])
    assert result.exit_code == 0
    assert "100" in result.output


# ==========================================
# 输出到文件
# ==========================================


def test_cli_output_to_file_saves_json(
    cli_runner: CliRunner, cli, tmp_path: Path
) -> None:
    """CLI -o 正确保存 JSON 输出到文件."""
    output_file = tmp_path / "out.json"

    result = cli_runner.invoke(
        cli, ["00 64", "--format", "json", "-o", str(output_file)]
    )
    assert result.exit_code == 0

    data = json.loads(output_file.read_text())
    assert data["0"] == 100


# ==========================================
# 错误处理
# ==========================================


def test_cli_no_input_returns_error(cli_runner: CliRunner, cli) -> None:
    """CLI 无输入时返回错误."""
    result = cli_runner.invoke(cli, [])
    assert result.exit_code != 0


def test_cli_invalid_hex_returns_error(cli_runner: CliRunner, cli) -> None:
    """CLI 无效 hex 输入时返回错误."""
    result = cli_runner.invoke(cli, ["not-hex"])
    assert result.exit_code != 0


def test_cli_both_inputs_returns_error(
    cli_runner: CliRunner, cli, tmp_path: Path
) -> None:
    """CLI 同时提供参数和文件时返回错误."""
    test_file = tmp_path / "test.bin"
    test_file.write_bytes(b"\x00\x64")

    result = cli_runner.invoke(cli, ["00 64", "-f", str(test_file)])
    assert result.exit_code != 0


# ==========================================
# 帮助信息
# ==========================================


def test_cli_help_shows_options(cli_runner: CliRunner, cli) -> None:
    """CLI --help 显示所有选项."""
    result = cli_runner.invoke(cli, ["--help"])
    assert result.exit_code == 0
    assert "--file" in result.output
    assert "--format" in result.output
    assert "--verbose" in result.output
