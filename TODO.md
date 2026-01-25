# TODO

---

## STEP Model Bugs

### 1. write_pcblib Silently Fails to Embed STEP Models

**Severity:** CRITICAL

**Status:** ✅ Fixed

When creating a footprint with `step_model` parameter, if the STEP file path was invalid,
`write_pcblib` would silently succeed without embedding the model.

**Fix:** Modified `prepare_3d_models_for_writing()` to return an error when:

- Footprint has no existing `component_bodies` (new footprint)
- AND the `step_model.filepath` doesn't point to a valid file

Now returns: `"STEP file not found for footprint 'NAME': 'path'. Provide a valid path or use embed: false for external reference."`

---

### 2. Delete Moves Orphaned Models to Other Components

**Severity:** High

**Status:** ✅ Fixed

After deleting a component, re-saving the library would create duplicate `component_bodies`
in other footprints if a file happened to exist with the same name as the model.

**Root Cause:** When reading a library, `model_3d.filepath` is set to the model NAME (e.g.,
`"RESC1005X04L.step"`). On save, if a file with that name existed in the current directory,
it would be read and a new `ComponentBody` created even though one already existed.

**Fix:** Modified `prepare_3d_models_for_writing()` to:

1. Skip if footprint has `component_bodies` AND `filepath` is just a name (no directory)
2. Only re-embed if user explicitly sets a full path to a STEP file
3. Clear old `component_bodies` when intentionally re-embedding from a new path

---

### 3. No External-Only STEP References

**Severity:** Medium

**Status:** ✅ Fixed

**Fix:** Added `embed: false` option to `step_model` in `write_pcblib`. When `embed: false`,
creates a `ComponentBody` with `embedded: false` without reading the STEP file from disk.

---

### 4. Orphaned References in Existing Libraries

**Severity:** Low

**Status:** ✅ Fixed

**Fix:** Added `repair_library` tool that removes orphaned component body references.
Also added `remove_orphaned_component_bodies()` method to `PcbLib` for programmatic access.

---

## Limitations

### 1. ASCII Render Pin Truncation

**Severity:** HIGH

**Status:** ✅ Fixed

**Fix:** Implemented smart designator mapping in `designator_to_char()`:

- Numeric 1-9: use digit ('1'-'9')
- Numeric 10-35: use letters ('A'-'Z')
- Numeric 36+: use '?'
- Alphanumeric (BGA style): use first character

---

## Nice-to-Have Features

### 1. Undo/Backup Functionality

**Status:** ✅ Implemented (timestamped backups on destructive operations)

### 2. Atomic/Transaction Operations

**Status:** ✅ Already Implemented

Both `PcbLib::save()` and `SchLib::save()` use atomic write pattern (temp file + rename).

### 3. Bulk Rename with Regex

**Status:** ✅ Implemented

Added `bulk_rename` tool with regex pattern matching and capture groups.

---

## Out of Scope

- **Component variants**: Board-level feature (.PcbDoc), not library (.PcbLib/.SchLib)
- **Net information**: Board-level feature, not library
