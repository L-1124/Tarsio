# Tarsio 测试套件

本目录包含 Tarsio 的完整测试套件，涵盖功能验证、类型检查与性能基准。

## 目录结构

* `test_*.py`: 核心功能的标准集成测试。
* `benchmarks/`: 原始接口与 Schema 接口的性能基准测试。
* `typechecking/`: 使用 `typing_extensions.assert_type` 验证静态类型推导行为。

## 运行测试

### 标准功能测试

```bash
uv run pytest python/tests
```

### 静态类型检查

使用 `basedpyright` 或 `pyright` 对 `typechecking` 目录进行扫描:

```bash
uv run basedpyright python/tests/typechecking
```

### 性能基准测试

```bash
uv run pytest python/tests/benchmarks
```

若需将结果导出为 JSON 文件进行分析:

```bash
uv run pytest python/tests/benchmarks --benchmark-json out.json
```
