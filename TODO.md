# TODO

## Future Enhancements

- [ ] Add integration tests with real Altium library files (requires sample .PcbLib/.SchLib files)
- [ ] Support component variants (board-level feature, not library)
- [ ] Support net information (board-level feature, not library)

---

## Known Limitations

- **Component name truncation**: Names limited to 31 bytes (Altium OLE format limitation)
- **Floating-point precision**: Minor artifacts from mm↔mils conversion (cosmetic, doesn't affect functionality)
