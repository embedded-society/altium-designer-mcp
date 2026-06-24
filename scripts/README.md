# Scripts

On-site developer tooling for working with the Altium binary formats. Everything here is for
**manual, local use only** — none of it is part of the automated test suite, and **none of it
runs in CI**. (CI verifies Altium-readability through the independent `pyaltiumlib` oracle in
[`tests/integration/`](../tests/integration/).)

The folder is organised into three parts:

| Folder | What it is | Needs Altium? |
|--------|------------|---------------|
| [`analyse/`](analyse/) | Passive binary-format dumpers (pure Python + `olefile`) | No |
| [`altium/`](altium/) | On-site automation driven by a real Altium install | **Yes** |
| [`samples/`](samples/) | Altium-authored reference libraries (ground truth for RE) | No |

---

## `analyse/` — binary-format analysis

Dump the OLE structure and decoded primitive data of any `.PcbLib` / `.SchLib`, to help
reverse-engineer the format. Pure Python; the only dependency is `olefile`.

### Prerequisites

Run these from the `scripts/` folder:

```bash
python -m venv .venv
.venv\Scripts\activate          # Windows
# or: source .venv/bin/activate  # Linux/macOS

pip install olefile
```

### Usage

```bash
# Default to the bundled Altium-authored sample in samples/
python analyse/analyse_pcblib.py
python analyse/analyse_schlib.py

# Or point at a specific file
python analyse/analyse_pcblib.py path/to/library.PcbLib
python analyse/analyse_schlib.py path/to/library.SchLib
```

Output: OLE streams and storages, component names and parameters, and decoded primitive data
(pads, tracks, arcs, pins, rectangles, …) with the byte-level detail needed for reverse
engineering.

---

## `altium/` — on-site Altium automation

Automation that drives a **real, locally-installed Altium Designer** to verify our output and to
author golden reference libraries. Because it needs the GUI application and a licence, it
**cannot run in CI** — it is strictly an on-site developer aid.

See [`altium/README.md`](altium/README.md) for the planned tooling and prerequisites.

---

## `samples/` — reference libraries

| File | Description |
|------|-------------|
| `sample.PcbLib` | Altium-authored PCB footprint library (generic chip resistor footprints) |
| `sample.SchLib` | Altium-authored schematic symbol library |

These were created by Altium itself, so they are authoritative ground truth for reverse
engineering and for byte-diffing against our own writer's output. The `analyse/` scripts default
to them when run with no argument.

---

## Note

The automated tests in [`tests/`](../tests/) generate their own data programmatically and do
**not** depend on anything in this folder, so CI runs without Altium or these samples.
