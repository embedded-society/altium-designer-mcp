# TODO

## Bugs to Fix

No known bugs at this time. All identified issues have been fixed:

- Issue #8: String length truncation in SchLib binary pins - Fixed
- Issue #9: String length truncation in PcbLib writer - Fixed
- Issue #10: Missing file type validation - Fixed

## Code Quality Notes (Non-Bugs)

All code quality issues have been addressed:

- Note #1: Generalized `most_common_*` helper functions into a single generic function - Fixed
- Note #2: Fixed ASCII decoding in WideStrings parser to replace non-ASCII bytes with U+FFFD - Fixed

## Future Enhancements

- [ ] Add integration tests with real Altium library files (requires sample .PcbLib/.SchLib files)
- [ ] Support component variants (board-level feature, not library)
- [ ] Support net information (board-level feature, not library)
