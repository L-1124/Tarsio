# Plan: JCEstruct Rust Refactor

## Context

### Original Request
Refactor `jce` core serialization logic from Python to Rust using PyO3/Maturin to achieve 5x-10x performance improvements.

### Interview Summary
**Key Decisions**:
- **Directory**: `jce-core` directory in root (uv workspace style).
- **Build**: Switch completely to `maturin`.
- **Fallback**: No Python fallback. Replace legacy implementation.
- **Architecture**: Hybrid Schema-Cached (Python defines schema, Rust executes).

**Metis Review Findings**:
- **Guardrail**: Must implement explicit recursion depth check (default 100).
- **Optimization**: Schema must flag fields with hooks (`@jce_field_serializer`) to avoid unnecessary Python callbacks.
- **Verification**: `tests/test_protocol.py` is the golden standard for binary compatibility.

---

## Work Objectives

### Core Objective
Replace Python-based `encoder.py`/`decoder.py` with a high-performance Rust extension (`jce-core`) while maintaining 100% API and binary compatibility.

### Concrete Deliverables
1. **Rust Crate**: `jce-core` with `dumps`, `loads`, `JceNode`.
2. **Build Configuration**: Updated `pyproject.toml` using `maturin`.
3. **Python Bridge**: Updated `jce/api.py` and `jce/struct.py` (schema generation).
4. **Deleted Legacy Code**: Removal of `jce/encoder.py` and `jce/decoder.py`.

### Definition of Done
- [ ] `uv run maturin develop` builds successfully.
- [ ] `uv run pytest` passes all tests (especially `test_protocol.py`).
- [ ] Performance benchmark shows >5x speedup for simple structs.
- [ ] `Zero Tag` and `SimpleList` optimizations work identical to legacy version.

### Must Have
- **Schema Caching**: Generate schema once per class, not per instance.
- **Accurate Error Paths**: `JceDecodeError` must contain `path="users[0].name"` info.
- **Recursion Limit**: Hard cap at depth 100 to prevent stack overflow.

### Must NOT Have
- **Runtime Reflection**: Rust should not scan `__dict__` blindly without schema guidance.
- **New Features**: No new JCE types or protocol extensions.

---

## Verification Strategy

### Test Decision
- **Infrastructure exists**: YES (`pytest`).
- **Strategy**: **Reuse Existing Tests**. The existing Python test suite covers the protocol thoroughly. We will run these tests against the new Rust backend.
- **New Tests**: Rust unit tests (`cargo test`) for low-level writer/reader logic.

### Manual QA Procedure
1. **Build**: `uv run maturin develop`
2. **Test**: `uv run pytest`
3. **Debug**: `jce "0C" -v` (using CLI to verify Rust backend output)

---

## Task Flow

```
Setup (1) → Rust Core (2-4) → Python Bridge (5-6) → Cleanup (7)
```

## Parallelization

| Group | Tasks | Reason |
|-------|-------|--------|
| A | 2, 3 | Writer and Reader are independent |

---

## TODOs

- [x] 1. **Infrastructure Setup**
  **What to do**:
  - Initialize Rust crate: `cargo init --lib jce-core`
  - Configure `jce-core/Cargo.toml`: Add `pyo3`, `serde`, `thiserror`.
  - Update root `pyproject.toml`:
    - Change `build-backend` to `maturin`.
    - Add `jce-core` to excludes/includes as needed.
  - Create `rust-toolchain.toml` (optional, for consistency).
  **Acceptance**:
  - `uv run maturin develop` creates a `.so` (or `.pyd`) file.
  - `import jce.jce_core` works in Python REPL.

- [x] 2. **Rust: Implement Constants & Types**
  **What to do**:
  - Port constants from `jce/const.py` to `jce-core/src/consts.rs`.
  - Define JCE Type enum.
  - Define `JceDecodeError` struct with `path` field in `jce-core/src/error.rs`.
  - **Type Mapping**: Define explicit mapping between Python `jce.types` constants (int IDs) and Rust enums.
  **References**:
  - `jce/const.py` - Source of truth for constants.
  **Acceptance**:
  - `cargo test` passes for constant values.

- [x] 3. **Rust: Implement Writer (Encoder)**
  **What to do**:
  - Create `jce-core/src/writer.rs`.
  - Implement `JceWriter` struct with `Vec<u8>`.
  - Implement `write_tag(tag, type)`.
  - Implement primitive writers: `write_int`, `write_float`, `write_string`.
  - **Critical**: Implement `Zero Tag` optimization (if value is 0, write tag with ZERO_TAG type).
  **References**:
  - `jce/encoder.py:DataWriter` - Python implementation reference.
  **Acceptance**:
  - `cargo test` verifies bytes for primitives match `test_protocol.py` expectations.

- [x] 4. **Rust: Implement Reader (Decoder)**
  **What to do**:
  - Create `jce-core/src/reader.rs`.
  - Implement `JceReader` struct with cursor.
  - Implement `read_tag()`, `peek_tag()`, `skip_field()`.
  - Implement `read_int` (1/2/4/8 byte handling).
  - Implement `read_string` (1/4 byte length).
  - **Critical**: Implement `path` tracking in reader context.
  **References**:
  - `jce/decoder.py:DataReader` - Python implementation reference.
  **Acceptance**:
  - `cargo test` verifies decoding primitives works.

- [x] 5. **Rust: Implement PyO3 Bindings (The Engine)**
  **What to do**:
  - Create `jce-core/src/lib.rs` (module entry point).
  - Create `jce-core/src/serde.rs` (high level logic).
  - Implement `dumps(obj, schema, options, context)`:
    - **Context Support**: Accept `context` dict from Python.
    - Iterate Python object based on schema.
    - **Optimization**: Check `if value == default_value` (from schema) -> skip if OMIT_DEFAULT option is set.
    - Check "has_serializer" flag -> call Python hook if true (passing context).
    - Call `JceWriter` to emit bytes.
  - Implement `loads(bytes, schema, options, context)`:
    - Create `JceReader`.
    - Iterate schema to construct Python object.
    - Check "has_deserializer" flag -> call Python hook if true (passing context).
  **References**:
  - `jce/encoder.py:JceEncoder` - Logic for iterating fields.
  - `jce/decoder.py:JceDecoder` - Logic for object construction.
  **Acceptance**:
  - `dumps` returns bytes.
  - `loads` returns Python objects.
  - `context` is correctly passed to field serializers.

- [x] 6. **Python: Schema Generation & Bridge**
  **What to do**:
  - Modify `jce/struct.py`: Add `__get_jce_core_schema__(cls)`.
    - Return list of `(field_name, tag_id, jce_type, default_value, has_serializer, has_deserializer)`.
    - **Optimization**: Include `default_value` in tuple to enable OMIT_DEFAULT in Rust without reflection.
    - Cache this on the class.
  - Modify `jce/api.py`:
    - Import `dumps`/`loads` from `jce_core`.
    - Remove old `JceEncoder`/`JceDecoder` usage.
    - Pass schema and context to Rust functions.
  **Acceptance**:
  - `JceStruct` instances can be passed to `jce.dumps`.
  - `jce.loads` returns correct `JceStruct` instances.

- [x] 7. **Rust: Implement Generic Serde (JceDict support)**
  **What to do**:
  - Modify `jce-core/src/serde.rs`:
    - Implement `dumps_generic(obj, options, context)`.
    - Implement `loads_generic(data, options, context)`.
  - Update `lib.rs` to expose them.
  - Update `jce/api.py` to use `jce_core.dumps_generic` / `loads_generic` for `JceDict`/`dict` targets.
  **Acceptance**:
  - `jce-core` supports schema-less encoding/decoding.
  - `jce/api.py` uses Rust for `JceDict`.

- [x] 8. **Cleanup & Verification** (Part 2: Update stream.py)
  **What to do**:
  - Modify `jce/stream.py` to use `jce_core` instead of `encoder.py` and `decoder.py`.
  - `JceStreamWriter.pack`: use `jce_core.dumps_generic` (or `dumps` if struct).
  - `LengthPrefixedWriter.pack`: use `jce_core.dumps_generic`.
  - `LengthPrefixedReader.__iter__`: use `jce_core.loads_generic` (or `loads`).
  - Remove imports of `JceEncoder`, `DataReader`, `GenericDecoder`, `SchemaDecoder`.
  - Ensure `convert_bytes_recursive` is imported from `api` (which I just fixed).
  **Acceptance**:
  - `jce/stream.py` no longer depends on legacy modules.
  - `tests/test_stream.py` passes.

- [ ] 9. **Cleanup & Verification** (Part 3: Delete Legacy Code)
  **What to do**:
  - Delete `jce/encoder.py`.
  - Delete `jce/decoder.py`.
  - Run full test suite: `uv run pytest`.
  - Fix any remaining imports or regressions.
  **Acceptance**:
  - All tests pass (GREEN).
  - No Python fallback code remains.
  **What to do**:
  - Delete `jce/encoder.py` and `jce/decoder.py`.
  - Run full test suite: `uv run pytest`.
  - Fix any regressions found in `test_protocol.py`.
  **Acceptance**:
  - All tests pass (GREEN).
  - No Python fallback code remains.

---

## Success Criteria
- [ ] `uv run pytest` passes 100%.
- [ ] `test_protocol.py` specifically confirms binary compatibility.
- [ ] Error messages include paths (e.g., `Error at data.id`).
