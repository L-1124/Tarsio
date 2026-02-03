# 项目知识库

**生成时间:** 2026-02-03
**上下文:** Rust/Python 混合 Tars/JCE 协议库

## 概览

Tarsio 是一个由 Rust 核心驱动的高性能 Python Tars (JCE) 协议实现。
它使用 **PyO3** 将 Rust 的速度（通过 `maturin`）与 Python 的灵活性结合起来。
核心二元性：**Rust** 处理原始字节/协议逻辑；**Python** 提供 `Struct` 模型和用户 API。

## 结构

```tree
.
├── src/                # Rust 核心（重活累活）
│   ├── binding/        # PyO3 绑定（Python <-> Rust 胶水层）
│   └── codec/          # 纯 Tars 协议实现（Reader/Writer）
├── python/             # Python 包装器 & 测试
│   ├── tarsio/         # 用户面代码包
│   └── tests/          # Python 侧集成测试
├── Cargo.toml          # Rust 工作区配置
└── pyproject.toml      # Python 构建配置 (Maturin)
```

## 导航指南

| 任务        | 位置             | 说明                                       |
|-------------|------------------|--------------------------------------------|
| 协议逻辑    | `src/codec/`     | 原始 JCE 编解码规则，零拷贝优化            |
| Python 绑定 | `src/binding/`   | `PyClass` 定义, `SchemaRegistry`, FFI 边界 |
| Python API  | `python/tarsio/` | `Field`, `Struct`, 以及 CLI 入口           |
| 构建配置    | `pyproject.toml` | 定义 `maturin` 构建后端及依赖              |

## 开发约定

* **混合构建**: 项目使用 `maturin` 构建。不要直接使用 `setup.py`。
* **类型安全**: Rust 强制内存安全；Python 使用运行时检查 (`typing.get_type_hints`) 构建 schema。
* **错误处理**: Rust 错误跨越 FFI 边界映射为 Python 异常。

## 常用命令

```bash
# 环境设置 (自动处理依赖安装和 Rust 扩展编译)
uv sync

# 仅在需要显式重编译时运行 (通常 uv sync 会自动处理)
uv run maturin develop

# 测试
uv run pytest        # Python 测试
cargo test           # Rust 单元测试

# 代码检查 (Linting)
uv run ruff check .  # Python linting
cargo clippy         # Rust linting
```

## 架构说明

* **Schema 注册表**: `src/binding/schema.rs` 维护全局注册表，将 Python 类连接到 Rust schema。
* **入口点**:
    * Rust 库: `src/lib.rs` (定义 `_core` 模块)。
    * Python 包: `python/tarsio/__init__.py`。

## 设计不变式 (Design Invariants)

以下核心机制已冻结，修改需极度谨慎：

1. **Schema 权威性**: `StructDef` (Rust侧) 是唯一事实来源。Python 类仅作为定义的入口。
2. **反序列化机制**: `decode` 必须使用 `__new__` 构造实例，**严禁触发** 用户的 `__init__`。
3. **API 对称性**: 编码 (`obj.encode()`) 与解码 (`Cls.decode(bytes)`) 必须在语义和数据上完全可逆。
4. **错误模型**:
    * `TypeError`: Schema 不匹配、类型未注册。
    * `ValueError`: 数据损坏、Required 字段缺失、Tag 未知（且无法跳过）。
5. **实现收敛**: `tarsio.encode/decode` 和 `obj.encode/Cls.decode` 必须路由到同一个 Rust 实现 (`encode_object`/`decode_object`)。
