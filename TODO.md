# TODO

---

## Test Coverage — Completed

All identified test gaps have been addressed. The test suite now includes:

### MCP Tool Handler Tests (Added)

- [x] `list_components` tool (PcbLib and SchLib)
- [x] `get_component` tool (found/not found cases)
- [x] `search_components` tool (glob/regex patterns)
- [x] `write_pcblib` tool (create new, append mode)
- [x] `write_schlib` tool (create new, append mode)
- [x] `delete_component` tool
- [x] `rename_component` tool
- [x] `copy_component_cross_library` tool (with/without models)
- [x] `render_footprint` tool
- [x] `render_symbol` tool

### Error Path Tests (Added)

- [x] Invalid/non-existent file paths
- [x] Path outside allowed directories
- [x] Unsupported file extensions

### Backup Functionality Tests (Added)

- [x] Timestamped backup creation on destructive operations

### STEP Model Tests (Added)

- [x] Case-insensitive GUID matching
- [x] Multiple STEP models in one library
- [x] Copying footprints with embedded STEP models
- [x] External (non-embedded) model references
- [x] Multiple footprints sharing same STEP model
- [x] Large STEP file compression

---

## Out of Scope

- **Component variants**: Board-level feature (.PcbDoc), not library (.PcbLib/.SchLib)
- **Net information**: Board-level feature, not library
