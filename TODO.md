# TODO

---

## STEP Model Bugs

### 1. Delete Does Not Clean Up Orphaned Models

**Severity:** Medium

When a component with an embedded STEP model is deleted, the model data remains in the library as an orphan. This causes library bloat over time.

**Status:** ✅ Fixed

**Fix:** Added `remove_orphaned_models()` method to `PcbLib` that is called after deleting components.
The delete operation now reports `orphaned_models_removed` count in its response.

### 2. Model References Get Duplicated

**Severity:** High

After certain operations (possibly cross-library copy), components end up with duplicate `component_bodies` entries pointing to different model GUIDs
for the same STEP file. A component created with 1 STEP model was found to have 2 `component_bodies` after cross-library copy operations.

**Status:** ✅ Fixed

**Root Cause:** When reading a library, `populate_model_3d_from_component_bodies()` set `model_3d.filepath` to the model NAME (not a real path).
On write, `prepare_3d_models_for_writing()` tried to read this as a file and created duplicate entries if a file with that name existed.

**Fix:** Modified `prepare_3d_models_for_writing()` to skip `model_3d` entries where the filepath doesn't exist as a readable file.

### 3. Cross-Library Copy May Corrupt Source Library

**Severity:** High

Cross-library copy appears to add extra model references back to the source library. After copying a component FROM library A TO library B,
library A had more models than expected.

**Status:** ✅ Investigated - No Bug Found

**Analysis:** Reviewed the `copy_pcblib_component_cross_library` code thoroughly. The source library is only opened for reading
and is never saved. The code path only saves to the target library. This bug may have been caused by Bug #2 (duplicate model references)
which is now fixed, or was a user misunderstanding about which library was which.

### 4. No External-Only STEP References

**Severity:** Medium

The API always embeds STEP models. No option to create external reference without having the file present.

**Status:** ✅ Implemented

**Fix:** Added `embed` option to `step_model` in `write_pcblib`. When `embed: false`, creates a `ComponentBody` with `embedded: false`
without reading the STEP file from disk. The `filepath` is used as the model name for external reference.

### 5. Orphaned References in Existing Libraries

**Severity:** Low (data issue)

Existing libraries have `component_bodies` with `embedded: true` and model GUIDs, but actual STEP binary data is missing. Fresh footprints work correctly.

**Status:** ✅ Fixed

**Fix:** Added `repair_library` tool that removes orphaned component body references (references to non-existent embedded models).
Also added `remove_orphaned_component_bodies()` method to `PcbLib` for programmatic access.

---

## Limitations

### 1. ASCII Render Pin Truncation

**Severity:** HIGH

Multi-digit pin designators truncated to single characters. A confusing legend is shown (e.g., `'1'=10,11,12...`) but this doesn't really help readability.

**Status:** ✅ Fixed

**Fix:** Implemented smart designator mapping in `designator_to_char()`:

- Single character: use as-is
- Numeric 1-9: use digit ('1'-'9')
- Numeric 10-35: use letters ('A'-'Z')
- Numeric 36+: use '?'
- Alphanumeric (BGA style): use first character

Now each pad up to 35 gets a unique display character, making the ASCII render much more readable.

---

## Nice-to-Have Features

### 1. Undo/Backup Functionality

Auto-backup before destructive operations.

**Status:** ✅ Implemented (timestamped backups exist)

### 2. Atomic/Transaction Operations

Partial failures shouldn't corrupt files.

**Status:** ✅ Already Implemented

Both `PcbLib::save()` and `SchLib::save()` use atomic write pattern:

1. Write to temp file (`.pcblib.tmp` / `.schlib.tmp`)
2. If write fails, delete temp file and return error
3. If write succeeds, atomically rename temp file to target

Combined with the backup system, this provides robust protection against data loss.

### 3. Bulk Rename with Regex

Pattern-based renaming.

**Status:** ✅ Implemented

**Fix:** Added `bulk_rename` tool that supports regex pattern matching with capture groups.
Example: `pattern: "^RESC(.*)$"`, `replacement: "RES_$1"` renames `RESC0402` to `RES_0402`.
Includes dry-run mode and conflict detection.

### 4. Library Statistics Dashboard

**Status:** ✅ Implemented (now in `extract_style` — layers, pad shapes, track widths)

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
