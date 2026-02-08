"""Tarsio Python Benchmark Script.

此脚本包含三个主要部分 (Workload, Benchmark, FlameGraph)，用于全面评估 Tarsio 性能。

Structure:
    1. Workload: 定义测试数据模型 (Struct) 和生成逻辑 (Scenarios).
    2. Benchmark: 执行核心测试循环，测量耗时与内存，输出统计报表.
    3. FlameGraph: 调用 perf (Linux) 或 py-spy (跨平台) 生成性能火焰图.
"""

import argparse
import gc
import json
import math
import os
import platform
import random
import shutil
import string
import subprocess
import sys
import time
import tracemalloc
from collections.abc import Callable
from dataclasses import dataclass
from datetime import date, datetime, timedelta, timezone
from datetime import time as dt_time
from decimal import Decimal
from time import perf_counter_ns
from typing import Annotated, Any
from uuid import UUID

from tarsio import Struct, decode, decode_raw, encode, encode_raw

# ==============================================================================
# SECTION 1: UTILS & HELPERS
# ==============================================================================


def quantile(values: list[float], q: float) -> float:
    """计算分位数."""
    if not values:
        return 0.0
    data = sorted(values)
    idx = round((len(data) - 1) * q)
    return data[idx]


def get_cpu_info() -> str:
    """获取 CPU 型号信息."""
    info = "unknown"
    if sys.platform.startswith("linux"):
        try:
            with open("/proc/cpuinfo", encoding="utf-8") as f:
                for line in f:
                    if line.lower().startswith("model name"):
                        info = line.split(":", 1)[1].strip()
                        break
        except OSError:
            pass
    else:
        info = platform.processor() or "unknown"

    # 简单的频率/Governor 检查 (Linux)
    if sys.platform.startswith("linux"):
        try:
            with open(
                "/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor",
                encoding="utf-8",
            ) as f:
                gov = f.read().strip()
                if gov != "performance":
                    print(
                        f"Warning: CPU governor is '{gov}', not 'performance'. Results may be unstable."
                    )
        except FileNotFoundError:
            pass

    return info


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
    except Exception:
        return "unknown"


def measure_memory(
    fn: Callable[[Any], Any], arg: Any, iterations: int
) -> tuple[int, int]:
    """测量函数的峰值内存和单次分配量."""
    gc_enabled = gc.isenabled()
    gc.disable()
    try:
        # 1. Measure Single Iteration Allocation (Approx)
        tracemalloc.start()
        snapshot1 = tracemalloc.take_snapshot()
        _ = fn(arg)
        snapshot2 = tracemalloc.take_snapshot()
        tracemalloc.stop()

        # Calculate diff sum
        stats = snapshot2.compare_to(snapshot1, "lineno")
        allocated_single = sum(stat.size_diff for stat in stats)

        del snapshot1, snapshot2, stats
        gc.collect()

        # 2. Measure Peak Usage (Loop without holding results)
        tracemalloc.start()
        tracemalloc.clear_traces()

        for _ in range(iterations):
            _ = fn(arg)  # Discard immediately

        _, peak = tracemalloc.get_traced_memory()
        tracemalloc.stop()

        return peak, max(0, int(allocated_single))
    finally:
        if gc_enabled:
            gc.enable()


def measure(
    fn: Callable[[Any], Any],
    arg: Any,
    iterations: int,
    warmup: int,
    repeats: int,
) -> tuple[list[float], float, float]:
    """测量函数执行耗时, 返回 (samples, mean, stdev)."""
    # Warmup
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

    # Calculate stats
    mean = sum(results) / len(results)
    variance = sum((x - mean) ** 2 for x in results) / len(results)
    stdev = math.sqrt(variance)

    return results, mean, stdev


def format_ns(ns: float) -> str:
    """格式化 ns 为毫秒/微秒."""
    if ns < 1000:
        return f"{ns:.0f}ns"
    if ns < 1_000_000:
        return f"{ns / 1000:.2f}us"
    return f"{ns / 1_000_000:.2f}ms"


def format_ops(ns: float) -> str:
    """格式化 ops/s."""
    return "-" if ns <= 0 else f"{1_000_000_000 / ns:.0f}"


def format_kb(size: int) -> str:
    """格式化字节为 KB."""
    return "-" if size <= 0 else f"{size / 1024:.1f}"


def format_mb_per_s(size: int, ns: float) -> str:
    """格式化 MB/s."""
    if ns <= 0:
        return "-"
    mb_per_s = (size / (ns / 1_000_000_000)) / 1024 / 1024
    return f"{mb_per_s:.2f}"


def normalize_raw(value: Any) -> Any:
    """规范化 raw 数据的解码结果 (bytes 转 str)."""
    if isinstance(value, bytes):
        try:
            return value.decode("utf-8")
        except UnicodeDecodeError:
            return value
    if isinstance(value, list):
        return [normalize_raw(v) for v in value]
    if isinstance(value, dict):
        return {k: normalize_raw(v) for k, v in value.items()}
    return value


def get_tool_path(name: str) -> str | None:
    """获取工具路径."""
    return shutil.which(name)


# ==============================================================================
# SECTION 2: WORKLOAD (Scenarios & Data Models)
# ==============================================================================


try:

    class _Small(Struct):
        value: Annotated[int, 0]
        name: Annotated[str, 1]
        flag: Annotated[bool, 2]
        score: Annotated[float, 3]
        blob: Annotated[bytes, 4]

    class _Medium(Struct):
        values: Annotated[list[int], 0]
        tags: Annotated[list[str], 1]
        props: Annotated[dict[str, int], 2]
        note: Annotated[str | None, 3] = None

    class _Child(Struct):
        id: Annotated[int, 0]
        label: Annotated[str, 1]

    class _Parent(Struct):
        children: Annotated[list[_Child], 0]
        meta: Annotated[dict[str, str], 1]

    class _Stdlib(Struct):
        dt: Annotated[datetime, 0]
        d: Annotated[date, 1]
        t: Annotated[dt_time, 2]
        td: Annotated[timedelta, 3]
        uid: Annotated[UUID, 4]
        dec: Annotated[Decimal, 5]

except NameError:
    # Fallback/Dummy for when import fails (to avoid crash during parsing if imports failed)
    _Small = Any  # type: ignore
    _Medium = Any  # type: ignore
    _Child = Any  # type: ignore
    _Parent = Any  # type: ignore
    _Stdlib = Any  # type: ignore


@dataclass(frozen=True)
class Scenario:
    """基准场景定义."""

    name: str
    payload: Any
    encode_fn: Callable[[Any], bytes]
    decode_fn: Callable[[bytes], Any]
    note: str


def _rand_str(k: int = 8) -> str:
    # Mixed charset: ASCII + Digits + Chinese + Emoji
    pool = (
        string.ascii_letters + string.digits + "你好世界测试数据常用汉字" + "😀🚀🔥🐍⚡"
    )
    return "".join(random.choices(pool, k=k))


def _rand_bytes(k: int = 16) -> bytes:
    return random.randbytes(k)


def build_deep_nested_raw(depth: int) -> dict[int, Any]:
    """构建深度嵌套的原始数据."""
    node: dict[int, Any] = {0: "leaf"}
    for i in range(depth):
        node = {0: node, 1: i}
    return node


def build_scenarios(randomize: bool = False) -> list[Scenario]:
    """构建所有基准测试场景."""

    # Data generation helpers
    def _build_small(rand):
        if rand:
            return _Small(
                random.randint(0, 10000),
                _rand_str(),
                random.choice([True, False]),
                random.random(),
                _rand_bytes(16),
            )
        return _Small(123, "hello", True, 3.14, b"\x01\x02\x03")

    def _build_medium(rand):
        if rand:
            return _Medium(
                [random.randint(0, 100) for _ in range(50)],
                [_rand_str(4) for _ in range(3)],
                {_rand_str(2): random.randint(0, 10) for _ in range(2)},
                _rand_str(10) if random.random() > 0.5 else None,
            )
        return _Medium(list(range(50)), ["a", "b", "c"], {"k": 1, "v": 2})

    def _build_parent(rand):
        if rand:
            children = [_Child(i, _rand_str(6)) for i in range(random.randint(10, 30))]
            meta = {_rand_str(4): _rand_str(8) for _ in range(random.randint(1, 5))}
            return _Parent(children, meta)
        return _Parent([_Child(i, f"n{i}") for i in range(20)], {"env": "prod"})

    def _build_stdlib(rand):
        if rand:
            return _Stdlib(
                datetime.now(timezone.utc),
                date.today(),
                datetime.now().time(),
                timedelta(days=random.randint(1, 100)),
                UUID(int=random.getrandbits(128)),
                Decimal(f"{random.random():.4f}"),
            )
        return _Stdlib(
            datetime(2025, 1, 1, 12, 0, 0, tzinfo=timezone.utc),
            date(2025, 1, 1),
            dt_time(12, 0, 0),
            timedelta(days=1, seconds=3600),
            UUID("12345678-1234-5678-1234-567812345678"),
            Decimal("3.14159"),
        )

    small = _build_small(randomize)
    medium = _build_medium(randomize)
    parent = _build_parent(randomize)
    stdlib = _build_stdlib(randomize)

    # Large objects (always generated dynamically for size accuracy)
    blob_1mb = _rand_bytes(1024 * 1024) if randomize else b"x" * (1024 * 1024)
    blob_10mb = (
        _rand_bytes(10 * 1024 * 1024) if randomize else b"x" * (10 * 1024 * 1024)
    )
    blob_100mb = (
        _rand_bytes(100 * 1024 * 1024) if randomize else b"x" * (100 * 1024 * 1024)
    )

    large_blob = _Small(1, "blob", False, 0.0, blob_1mb)
    extra_large_blob = _Small(1, "blob", False, 0.0, blob_10mb)
    huge_blob = _Small(1, "blob", False, 0.0, blob_100mb)

    many_small = _Medium(list(range(10_000)), ["x"] * 10, {"k": 1, "v": 2})

    # Raw data
    raw_small = {
        0: random.randint(0, 10000) if randomize else 123,
        1: _rand_str() if randomize else "hello",
        2: random.choice([True, False]) if randomize else True,
        3: random.random() if randomize else 3.14,
        4: _rand_bytes(16) if randomize else b"\xff\xfe",
    }

    raw_medium = {
        0: [random.randint(0, 100) for _ in range(50)]
        if randomize
        else list(range(50)),
        1: [_rand_str(4) for _ in range(3)] if randomize else {"a": 1, "b": 2},
        2: {_rand_str(2): random.randint(0, 10) for _ in range(2)}
        if randomize
        else ["x", "y", "z"],
        3: (_rand_str(10) if random.random() > 0.5 else None) if randomize else None,
    }

    raw_nested = (
        {
            0: [{0: i, 1: _rand_str(6)} for i in range(random.randint(10, 30))],
            1: {_rand_str(4): _rand_str(8) for _ in range(random.randint(1, 5))},
        }
        if randomize
        else {
            0: {1: 123, 2: "hi"},
            1: [{0: i, 1: f"n{i}"} for i in range(20)],
        }
    )

    raw_deep_nested = build_deep_nested_raw(30)

    # 序列类型专项基准
    list_subclass = type("_BenchList", (list,), {})
    raw_range = {0: range(10_000)}
    raw_list_subclass = {0: list_subclass(range(10_000))}

    return [
        Scenario(
            "struct_small", small, encode, lambda d: decode(_Small, d), "基础字段"
        ),
        Scenario(
            "struct_medium", medium, encode, lambda d: decode(_Medium, d), "列表/映射"
        ),
        Scenario(
            "struct_nested", parent, encode, lambda d: decode(_Parent, d), "嵌套结构"
        ),
        Scenario(
            "struct_1mb", large_blob, encode, lambda d: decode(_Small, d), "1MB 字节"
        ),
        Scenario(
            "struct_10mb",
            extra_large_blob,
            encode,
            lambda d: decode(_Small, d),
            "10MB 字节",
        ),
        Scenario(
            "struct_100mb", huge_blob, encode, lambda d: decode(_Small, d), "100MB 字节"
        ),
        Scenario(
            "struct_many_small",
            many_small,
            encode,
            lambda d: decode(_Medium, d),
            "大量小对象",
        ),
        Scenario("raw_small", raw_small, encode_raw, decode_raw, "原始字典"),
        Scenario("raw_medium", raw_medium, encode_raw, decode_raw, "原始容器"),
        Scenario("raw_nested", raw_nested, encode_raw, decode_raw, "原始嵌套"),
        Scenario(
            "raw_1mb", {0: blob_1mb, 1: "blob"}, encode_raw, decode_raw, "原始 1MB"
        ),
        Scenario(
            "raw_10mb", {0: blob_10mb, 1: "blob"}, encode_raw, decode_raw, "原始 10MB"
        ),
        Scenario(
            "raw_100mb",
            {0: blob_100mb, 1: "blob"},
            encode_raw,
            decode_raw,
            "原始 100MB",
        ),
        Scenario(
            "raw_many_small",
            {0: list(range(10_000))},
            encode_raw,
            decode_raw,
            "原始小对象",
        ),
        Scenario("raw_range", raw_range, encode_raw, decode_raw, "原始 range 序列"),
        Scenario(
            "raw_list_subclass",
            raw_list_subclass,
            encode_raw,
            decode_raw,
            "原始 list 子类",
        ),
        Scenario(
            "raw_deep_nested", raw_deep_nested, encode_raw, decode_raw, "原始深度嵌套"
        ),
        Scenario(
            "struct_stdlib", stdlib, encode, lambda d: decode(_Stdlib, d), "Stdlib 类型"
        ),
    ]


# ==============================================================================
# SECTION 3: BENCHMARK ENGINE
# ==============================================================================


def run_benchmark(args: argparse.Namespace) -> int:
    """执行基准测试."""
    if args.seed is not None:
        random.seed(args.seed)

    scenarios = build_scenarios(randomize=args.random)
    if args.scenarios:
        names = {n.strip() for n in args.scenarios.split(",") if n.strip()}
        scenarios = [s for s in scenarios if s.name in names]

    results = []
    print("\nTarsio Python Benchmark\n")
    print(
        f"Python {sys.version.split()[0]} | {platform.platform()} | CPU {get_cpu_info()}\n"
        f"Settings: repeats={args.repeats} | iters={args.iterations} | "
        f"warmup={args.warmup} | random={args.random}\n"
    )

    header = (
        "name",
        "bytes",
        "enc_p99",
        "enc_cv%",
        "dec_p99",
        "dec_cv%",
        "mem/op",
        "peak_mem",
        "enc_mb/s",
        "dec_mb/s",
        "enc_ops",
        "dec_ops",
        "note",
    )
    # Compressed header for display
    print(
        "{:<16} {:>7} {:>10} {:>7} {:>10} {:>7} {:>9} {:>8} {:>9} {:>9} {:>8} {:>8}  {}".format(
            *header
        )
    )
    print("-" * 130)

    for sc in scenarios:
        current_iters = args.iterations
        try:
            encoded = sc.encode_fn(sc.payload)
            size = len(encoded)
            # 对于大对象, 减少性能测试迭代次数以避免过长等待
            if size > 100 * 1024 * 1024:  # > 100MB
                current_iters = max(1, min(current_iters, 2))
            elif size > 10 * 1024 * 1024:  # > 10MB
                current_iters = max(1, min(current_iters, 5))
            elif size > 1024 * 1024:  # > 1MB
                current_iters = max(1, min(current_iters, 50))
        except Exception as e:
            print(f"Skipping {sc.name}: encode failed with error: {e}")
            continue

        # Verify decode
        decoded = sc.decode_fn(encoded)
        if sc.encode_fn is encode_raw:
            normalized = normalize_raw(decoded)
            redecoded = decode_raw(encode_raw(decoded))
            if normalize_raw(redecoded) != normalized:
                pass

        # Measure Time & Stats
        enc_samples, enc_mean, enc_stdev = measure(
            sc.encode_fn, sc.payload, current_iters, args.warmup, args.repeats
        )
        dec_samples, dec_mean, dec_stdev = measure(
            sc.decode_fn, encoded, current_iters, args.warmup, args.repeats
        )

        # Calculate CV (Coefficient of Variation)
        enc_cv = (enc_stdev / enc_mean * 100) if enc_mean > 0 else 0.0
        dec_cv = (dec_stdev / dec_mean * 100) if dec_mean > 0 else 0.0

        # Measure Memory
        mem_iters = min(args.mem_iterations, args.iterations)
        if size > 1024 * 1024:
            mem_iters = max(1, 5)

        enc_mem_peak, enc_alloc_single = measure_memory(
            sc.encode_fn, sc.payload, mem_iters
        )
        dec_mem_peak, dec_alloc_single = measure_memory(
            sc.decode_fn, encoded, mem_iters
        )

        # Stats
        enc_p50 = quantile(enc_samples, 0.5)
        enc_p95 = quantile(enc_samples, 0.95)
        enc_p99 = quantile(enc_samples, 0.99)

        dec_p50 = quantile(dec_samples, 0.5)
        dec_p95 = quantile(dec_samples, 0.95)
        dec_p99 = quantile(dec_samples, 0.99)

        print(
            f"{sc.name:<16} {format_kb(size):>7} {format_ns(enc_p99):>10} {enc_cv:>6.1f}% "
            f"{format_ns(dec_p99):>10} {dec_cv:>6.1f}% {format_kb(enc_alloc_single):>9} "
            f"{format_kb(enc_mem_peak):>8} {format_mb_per_s(size, enc_p50):>9} "
            f"{format_mb_per_s(size, dec_p50):>9} {format_ops(enc_p50):>8} {format_ops(dec_p50):>8}  {sc.note}"
        )

        results.append(
            {
                "name": sc.name,
                "bytes": size,
                "encode": {
                    "p50_ns": enc_p50,
                    "p95_ns": enc_p95,
                    "p99_ns": enc_p99,
                    "cv_pct": enc_cv,
                    "alloc_bytes": enc_alloc_single,
                    "mem_peak": enc_mem_peak,
                },
                "decode": {
                    "p50_ns": dec_p50,
                    "p95_ns": dec_p95,
                    "p99_ns": dec_p99,
                    "cv_pct": dec_cv,
                    "alloc_bytes": dec_alloc_single,
                    "mem_peak": dec_mem_peak,
                },
            }
        )

    if args.output:
        payload = {
            "meta": {
                "timestamp": time.strftime("%Y-%m-%d %H:%M:%S", time.localtime()),
                "python": sys.version,
                "platform": platform.platform(),
                "cpu": get_cpu_info(),
            },
            "results": results,
        }
        with open(args.output, "w", encoding="utf-8") as f:
            json.dump(payload, f, ensure_ascii=False, indent=2)
        print(f"\n结果已写入: {args.output}\n")

    return 0


# ==============================================================================
# SECTION 4: FLAMEGRAPH PROFILER (Linux Only / Py-Spy)
# ==============================================================================


def run_flamegraph(args: argparse.Namespace) -> int:
    """生成火焰图."""
    tool = args.flamegraph_tool

    # Auto-detect tool if not specified
    if not tool:
        # Check py-spy explicitly (str | None -> bool)
        py_spy_path = get_tool_path("py-spy")
        if py_spy_path:
            tool = "py-spy"
        elif sys.platform.startswith("linux") and get_tool_path("perf"):
            tool = "perf"
        elif get_tool_path("samply"):
            tool = "samply"
        else:
            print("Error: No suitable profiling tool found (install py-spy or samply).")
            return 1

    print(f"Using profiler: {tool}")

    _ensure_pdb_symbols()

    # Determine the python executable to profile
    target_python = sys.executable
    if sys.platform == "win32":
        if hasattr(sys, "_base_executable"):
            target_python = sys._base_executable  # type: ignore
        elif hasattr(sys, "base_exec_prefix"):
            base_python = os.path.join(sys.base_exec_prefix, "python.exe")
            if os.path.exists(base_python):
                target_python = base_python

    print(f"Target Python: {target_python}")

    env = os.environ.copy()
    if target_python != sys.executable:
        current_paths = [p for p in sys.path if p]
        new_python_path = os.pathsep.join(current_paths)

        existing_python_path = env.get("PYTHONPATH", "")
        if existing_python_path:
            env["PYTHONPATH"] = new_python_path + os.pathsep + existing_python_path
        else:
            env["PYTHONPATH"] = new_python_path

        print(
            f"Augmented PYTHONPATH: {len(current_paths)} paths added to support venv."
        )

    output_file = args.output or "bench.svg"

    if tool == "py-spy":
        spy = get_tool_path("py-spy")
        if not spy:
            print("Error: py-spy not found. Install with: pip install py-spy")
            return 1

        # Determine format based on extension
        fmt = "flamegraph"
        if output_file.endswith(".json"):
            fmt = "speedscope"
        elif output_file.endswith(".txt"):
            fmt = "raw"
        elif output_file.endswith(".svg"):
            fmt = "flamegraph"

        cmd: list[str] = [
            spy,
            "record",
            "--native",
            "--format",
            fmt,
            "-o",
            output_file,
            "--",
            target_python,
            __file__,
            "--profile",
        ]

        # Propagate other args
        if args.scenarios:
            cmd.extend(["--scenarios", args.scenarios])
        if args.duration:
            cmd.extend(["--duration", str(args.duration)])

        # Propagate benchmark settings
        cmd.extend(["--iterations", str(args.iterations)])
        cmd.extend(["--warmup", str(args.warmup)])
        cmd.extend(["--repeats", str(args.repeats)])
        cmd.extend(["--mem-iterations", str(args.mem_iterations)])
        if args.random:
            cmd.append("--random")
        if args.seed is not None:
            cmd.extend(["--seed", str(args.seed)])

        print(f"Executing: {' '.join(cmd)}")
        return subprocess.call(cmd, env=env)

    elif tool == "samply":
        samply = get_tool_path("samply") or get_tool_path("samply.exe")
        if not samply:
            print("Error: samply not found. Install with: cargo install samply")
            return 1

        print(
            "Note: samply runs a local server. Press Ctrl+C to stop recording (if interactive) or wait for duration."
        )

        # Usage: samply record [options] command [args]...
        samply_args = []
        if args.output:
            # save to file instead of opening browser
            samply_args.extend(["--save-only", "--profile-name", args.output])

        script_args = ["--profile"]
        if args.scenarios:
            script_args.extend(["--scenarios", args.scenarios])
        if args.duration:
            script_args.extend(["--duration", str(args.duration)])

        # Propagate benchmark settings
        script_args.extend(["--iterations", str(args.iterations)])
        script_args.extend(["--warmup", str(args.warmup)])
        script_args.extend(["--repeats", str(args.repeats)])
        script_args.extend(["--mem-iterations", str(args.mem_iterations)])
        if args.random:
            script_args.append("--random")
        if args.seed is not None:
            script_args.extend(["--seed", str(args.seed)])

        # samply record [samply_options] python script.py [script_options]
        cmd = [samply, "record", *samply_args, target_python, __file__, *script_args]

        print(f"Executing: {' '.join(cmd)}")
        return subprocess.call(cmd, env=env)

    elif tool == "perf":
        if not sys.platform.startswith("linux"):
            print("Error: perf is only supported on Linux.")
            return 1

        perf = get_tool_path("perf")
        stackcollapse = get_tool_path("stackcollapse-perf.pl")
        flamegraph = get_tool_path("flamegraph.pl")

        if not all([perf, stackcollapse, flamegraph]):
            print("Error: Required tools (perf, FlameGraph scripts) not found.")
            return 1

        # Help type checker/runtime
        assert perf is not None
        assert stackcollapse is not None
        assert flamegraph is not None

        cmd = [
            perf,
            "record",
            "-F",
            "99",
            "-g",
            "--",
            target_python,
            __file__,
            "--profile",
            "--duration",
            str(args.duration),
        ]
        if args.scenarios:
            cmd.extend(["--scenarios", args.scenarios])

        # Propagate benchmark settings
        cmd.extend(["--iterations", str(args.iterations)])
        cmd.extend(["--warmup", str(args.warmup)])
        cmd.extend(["--repeats", str(args.repeats)])
        cmd.extend(["--mem-iterations", str(args.mem_iterations)])
        if args.random:
            cmd.append("--random")
        if args.seed is not None:
            cmd.extend(["--seed", str(args.seed)])

        print(f"Running perf record: {' '.join(cmd)}")
        subprocess.run(cmd, check=True)

        with open(output_file, "w") as out:
            p1 = subprocess.Popen([perf, "script"], stdout=subprocess.PIPE)
            # cast to ensure type checker is happy if it was stricter, but assert above should suffice for runtime
            p2 = subprocess.Popen(
                [stackcollapse], stdin=p1.stdout, stdout=subprocess.PIPE
            )
            p3 = subprocess.Popen([flamegraph], stdin=p2.stdout, stdout=out)
            p3.communicate()

        print(f"\nFlameGraph generated: {output_file}")
        return 0

    return 1


# ==============================================================================
# CLI ENTRY POINT
# ==============================================================================


def parse_args() -> argparse.Namespace:
    """解析命令行参数."""
    p = argparse.ArgumentParser(description="Tarsio Python Benchmark")
    p.add_argument("--iterations", type=int, default=2000, help="每轮测试的迭代次数")
    p.add_argument("--warmup", type=int, default=200, help="预热迭代次数")
    p.add_argument("--repeats", type=int, default=5, help="重复轮数")
    p.add_argument(
        "--scenarios", type=str, default="", help="指定运行的场景 (逗号分隔)"
    )
    p.add_argument(
        "--output", type=str, help="输出结果文件路径 (JSON results or SVG flamegraph)"
    )
    p.add_argument("--mem-iterations", type=int, default=100, help="内存测试迭代次数")

    # Mode selection
    p.add_argument(
        "--flamegraph", action="store_true", help="生成火焰图 (Profiling mode)"
    )
    p.add_argument(
        "--profile",
        action="store_true",
        default=False,
        help="运行基准测试 (Benchmark mode)",
    )

    p.add_argument(
        "--flamegraph-tool",
        choices=["perf", "py-spy", "samply"],
        help="指定火焰图工具",
    )
    p.add_argument("--duration", type=float, default=10.0, help="基准测试持续时间 (秒)")
    p.add_argument("--random", action="store_true", help="使用随机生成的数据")
    p.add_argument("--seed", type=int, help="随机种子")

    args = p.parse_args()

    # Default behavior: if neither flag is set, run benchmark
    if not args.flamegraph and not args.profile:
        args.profile = True

    return args


def _ensure_pdb_symbols() -> None:
    """Windows Only: 尝试将 target/release 下的 .pdb 复制到 .pyd 同目录."""
    if sys.platform != "win32":
        return

    try:
        from tarsio import _core

        pyd_path = _core.__file__
        if not pyd_path:
            return

        # Locate project root (assuming script is in scripts/)
        project_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

        # Check target/release/tarsio_core.pdb
        pdb_src = os.path.join(project_root, "target", "release", "tarsio_core.pdb")

        if not os.path.exists(pdb_src):
            return

        # 保持原始文件名，不要重命名为 .pyd 的名称
        pdb_dest = os.path.join(os.path.dirname(pyd_path), "tarsio_core.pdb")

        # Only copy if source PDB is newer or destination missing
        src_mtime = os.path.getmtime(pdb_src)
        if not os.path.exists(pdb_dest) or src_mtime > os.path.getmtime(pdb_dest):
            print(f"Updating symbols: {pdb_dest}")
            try:
                shutil.copy2(pdb_src, pdb_dest)
            except (PermissionError, OSError) as e:
                print(f"Warning: Failed to update symbols: {e}")

    except Exception:
        pass


def main() -> int:
    """主入口函数."""
    args = parse_args()

    ret = 0

    if args.profile:
        ret |= run_benchmark(args)

    if args.flamegraph:
        ret |= run_flamegraph(args)

    return ret


if __name__ == "__main__":
    sys.exit(main())
