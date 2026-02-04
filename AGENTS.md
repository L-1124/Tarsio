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

## 子 AGENTS.md 说明

项目内包含多个子级 `AGENTS.md`，用于按目录细化规范与注意事项：

* `src/AGENTS.md`：Rust 核心层约定与模块说明。
* `src/binding/AGENTS.md`：Python 绑定层的边界规则与约束。
* `src/codec/AGENTS.md`：协议读写器与低层编解码注意事项。
* `python/tests/AGENTS.md`：Python 集成测试规范与约束。

使用原则：**进入对应目录时优先遵循该目录下的子 AGENTS.md**，与本文件冲突时以更近的子级规范为准。

## 开发约定

* **混合构建**: 项目使用 `maturin` 构建，`uv` 作为依赖管理工具。
* **类型安全**: Rust 强制内存安全；Python 使用运行时检查 (`typing.get_type_hints`) 构建 schema。
* **错误处理**: Rust 错误跨越 FFI 边界映射为 Python 异常。

## 注释规范

* **语言**: 所有注释均使用中文撰写，但是使用英文标点符号。

## Commit 格式规范

采用 Conventional Commits 格式，提交信息均使用中文撰写：

```text
<type>(<scope>): <subject>

<body>

<footer>
```

**类型定义:**

* `feat`: 新功能
* `fix`: 缺陷修复
* `perf`: 性能优化
* `refactor`: 重构（无功能变更）
* `style`: 格式调整（不改变代码逻辑）
* `test`: 添加或修改测试
* `docs`: 文档更新
* `chore`: 构建工具、依赖版本等变更
* `ci`: CI/CD 配置变更

**示例:**

```text
feat(binding): 添加 MAX_DEPTH 递归深度限制

实现反序列化和序列化路径的递归深度检查，防止
恶意输入导致的栈溢出和 DoS 攻击。

- 在 de.rs 中集成递归深度追踪
- 在 ser.rs 中检测循环引用
- 默认最大深度设置为 100
```

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
