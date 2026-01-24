# TODO

## Missing Features (Would Unblock Workflows)

| Feature | Current Workaround | Impact |
|---------|-------------------|--------|
| `import_library` (from JSON) | Manual `write_pcblib`/`write_schlib` | Can't round-trip exported data back in |
| `merge_libraries` | Manual component-by-component | Common need when combining projects |

## Nice-to-Haves (Quality of Life)

| Feature | Why It Helps |
|---------|--------------|
| Append mode for `write_schlib` | PcbLib has it, SchLib should too for consistency |
| `search_components` with regex/glob across multiple libraries | Finding components without knowing exact library |
| `get_component` (single component by name without full read) | Faster than read + filter for large libraries |
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
