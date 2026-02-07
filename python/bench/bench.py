"""Python 基准测试脚本.

用于分析 tarsio 在 Python 侧的 encode/decode 与 raw 编解码性能。
"""

import argparse
import gc
import json
import os
import platform
import statistics
import subprocess
import sys
import time
import tracemalloc
from collections.abc import Callable
from dataclasses import dataclass
from time import perf_counter_ns
from typing import Annotated, Any

from tarsio import Struct, decode, decode_raw, encode, encode_raw


class _Small(Struct):
    """小规模结构体基准数据."""

    value: Annotated[int, 0]
    name: Annotated[str, 1]
    flag: Annotated[bool, 2]
    score: Annotated[float, 3]
    blob: Annotated[bytes, 4]


class _Medium(Struct):
    """中等规模结构体基准数据."""

    values: Annotated[list[int], 0]
    tags: Annotated[list[str], 1]
    props: Annotated[dict[str, int], 2]
    note: Annotated[str | None, 3] = None


class _Child(Struct):
    """嵌套结构体子节点."""

    id: Annotated[int, 0]
    label: Annotated[str, 1]


class _Parent(Struct):
    """嵌套结构体父节点."""

    children: Annotated[list[_Child], 0]
    meta: Annotated[dict[str, str], 1]


@dataclass(frozen=True)
class Scenario:
    """基准场景定义."""

    name: str
    payload: Any
    encode_fn: Callable[[Any], bytes]
    decode_fn: Callable[[bytes], Any]
    note: str


def quantile(values: list[float], q: float) -> float:
    """计算分位数.

    Args:
        values: 样本列表.
        q: 分位点, 范围 0-1.

    Returns:
        分位数值.
    """
    if not values:
        return 0.0
    data = sorted(values)
    idx = round((len(data) - 1) * q)
    return data[idx]


def get_cpu_info() -> str:
    """获取 CPU 信息."""
    env = os.environ.get("PROCESSOR_IDENTIFIER")
    if env:
        return env
    if sys.platform.startswith("linux"):
        try:
            with open("/proc/cpuinfo", encoding="utf-8") as f:
                for line in f:
                    if line.lower().startswith("model name"):
                        return line.split(":", 1)[1].strip()
        except OSError:
            pass
    if sys.platform == "darwin":
        try:
            result = subprocess.run(
                ["sysctl", "-n", "machdep.cpu.brand_string"],
                check=True,
                capture_output=True,
                text=True,
            )
            if result.stdout.strip():
                return result.stdout.strip()
        except (OSError, subprocess.SubprocessError):
            pass
    return platform.processor() or "unknown"


def get_git_hash() -> str:
    """获取当前 Git commit hash."""
    try:
        result = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            check=True,
            capture_output=True,
            text=True,
        )
        return result.stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return "unknown"


def measure_memory(fn: Callable[[Any], Any], arg: Any, iterations: int) -> int:
    """测量函数的峰值内存.

    Args:
        fn: 待测函数.
        arg: 函数参数.
        iterations: 迭代次数.

    Returns:
        峰值内存字节数.
    """
    tracemalloc.start()
    for _ in range(iterations):
        fn(arg)
    _current, peak = tracemalloc.get_traced_memory()
    tracemalloc.stop()
    return peak


def measure(
    fn: Callable[[Any], Any], arg: Any, iterations: int, warmup: int, repeats: int
) -> list[float]:
    """测量单个函数的平均耗时.

    Args:
        fn: 待测函数.
        arg: 函数参数.
        iterations: 单次测量的循环次数.
        warmup: 预热次数.
        repeats: 重复次数.

    Returns:
        每次重复的平均耗时(ns)列表.
    """
    for _ in range(warmup):
        fn(arg)

    results: list[float] = []
    for _ in range(repeats):
        gc_enabled = gc.isenabled()
        gc.disable()
        start = perf_counter_ns()
        try:
            for _ in range(iterations):
                fn(arg)
        finally:
            if gc_enabled:
                gc.enable()
        elapsed = perf_counter_ns() - start
        results.append(elapsed / iterations)
    return results


def format_ns(ns: float) -> str:
    """格式化纳秒为微秒字符串."""
    return f"{ns / 1000:.2f}"


def format_ops(ns: float) -> str:
    """格式化 ops/s."""
    if ns <= 0:
        return "-"
    return f"{1_000_000_000 / ns:.0f}"


def format_kb(size: int) -> str:
    """格式化字节为 KB 字符串."""
    if size <= 0:
        return "-"
    return f"{size / 1024:.1f}"


def format_mb_per_s(size: int, ns: float) -> str:
    """格式化吞吐量 MB/s."""
    if ns <= 0:
        return "-"
    mb_per_s = (size / (ns / 1_000_000_000)) / 1024 / 1024
    return f"{mb_per_s:.2f}"


def normalize_raw(value: Any) -> Any:
    """规范化 raw 解码结果用于对比.

    bytes 如果可 UTF-8 解码则转为 str.
    """
    if isinstance(value, bytes):
        try:
            return value.decode("utf-8")
        except UnicodeDecodeError:
            return value
    if isinstance(value, list):
        return [normalize_raw(item) for item in value]
    if isinstance(value, tuple):
        return [normalize_raw(item) for item in value]
    if isinstance(value, dict):
        return {key: normalize_raw(val) for key, val in value.items()}
    return value


def build_deep_nested_raw(depth: int) -> dict[int, Any]:
    """构建深度嵌套 raw 数据."""
    node: dict[int, Any] = {0: "leaf"}
    for i in range(depth):
        node = {0: node, 1: i}
    return node


def build_scenarios() -> list[Scenario]:
    """构建基准场景."""
    small = _Small(123, "hello", True, 3.14, b"\x01\x02\x03")
    medium = _Medium(list(range(50)), ["a", "b", "c"], {"k": 1, "v": 2})
    parent = _Parent([_Child(i, f"n{i}") for i in range(20)], {"env": "prod"})
    large_blob = _Small(1, "blob", False, 0.0, b"x" * (1024 * 1024))
    many_small = _Medium(list(range(10_000)), ["x"] * 10, {"k": 1, "v": 2})

    raw_small = {0: 123, 1: "hello", 2: True, 3: 3.14, 4: b"\xff\xfe"}
    raw_medium = {0: list(range(50)), 1: {"a": 1, "b": 2}, 2: ["x", "y", "z"]}
    raw_nested = {0: {1: 123, 2: "hi"}, 1: [{0: i, 1: f"n{i}"} for i in range(20)]}
    raw_large_blob = {0: b"\xff" * (1024 * 1024), 1: "blob"}
    raw_many_small = {0: list(range(10_000))}
    raw_deep_nested = build_deep_nested_raw(30)

    return [
        Scenario(
            name="struct_small",
            payload=small,
            encode_fn=encode,
            decode_fn=lambda data: decode(_Small, data),
            note="基础字段",
        ),
        Scenario(
            name="struct_medium",
            payload=medium,
            encode_fn=encode,
            decode_fn=lambda data: decode(_Medium, data),
            note="列表/映射",
        ),
        Scenario(
            name="struct_nested",
            payload=parent,
            encode_fn=encode,
            decode_fn=lambda data: decode(_Parent, data),
            note="嵌套结构",
        ),
        Scenario(
            name="struct_large_blob",
            payload=large_blob,
            encode_fn=encode,
            decode_fn=lambda data: decode(_Small, data),
            note="大块字节",
        ),
        Scenario(
            name="struct_many_small",
            payload=many_small,
            encode_fn=encode,
            decode_fn=lambda data: decode(_Medium, data),
            note="大量小对象",
        ),
        Scenario(
            name="raw_small",
            payload=raw_small,
            encode_fn=encode_raw,
            decode_fn=decode_raw,
            note="原始字典",
        ),
        Scenario(
            name="raw_medium",
            payload=raw_medium,
            encode_fn=encode_raw,
            decode_fn=decode_raw,
            note="原始容器",
        ),
        Scenario(
            name="raw_nested",
            payload=raw_nested,
            encode_fn=encode_raw,
            decode_fn=decode_raw,
            note="原始嵌套",
        ),
        Scenario(
            name="raw_large_blob",
            payload=raw_large_blob,
            encode_fn=encode_raw,
            decode_fn=decode_raw,
            note="原始大块",
        ),
        Scenario(
            name="raw_many_small",
            payload=raw_many_small,
            encode_fn=encode_raw,
            decode_fn=decode_raw,
            note="原始小对象",
        ),
        Scenario(
            name="raw_deep_nested",
            payload=raw_deep_nested,
            encode_fn=encode_raw,
            decode_fn=decode_raw,
            note="原始深度嵌套",
        ),
    ]


def run(args: argparse.Namespace) -> int:
    """运行基准测试."""
    scenarios = build_scenarios()
    if args.scenarios:
        names = {name.strip() for name in args.scenarios.split(",") if name.strip()}
        scenarios = [s for s in scenarios if s.name in names]

    results = []
    print("\nTarsio Python Benchmark\n")
    print(
        f"Python {sys.version.split()[0]} | {platform.platform()} | CPU {get_cpu_info()} | repeats={args.repeats} | "
        f"iters={args.iterations} | warmup={args.warmup}\n"
    )

    header = (
        "name",
        "bytes",
        "enc_p50(us)",
        "enc_p95(us)",
        "dec_p50(us)",
        "dec_p95(us)",
        "enc_mem_kb",
        "dec_mem_kb",
        "enc_mb/s",
        "dec_mb/s",
        "enc_ops/s",
        "dec_ops/s",
        "note",
    )
    print(
        "{:<16} {:>7} {:>12} {:>12} {:>12} {:>12} {:>11} {:>11} {:>10} {:>10} {:>10} {:>10}  {}".format(
            *header
        )
    )

    for sc in scenarios:
        encoded = sc.encode_fn(sc.payload)
        decoded = sc.decode_fn(encoded)
        if sc.encode_fn is encode_raw:
            normalized = normalize_raw(decoded)
            redecoded = decode_raw(encode_raw(decoded))
            if normalize_raw(redecoded) != normalized:
                raise ValueError(f"基准数据校验失败: {sc.name}")
        elif sc.encode_fn(decoded) != encoded:
            raise ValueError(f"基准数据校验失败: {sc.name}")
        size = len(encoded)

        enc_samples = measure(
            sc.encode_fn, sc.payload, args.iterations, args.warmup, args.repeats
        )
        dec_samples = measure(
            sc.decode_fn, encoded, args.iterations, args.warmup, args.repeats
        )

        mem_iters = min(args.mem_iterations, args.iterations)
        enc_mem_peak = measure_memory(sc.encode_fn, sc.payload, mem_iters)
        dec_mem_peak = measure_memory(sc.decode_fn, encoded, mem_iters)

        enc_p50 = quantile(enc_samples, 0.5)
        enc_p95 = quantile(enc_samples, 0.95)
        dec_p50 = quantile(dec_samples, 0.5)
        dec_p95 = quantile(dec_samples, 0.95)

        print(
            f"{sc.name:<16} {size:>7} {format_ns(enc_p50):>12} {format_ns(enc_p95):>12} "
            f"{format_ns(dec_p50):>12} {format_ns(dec_p95):>12} {format_kb(enc_mem_peak):>11} "
            f"{format_kb(dec_mem_peak):>11} {format_mb_per_s(size, enc_p50):>10} "
            f"{format_mb_per_s(size, dec_p50):>10} {format_ops(enc_p50):>10} {format_ops(dec_p50):>10}  {sc.note}"
        )

        results.append(
            {
                "name": sc.name,
                "note": sc.note,
                "bytes": size,
                "encode": {
                    "samples_ns": enc_samples,
                    "p50_ns": enc_p50,
                    "p95_ns": enc_p95,
                    "mean_ns": statistics.mean(enc_samples),
                    "stdev_ns": statistics.pstdev(enc_samples),
                    "mem_peak_bytes": enc_mem_peak,
                    "mem_iterations": mem_iters,
                    "mb_per_s": (size / (enc_p50 / 1_000_000_000)) / 1024 / 1024
                    if enc_p50 > 0
                    else 0.0,
                },
                "decode": {
                    "samples_ns": dec_samples,
                    "p50_ns": dec_p50,
                    "p95_ns": dec_p95,
                    "mean_ns": statistics.mean(dec_samples),
                    "stdev_ns": statistics.pstdev(dec_samples),
                    "mem_peak_bytes": dec_mem_peak,
                    "mem_iterations": mem_iters,
                    "mb_per_s": (size / (dec_p50 / 1_000_000_000)) / 1024 / 1024
                    if dec_p50 > 0
                    else 0.0,
                },
            }
        )

    payload = {
        "timestamp": time.strftime("%Y-%m-%d %H:%M:%S", time.localtime()),
        "python": sys.version,
        "platform": platform.platform(),
        "cpu": get_cpu_info(),
        "git_hash": get_git_hash(),
        "repeats": args.repeats,
        "iterations": args.iterations,
        "warmup": args.warmup,
        "mem_iterations": args.mem_iterations,
        "results": results,
    }

    with open(args.output, "w", encoding="utf-8") as f:
        json.dump(payload, f, ensure_ascii=False, indent=2)

    print(f"\n结果已写入: {args.output}\n")
    return 0


def parse_args() -> argparse.Namespace:
    """解析命令行参数."""
    parser = argparse.ArgumentParser(description="Tarsio Python 基准测试")
    parser.add_argument(
        "--iterations", type=int, default=2000, help="单次测量的循环次数"
    )
    parser.add_argument("--warmup", type=int, default=200, help="预热次数")
    parser.add_argument("--repeats", type=int, default=5, help="重复次数")
    parser.add_argument(
        "--scenarios",
        type=str,
        default="",
        help="仅运行指定场景(逗号分隔),如 struct_small,raw_small",
    )
    parser.add_argument(
        "--output",
        type=str,
        default="bench_results.json",
        help="输出 JSON 路径",
    )
    parser.add_argument(
        "--mem-iterations",
        type=int,
        default=200,
        help="内存检测迭代次数(每个场景)",
    )
    return parser.parse_args()


def main() -> int:
    """主入口."""
    args = parse_args()
    return run(args)


if __name__ == "__main__":
    raise SystemExit(main())
