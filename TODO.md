# TODO

---

## Critical Issues

### 1. `delete_component` Corrupts Library File

**Severity:** CRITICAL
**Status:** Reproducible

**Description:**
After executing `delete_component` on a PcbLib file, the library file becomes corrupted (0 bytes). This results in complete data loss.

**Steps to Reproduce:**

1. Create a library with multiple components using `write_pcblib`
2. Execute `delete_component` to remove one component
3. Attempt to read/list the library — fails with "Invalid CFB file (0 bytes is too small)"

**Expected Behaviour:**
The library should remain valid with the specified component(s) removed.

**Actual Behaviour:**
The library file is truncated to 0 bytes, destroying all data.

**Impact:**
This is a data-destroying bug. Users could lose entire libraries.

---

### 2. `copy_component_cross_library` Fails with 3D Models

**Severity:** High
**Status:** Reproducible

**Description:**
When copying a component from one library to another, if the source component has a 3D STEP model reference, the operation fails with "Failed to read file: [modelname].step"

**Error Message:**

```text
Failed to write target library: Failed to read file: res0603.step
```

**Expected Behaviour:**
Either:

- Copy the component without the 3D model reference, or
- Copy the embedded 3D model along with the component, or
- Provide a clear warning that 3D models will not be copied

**Workaround:**
None currently available.

---

## Minor Issues / Limitations

### 3. No Embedded STEP Models Found

**Severity:** Low
**Status:** Limitation

**Description:**
The `extract_step_model` tool reports "No embedded 3D models found" for all tested libraries, even though `read_pcblib` shows `model_3d` references in footprints.

**Possible Causes:**

- The libraries may use external STEP file references rather than embedded models
- The extraction tool may not be looking in the correct OLE storage location

**Note:** This may not be a bug — the existing libraries may simply not have embedded models.

---

### 4. ASCII Render Pin Designator Truncation

**Severity:** Very Low (cosmetic)
**Status:** Known limitation

**Description:**
In `render_symbol` output, pin designators are truncated to single characters. For example, pins 17–32 all display as their first digit
(1, 2, 3...) rather than the full designator.

**Example:**

```text
| ~~1           1~~|   <- These are pins 17 and 18, not 1
```

**Impact:**
Minor — ASCII rendering is for quick preview only.

---

### 5. Floating-Point Precision

**Severity:** Very Low (cosmetic)
**Status:** Known limitation

**Description:**
Minor artifacts from mm↔mils conversion. Does not affect functionality.

---

## Nice-to-Have Features

### 1. Undo/Backup Functionality

Before performing destructive operations (delete, update, merge), automatically create a backup of the original file. This would prevent
data loss from bugs like Critical Issue #1.

### 2. Dry-Run Mode

Add an optional `dry_run` parameter to destructive operations that would show what changes would be made without actually modifying files.

### 3. Better 3D Model Handling

- Support for copying embedded 3D models between libraries
- Option to extract model references as external files
- Better error messages when models are missing

### 4. Validation Before Write

Run `validate_library` automatically after write operations to catch corruption immediately.

### 5. Transaction/Atomic Operations

Ensure write operations are atomic — if any part fails, the original file should remain unchanged.

### 6. Batch Parameter Update for SchLib

Similar to `batch_update` for PcbLib track widths, add ability to update parameters across all symbols in a SchLib (e.g., update all "Manufacturer" parameters).

### 7. Component Comparison

Add a tool to compare two specific components (not just libraries) and show detailed differences in pads, tracks, parameters, etc.

### 8. Better Error Context

When operations fail, provide more context about what was being processed and what state the file is in.

---

## Out of Scope

- **Component variants**: Board-level feature (.PcbDoc), not library (.PcbLib/.SchLib)
- **Net information**: Board-level feature, not library
