# TODO

## Bugs to Fix

These bugs were identified during a comprehensive code review (January 2026).

### ~~Bug #1: Duplicate ARCRESOLUTION Parameter~~ - FIXED

- **Status**: Fixed (January 2026)
- **Resolution**: Removed duplicate `ARCRESOLUTION=0.5mil` parameter from `build_component_body_params()`.

### ~~Bug #2: Weak Unique ID Generation~~ - FIXED

- **Status**: Fixed (January 2026)
- **Resolution**: Added atomic counter to `generate_unique_id()` to ensure uniqueness across rapid successive calls.

### ~~Bug #3: Log Level Match Inconsistency~~ - FIXED

- **Status**: Fixed (January 2026)
- **Resolution**: Added explicit `"warn" => Level::WARN` match arm in `get_log_level()`.

### Potential Issue #4: Silent Read Error in SchLib

- **Location**: [src/altium/schlib/mod.rs:111-113](src/altium/schlib/mod.rs#L111-L113)
- **Issue**: When `stream.read_to_end()` fails, the component is silently skipped.
- **Impact**: Corrupted components are silently dropped without user notification.
- **Fix**: Log a warning when skipping components due to read errors.

### Potential Issue #5: Model Compression Failure Returns Empty

- **Location**: [src/altium/pcblib/writer.rs:1085-1090](src/altium/pcblib/writer.rs#L1085-L1090)
- **Issue**: If compression fails, an empty `Vec<u8>` is returned instead of propagating the error.
- **Impact**: Silently produces invalid model data.
- **Fix**: Return `Result<Vec<u8>>` and propagate errors.

### Potential Issue #6: Integer Truncation on SchLib Pin Coordinates

- **Location**: [src/altium/schlib/writer.rs:96-101](src/altium/schlib/writer.rs#L96-L101)
- **Issue**: Pin `x`, `y`, `length` are `i32` but truncated to `i16` when writing binary pin records.
- **Impact**: Coordinates exceeding ±32767 will wrap around silently.
- **Fix**: Add validation or use the text-based pin format for large coordinates.

### Code Quality #7: Windows-1252 Encoding Approximation

- **Location**: [src/altium/schlib/reader.rs:79-84](src/altium/schlib/reader.rs#L79-L84)
- **Issue**: Uses `b as char` which only works for ASCII. Non-ASCII Windows-1252 bytes produce incorrect characters.
- **Fix**: Use the `encoding_rs` crate for proper Windows-1252 decoding.

## Future Enhancements

- [ ] Add integration tests with real Altium library files (requires sample .PcbLib/.SchLib files)
- [ ] Support component variants (board-level feature, not library)
- [ ] Support net information (board-level feature, not library)
