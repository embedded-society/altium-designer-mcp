# TODO

Missing features and improvements for altium-designer-mcp.

---

## PcbLib - Potential Improvements

### Additional Layer Support

**Priority:** Low

Some layer IDs default to `MultiLayer` when unknown. Consider adding explicit support for:

- Layers 86-255 (extended mechanical layers in newer Altium versions)

---

## Testing

### Real-World Library Testing

**Priority:** High

Test with real Altium libraries across different Altium Designer versions to ensure compatibility.

**Areas to test:**

- Libraries created in Altium Designer 20, 21, 22, 23, 24
- Libraries with complex multi-part symbols
- Libraries with embedded 3D models from different sources
- Libraries using pin symbol decorations

---

## Documentation

### API Documentation

**Priority:** Medium

- Add more rustdoc examples for public API functions
- Document edge cases and limitations

---
