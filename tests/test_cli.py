"""JCE 命令行工具测试."""

import json
from pathlib import Path

import pytest
from click.testing import CliRunner

try:
    from jce.__main__ import cli  # noqa: PLC2701
except ImportError:
    pytest.skip("click not installed", allow_module_level=True)


@pytest.fixture
def runner():
    """Click CLI 测试运行器."""
    return CliRunner()


def test_cli_help(runner):
    """测试 --help 选项应显示帮助信息."""
    result = runner.invoke(cli, ["--help"])
    assert result.exit_code == 0
    assert "Usage:" in result.output


def test_missing_input(runner):
    """未提供输入参数时应报错并提示用法."""
    result = runner.invoke(cli, [])
    assert result.exit_code != 0
    # "必须指定 ENCODED 数据或 --file 参数"
    assert "必须指定" in result.output


def test_mutual_exclusion(runner):
    """同时提供参数和文件时应报错."""
    with runner.isolated_filesystem():
        Path("test.bin").write_text("00", encoding="utf-8")
        result = runner.invoke(cli, ["00", "-f", "test.bin"])
        assert result.exit_code != 0
        # "不能同时指定 ENCODED 数据和 --file 参数"
        assert "不能同时指定" in result.output


def test_decode_hex_string(runner):
    """应能正确解码命令行参数提供的十六进制字符串."""
    # Tag 0, Int 100 -> 00 64
    result = runner.invoke(cli, ["0064"])
    assert result.exit_code == 0
    assert "100" in result.output


def test_decode_file(runner):
    """应能正确从文件中读取并解码数据."""
    with runner.isolated_filesystem():
        Path("data.txt").write_text("0064", encoding="utf-8")
        result = runner.invoke(cli, ["-f", "data.txt"])
        assert result.exit_code == 0
        assert "100" in result.output


def test_output_file(runner):
    """应能将解码结果保存到指定文件."""
    with runner.isolated_filesystem():
        result = runner.invoke(cli, ["0064", "-o", "out.txt"])
        assert result.exit_code == 0

        content = Path("out.txt").read_text(encoding="utf-8")
        assert "100" in content


def test_format_json(runner):
    """--format json 选项应输出合法的 JSON 数据."""
    # Tag 0, Int 100 -> 00 64
    result = runner.invoke(cli, ["0064", "--format", "json"])
    assert result.exit_code == 0

    # Parse output to verify valid JSON
    data = json.loads(result.output)
    assert str(data["0"]) == "100" or data["0"] == 100


def test_invalid_hex(runner):
    """提供无效的十六进制字符串时应报错."""
    result = runner.invoke(cli, ["zz"])
    assert result.exit_code != 0
    # "无效的十六进制格式"
    assert "无效的十六进制格式" in result.output


def test_decode_error(runner):
    """解码过程中发生错误时应优雅退出并显示错误信息."""
    # INT4 (Tag 0, Type 2) requires 4 bytes, provided 1 -> 0201
    result = runner.invoke(cli, ["0201"])
    assert result.exit_code != 0
    # "解码失败:"
    assert "解码失败" in result.output


def test_verbose_output(runner):
    """-v 选项应在出错时显示详细堆栈信息."""
    # Use invalid data to trigger error trace
    result = runner.invoke(cli, ["0201", "-v"])
    assert result.exit_code != 0
    # Verbose should print traceback
    assert "Traceback" in result.output


def test_bytes_mode(runner):
    """--bytes-mode 选项应能控制字节数据的显示方式."""
    # Bytes "test" -> Tag 0, Type String1(6), Len 4(04), Data(test)
    # 06 04 74657374
    # But wait, String1 is Type 6.
    # Let's construct a simple Map with bytes value: {0: b'val'}
    # dumps({0: b'val'}) -> Tag 0, String1(6), Len 3, val -> 06 03 76616c
    hex_data = "060376616c"

    # Test raw mode
    result = runner.invoke(cli, [hex_data, "--bytes-mode", "raw"])
    assert result.exit_code == 0
    assert "b'val'" in result.output or "val" in result.output
