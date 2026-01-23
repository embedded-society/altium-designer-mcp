# TODO

## Bugs to Fix

No known bugs at this time. All identified issues have been fixed:

- Issue #8: String length truncation in SchLib binary pins - Fixed
- Issue #9: String length truncation in PcbLib writer - Fixed
- Issue #10: Missing file type validation - Fixed

## Code Quality Notes (Non-Bugs)

### Note #1: Similar Helper Functions in MCP Server

- **Location**: [server.rs:1748-1788](src/mcp/server.rs#L1748-L1788)
- **Observation**: The `most_common_f64()`, `most_common_i32()`, and `most_common_u8()` functions are very similar and
  could potentially be generalized into a single generic function using traits.
- **Impact**: None - this is purely cosmetic and the current code is clear and works correctly.
- **Action**: Optional refactoring if desired.

### Note #2: ASCII Decoding in WideStrings Parser

- **Location**: [reader.rs:116-122](src/altium/pcblib/reader.rs#L116-L122)
- **Observation**: `decode_ascii_codes()` uses `c as char` for u8 to char conversion. This is correct for ASCII
  (0-127) but would produce extended Latin-1 characters for values 128-255.
- **Impact**: Low - Altium's ENCODEDTEXT format uses ASCII codes, so values 128-255 are unlikely in practice.
- **Action**: Monitor if non-ASCII text causes issues in WideStrings parsing.

## Future Enhancements

- [ ] Add integration tests with real Altium library files (requires sample .PcbLib/.SchLib files)
- [ ] Support component variants (board-level feature, not library)
- [ ] Support net information (board-level feature, not library)
