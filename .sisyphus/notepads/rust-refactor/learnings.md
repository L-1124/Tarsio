## Infrastructure Setup
- Successfully switched build backend from hatchling to maturin.
- Configured maturin to build jce-core as jce.jce_core extension.
- Added jce-core to uv workspace and sources in pyproject.toml.
- Verified build and import of jce.jce_core.
## Rust Implementation Learnings
- Ported JCE constants and types to Rust.
- Used `#[repr(u8)]` for `JceType` enum to match protocol byte values.
- Implemented `TryFrom<u8>` for `JceType` for safe conversion.
- Defined `JceDecodeError` using `thiserror` for structured error handling.
## Learnings - Rust JceWriter Implementation

- Implemented `JceWriter` in Rust with primitive support.
- **Zero Tag Optimization**: Successfully implemented `Zero Tag` (0x0C) for `write_int` when value is 0.
- **Integer Compression**: Implemented range-based compression (Int1, Int2, Int4, Int8).
- **SimpleList (Bytes)**: Discovered that `SimpleList` header for bytes requires a fixed `Int1` with value 0 (representing BYTE type) and does NOT use `Zero Tag` optimization for that specific byte.
- **Tag Encoding**: Tags < 15 use 1 byte (Tag << 4 | Type), Tags >= 15 use 2 bytes (0xF0 | Type, Tag).
- **Endianness**: Used Big Endian for all multi-byte primitives as per JCE standard.
## Rust Implementation Learnings

- **JceReader**: Implemented with  and  for efficient binary parsing.
- **Header Parsing**: Correctly handles 1-byte and 2-byte headers (Tag >= 15).
- **Skip Logic**: Implemented recursive skipping for complex types (Map, List, Struct).
- **Error Handling**: Uses  with offset-based path tracking, which can be augmented by higher-level callers.

## Rust Implementation Learnings (Reader)

- **JceReader**: Implemented with std::io::Cursor and byteorder for efficient binary parsing.
- **Header Parsing**: Correctly handles 1-byte and 2-byte headers (Tag >= 15).
- **Skip Logic**: Implemented recursive skipping for complex types (Map, List, Struct).
- **Error Handling**: Uses JceDecodeError with offset-based path tracking, which can be augmented by higher-level callers.
Implemented PyO3 bindings for dumps and loads in jce-core/src/serde.rs. Handled context, OMIT_DEFAULT, and SimpleList. Updated lib.rs and verified with cargo build.
- Updated jce/stream.py to use jce.api.dumps and jce.api.loads, removing legacy encoder/decoder dependencies.
- Fixed jce/api.py to handle bytearray input by converting to bytes before passing to Rust core.
- Fixed convert_bytes_recursive in jce/api.py to preserve JceDict type when processing nested structures.
