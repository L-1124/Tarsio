from dataclasses import dataclass

from tarsio import encode


@dataclass
class U1:
    """测试用 Union 变体 1."""

    x: int


@dataclass
class U2:
    """测试用 Union 变体 2."""

    y: int


@dataclass
class U3:
    """测试用 Union 变体 3."""

    z: str


@dataclass
class U4:
    """测试用 Union 变体 4."""

    a: bool


@dataclass
class U5:
    """测试用 Union 变体 5."""

    b: float


@dataclass
class Wrapper:
    """含有 5 个变体的 Union 容器."""

    val: U1 | U2 | U3 | U4 | U5


def test_union_encoding_perf(benchmark):
    """验证 Union 类型在特定变体命中时的编码性能."""
    # 测试场景：编码最后一个变体，触发初始线性扫描但在缓存命中后降至 O(1).
    obj = Wrapper(val=U5(b=3.14))

    def run_encode():
        encode(obj)

    benchmark(run_encode)


def test_union_encoding_mixed_perf(benchmark):
    """验证 Union 类型在混合变体场景下的缓存压力与分发性能."""
    # 测试场景：混合多种变体以压测缓存的并发读取与分发稳定性.
    objs = [
        Wrapper(val=U1(x=1)),
        Wrapper(val=U2(y=2)),
        Wrapper(val=U3(z="hello")),
        Wrapper(val=U4(a=True)),
        Wrapper(val=U5(b=3.14)),
    ]

    def run_encode_mixed():
        for obj in objs:
            encode(obj)

    benchmark(run_encode_mixed)
