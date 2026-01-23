# TODO

## Bugs to Fix

### ~~Potential Issue #8: String Length Truncation in SchLib Binary Pins~~ - FIXED

- **Status**: Fixed (January 2026)
- **Resolution**: Added validation in `write_binary_pin()` to return `AltiumError::InvalidParameter` if pin name,
  designator, or description exceeds 255 bytes.

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
