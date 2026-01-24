# TODO

## Nice-to-Haves (Quality of Life)

| Feature | Why It Helps |
|---------|--------------|
| `reorder_components` | Control component order in library (some teams care about this) |
| `update_component` (in-place replace) | Currently must delete + write or rewrite entire library |

## Documentation/UX Polish

| Item | Notes |
|------|-------|
| Units reminder in responses | "Coordinates are in mm" / "Coordinates are in schematic units (10=1 grid)" |
| Layer name examples | List of valid layer names in tool description |
| Pad shape examples | Clarify "round" vs "circle" equivalence |

---

## Known Limitations

- **Floating-point precision**: Minor artifacts from mm↔mils conversion (cosmetic, doesn't affect functionality)

## Out of Scope

- **Component variants**: Board-level feature (.PcbDoc), not library (.PcbLib/.SchLib)
- **Net information**: Board-level feature, not library
