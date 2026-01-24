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
# Analyze PcbLib sample file
python analyze_pcblib.py

# Analyze a specific PcbLib file
python analyze_pcblib.py /path/to/library.PcbLib

# Analyze SchLib sample file
python analyze_schlib.py

# Analyze a specific SchLib file
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

## Binary Format Reference

### PcbLib Data Stream Format

```text
[name_block_len:4][str_len:1][name:str_len]  // Component name block
[record_type:1][blocks...]                   // First primitive
[record_type:1][blocks...]                   // Second primitive
...
[0x00]                                       // End marker
```

### Record Types

| ID | Type | Description |
|----|------|-------------|
| 0x01 | Arc | Arc or circle |
| 0x02 | Pad | SMD or through-hole pad |
| 0x03 | Via | Via (similar to pad) |
| 0x04 | Track | Line segment |
| 0x05 | Text | Text string |
| 0x06 | Fill | Filled rectangle |
| 0x0B | Region | Filled polygon |
| 0x0C | ComponentBody | 3D body reference |

### Coordinate System

- Altium uses internal units: 10000 units = 1 mil = 0.0254 mm
- Conversion: `mm = internal_units / 10000.0 * 0.0254`
- Reverse: `internal_units = mm / 0.0254 * 10000.0`

### Layer IDs

| ID | Layer |
|----|-------|
| 1 | Top Layer |
| 32 | Bottom Layer |
| 33 | Top Overlay |
| 34 | Bottom Overlay |
| 35 | Top Paste |
| 36 | Bottom Paste |
| 37 | Top Solder |
| 38 | Bottom Solder |
| 56 | Keep-Out Layer |
| 57 | Mechanical 1 |
| 74 | Multi-Layer |

### Component Layer Pairs (from sample library)

These are mechanical layers configured as component layer pairs:

| ID | Mechanical | Layer Type |
|----|------------|------------|
| 58 | M2 | Top Assembly |
| 59 | M3 | Bottom Assembly |
| 60 | M4 | Top Courtyard |
| 61 | M5 | Bottom Courtyard |
| 62 | M6 | Top 3D Body |
| 63 | M7 | Bottom 3D Body |

**Note:** AI assistants should prefer these dedicated component layer pairs over generic mechanical layers when creating footprints.
