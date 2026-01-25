# TODO

---

## Minor Issues / Limitations

### 1. No Embedded STEP Models Found

**Severity:** Low
**Status:** Limitation

**Description:**
The `extract_step_model` tool reports "No embedded 3D models found" for all tested libraries, even though `read_pcblib` shows `model_3d` references in footprints.

**Possible Causes:**

- The libraries may use external STEP file references rather than embedded models
- The extraction tool may not be looking in the correct OLE storage location

**Note:** This may not be a bug — the existing libraries may simply not have embedded models.

---

### 2. ASCII Render Pin Designator Truncation

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

### 3. Floating-Point Precision

**Severity:** Very Low (cosmetic)
**Status:** Known limitation

**Description:**
Minor artifacts from mm↔mils conversion. Does not affect functionality.

---

## Nice-to-Have Features

### 1. Dry-Run Mode

Add an optional `dry_run` parameter to destructive operations that would show what changes would be made without actually modifying files.

### 2. Better 3D Model Handling

- Option to extract model references as external files
- Better error messages when models are missing

### 3. Validation Before Write

Run `validate_library` automatically after write operations to catch corruption immediately.

### 4. Batch Parameter Update for SchLib

Similar to `batch_update` for PcbLib track widths, add ability to update parameters across all symbols in a SchLib (e.g., update all "Manufacturer" parameters).

### 5. Component Comparison

Add a tool to compare two specific components (not just libraries) and show detailed differences in pads, tracks, parameters, etc.

### 6. Better Error Context

When operations fail, provide more context about what was being processed and what state the file is in.

---

## Out of Scope

- **Component variants**: Board-level feature (.PcbDoc), not library (.PcbLib/.SchLib)
- **Net information**: Board-level feature, not library
