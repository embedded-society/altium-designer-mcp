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

### ~~Potential Issue #4: Silent Read Error in SchLib~~ - FIXED

- **Status**: Fixed (January 2026)
- **Resolution**: Added `tracing::warn!` logs when component stream open or read fails.

### ~~Potential Issue #5: Model Compression Failure Returns Empty~~ - FIXED

- **Status**: Fixed (January 2026)
- **Resolution**: Changed `compress_model_data()` to return `AltiumResult<Vec<u8>>` and added `CompressionError` variant to `AltiumError`. Errors now propagate properly.

### ~~Potential Issue #6: Integer Truncation on SchLib Pin Coordinates~~ - FIXED

- **Status**: Fixed (January 2026)
- **Resolution**: Added validation in `write_binary_pin()` to return `AltiumError::InvalidParameter` if x, y, or length
  exceed ±32767. The `encode_data_stream()` now returns `AltiumResult<Vec<u8>>` and propagates errors.

### ~~Code Quality #7: Windows-1252 Encoding Approximation~~ - FIXED

- **Status**: Fixed (January 2026)
- **Resolution**: Added `encoding_rs` crate dependency and replaced `b as char` with proper `WINDOWS_1252.decode()`
  for non-UTF-8 text records in SchLib reader.

## Potential Issues (Second Code Review - January 2026)

### Potential Issue #8: String Length Truncation in SchLib Binary Pins

- **Location**: [src/altium/schlib/writer.rs:104, 150, 156](src/altium/schlib/writer.rs#L104)
- **Issue**: Pin description, name, and designator byte lengths are cast to `u8`. Strings longer than 255 bytes will be
  silently truncated.
- **Impact**: Pin names/designators exceeding 255 characters would be corrupted. Very unlikely in practice.
- **Fix**: Add validation to return an error if string length exceeds 255, or use text-based pin format for long strings.

### Potential Issue #9: String Length Truncation in PcbLib Writer

- **Location**: [src/altium/pcblib/writer.rs:122](src/altium/pcblib/writer.rs#L122)
- **Issue**: Pad designator and footprint name lengths are cast to `u8`. Strings longer than 255 bytes will be
  silently truncated.
- **Impact**: Designators exceeding 255 characters would be corrupted. Extremely unlikely in practice.
- **Fix**: Add validation to return an error if string length exceeds 255.

### Code Quality #10: Missing File Type Validation

- **Location**: [src/altium/schlib/mod.rs](src/altium/schlib/mod.rs),
  [src/altium/pcblib/mod.rs](src/altium/pcblib/mod.rs)
- **Issue**: When opening files, there's no validation that the file is actually the expected type (SchLib vs PcbLib).
- **Impact**: Opening a PcbLib as SchLib (or vice versa) produces confusing error messages rather than a clear
  "wrong file type" error.
- **Fix**: Check the FileHeader stream for type-specific markers and return a clear error message.

## Future Enhancements

- [ ] Add integration tests with real Altium library files (requires sample .PcbLib/.SchLib files)
- [ ] Support component variants (board-level feature, not library)
- [ ] Support net information (board-level feature, not library)
