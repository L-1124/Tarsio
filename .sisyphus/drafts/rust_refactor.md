# Draft: JceStruct Rust Core Refactoring

## Requirements (Confirmed)
- **Goal**: 5x-10x performance boost via Rust core.
- **Tech Stack**: Rust + PyO3 + Maturin.
- **Architecture**: Hybrid (Python defines models, Rust executes logic).
- **Features**:
    - Schema-Aware Serialization (no intermediate dicts).
    - Smart Parsing for SIMPLE_LIST (Type 13).
    - Accurate Error Paths (e.g., `data.users[0].id`).
    - Binary Blob recursive serialization.
    - Debugging support (`decode_nodes`).
- **Compatibility**:
    - `api.dumps/loads` signature match.
    - Fallback to Python implementation (TBD: mechanism).

## Technical Decisions
- **Build System**: Maturin (needs integration with `uv` and `pyproject.toml`).
- **Data Flow**: Python `JceStruct` -> Schema Extraction -> Rust `Encoder` -> Bytes.
- **Parsing Strategy**: Iterative stack-based parsing in Rust to avoid recursion limits.

## Open Questions
1.  **Project Layout**: Where does the Rust code live? (`src/rust`? separate crate root?)
2.  **Fallback Mechanism**: Runtime switch? Compile-time flag?
3.  **Schema Format**: What exactly does the schema look like passed to Rust? `Vec<(tag, type, name)>`?
4.  **Error Mapping**: How to map Rust `Result` to Python `JceDecodeError` with context?
5.  **CI/CD**: How does this affect the build pipeline?

## Scope Boundaries
- **IN**: `jce-core` extension, `api.py` routing, `struct.py` schema generation.
- **OUT**: `config.py`, CLI, Pydantic core logic.
