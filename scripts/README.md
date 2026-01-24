# Scripts and Sample Files

This folder contains analysis scripts and sample Altium library files for testing and development.

## Sample Files

| File | Description |
|------|-------------|
| `sample.PcbLib` | Sample PCB footprint library containing generic chip resistor footprints (0402, 0603, 0805, etc.) with IPC-7351B compliant land patterns |
| `sample.SchLib` | Corresponding schematic symbol library with generic chip resistor symbols |

These files are used for:

- Testing the binary format reader/writer
- Validating round-trip encoding/decoding
- Running the manual analysis test

## Analysis Scripts

### Python Analysis (`analyze_pcblib.py`, `analyze_schlib.py`)

Analyzes the binary format of PcbLib and SchLib files using olefile.

**Prerequisites:**

```bash
# Create virtual environment (one-time setup)
python -m venv .venv
source .venv/bin/activate  # Linux/macOS
# or: .venv\Scripts\activate  # Windows

pip install olefile
```

**Usage:**

```bash
# Analyse PcbLib sample file
python analyze_pcblib.py

# Analyse a specific PcbLib file
python analyze_pcblib.py /path/to/library.PcbLib

# Analyse SchLib sample file
python analyze_schlib.py

# Analyse a specific SchLib file
python analyze_schlib.py /path/to/library.SchLib
```

**Output:**

- OLE structure (streams and storages)
- Component names and parameters
- Primitive data (pads, tracks, pins, rectangles, etc.)
- Binary format details for reverse engineering

### Rust Analysis Tests

The Rust analysis tests (`tests/pcblib_analysis.rs`, `tests/schlib_analysis.rs`) provide similar functionality in Rust.

**Usage:**

```bash
# From project root
cargo test --test pcblib_analysis -- --ignored --nocapture
cargo test --test schlib_analysis -- --ignored --nocapture
```

## Binary Format Documentation

See [docs/PCBLIB_FORMAT.md](../docs/PCBLIB_FORMAT.md) and [docs/SCHLIB_FORMAT.md](../docs/SCHLIB_FORMAT.md) for complete binary format documentation.
