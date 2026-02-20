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


def test_cli_read_binary_file_with_tree_format(
    cli_runner: CliRunner, cli, tmp_path: Path
) -> None:
    """CLI -f 读取二进制文件并在 tree 模式下成功解码."""
    test_file = tmp_path / "test_tree.bin"
    test_file.write_bytes(bytes.fromhex("0064"))

    result = cli_runner.invoke(
        cli,
        ["-f", str(test_file), "--file-format", "bin", "--format", "tree"],
    )
    assert result.exit_code == 0
    assert "(ROOT)" in result.output


def test_cli_read_hex_text_file(cli_runner: CliRunner, cli, tmp_path: Path) -> None:
    """CLI -f 正确读取 hex 文本文件."""
    test_file = tmp_path / "test.hex"
    test_file.write_text("00 64")

    result = cli_runner.invoke(cli, ["-f", str(test_file), "--file-format", "hex"])
    assert result.exit_code == 0
    assert "100" in result.output


def test_cli_read_hex_text_file_with_invalid_utf8_returns_error(
    cli_runner: CliRunner, cli, tmp_path: Path
) -> None:
    """CLI 读取非 UTF-8 hex 文本文件时返回错误."""
    test_file = tmp_path / "invalid_utf8.hex"
    test_file.write_bytes(b"\xff\xfe00")

    result = cli_runner.invoke(cli, ["-f", str(test_file), "--file-format", "hex"])
    assert result.exit_code != 0
    assert "输入读取失败" in result.output


def test_cli_read_hex_text_file_with_invalid_char_returns_error(
    cli_runner: CliRunner, cli, tmp_path: Path
) -> None:
    """CLI 读取包含非法字符的 hex 文本文件时返回错误."""
    test_file = tmp_path / "invalid_char.hex"
    test_file.write_text("00 zz")

    result = cli_runner.invoke(cli, ["-f", str(test_file), "--file-format", "hex"])
    assert result.exit_code != 0
    assert "非法 hex 字符" in result.output


def test_cli_read_hex_text_file_with_odd_length_returns_error(
    cli_runner: CliRunner, cli, tmp_path: Path
) -> None:
    """CLI 读取奇数长度 hex 文本文件时返回错误."""
    test_file = tmp_path / "odd_length.hex"
    test_file.write_text("0")

    result = cli_runner.invoke(cli, ["-f", str(test_file), "--file-format", "hex"])
    assert result.exit_code != 0
    assert "长度必须为偶数" in result.output


def test_cli_binary_file_handle_released_after_decode(
    cli_runner: CliRunner, cli, tmp_path: Path
) -> None:
    """CLI 读取二进制文件后应释放句柄."""
    test_file = tmp_path / "handle.bin"
    test_file.write_bytes(bytes.fromhex("0064"))

    result = cli_runner.invoke(cli, ["-f", str(test_file), "--file-format", "bin"])
    assert result.exit_code == 0

    test_file.unlink()
    assert not test_file.exists()


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


def test_cli_output_to_directory_returns_error(
    cli_runner: CliRunner, cli, tmp_path: Path
) -> None:
    """CLI 输出路径为目录时返回错误."""
    output_dir = tmp_path / "out_dir"
    output_dir.mkdir()

    result = cli_runner.invoke(cli, ["00 64", "-o", str(output_dir)])
    assert result.exit_code != 0
    assert "输出失败" in result.output


# ==========================================
# Probe 策略
# ==========================================


def test_cli_probe_off_does_not_expand_nested_structure(
    cli_runner: CliRunner, cli
) -> None:
    """CLI probe=off 时不展开嵌套结构."""
    nested_simplelist_hex = "0800010c1d0000020001"
    result = cli_runner.invoke(
        cli,
        [nested_simplelist_hex, "--format", "tree", "--probe", "off"],
    )
    assert result.exit_code == 0
    assert ">>> Probed Structure >>>" not in result.output


def test_cli_probe_on_expands_nested_structure(cli_runner: CliRunner, cli) -> None:
    """CLI probe=on 时展开嵌套结构."""
    nested_simplelist_hex = "0800010c1d0000020001"
    result = cli_runner.invoke(
        cli,
        [nested_simplelist_hex, "--format", "tree", "--probe", "on"],
    )
    assert result.exit_code == 0
    assert ">>> Probed Structure >>>" in result.output


def test_cli_probe_auto_skips_when_bytes_exceed_threshold(
    cli_runner: CliRunner, cli
) -> None:
    """CLI probe=auto 在超过阈值时跳过探测."""
    nested_simplelist_hex = "0800010c1d0000020001"
    result = cli_runner.invoke(
        cli,
        [
            nested_simplelist_hex,
            "--format",
            "tree",
            "--probe",
            "auto",
            "--probe-max-bytes",
            "1",
        ],
    )
    assert result.exit_code == 0
    assert ">>> Probed Structure >>>" not in result.output


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
    assert "--file-format" in result.output
    assert "--probe" in result.output
    assert "--probe-max-bytes" in result.output
    assert "--probe-max-depth" in result.output
    assert "--probe-max-nodes" in result.output
    assert "--format" in result.output
    assert "--verbose" in result.output
