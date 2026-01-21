"""测试 JCE 命令行工具."""

import json
from pathlib import Path

import pytest
from click.testing import CliRunner

from jce import JceField, JceStruct, dumps, types

try:
    from jce.__main__ import cli
except ImportError:
    pytest.skip("click not installed", allow_module_level=True)


class SimpleCliStruct(JceStruct):
    """CLI 测试用简单结构体."""

    name: str = JceField(jce_id=1, jce_type=types.STRING1)
    val: int = JceField(jce_id=2, jce_type=types.INT32)


class ComplexCliStruct(JceStruct):
    """CLI 测试用复杂结构体."""

    id: int = JceField(jce_id=0, jce_type=types.INT32)
    data: bytes = JceField(jce_id=1, jce_type=types.BYTES)
    items: list[int] = JceField(jce_id=2, jce_type=types.LIST)
    mapping: dict[str, str] = JceField(jce_id=3, jce_type=types.MAP)


@pytest.fixture
def runner() -> CliRunner:
    """提供 Click CLI 测试运行器.

    Returns:
        CliRunner 实例.
    """
    return CliRunner()


# --- 基础 CLI 功能测试 ---


def test_cli_help(runner: CliRunner) -> None:
    """--help 选项应显示帮助信息."""
    result = runner.invoke(cli, ["--help"])

    assert result.exit_code == 0
    assert "Usage:" in result.output


def test_cli_missing_input(runner: CliRunner) -> None:
    """未提供输入参数时应报错并提示用法."""
    result = runner.invoke(cli, [])

    assert result.exit_code != 0
    assert "必须指定" in result.output


def test_cli_mutual_exclusion(runner: CliRunner) -> None:
    """同时提供参数和文件时应报错."""
    with runner.isolated_filesystem():
        Path("test.bin").write_text("00", encoding="utf-8")

        result = runner.invoke(cli, ["00", "-f", "test.bin"])

        assert result.exit_code != 0
        assert "不能同时指定" in result.output


def test_cli_decode_hex_string(runner: CliRunner) -> None:
    """应能正确解码命令行参数提供的十六进制字符串."""
    result = runner.invoke(cli, ["0064"])

    assert result.exit_code == 0
    assert "100" in result.output


def test_cli_decode_file(runner: CliRunner) -> None:
    """应能正确从文件中读取并解码数据."""
    with runner.isolated_filesystem():
        Path("data.txt").write_text("0064", encoding="utf-8")

        result = runner.invoke(cli, ["-f", "data.txt"])

        assert result.exit_code == 0
        assert "100" in result.output


def test_cli_output_file(runner: CliRunner) -> None:
    """应能将解码结果保存到指定文件."""
    with runner.isolated_filesystem():
        result = runner.invoke(cli, ["0064", "-o", "out.txt"])

        assert result.exit_code == 0
        content = Path("out.txt").read_text(encoding="utf-8")
        assert "100" in content


def test_cli_format_json(runner: CliRunner) -> None:
    """--format json 选项应输出合法的 JSON 数据."""
    result = runner.invoke(cli, ["0064", "--format", "json"])

    assert result.exit_code == 0
    data = json.loads(result.output)
    assert str(data["0"]) == "100" or data["0"] == 100


def test_cli_invalid_hex(runner: CliRunner) -> None:
    """提供无效的十六进制字符串时应报错."""
    result = runner.invoke(cli, ["zz"])

    assert result.exit_code != 0
    assert "无效的十六进制格式" in result.output


def test_cli_decode_error(runner: CliRunner) -> None:
    """解码过程中发生错误时应优雅退出并显示错误信息."""
    result = runner.invoke(cli, ["0201"])

    assert result.exit_code != 0
    assert "解码失败" in result.output


def test_cli_verbose_output(runner: CliRunner) -> None:
    """-v 选项应在出错时显示详细堆栈信息."""
    result = runner.invoke(cli, ["0201", "-v"])

    assert result.exit_code != 0
    assert "Traceback" in result.output


def test_cli_bytes_mode(runner: CliRunner) -> None:
    """--bytes-mode 选项应能控制字节数据的显示方式."""
    hex_data = "060376616c"

    result = runner.invoke(cli, [hex_data, "--bytes-mode", "raw"])

    assert result.exit_code == 0
    assert "b'val'" in result.output or "val" in result.output


# --- Tree 格式输出测试 ---


def test_cli_tree_simple(runner: CliRunner) -> None:
    """简单结构体的树状输出应正确显示字段."""
    data = SimpleCliStruct(name="test", val=123)
    encoded = dumps(data).hex()

    result = runner.invoke(cli, [encoded, "--format", "tree"])

    assert result.exit_code == 0
    assert "[1](Str=4):test" in result.output
    assert "[2](Byte):123" in result.output


def test_cli_tree_recursive_simplelist(runner: CliRunner) -> None:
    """SimpleList 的递归解析应正确显示嵌套结构."""
    inner = SimpleCliStruct(name="inner", val=999)
    inner_bytes = dumps(inner)
    outer = ComplexCliStruct(
        id=100, data=inner_bytes, items=[1, 2, 3], mapping={"key": "val"}
    )
    encoded = dumps(outer).hex()

    result = runner.invoke(cli, [encoded, "--format", "tree"])

    assert result.exit_code == 0
    assert "[0](Byte):100" in result.output
    assert "[1](SimpleList=" in result.output
    assert "[1.1](Str=5):inner" in result.output
    assert "[1.2](Short):999" in result.output
    assert "[2](List=3)" in result.output
    assert "[2[0]](Byte):1" in result.output
    assert "[2[1]](Byte):2" in result.output
    assert "[3](Map=1)" in result.output
    assert "[3[0].key0](Str=3):key" in result.output
    assert "[3[0].val1](Str=3):val" in result.output


def test_cli_tree_invalid_data(runner: CliRunner) -> None:
    """无效数据的树状输出应报错."""
    result = runner.invoke(cli, ["ZZZZ", "--format", "tree"])
    assert result.exit_code != 0
    assert "无效的十六进制格式" in result.output

    result = runner.invoke(cli, ["0E", "--format", "tree"])
    assert result.exit_code != 0
    assert "Tree解码失败" in result.output


def test_cli_tree_output_file(runner: CliRunner, tmp_path: Path) -> None:
    """树状输出应能正确保存到文件."""
    data = SimpleCliStruct(name="file_test", val=456)
    encoded = dumps(data).hex()
    output_file = tmp_path / "output.txt"

    result = runner.invoke(
        cli, [encoded, "--format", "tree", "--output", str(output_file)]
    )

    assert result.exit_code == 0
    assert f"结果已保存到: {output_file}" in result.output
    content = output_file.read_text(encoding="utf-8")
    assert "[1](Str=9):file_test" in content
    assert "[2](Short):456" in content
