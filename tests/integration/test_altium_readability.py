#!/usr/bin/env python3
"""Altium-readability oracle for generated libraries (issue #68).

The in-crate Rust tests and ``test_mcp_tools.py`` only prove that *our own*
reader agrees with *our own* writer. They cannot detect a file that Altium
Designer refuses to open. This harness closes that gap by parsing freshly
generated ``.PcbLib`` / ``.SchLib`` files with an INDEPENDENT reader —
``pyaltiumlib`` — and comparing their OLE structure against what a valid
Altium-authored file contains. If pyaltiumlib reads our output cleanly, Altium
Designer very probably can too.

While issue #68 is open the writer is known to emit several malformed records.
Rather than fail CI outright, each check is matched against ``KNOWN_FAILURES``:
a documented-broken check that fails is tolerated, a NEW failure (regression)
fails the run, and a known failure that starts passing is flagged so it can be
promoted. When ``KNOWN_FAILURES`` is empty the harness is fully strict.

Requires ``pyaltiumlib`` and ``olefile`` (``pip install pyaltiumlib olefile``);
skips cleanly if they are absent so environments without them are not broken.

    cargo build
    python3 tests/integration/test_altium_readability.py
"""

import contextlib
import io
import json
import os
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.realpath(__file__)))
from mcp_client import McpTestClient, find_binary  # noqa: E402

# Checks expected to fail until issue #68's writer fixes land. Remove an entry
# as the corresponding fix makes the check pass (the harness will nag if you
# forget). Keep this list as the living scoreboard for #68.
KNOWN_FAILURES = {
    # Both PcbLib and SchLib writer fixes have landed (#68). The harness is now
    # fully strict: any failure here is a regression.
}

WARN_TOKENS = ("Failed", "does not end", "Error", "map value", "Unknown record")


def _read_with_pyaltiumlib(path):
    """Return (component_count, {part: record_count}, [parser warnings])."""
    import pyaltiumlib

    log = io.StringIO()
    with contextlib.redirect_stderr(log), contextlib.redirect_stdout(log):
        lib = pyaltiumlib.read(path)
        count = lib.ComponentCount
        parts = lib.list_parts()
        records = {}
        for name in parts:
            part = lib.get_part(name)
            recs = getattr(part, "Records", None)
            records[name] = len(recs) if recs is not None else None
    warnings = [
        ln.strip()
        for ln in log.getvalue().splitlines()
        if any(tok in ln for tok in WARN_TOKENS)
    ]
    return count, records, warnings


def _ole_streams(path):
    import olefile

    ole = olefile.OleFileIO(path)
    try:
        return {"/".join(s) for s in ole.listdir(streams=True, storages=True)}
    finally:
        ole.close()


def _generate(binary, out_dir):
    """Drive the real server to write a 0402 footprint and a resistor symbol."""
    cfg = os.path.join(out_dir, "config.json")
    with open(cfg, "w", encoding="utf-8") as f:
        json.dump({"allowed_paths": [out_dir], "logging": {"level": "error"}}, f)
    client = McpTestClient(binary, cfg)
    client.start()
    try:
        client.send(
            "initialize",
            {"protocolVersion": "2024-11-05", "capabilities": {},
             "clientInfo": {"name": "oracle", "version": "1.0.0"}},
        )
        client.notify("notifications/initialized")
        pcb = os.path.join(out_dir, "Oracle.PcbLib")
        sch = os.path.join(out_dir, "Oracle.SchLib")
        # Rich footprint: exercises every user-creatable PcbLib primitive
        # (pads, tracks, arcs, text, regions) so the oracle guards them all.
        footprint = {
            "name": "RESC1005X40N", "description": "0402 chip resistor",
            "pads": [
                {"designator": "1", "x": -0.5, "y": 0.0, "width": 0.6, "height": 0.5},
                {"designator": "2", "x": 0.5, "y": 0.0, "width": 0.6, "height": 0.5},
            ],
            "tracks": [{"x1": -1, "y1": 0.5, "x2": 1, "y2": 0.5,
                        "width": 0.1, "layer": "Top Overlay"}],
            "arcs": [{"x": 0, "y": 0, "radius": 0.3, "start_angle": 0,
                      "end_angle": 180, "width": 0.1, "layer": "Top Overlay"}],
            "text": [{"x": -1, "y": 1, "text": "R1", "height": 0.5, "layer": "Top Overlay"}],
            "regions": [{"layer": "Mechanical 1", "vertices": [
                {"x": -1, "y": -1}, {"x": 1, "y": -1}, {"x": 1, "y": 1}, {"x": -1, "y": 1}]}],
        }
        wp = client.call_tool(
            "write_pcblib", {"filepath": pcb, "footprints": [footprint], "append": False})
        # Rich symbol: pins, rectangle, line, text.
        symbol = {
            "name": "Resistor", "description": "Generic resistor",
            "designator_prefix": "R",
            "pins": [
                {"designator": "1", "name": "1", "x": 0, "y": 100, "length": 30,
                 "orientation": "down", "electrical_type": "passive"},
                {"designator": "2", "name": "2", "x": 0, "y": -100, "length": 30,
                 "orientation": "up", "electrical_type": "passive"},
            ],
            "rectangles": [{"x1": -20, "y1": -50, "x2": 20, "y2": 50}],
            "lines": [{"x1": -20, "y1": 0, "x2": 20, "y2": 0}],
            "text": [{"x": -20, "y": 60, "text": "R"}],
        }
        ws = client.call_tool(
            "write_schlib", {"filepath": sch, "symbols": [symbol], "append": False})
        if wp.get("_isError") or ws.get("_isError"):
            raise RuntimeError(f"generation failed: pcb={wp} sch={ws}")
        return pcb, sch
    finally:
        client.stop()


class Scoreboard:
    def __init__(self):
        self.regressions = []   # unexpected failures
        self.fixed = []         # known failures that now pass
        self.passed = 0
        self.known = 0

    def check(self, cid, ok, detail=""):
        known = cid in KNOWN_FAILURES
        if ok and not known:
            self.passed += 1
            print(f"  PASS  {cid}")
        elif ok and known:
            self.fixed.append(cid)
            print(f"  FIXED {cid}  <- remove from KNOWN_FAILURES! {detail}")
        elif not ok and known:
            self.known += 1
            print(f"  known {cid}  (#68) {detail}")
        else:
            self.regressions.append(cid)
            print(f"  FAIL  {cid}  {detail}")

    def summary(self):
        print(
            f"\nReadability: {self.passed} passed, {self.known} known-broken (#68), "
            f"{len(self.fixed)} newly-fixed, {len(self.regressions)} regressions")
        if self.fixed:
            print("  -> promote newly-fixed checks by removing them from KNOWN_FAILURES")
        return 1 if self.regressions else 0


def run(board, kind, path, expected_part, required_streams):
    print(f"\n=== {kind}: {os.path.basename(path)} ===")
    try:
        count, records, warnings = _read_with_pyaltiumlib(path)
    except Exception as e:  # noqa: BLE001
        board.check(f"{kind}:opens", False, f"pyaltiumlib raised {e!r}")
        return
    board.check(f"{kind}:opens", True)
    board.check(f"{kind}:component_count==1", count == 1, f"got {count}")
    board.check(f"{kind}:part_named", expected_part in records, f"parts={list(records)}")
    board.check(f"{kind}:no_parser_warnings", not warnings,
                f"{len(warnings)} warning(s): {warnings[:4]}")
    streams = _ole_streams(path)
    for s in required_streams:
        board.check(f"{kind}:has_{s}", s in streams, "stream absent")


def main():
    try:
        import pyaltiumlib  # noqa: F401
        import olefile  # noqa: F401
    except ImportError as e:
        print(f"SKIP: Altium-readability oracle needs pyaltiumlib + olefile ({e}). "
              "Install with: pip install pyaltiumlib olefile")
        return 0

    binary = find_binary()
    print(f"Using binary: {binary}")
    out = tempfile.mkdtemp(prefix="altium-oracle-")
    pcb, sch = _generate(binary, out)

    board = Scoreboard()
    run(board, "pcblib", pcb, "RESC1005X40N",
        ["FileVersionInfo", "Library/ComponentParamsTOC",
         "Library/LayerKindMapping", "Library/PadViaLibrary"])
    run(board, "schlib", sch, "Resistor", ["Storage"])
    return board.summary()


if __name__ == "__main__":
    sys.exit(main())
