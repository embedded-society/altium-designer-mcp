# Scripts and Sample Files

This folder contains analysis scripts and sample Altium library files for **manual debugging only**.
These are NOT used by automated tests.

---

## Sample Files

| File | Description |
|------|-------------|
| `sample.PcbLib` | Sample PCB footprint library with generic chip resistor footprints |
| `sample.SchLib` | Corresponding schematic symbol library |

These files are useful for:

- Manual debugging of binary format issues
- Reverse engineering Altium file formats
- Testing with real Altium-created libraries

---

## Python Analysis Scripts

Analyses the binary format of Altium files using olefile.

### Prerequisites

```bash
# Create virtual environment (one-time setup)
python -m venv .venv
source .venv/bin/activate  # Linux/macOS
# or: .venv\Scripts\activate  # Windows

pip install olefile
```

### Usage

```bash
# Analyse the sample PcbLib
python analyze_pcblib.py

# Analyse a specific PcbLib file
python analyze_pcblib.py /path/to/library.PcbLib

# Analyse the sample SchLib
python analyze_schlib.py

# Analyse a specific SchLib file
python analyze_schlib.py /path/to/library.SchLib
```

### Output

- OLE structure (streams and storages)
- Component names and parameters
- Primitive data (pads, tracks, pins, rectangles, etc.)
- Binary format details for reverse engineering

---

## Note

Automated tests in `tests/` generate their own test data programmatically and do not depend
on these sample files. This ensures tests work in CI environments without external dependencies.
