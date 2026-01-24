# TODO

## Future Enhancements

- [x] Add `render_symbol` tool for ASCII rendering of schematic symbols (parallel to `render_footprint`)
- [x] Improve error messages with more descriptive failure reasons
- [x] Add integration tests with real Altium library files (requires sample .PcbLib/.SchLib files)

## Testing Gaps

- [x] Verify `append: true` mode works correctly for `write_pcblib` and `write_schlib`
- [x] Test multi-part schematic symbols (e.g., dual op-amps)
- [x] Test 3D model attachment (`step_model` parameter)
- [x] Stress test pagination (`limit`/`offset`) with large libraries

## Documentation

- [x] Clarify pin 1 indicator usage in `write_pcblib` (rectangular vs round pad shape)

---

## Known Limitations

- **Floating-point precision**: Minor artifacts from mm↔mils conversion (cosmetic, doesn't affect functionality)

## Out of Scope

- **Component variants**: Board-level feature (.PcbDoc), not library (.PcbLib/.SchLib)
- **Net information**: Board-level feature, not library
