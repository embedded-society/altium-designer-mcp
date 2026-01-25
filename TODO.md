# TODO

---

## Minor Issues / Limitations

### 1. External vs Embedded STEP Models

**Severity:** Low
**Status:** Documented behaviour

**Description:**
Altium libraries can reference 3D models in two ways:

- **External references** (`model_3d`): Path to a STEP file on disk
- **Embedded models** (`component_bodies`): STEP data stored inside the library

The `extract_step_model` tool only extracts embedded models. If a library uses external references, the error message now explains this and shows the external file paths.

**Note:** Most libraries use external STEP file references. Use `read_pcblib` to see both `model_3d` (external) and `component_bodies` (embedded) for each footprint.

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

## Out of Scope

- **Component variants**: Board-level feature (.PcbDoc), not library (.PcbLib/.SchLib)
- **Net information**: Board-level feature, not library
