#!/usr/bin/env python3
"""End-to-end integration tests for altium-designer-mcp.

Spawns the real server binary and drives it over stdin/stdout with JSON-RPC,
exercising tool dispatch, the request/response envelope, a create -> read
round trip, and the error paths — none of which the in-crate Rust tests cover
(they call the private handlers directly).

Run after building the binary:

    cargo build
    python3 tests/integration/test_mcp_tools.py
"""

import json
import os
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.realpath(__file__)))
from mcp_client import McpTestClient, TestRunner, find_binary  # noqa: E402

# The UTF-8 round-trip test prints non-Latin values (Ω, Cyrillic, CJK); force
# UTF-8 stdout so a legacy-code-page Windows console (e.g. cp1250) does not raise
# UnicodeEncodeError while reporting results. No-op where stdout is already UTF-8.
for _stream in (sys.stdout, sys.stderr):
    try:
        _stream.reconfigure(encoding="utf-8")
    except (AttributeError, ValueError):
        # Stream is already UTF-8, or cannot be reconfigured (redirected / older
        # Python): safe to ignore — this only affects console reporting of the
        # non-Latin PASS lines, never the test results themselves.
        pass


def test_initialise(client, runner):
    print("\n=== Test: initialise ===")
    response = client.send(
        "initialize",
        {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "integration-test", "version": "1.0.0"},
        },
    )
    runner.check("error" not in response, "initialise — no error")
    result = response.get("result", {})
    runner.check(
        result.get("protocolVersion") == "2024-11-05",
        "protocol version",
        actual=result.get("protocolVersion"),
        expected="2024-11-05",
    )
    runner.check(
        result.get("serverInfo", {}).get("name") == "altium-designer-mcp",
        "server name",
        actual=result.get("serverInfo", {}).get("name"),
        expected="altium-designer-mcp",
    )
    client.notify("notifications/initialized")


def test_tools_list(client, runner):
    print("\n=== Test: tools/list ===")
    response = client.send("tools/list")
    runner.check("error" not in response, "tools/list — no error")
    names = {t["name"] for t in response.get("result", {}).get("tools", [])}
    for expected in ("read_pcblib", "write_pcblib", "list_components", "get_component"):
        runner.check(expected in names, f"tools/list contains {expected}")


def test_write_read_roundtrip(client, runner, lib_path):
    print("\n=== Test: write -> read round trip ===")
    footprint = {
        "name": "RESC0402",
        "description": "0402 chip resistor",
        "pads": [
            {"designator": "1", "x": -0.5, "y": 0.0, "width": 0.6, "height": 0.5},
            {"designator": "2", "x": 0.5, "y": 0.0, "width": 0.6, "height": 0.5},
        ],
    }
    write = client.call_tool(
        "write_pcblib",
        {"filepath": lib_path, "footprints": [footprint], "append": False},
    )
    runner.check(not write.get("_isError"), "write_pcblib succeeded", actual=write)

    read = client.call_tool("read_pcblib", {"filepath": lib_path})
    runner.check(not read.get("_isError"), "read_pcblib succeeded")
    runner.check("RESC0402" in json.dumps(read), "read-back contains RESC0402")

    listing = client.call_tool("list_components", {"filepath": lib_path})
    runner.check(not listing.get("_isError"), "list_components succeeded")
    runner.check(
        listing.get("total_count", 0) >= 1,
        "list_components total_count >= 1",
        actual=listing.get("total_count"),
    )

    got = client.call_tool(
        "get_component", {"filepath": lib_path, "component_name": "RESC0402"}
    )
    runner.check(not got.get("_isError"), "get_component succeeded")
    runner.check("RESC0402" in json.dumps(got), "get_component returns RESC0402")


def test_read_pcblib_exposes_vias_fills(client, runner, sample_path):
    print("\n=== Test: read_pcblib exposes vias and fills ===")
    # Regression guard: the read_pcblib / get_component / list_components JSON
    # builders once omitted the `vias` and `fills` collections even though the
    # reader parsed them. Drive the real tool against the committed sample
    # (the MCP write path cannot author vias/fills) and assert both collections
    # survive into the JSON output for the footprints that contain them.
    read = client.call_tool("read_pcblib", {"filepath": sample_path})
    runner.check(not read.get("_isError"), "read_pcblib succeeded", actual=read)

    footprints = {fp.get("name"): fp for fp in read.get("footprints", [])}

    vias_fp = footprints.get("VIAS", {})
    runner.check("vias" in vias_fp, "VIAS footprint has 'vias' field")
    runner.check(
        len(vias_fp.get("vias", [])) == 2,
        "VIAS footprint exposes 2 vias",
        actual=len(vias_fp.get("vias", [])),
        expected=2,
    )

    fills_fp = footprints.get("FILLS", {})
    runner.check("fills" in fills_fp, "FILLS footprint has 'fills' field")
    runner.check(
        len(fills_fp.get("fills", [])) == 2,
        "FILLS footprint exposes 2 fills",
        actual=len(fills_fp.get("fills", [])),
        expected=2,
    )

    # The fields must be present (as empty arrays) on footprints without them too.
    shapes_fp = footprints.get("PAD_SHAPES", {})
    runner.check("vias" in shapes_fp, "PAD_SHAPES footprint has 'vias' field")
    runner.check("fills" in shapes_fp, "PAD_SHAPES footprint has 'fills' field")

    # get_component must carry the same collections through. It returns the
    # footprint under a singular `component` key (serialised straight from the
    # Footprint struct), so guard that path too.
    got = client.call_tool(
        "get_component", {"filepath": sample_path, "component_name": "VIAS"}
    )
    runner.check(not got.get("_isError"), "get_component(VIAS) succeeded", actual=got)
    got_component = got.get("component", {})
    runner.check(
        len(got_component.get("vias", [])) == 2,
        "get_component(VIAS) exposes 2 vias",
        actual=len(got_component.get("vias", [])),
        expected=2,
    )


def test_unknown_tool(client, runner):
    print("\n=== Test: unknown tool ===")
    response = client.send("tools/call", {"name": "nope_not_a_tool", "arguments": {}})
    runner.check("error" not in response, "no protocol error")
    result = response.get("result", {})
    runner.check(result.get("isError") is True, "isError true", actual=result.get("isError"))
    text = result.get("content", [{}])[0].get("text", "")
    runner.check("Unknown tool" in text or "unknown" in text.lower(), "mentions unknown tool")


def test_unknown_method(client, runner):
    print("\n=== Test: unknown method ===")
    response = client.send("nonexistent/method")
    runner.check("error" in response, "has error field")
    runner.check(
        response.get("error", {}).get("code") == -32601,
        "method not found code -32601",
        actual=response.get("error", {}).get("code"),
        expected=-32601,
    )


def test_missing_params(client, runner):
    print("\n=== Test: missing required params ===")
    response = client.send("tools/call", {"name": "write_pcblib", "arguments": {}})
    has_error = "error" in response
    has_tool_error = not has_error and response.get("result", {}).get("isError", False)
    runner.check(has_error or has_tool_error, "error for missing args")


def test_path_outside_allowed(client, runner, outside_path):
    print("\n=== Test: path outside allowed_paths ===")
    footprint = {"name": "X", "pads": [{"designator": "1", "x": 0.0, "y": 0.0, "width": 0.5, "height": 0.5}]}
    result = client.call_tool(
        "write_pcblib",
        {"filepath": outside_path, "footprints": [footprint], "append": False},
    )
    runner.check(result.get("_isError") is True, "write outside allowed rejected", actual=result)
    text = result.get("_error", "")
    runner.check("Access denied" in text, "denial message", actual=text)


def test_ping(client, runner):
    print("\n=== Test: ping ===")
    response = client.send("ping")
    runner.check("error" not in response, "ping — no error")
    runner.check("result" in response, "ping has result")


def test_read_schlib_exposes_round_rects_polygons(client, runner, schlib_sample_path):
    print("\n=== Test: read_schlib exposes round_rects and polygons ===")
    # Regression guard mirroring the pcblib vias/fills one: the read_schlib /
    # get_component / list_components symbol JSON builders omitted the
    # `round_rects` and `polygons` collections even though the reader parsed
    # them. Drive the real tool against the committed SchLib sample (the MCP
    # write path cannot author them) and assert both survive into the output.
    read = client.call_tool("read_schlib", {"filepath": schlib_sample_path})
    runner.check(not read.get("_isError"), "read_schlib succeeded", actual=read)

    symbols = {s.get("name"): s for s in read.get("symbols", [])}

    rr = symbols.get("ROUNDRECTS", {})
    runner.check("round_rects" in rr, "ROUNDRECTS symbol has 'round_rects' field")
    runner.check(
        len(rr.get("round_rects", [])) == 1,
        "ROUNDRECTS exposes 1 round_rect",
        actual=len(rr.get("round_rects", [])),
        expected=1,
    )

    pg = symbols.get("POLYGONS", {})
    runner.check("polygons" in pg, "POLYGONS symbol has 'polygons' field")
    runner.check(
        len(pg.get("polygons", [])) == 2,
        "POLYGONS exposes 2 polygons",
        actual=len(pg.get("polygons", [])),
        expected=2,
    )

    # Empty-case guard: a non-shape symbol still carries both (empty) fields,
    # so they can't silently vanish from the schema.
    pins = symbols.get("PINS_ETYPE", {})
    runner.check("round_rects" in pins, "PINS_ETYPE symbol has 'round_rects' field")
    runner.check("polygons" in pins, "PINS_ETYPE symbol has 'polygons' field")


def test_write_schlib_shapes(client, runner, schlib_path):
    print("\n=== Test: write_schlib shapes round trip ===")
    # Coverage PR-1: every SHAPE primitive the read tool round-trips must be
    # authorable via write_schlib. Write a symbol with a round_rect, polygon,
    # ellipse, arc and label, then read it back and assert each survived.
    symbol = {
        "name": "SHAPES",
        "pins": [
            {
                "designator": "1",
                "name": "P1",
                "x": -50,
                "y": 0,
                "length": 30,
                "orientation": "left",
            }
        ],
        "round_rects": [
            {
                "x1": 0,
                "y1": 0,
                "x2": 40,
                "y2": 30,
                "corner_x_radius": 5,
                "corner_y_radius": 7,
                "fill_color": 0x112233,
            }
        ],
        "polygons": [
            {
                "points": [
                    {"x": 0, "y": 0},
                    {"x": 20, "y": 0},
                    {"x": 10, "y": 20},
                ],
                "fill_color": 0x445566,
                "line_style": 2,
                "transparent": True,
                "is_not_accessible": False,
            }
        ],
        "ellipses": [
            {"x": 60, "y": 60, "radius_x": 15, "radius_y": 10, "fill_color": 0x778899}
        ],
        "arcs": [
            {"x": 80, "y": 80, "radius": 25, "start_angle": 30, "end_angle": 270}
        ],
        "labels": [{"x": 5, "y": 45, "text": "HELLO"}],
    }

    write = client.call_tool(
        "write_schlib",
        {"filepath": schlib_path, "symbols": [symbol], "append": False},
    )
    runner.check(not write.get("_isError"), "write_schlib succeeded", actual=write)

    read = client.call_tool("read_schlib", {"filepath": schlib_path})
    runner.check(not read.get("_isError"), "read_schlib succeeded", actual=read)

    symbols = {s.get("name"): s for s in read.get("symbols", [])}
    sym = symbols.get("SHAPES", {})
    runner.check(bool(sym), "SHAPES symbol present", actual=list(symbols))

    # round_rect: count + corner radii + geometry survived
    round_rects = sym.get("round_rects", [])
    runner.check(len(round_rects) == 1, "1 round_rect survived", actual=len(round_rects))
    if round_rects:
        rr = round_rects[0]
        runner.check(rr.get("corner_x_radius") == 5, "round_rect corner_x_radius", actual=rr.get("corner_x_radius"), expected=5)
        runner.check(rr.get("corner_y_radius") == 7, "round_rect corner_y_radius", actual=rr.get("corner_y_radius"), expected=7)
        runner.check(rr.get("x2") == 40 and rr.get("y2") == 30, "round_rect geometry", actual=rr)

    # polygon: count + vertex count
    polygons = sym.get("polygons", [])
    runner.check(len(polygons) == 1, "1 polygon survived", actual=len(polygons))
    if polygons:
        pg0 = polygons[0]
        pts = pg0.get("points", [])
        runner.check(len(pts) == 3, "polygon has 3 vertices", actual=len(pts))
        runner.check(pg0.get("line_style") == 2, "polygon line_style round-trips", actual=pg0.get("line_style"), expected=2)
        runner.check(pg0.get("transparent") is True, "polygon transparent round-trips", actual=pg0.get("transparent"))
        runner.check(pg0.get("is_not_accessible") is False, "polygon is_not_accessible=false round-trips", actual=pg0.get("is_not_accessible"))

    # ellipse: count + radii
    ellipses = sym.get("ellipses", [])
    runner.check(len(ellipses) == 1, "1 ellipse survived", actual=len(ellipses))
    if ellipses:
        el = ellipses[0]
        runner.check(el.get("radius_x") == 15 and el.get("radius_y") == 10, "ellipse radii", actual=el)

    # arc: count + angle range
    arcs = sym.get("arcs", [])
    runner.check(len(arcs) == 1, "1 arc survived", actual=len(arcs))
    if arcs:
        ar = arcs[0]
        runner.check(ar.get("radius") == 25, "arc radius", actual=ar.get("radius"), expected=25)
        runner.check(ar.get("start_angle") == 30 and ar.get("end_angle") == 270, "arc angles", actual=ar)

    # label: count + text
    labels = sym.get("labels", [])
    runner.check(len(labels) == 1, "1 label survived", actual=len(labels))
    if labels:
        runner.check(labels[0].get("text") == "HELLO", "label text", actual=labels[0].get("text"), expected="HELLO")


def test_write_pcblib_auto_3d_body_opt_in(client, runner, lib_path):
    print("\n=== Test: write_pcblib auto_3d_body is opt-in ===")
    # A footprint with a pad but no 3D body. By default the tool must NOT synthesise
    # a body (geometry the caller didn't request); the `bodies` echo reports 'none'.
    # Passing auto_3d_body:true opts into a flagged 1.0mm placeholder.
    fp = {
        "name": "NOBODY",
        "pads": [{"designator": "1", "x": 0, "y": 0, "width": 1.0, "height": 1.0}],
    }

    default = client.call_tool("write_pcblib", {"filepath": lib_path, "footprints": [fp], "append": False})
    runner.check(not default.get("_isError"), "write_pcblib (default) succeeded", actual=default)
    d_bodies = {b.get("name"): b for b in default.get("bodies", [])}
    runner.check(
        d_bodies.get("NOBODY", {}).get("source") == "none",
        "default: no auto 3D body (source 'none')",
        actual=d_bodies.get("NOBODY"),
    )

    optin = client.call_tool(
        "write_pcblib",
        {"filepath": lib_path, "footprints": [fp], "append": False, "auto_3d_body": True},
    )
    runner.check(not optin.get("_isError"), "write_pcblib (auto_3d_body) succeeded", actual=optin)
    o_bodies = {b.get("name"): b for b in optin.get("bodies", [])}
    runner.check(
        o_bodies.get("NOBODY", {}).get("source") == "auto-extruded",
        "auto_3d_body:true adds an extruded body",
        actual=o_bodies.get("NOBODY"),
    )
    runner.check(
        o_bodies.get("NOBODY", {}).get("assumed_height") is True,
        "auto body is flagged assumed_height",
    )


def test_write_pcblib_via_fill(client, runner, lib_path):
    print("\n=== Test: write_pcblib via + fill round trip ===")
    # Coverage PR-2/PR-3: vias and fills the read tool round-trips must be
    # authorable via write_pcblib. Write a footprint with one via and one fill,
    # read it back, and assert each survived with its key field values.
    footprint = {
        "name": "VIAFILL",
        "pads": [
            {"designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0},
        ],
        "vias": [
            {
                "x": 1.5,
                "y": 2.5,
                "diameter": 0.6,
                "hole_size": 0.3,
                "from_layer": "Top Layer",
                "to_layer": "Bottom Layer",
            }
        ],
        "fills": [
            {
                "x1": -1.0,
                "y1": -2.0,
                "x2": 3.0,
                "y2": 4.0,
                "layer": "Top Layer",
                "rotation": 45.0,
            }
        ],
    }
    write = client.call_tool(
        "write_pcblib",
        {"filepath": lib_path, "footprints": [footprint], "append": False},
    )
    runner.check(not write.get("_isError"), "write_pcblib (via+fill) succeeded", actual=write)

    read = client.call_tool("read_pcblib", {"filepath": lib_path})
    runner.check(not read.get("_isError"), "read_pcblib succeeded", actual=read)
    footprints = {fp.get("name"): fp for fp in read.get("footprints", [])}
    fp = footprints.get("VIAFILL", {})

    # Tolerance: Altium stores coordinates as fixed-point internal units, so a
    # round-trip carries sub-micron quantisation error (~1e-6 mm). 1e-4 mm
    # (0.1 micron) is well below any meaningful PCB tolerance.
    tol = 1e-4

    vias = fp.get("vias", [])
    runner.check(len(vias) == 1, "1 via survived", actual=len(vias))
    if vias:
        v = vias[0]
        runner.check(
            abs(v.get("diameter", 0) - 0.6) < tol,
            "via diameter",
            actual=v.get("diameter"),
            expected=0.6,
        )
        runner.check(
            abs(v.get("hole_size", 0) - 0.3) < tol,
            "via hole_size",
            actual=v.get("hole_size"),
            expected=0.3,
        )

    fills = fp.get("fills", [])
    runner.check(len(fills) == 1, "1 fill survived", actual=len(fills))
    if fills:
        fl = fills[0]
        runner.check(
            abs(fl.get("x1", 0) - (-1.0)) < tol
            and abs(fl.get("y1", 0) - (-2.0)) < tol
            and abs(fl.get("x2", 0) - 3.0) < tol
            and abs(fl.get("y2", 0) - 4.0) < tol,
            "fill corners",
            actual=(fl.get("x1"), fl.get("y1"), fl.get("x2"), fl.get("y2")),
            expected=(-1.0, -2.0, 3.0, 4.0),
        )
        runner.check(
            abs(fl.get("rotation", 0) - 45.0) < tol,
            "fill rotation",
            actual=fl.get("rotation"),
            expected=45.0,
        )


def test_write_pcblib_flags_mask_keepout(client, runner, lib_path):
    print("\n=== Test: write_pcblib flags + solder_mask + keepout round trip ===")
    # Coverage PR-4: the three EE-meaningful fields — flags, solder_mask_expansion,
    # and keepout_restrictions — must be authorable via write_pcblib for every 2D
    # primitive that carries them, and round-trip through read_pcblib. `flags` is
    # serialised by read_pcblib as the bitflags name string (PcbFlags' serde impl),
    # e.g. "LOCKED" or "LOCKED | KEEPOUT" — the write path accepts that same shape.
    LOCKED = "LOCKED"
    KEEPOUT = "KEEPOUT"
    footprint = {
        "name": "FLAGS_RT",
        "pads": [
            {
                "designator": "1",
                "x": 0.0,
                "y": 0.0,
                "width": 1.0,
                "height": 1.0,
                "flags": LOCKED,
                "solder_mask_expansion": 0.05,
                "solder_mask_expansion_mode": "manual",
            }
        ],
        "tracks": [
            {
                "x1": -1.0,
                "y1": 0.0,
                "x2": 1.0,
                "y2": 0.0,
                "width": 0.15,
                "layer": "Top Overlay",
                "flags": LOCKED,
                "solder_mask_expansion": 0.1,
                "keepout_restrictions": 3,
            }
        ],
        "arcs": [
            {
                "x": 0.0,
                "y": 2.0,
                "radius": 0.5,
                "start_angle": 0.0,
                "end_angle": 90.0,
                "width": 0.15,
                "layer": "Top Overlay",
                "flags": LOCKED,
                "solder_mask_expansion": 0.2,
                "keepout_restrictions": 5,
            }
        ],
        "regions": [
            {
                "vertices": [
                    {"x": 0.0, "y": 0.0},
                    {"x": 1.0, "y": 0.0},
                    {"x": 0.0, "y": 1.0},
                ],
                "layer": "Top Courtyard",
                "flags": KEEPOUT,
            }
        ],
        "fills": [
            {
                "x1": -1.0,
                "y1": -1.0,
                "x2": 1.0,
                "y2": 1.0,
                "layer": "Top Layer",
                "flags": LOCKED,
                "solder_mask_expansion": 0.05,
                "keepout_restrictions": 2,
            }
        ],
        "text": [
            {
                "x": 0.0,
                "y": 3.0,
                "text": "REF",
                "height": 0.5,
                "layer": "Top Overlay",
                "flags": LOCKED,
            }
        ],
    }
    write = client.call_tool(
        "write_pcblib",
        {"filepath": lib_path, "footprints": [footprint], "append": False},
    )
    runner.check(not write.get("_isError"), "write_pcblib (flags) succeeded", actual=write)

    read = client.call_tool("read_pcblib", {"filepath": lib_path})
    runner.check(not read.get("_isError"), "read_pcblib succeeded", actual=read)
    footprints = {fp.get("name"): fp for fp in read.get("footprints", [])}
    fp = footprints.get("FLAGS_RT", {})
    runner.check(bool(fp), "FLAGS_RT footprint present", actual=list(footprints))

    # Tolerance: Altium stores mask values as fixed-point internal units, so a
    # round-trip carries sub-micron quantisation error. 1e-4 mm is well below any
    # meaningful PCB tolerance.
    tol = 1e-4

    def has_flag(value, name):
        # read_pcblib serialises PcbFlags as a "|"-joined name string, e.g.
        # "LOCKED" or "LOCKED | KEEPOUT". Assert set membership of the bit we set.
        parts = {p.strip() for p in str(value or "").split("|")}
        return name in parts

    pads = fp.get("pads", [])
    runner.check(len(pads) == 1, "1 pad survived", actual=len(pads))
    if pads:
        p = pads[0]
        runner.check(has_flag(p.get("flags"), LOCKED), "pad flags LOCKED", actual=p.get("flags"))
        runner.check(
            abs(p.get("solder_mask_expansion", 0) - 0.05) < tol,
            "pad solder_mask_expansion",
            actual=p.get("solder_mask_expansion"),
            expected=0.05,
        )
        runner.check(
            p.get("solder_mask_expansion_mode") == "manual",
            "pad solder_mask_expansion_mode",
            actual=p.get("solder_mask_expansion_mode"),
            expected="manual",
        )

    tracks = fp.get("tracks", [])
    runner.check(len(tracks) == 1, "1 track survived", actual=len(tracks))
    if tracks:
        t = tracks[0]
        runner.check(has_flag(t.get("flags"), LOCKED), "track flags LOCKED", actual=t.get("flags"))
        runner.check(
            abs(t.get("solder_mask_expansion", 0) - 0.1) < tol,
            "track solder_mask_expansion",
            actual=t.get("solder_mask_expansion"),
            expected=0.1,
        )
        runner.check(
            t.get("keepout_restrictions") == 3,
            "track keepout_restrictions",
            actual=t.get("keepout_restrictions"),
            expected=3,
        )

    arcs = fp.get("arcs", [])
    runner.check(len(arcs) == 1, "1 arc survived", actual=len(arcs))
    if arcs:
        a = arcs[0]
        runner.check(has_flag(a.get("flags"), LOCKED), "arc flags LOCKED", actual=a.get("flags"))
        runner.check(
            abs(a.get("solder_mask_expansion", 0) - 0.2) < tol,
            "arc solder_mask_expansion",
            actual=a.get("solder_mask_expansion"),
            expected=0.2,
        )
        runner.check(
            a.get("keepout_restrictions") == 5,
            "arc keepout_restrictions",
            actual=a.get("keepout_restrictions"),
            expected=5,
        )

    regions = fp.get("regions", [])
    runner.check(len(regions) == 1, "1 region survived", actual=len(regions))
    if regions:
        rg = regions[0]
        runner.check(has_flag(rg.get("flags"), KEEPOUT), "region flags KEEPOUT", actual=rg.get("flags"))

    fills = fp.get("fills", [])
    runner.check(len(fills) == 1, "1 fill survived", actual=len(fills))
    if fills:
        fl = fills[0]
        runner.check(has_flag(fl.get("flags"), LOCKED), "fill flags LOCKED", actual=fl.get("flags"))
        runner.check(
            abs(fl.get("solder_mask_expansion", 0) - 0.05) < tol,
            "fill solder_mask_expansion",
            actual=fl.get("solder_mask_expansion"),
            expected=0.05,
        )
        runner.check(
            fl.get("keepout_restrictions") == 2,
            "fill keepout_restrictions",
            actual=fl.get("keepout_restrictions"),
            expected=2,
        )

    # The writer may inject an auto designator/comment text, so locate the
    # one we authored by content rather than asserting an exact count.
    texts = fp.get("text", [])
    ref_text = next((t for t in texts if t.get("text") == "REF"), None)
    runner.check(ref_text is not None, "REF text survived", actual=[t.get("text") for t in texts])
    if ref_text:
        runner.check(
            has_flag(ref_text.get("flags"), LOCKED),
            "text flags LOCKED",
            actual=ref_text.get("flags"),
        )


def test_write_pcblib_pad_thermal_relief(client, runner, lib_path):
    print("\n=== Test: write_pcblib pad thermal-relief / power-plane round trip ===")
    # Coverage PR-6: the six pad thermal-relief / power-plane connection fields
    # must be authorable via write_pcblib and round-trip through read_pcblib.
    footprint = {
        "name": "PAD_RELIEF_RT",
        "pads": [
            {
                "designator": "1",
                "x": 0.0,
                "y": 0.0,
                "width": 1.6,
                "height": 1.6,
                "hole_size": 0.8,
                "layer": "Multi-Layer",
                "power_plane_connect_style": "direct",
                "relief_conductor_width": 0.3,
                "relief_entries": 2,
                "relief_air_gap": 0.2,
                "power_plane_relief_expansion": 0.6,
                "power_plane_clearance": 0.7,
            }
        ],
    }
    write = client.call_tool(
        "write_pcblib",
        {"filepath": lib_path, "footprints": [footprint], "append": False},
    )
    runner.check(
        not write.get("_isError"), "write_pcblib (pad relief) succeeded", actual=write
    )

    read = client.call_tool("read_pcblib", {"filepath": lib_path})
    runner.check(not read.get("_isError"), "read_pcblib succeeded", actual=read)
    footprints = {fp.get("name"): fp for fp in read.get("footprints", [])}
    fp = footprints.get("PAD_RELIEF_RT", {})
    runner.check(bool(fp), "PAD_RELIEF_RT footprint present", actual=list(footprints))

    # Coord fields carry sub-micron fixed-point quantisation; 1e-4 mm is well
    # below any meaningful PCB tolerance.
    tol = 1e-4
    pads = fp.get("pads", [])
    runner.check(len(pads) == 1, "1 pad survived", actual=len(pads))
    if pads:
        p = pads[0]
        runner.check(
            p.get("power_plane_connect_style") == "direct",
            "pad power_plane_connect_style",
            actual=p.get("power_plane_connect_style"),
            expected="direct",
        )
        runner.check(
            abs(p.get("relief_conductor_width", 0) - 0.3) < tol,
            "pad relief_conductor_width",
            actual=p.get("relief_conductor_width"),
            expected=0.3,
        )
        runner.check(
            p.get("relief_entries") == 2,
            "pad relief_entries",
            actual=p.get("relief_entries"),
            expected=2,
        )
        runner.check(
            abs(p.get("relief_air_gap", 0) - 0.2) < tol,
            "pad relief_air_gap",
            actual=p.get("relief_air_gap"),
            expected=0.2,
        )
        runner.check(
            abs(p.get("power_plane_relief_expansion", 0) - 0.6) < tol,
            "pad power_plane_relief_expansion",
            actual=p.get("power_plane_relief_expansion"),
            expected=0.6,
        )
        runner.check(
            abs(p.get("power_plane_clearance", 0) - 0.7) < tol,
            "pad power_plane_clearance",
            actual=p.get("power_plane_clearance"),
            expected=0.7,
        )


def test_write_pcblib_via_thermal_power_plane(client, runner, lib_path):
    print("\n=== Test: write_pcblib via thermal / power-plane / tenting round trip ===")
    # Coverage PR-7: the via flag word (tenting/keepout/locked), power-plane
    # connection style/expansion/clearance, paste-mask expansion and net index
    # must be authorable via write_pcblib and round-trip through read_pcblib.
    footprint = {
        "name": "VIA_RELIEF_RT",
        "vias": [
            {
                "x": 1.0,
                "y": 2.0,
                "diameter": 0.8,
                "hole_size": 0.4,
                "from_layer": "Top Layer",
                "to_layer": "Bottom Layer",
                "power_plane_connect_style": "direct",
                "power_plane_relief_expansion": 0.6,
                "power_plane_clearance": 0.7,
                "paste_mask_expansion": 0.05,
                "net_index": 42,
                "flags": "TENTING_TOP | TENTING_BOTTOM | KEEPOUT | LOCKED",
            }
        ],
    }
    write = client.call_tool(
        "write_pcblib",
        {"filepath": lib_path, "footprints": [footprint], "append": False},
    )
    runner.check(
        not write.get("_isError"), "write_pcblib (via relief) succeeded", actual=write
    )

    read = client.call_tool("read_pcblib", {"filepath": lib_path})
    runner.check(not read.get("_isError"), "read_pcblib succeeded", actual=read)
    footprints = {fp.get("name"): fp for fp in read.get("footprints", [])}
    fp = footprints.get("VIA_RELIEF_RT", {})
    runner.check(bool(fp), "VIA_RELIEF_RT footprint present", actual=list(footprints))

    # Coord fields carry sub-micron fixed-point quantisation; 1e-4 mm is well
    # below any meaningful PCB tolerance.
    tol = 1e-4
    vias = fp.get("vias", [])
    runner.check(len(vias) == 1, "1 via survived", actual=len(vias))
    if vias:
        v = vias[0]
        runner.check(
            v.get("power_plane_connect_style") == "direct",
            "via power_plane_connect_style",
            actual=v.get("power_plane_connect_style"),
            expected="direct",
        )
        runner.check(
            abs(v.get("power_plane_relief_expansion", 0) - 0.6) < tol,
            "via power_plane_relief_expansion",
            actual=v.get("power_plane_relief_expansion"),
            expected=0.6,
        )
        runner.check(
            abs(v.get("power_plane_clearance", 0) - 0.7) < tol,
            "via power_plane_clearance",
            actual=v.get("power_plane_clearance"),
            expected=0.7,
        )
        runner.check(
            abs(v.get("paste_mask_expansion", 0) - 0.05) < tol,
            "via paste_mask_expansion",
            actual=v.get("paste_mask_expansion"),
            expected=0.05,
        )
        runner.check(
            v.get("net_index") == 42,
            "via net_index",
            actual=v.get("net_index"),
            expected=42,
        )
        # read_pcblib serialises PcbFlags as the bitflags name string; order is
        # canonical (LOCKED before KEEPOUT before tenting). Assert each bit is set.
        flags = v.get("flags", "")
        runner.check(
            all(f in flags for f in ("LOCKED", "KEEPOUT", "TENTING_TOP", "TENTING_BOTTOM")),
            "via flags (tenting/keepout/locked)",
            actual=flags,
            expected="LOCKED | KEEPOUT | TENTING_TOP | TENTING_BOTTOM",
        )


def test_write_pcblib_pad_slot_hole(client, runner, lib_path):
    print("\n=== Test: write_pcblib pad slot-hole / drill tolerances round trip ===")
    # Coverage PR-8: a slot hole (length + rotation) and drill tolerances must be
    # authorable via write_pcblib and round-trip through read_pcblib.
    footprint = {
        "name": "PAD_SLOT_RT",
        "pads": [
            {
                "designator": "1",
                "x": 0.0,
                "y": 0.0,
                "width": 2.0,
                "height": 1.2,
                "hole_size": 0.8,
                "layer": "Multi-Layer",
                "hole_shape": "slot",
                "hole_slot_length": 1.5,
                "hole_rotation": 45.0,
                "hole_positive_tolerance": 0.05,
                "hole_negative_tolerance": 0.02,
            }
        ],
    }
    write = client.call_tool(
        "write_pcblib",
        {"filepath": lib_path, "footprints": [footprint], "append": False},
    )
    runner.check(
        not write.get("_isError"), "write_pcblib (pad slot) succeeded", actual=write
    )

    read = client.call_tool("read_pcblib", {"filepath": lib_path})
    runner.check(not read.get("_isError"), "read_pcblib succeeded", actual=read)
    footprints = {fp.get("name"): fp for fp in read.get("footprints", [])}
    fp = footprints.get("PAD_SLOT_RT", {})
    runner.check(bool(fp), "PAD_SLOT_RT footprint present", actual=list(footprints))

    # Coord fields carry sub-micron fixed-point quantisation; 1e-4 mm is well
    # below any meaningful PCB tolerance.
    tol = 1e-4
    pads = fp.get("pads", [])
    runner.check(len(pads) == 1, "1 pad survived", actual=len(pads))
    if pads:
        p = pads[0]
        runner.check(
            p.get("hole_shape") == "slot",
            "pad hole_shape",
            actual=p.get("hole_shape"),
            expected="slot",
        )
        runner.check(
            abs(p.get("hole_slot_length", 0) - 1.5) < tol,
            "pad hole_slot_length",
            actual=p.get("hole_slot_length"),
            expected=1.5,
        )
        runner.check(
            abs(p.get("hole_rotation", 0) - 45.0) < tol,
            "pad hole_rotation",
            actual=p.get("hole_rotation"),
            expected=45.0,
        )
        runner.check(
            abs(p.get("hole_positive_tolerance", 0) - 0.05) < tol,
            "pad hole_positive_tolerance",
            actual=p.get("hole_positive_tolerance"),
            expected=0.05,
        )
        runner.check(
            abs(p.get("hole_negative_tolerance", 0) - 0.02) < tol,
            "pad hole_negative_tolerance",
            actual=p.get("hole_negative_tolerance"),
            expected=0.02,
        )


def test_write_pcblib_via_slot_tolerances(client, runner, lib_path):
    print("\n=== Test: write_pcblib via drill tolerances round trip ===")
    # Coverage PR-8: via drill tolerances must round-trip; vias carry no slot
    # geometry.
    footprint = {
        "name": "VIA_TOL_RT",
        "vias": [
            {
                "x": 1.0,
                "y": 2.0,
                "diameter": 0.8,
                "hole_size": 0.4,
                "hole_positive_tolerance": 0.05,
                "hole_negative_tolerance": 0.02,
            }
        ],
    }
    write = client.call_tool(
        "write_pcblib",
        {"filepath": lib_path, "footprints": [footprint], "append": False},
    )
    runner.check(
        not write.get("_isError"), "write_pcblib (via tol) succeeded", actual=write
    )

    read = client.call_tool("read_pcblib", {"filepath": lib_path})
    runner.check(not read.get("_isError"), "read_pcblib succeeded", actual=read)
    footprints = {fp.get("name"): fp for fp in read.get("footprints", [])}
    fp = footprints.get("VIA_TOL_RT", {})
    runner.check(bool(fp), "VIA_TOL_RT footprint present", actual=list(footprints))

    tol = 1e-4
    vias = fp.get("vias", [])
    runner.check(len(vias) == 1, "1 via survived", actual=len(vias))
    if vias:
        v = vias[0]
        runner.check(
            abs(v.get("hole_positive_tolerance", 0) - 0.05) < tol,
            "via hole_positive_tolerance",
            actual=v.get("hole_positive_tolerance"),
            expected=0.05,
        )
        runner.check(
            abs(v.get("hole_negative_tolerance", 0) - 0.02) < tol,
            "via hole_negative_tolerance",
            actual=v.get("hole_negative_tolerance"),
            expected=0.02,
        )


def test_write_pcblib_region_kind_net_name(client, runner, lib_path):
    print("\n=== Test: write_pcblib region kind/net/name/cavity round trip ===")
    # Coverage PR-9: the region's nested parameter block is now parsed and written,
    # so KIND (copper vs cutout), NAME, net_index, and cavity_height must be
    # authorable via write_pcblib and round-trip through read_pcblib.
    tol = 1e-4
    footprint = {
        "name": "REGION_FIELDS_RT",
        "regions": [
            {
                "vertices": [
                    {"x": -1.0, "y": -1.0},
                    {"x": 1.0, "y": -1.0},
                    {"x": 1.0, "y": 1.0},
                    {"x": -1.0, "y": 1.0},
                ],
                "layer": "Top Layer",
                "kind": "cutout",
                "name": "POUR_A",
                "net_index": 7,
                "cavity_height": 0.254,  # 10 mil in mm
            }
        ],
    }
    write = client.call_tool(
        "write_pcblib",
        {"filepath": lib_path, "footprints": [footprint], "append": False},
    )
    runner.check(not write.get("_isError"), "write_pcblib (region) succeeded", actual=write)

    read = client.call_tool("read_pcblib", {"filepath": lib_path})
    runner.check(not read.get("_isError"), "read_pcblib succeeded", actual=read)
    footprints = {fp.get("name"): fp for fp in read.get("footprints", [])}
    fp = footprints.get("REGION_FIELDS_RT", {})
    runner.check(bool(fp), "REGION_FIELDS_RT footprint present", actual=list(footprints))

    regions = fp.get("regions", [])
    runner.check(len(regions) == 1, "1 region survived", actual=len(regions))
    if regions:
        rg = regions[0]
        runner.check(
            rg.get("kind") == "cutout", "region kind=cutout", actual=rg.get("kind"), expected="cutout"
        )
        runner.check(
            rg.get("name") == "POUR_A", "region name", actual=rg.get("name"), expected="POUR_A"
        )
        runner.check(
            rg.get("net_index") == 7, "region net_index", actual=rg.get("net_index"), expected=7
        )
        runner.check(
            abs(rg.get("cavity_height", 0) - 0.254) < tol,
            "region cavity_height",
            actual=rg.get("cavity_height"),
            expected=0.254,
        )


def test_write_pcblib_component_body_fields(client, runner, lib_path):
    print("\n=== Test: write_pcblib component body fields round trip ===")
    # Coverage PR-11: the ComponentBody struct/reader/writer already model
    # body_color_3d / body_opacity_3d / body_projection / model_2d_rotation /
    # is_shape_based / kind / name, but the write handler used to hard-code them,
    # so a caller could not author them. They must now be settable via
    # component_bodies[] and round-trip through read_pcblib (proving the tool no
    # longer resets them). The body is authored on Mechanical 13 to also exercise
    # the layer-reader fix (header layer byte, not just the V7_LAYER string).
    tol = 1e-4
    footprint = {
        "name": "BODY_FIELDS_RT",
        "pads": [
            {"designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0},
        ],
        "component_bodies": [
            {
                "overall_height": 1.5,
                "standoff_height": 0.2,
                "layer": "Mechanical 13",
                "outline": [
                    {"x": -1.0, "y": -1.0},
                    {"x": 1.0, "y": -1.0},
                    {"x": 1.0, "y": 1.0},
                    {"x": -1.0, "y": 1.0},
                ],
                "body_color_3d": 16711680,  # 0xFF0000 (red) — non-default
                "body_opacity_3d": 0.5,
                "body_projection": 1,
                "model_2d_rotation": 90.0,
                "is_shape_based": True,
                "kind": 2,
                "name": "BODY_A",
            }
        ],
    }
    write = client.call_tool(
        "write_pcblib",
        {"filepath": lib_path, "footprints": [footprint], "append": False},
    )
    runner.check(not write.get("_isError"), "write_pcblib (component body) succeeded", actual=write)

    read = client.call_tool("read_pcblib", {"filepath": lib_path})
    runner.check(not read.get("_isError"), "read_pcblib succeeded", actual=read)
    footprints = {fp.get("name"): fp for fp in read.get("footprints", [])}
    fp = footprints.get("BODY_FIELDS_RT", {})
    runner.check(bool(fp), "BODY_FIELDS_RT footprint present", actual=list(footprints))

    bodies = fp.get("component_bodies", [])
    runner.check(len(bodies) == 1, "1 component body survived", actual=len(bodies))
    if bodies:
        b = bodies[0]
        runner.check(
            b.get("layer") == "Mechanical 13",
            "body layer (layer-reader fix)",
            actual=b.get("layer"),
            expected="Mechanical 13",
        )
        runner.check(
            b.get("body_color_3d") == 16711680,
            "body_color_3d",
            actual=b.get("body_color_3d"),
            expected=16711680,
        )
        runner.check(
            abs(b.get("body_opacity_3d", 0) - 0.5) < tol,
            "body_opacity_3d",
            actual=b.get("body_opacity_3d"),
            expected=0.5,
        )
        runner.check(
            b.get("body_projection") == 1,
            "body_projection",
            actual=b.get("body_projection"),
            expected=1,
        )
        runner.check(
            abs(b.get("model_2d_rotation", 0) - 90.0) < tol,
            "model_2d_rotation",
            actual=b.get("model_2d_rotation"),
            expected=90.0,
        )
        runner.check(
            b.get("is_shape_based") is True,
            "is_shape_based",
            actual=b.get("is_shape_based"),
            expected=True,
        )
        runner.check(
            b.get("kind") == 2, "body kind", actual=b.get("kind"), expected=2
        )
        runner.check(
            b.get("name") == "BODY_A", "body name", actual=b.get("name"), expected="BODY_A"
        )


def test_write_pcblib_additional_parameters_roundtrip(client, runner, lib_path):
    print("\n=== Test: write_pcblib region/body additional_parameters round trip ===")
    # Coverage PR-R5: Region and ComponentBody carry unmodelled |KEY=VAL| keys the
    # typed model does not recognise. They must be authorable via
    # additional_parameters (an array of [key, value] pairs) and round-trip through
    # read_pcblib so a read-modify-write does not silently drop them.
    region_extra = [["LAYER", "TOP"], ["ISBOARDCUTOUT", "FALSE"], ["LAYERSTACKID", "7"]]
    body_extra = [["TEXTURE", "wood"], ["MODEL.2D.X", "5mil"]]
    footprint = {
        "name": "ADDL_PARAMS_RT",
        "pads": [
            {"designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0},
        ],
        "regions": [
            {
                "vertices": [
                    {"x": -1.0, "y": -1.0},
                    {"x": 1.0, "y": -1.0},
                    {"x": 1.0, "y": 1.0},
                    {"x": -1.0, "y": 1.0},
                ],
                "layer": "Top Layer",
                "additional_parameters": region_extra,
            }
        ],
        "component_bodies": [
            {
                "overall_height": 1.0,
                "layer": "Top 3D Body",
                "additional_parameters": body_extra,
            }
        ],
    }
    write = client.call_tool(
        "write_pcblib",
        {"filepath": lib_path, "footprints": [footprint], "append": False},
    )
    runner.check(
        not write.get("_isError"),
        "write_pcblib (additional_parameters) succeeded",
        actual=write,
    )

    read = client.call_tool("read_pcblib", {"filepath": lib_path})
    runner.check(not read.get("_isError"), "read_pcblib succeeded", actual=read)
    footprints = {fp.get("name"): fp for fp in read.get("footprints", [])}
    fp = footprints.get("ADDL_PARAMS_RT", {})
    runner.check(bool(fp), "ADDL_PARAMS_RT footprint present", actual=list(footprints))

    # The value is not DROPPING the keys; order/interleaving with Altium's own keys
    # is not asserted (Altium's reader is order-independent), so we compare as sets.
    regions = fp.get("regions", [])
    runner.check(len(regions) == 1, "1 region survived", actual=len(regions))
    if regions:
        got = {(k, v) for k, v in regions[0].get("additional_parameters", [])}
        want = {tuple(pair) for pair in region_extra}
        runner.check(
            want.issubset(got),
            "region additional_parameters preserved",
            actual=sorted(got),
            expected=sorted(want),
        )

    bodies = fp.get("component_bodies", [])
    runner.check(len(bodies) == 1, "1 component body survived", actual=len(bodies))
    if bodies:
        got = {(k, v) for k, v in bodies[0].get("additional_parameters", [])}
        want = {tuple(pair) for pair in body_extra}
        runner.check(
            want.issubset(got),
            "body additional_parameters preserved",
            actual=sorted(got),
            expected=sorted(want),
        )


def test_write_schlib_fields(client, runner, schlib_path):
    print("\n=== Test: write_schlib field-completeness round trip ===")
    # Coverage PR-12/PR-13: every primitive field the read tool round-trips must
    # be authorable via write_schlib. Author a pin that sets all six previously
    # hard-coded fields plus an open-collector pin, and shapes that set
    # line_style/transparent/fill where the struct carries them; read back and
    # assert each survived. This also proves the strict-deser allow-lists accept
    # every new field (an un-allowed field would make write_schlib error here).
    symbol = {
        "name": "FIELDS",
        "pins": [
            {
                "designator": "1",
                "name": "CLK",
                "x": -50,
                "y": 0,
                "length": 30,
                "orientation": "left",
                "description": "clock input",
                "colour": 0x00FF00,
                "graphically_locked": True,
                "swap_id_group": "grpA",
                "part_and_sequence": "|1&2|",
                "default_value": "0",
            },
            {
                "designator": "2",
                "name": "OC",
                "x": -50,
                "y": 20,
                "length": 30,
                "orientation": "left",
                "electrical_type": "open_collector",
            },
        ],
        "rectangles": [
            {"x1": 0, "y1": 0, "x2": 40, "y2": 30, "line_style": 2, "transparent": True}
        ],
        "round_rects": [
            {
                "x1": 0,
                "y1": 40,
                "x2": 40,
                "y2": 70,
                "corner_x_radius": 5,
                "corner_y_radius": 7,
                "line_style": 1,
                "transparent": True,
            }
        ],
        "lines": [{"x1": 0, "y1": 80, "x2": 40, "y2": 80, "line_style": 2}],
        "polylines": [
            {
                "points": [{"x": 0, "y": 90}, {"x": 20, "y": 90}, {"x": 20, "y": 110}],
                "line_style": 1,
                "start_line_shape": 2,
                "end_line_shape": 3,
                "line_shape_size": 4,
                "transparent": True,
            }
        ],
        "arcs": [{"x": 80, "y": 80, "radius": 25, "fill_color": 0x112233}],
        "ellipses": [
            {"x": 60, "y": 120, "radius_x": 15, "radius_y": 10, "transparent": True}
        ],
    }

    write = client.call_tool(
        "write_schlib",
        {"filepath": schlib_path, "symbols": [symbol], "append": False},
    )
    runner.check(not write.get("_isError"), "write_schlib (fields) succeeded", actual=write)

    read = client.call_tool("read_schlib", {"filepath": schlib_path})
    runner.check(not read.get("_isError"), "read_schlib succeeded", actual=read)

    symbols = {s.get("name"): s for s in read.get("symbols", [])}
    sym = symbols.get("FIELDS", {})
    runner.check(bool(sym), "FIELDS symbol present", actual=list(symbols))

    pins = {p.get("name"): p for p in sym.get("pins", [])}
    clk = pins.get("CLK", {})
    runner.check(clk.get("description") == "clock input", "pin description", actual=clk.get("description"))
    runner.check(clk.get("colour") == 0x00FF00, "pin colour", actual=clk.get("colour"), expected=0x00FF00)
    runner.check(clk.get("graphically_locked") is True, "pin graphically_locked", actual=clk.get("graphically_locked"))
    runner.check(clk.get("swap_id_group") == "grpA", "pin swap_id_group", actual=clk.get("swap_id_group"))
    runner.check(clk.get("part_and_sequence") == "|1&2|", "pin part_and_sequence", actual=clk.get("part_and_sequence"))
    runner.check(clk.get("default_value") == "0", "pin default_value", actual=clk.get("default_value"))

    oc = pins.get("OC", {})
    runner.check(
        oc.get("electrical_type") == "open_collector",
        "pin electrical_type open_collector",
        actual=oc.get("electrical_type"),
        expected="open_collector",
    )

    rects = sym.get("rectangles", [])
    runner.check(len(rects) == 1, "1 rectangle survived", actual=len(rects))
    if rects:
        r = rects[0]
        runner.check(r.get("line_style") == 2, "rectangle line_style", actual=r.get("line_style"), expected=2)
        runner.check(r.get("transparent") is True, "rectangle transparent", actual=r.get("transparent"))

    rrs = sym.get("round_rects", [])
    runner.check(len(rrs) == 1, "1 round_rect survived", actual=len(rrs))
    if rrs:
        rr = rrs[0]
        runner.check(rr.get("line_style") == 1, "round_rect line_style", actual=rr.get("line_style"), expected=1)
        runner.check(rr.get("transparent") is True, "round_rect transparent", actual=rr.get("transparent"))

    lines = sym.get("lines", [])
    runner.check(len(lines) == 1, "1 line survived", actual=len(lines))
    if lines:
        runner.check(lines[0].get("line_style") == 2, "line line_style", actual=lines[0].get("line_style"), expected=2)

    polylines = sym.get("polylines", [])
    runner.check(len(polylines) == 1, "1 polyline survived", actual=len(polylines))
    if polylines:
        pl = polylines[0]
        runner.check(pl.get("line_style") == 1, "polyline line_style", actual=pl.get("line_style"), expected=1)
        runner.check(pl.get("start_line_shape") == 2, "polyline start_line_shape", actual=pl.get("start_line_shape"), expected=2)
        runner.check(pl.get("end_line_shape") == 3, "polyline end_line_shape", actual=pl.get("end_line_shape"), expected=3)
        runner.check(pl.get("line_shape_size") == 4, "polyline line_shape_size", actual=pl.get("line_shape_size"), expected=4)
        runner.check(pl.get("transparent") is True, "polyline transparent", actual=pl.get("transparent"))

    arcs = sym.get("arcs", [])
    runner.check(len(arcs) == 1, "1 arc survived", actual=len(arcs))
    if arcs:
        runner.check(arcs[0].get("fill_color") == 0x112233, "arc fill_color", actual=arcs[0].get("fill_color"), expected=0x112233)

    ellipses = sym.get("ellipses", [])
    runner.check(len(ellipses) == 1, "1 ellipse survived", actual=len(ellipses))
    if ellipses:
        runner.check(ellipses[0].get("transparent") is True, "ellipse transparent", actual=ellipses[0].get("transparent"))


def test_write_schlib_display_flags(client, runner, schlib_path):
    print("\n=== Test: write_schlib universal display/lock flags round trip ===")
    # Coverage PR-14: the four universal display/lock flags (graphically_locked,
    # disabled, dimmed, owner_part_display_mode) must be authorable on every
    # graphic shape and survive a write -> read. Proving they survive also proves
    # each shape's strict-deser allow-list accepts them (an un-allowed field would
    # make write_schlib error here).
    flags = {
        "graphically_locked": True,
        "disabled": True,
        "dimmed": True,
        "owner_part_display_mode": 1,
    }
    symbol = {
        "name": "FLAGS",
        "pins": [
            {
                "designator": "1",
                "name": "P1",
                "x": -50,
                "y": 0,
                "length": 30,
                "orientation": "left",
            }
        ],
        "rectangles": [dict({"x1": 0, "y1": 0, "x2": 40, "y2": 30}, **flags)],
        "round_rects": [
            dict(
                {
                    "x1": 0,
                    "y1": 40,
                    "x2": 40,
                    "y2": 70,
                    "corner_x_radius": 5,
                    "corner_y_radius": 7,
                },
                **flags,
            )
        ],
        "ellipses": [dict({"x": 60, "y": 120, "radius_x": 15, "radius_y": 10}, **flags)],
        "lines": [dict({"x1": 0, "y1": 80, "x2": 40, "y2": 80}, **flags)],
        "polylines": [
            dict(
                {"points": [{"x": 0, "y": 90}, {"x": 20, "y": 90}, {"x": 20, "y": 110}]},
                **flags,
            )
        ],
        "polygons": [
            dict(
                {"points": [{"x": 0, "y": 0}, {"x": 20, "y": 0}, {"x": 10, "y": 20}]},
                **flags,
            )
        ],
        "arcs": [dict({"x": 80, "y": 80, "radius": 25}, **flags)],
        "labels": [dict({"x": 0, "y": 130, "text": "L"}, **flags)],
        "parameters": [dict({"name": "Value", "value": "1k"}, **flags)],
    }

    write = client.call_tool(
        "write_schlib",
        {"filepath": schlib_path, "symbols": [symbol], "append": False},
    )
    runner.check(
        not write.get("_isError"),
        "write_schlib (display flags) succeeded",
        actual=write,
    )

    read = client.call_tool("read_schlib", {"filepath": schlib_path})
    runner.check(not read.get("_isError"), "read_schlib succeeded", actual=read)

    symbols = {s.get("name"): s for s in read.get("symbols", [])}
    sym = symbols.get("FLAGS", {})
    runner.check(bool(sym), "FLAGS symbol present", actual=list(symbols))

    def check_flags(shape, label):
        runner.check(
            shape.get("graphically_locked") is True,
            f"{label} graphically_locked",
            actual=shape.get("graphically_locked"),
        )
        runner.check(
            shape.get("disabled") is True,
            f"{label} disabled",
            actual=shape.get("disabled"),
        )
        runner.check(
            shape.get("dimmed") is True,
            f"{label} dimmed",
            actual=shape.get("dimmed"),
        )
        runner.check(
            shape.get("owner_part_display_mode") == 1,
            f"{label} owner_part_display_mode",
            actual=shape.get("owner_part_display_mode"),
            expected=1,
        )

    for coll, label in [
        ("rectangles", "rectangle"),
        ("round_rects", "round_rect"),
        ("ellipses", "ellipse"),
        ("lines", "line"),
        ("polylines", "polyline"),
        ("polygons", "polygon"),
        ("arcs", "arc"),
        ("labels", "label"),
        ("parameters", "parameter"),
    ]:
        shapes = sym.get(coll, [])
        runner.check(len(shapes) == 1, f"1 {label} survived", actual=len(shapes))
        if shapes:
            check_flags(shapes[0], label)


def test_write_schlib_parameter_display_fields(client, runner, schlib_path):
    print("\n=== Test: write_schlib parameter display fields round trip ===")
    # Coverage PR-15: the de-hardcoded core fields (read_only_state, param_type,
    # unique_id) plus the EE-meaningful display fields (orientation, show_name,
    # hide_name, description, is_configurable) must be authorable on a SchLib
    # parameter and survive a write -> read. Proving they survive also proves the
    # parameter's strict-deser allow-list accepts them (an un-allowed field would
    # make write_schlib error here) and that the write tool is no longer
    # hard-coding read_only_state/param_type/unique_id.
    symbol = {
        "name": "PARAMFIELDS",
        "pins": [
            {
                "designator": "1",
                "name": "P1",
                "x": -50,
                "y": 0,
                "length": 30,
                "orientation": "left",
            }
        ],
        "parameters": [
            {
                "name": "Value",
                "value": "10k",
                "read_only_state": 1,
                "param_type": 2,
                "unique_id": "WXYZ7890",
                "orientation": 3,
                "show_name": True,
                "hide_name": True,
                "description": "Resistance",
                "is_configurable": True,
            }
        ],
    }

    write = client.call_tool(
        "write_schlib",
        {"filepath": schlib_path, "symbols": [symbol], "append": False},
    )
    runner.check(
        not write.get("_isError"),
        "write_schlib (parameter display fields) succeeded",
        actual=write,
    )

    read = client.call_tool("read_schlib", {"filepath": schlib_path})
    runner.check(not read.get("_isError"), "read_schlib succeeded", actual=read)

    symbols = {s.get("name"): s for s in read.get("symbols", [])}
    sym = symbols.get("PARAMFIELDS", {})
    runner.check(bool(sym), "PARAMFIELDS symbol present", actual=list(symbols))

    params = {p.get("name"): p for p in sym.get("parameters", [])}
    val = params.get("Value", {})
    runner.check(bool(val), "Value parameter present", actual=list(params))

    runner.check(
        val.get("read_only_state") == 1,
        "parameter read_only_state",
        actual=val.get("read_only_state"),
        expected=1,
    )
    runner.check(
        val.get("param_type") == 2,
        "parameter param_type",
        actual=val.get("param_type"),
        expected=2,
    )
    runner.check(
        val.get("unique_id") == "WXYZ7890",
        "parameter unique_id preserved",
        actual=val.get("unique_id"),
        expected="WXYZ7890",
    )
    runner.check(
        val.get("orientation") == 3,
        "parameter orientation",
        actual=val.get("orientation"),
        expected=3,
    )
    runner.check(
        val.get("show_name") is True,
        "parameter show_name",
        actual=val.get("show_name"),
    )
    runner.check(
        val.get("hide_name") is True,
        "parameter hide_name",
        actual=val.get("hide_name"),
    )
    runner.check(
        val.get("description") == "Resistance",
        "parameter description",
        actual=val.get("description"),
        expected="Resistance",
    )
    runner.check(
        val.get("is_configurable") is True,
        "parameter is_configurable",
        actual=val.get("is_configurable"),
    )


def test_write_schlib_utf8_text(client, runner, schlib_path):
    print("\n=== Test: write_schlib non-Latin (UTF-8) text round trip ===")
    # Coverage PR-16 (correctness): a label / parameter / designator value with
    # characters outside Windows-1252 (Greek Ω, Cyrillic, CJK) must survive a
    # write -> read with the exact Unicode value intact. Before the fix the value
    # was silently corrupted to '?' (Windows-1252 could not represent it); now the
    # writer stores it behind `%UTF8%Text` and the reader decodes it back.
    omega = "10kΩ"  # 10kΩ (Greek capital omega, not in Windows-1252)
    cyrillic = "Привет"  # Привет
    cjk = "抄抗器"  # CJK string
    symbol = {
        "name": "UTF8SYM",
        "designator": cjk,
        "pins": [
            {
                "designator": "1",
                "name": "P1",
                "x": -50,
                "y": 0,
                "length": 30,
                "orientation": "left",
            }
        ],
        "labels": [{"x": 0, "y": 40, "text": cyrillic}],
        "parameters": [{"name": "Value", "value": omega}],
    }

    write = client.call_tool(
        "write_schlib",
        {"filepath": schlib_path, "symbols": [symbol], "append": False},
    )
    runner.check(
        not write.get("_isError"),
        "write_schlib (UTF-8 text) succeeded",
        actual=write,
    )

    read = client.call_tool("read_schlib", {"filepath": schlib_path})
    runner.check(not read.get("_isError"), "read_schlib succeeded", actual=read)

    symbols = {s.get("name"): s for s in read.get("symbols", [])}
    sym = symbols.get("UTF8SYM", {})
    runner.check(bool(sym), "UTF8SYM symbol present", actual=list(symbols))

    params = {p.get("name"): p for p in sym.get("parameters", [])}
    val = params.get("Value", {})
    runner.check(
        val.get("value") == omega,
        "parameter Ω value survives UTF-8 round-trip intact",
        actual=val.get("value"),
        expected=omega,
    )

    labels = sym.get("labels", [])
    runner.check(
        any(l.get("text") == cyrillic for l in labels),
        "label Cyrillic value survives UTF-8 round-trip intact",
        actual=[l.get("text") for l in labels],
        expected=cyrillic,
    )

    runner.check(
        sym.get("designator") == cjk,
        "designator CJK value survives UTF-8 round-trip intact",
        actual=sym.get("designator"),
        expected=cjk,
    )


def test_write_pcblib_text_font_style(client, runner, lib_path):
    print("\n=== Test: write_pcblib text mirror/bold/font/kind/justification round trip ===")
    # Coverage PR-10: the text primitive's mirror/bold/font_name plus the
    # previously tool-blocked kind/italic/justification must be authorable via
    # write_pcblib and round-trip through read_pcblib.
    footprint = {
        "name": "TEXT_STYLE_RT",
        "text": [
            {
                "x": 0.0,
                "y": 0.0,
                "text": "REF",
                "height": 1.0,
                "layer": "Top Overlay",
                "kind": "true_type",
                "font_name": "Times New Roman",
                "bold": True,
                "italic": True,
                "mirror": True,
                "justification": "top_right",
                "flags": "LOCKED",
            }
        ],
    }
    write = client.call_tool(
        "write_pcblib",
        {"filepath": lib_path, "footprints": [footprint], "append": False},
    )
    runner.check(not write.get("_isError"), "write_pcblib (text style) succeeded", actual=write)

    read = client.call_tool("read_pcblib", {"filepath": lib_path})
    runner.check(not read.get("_isError"), "read_pcblib succeeded", actual=read)
    footprints = {fp.get("name"): fp for fp in read.get("footprints", [])}
    fp = footprints.get("TEXT_STYLE_RT", {})
    runner.check(bool(fp), "TEXT_STYLE_RT footprint present", actual=list(footprints))

    texts = fp.get("text", [])
    ref = next((t for t in texts if t.get("text") == "REF"), None)
    runner.check(ref is not None, "REF text survived", actual=[t.get("text") for t in texts])
    if ref is not None:
        runner.check(ref.get("kind") == "true_type", "text kind", actual=ref.get("kind"), expected="true_type")
        runner.check(ref.get("font_name") == "Times New Roman", "text font_name", actual=ref.get("font_name"), expected="Times New Roman")
        runner.check(ref.get("bold") is True, "text bold", actual=ref.get("bold"))
        runner.check(ref.get("italic") is True, "text italic", actual=ref.get("italic"))
        runner.check(ref.get("mirror") is True, "text mirror", actual=ref.get("mirror"))
        runner.check(ref.get("justification") == "top_right", "text justification", actual=ref.get("justification"), expected="top_right")


def test_write_pcblib_unique_id_roundtrip(client, runner, lib_path):
    print("\n=== Test: write_pcblib unique_id round trip ===")
    # Coverage PR-R1: a primitive's identity GUID (unique_id) written via the tool
    # must survive read_pcblib unchanged, so a read-modify-write keeps stable
    # primitive identity instead of regenerating a fresh GUID. Also proves the
    # write-tool parsers/allow-lists now accept unique_id. Covers a via (parser +
    # UniqueIDs stream), a region and a text.
    footprint = {
        "name": "UID_RT",
        "pads": [
            {"designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0},
        ],
        "vias": [
            {
                "x": 1.5,
                "y": 2.5,
                "diameter": 0.6,
                "hole_size": 0.3,
                "unique_id": "VIAUID01",
            }
        ],
        "regions": [
            {
                "vertices": [
                    {"x": -1.0, "y": -1.0},
                    {"x": 1.0, "y": -1.0},
                    {"x": 1.0, "y": 1.0},
                ],
                "layer": "Top Layer",
                "unique_id": "REGUID02",
            }
        ],
        "text": [
            {
                "x": 0.0,
                "y": 3.0,
                "text": "REF",
                "height": 1.0,
                "layer": "Top Overlay",
                "unique_id": "TXTUID03",
            }
        ],
    }
    write = client.call_tool(
        "write_pcblib",
        {"filepath": lib_path, "footprints": [footprint], "append": False},
    )
    runner.check(
        not write.get("_isError"), "write_pcblib (unique_id) succeeded", actual=write
    )

    read = client.call_tool("read_pcblib", {"filepath": lib_path})
    runner.check(not read.get("_isError"), "read_pcblib succeeded", actual=read)
    footprints = {fp.get("name"): fp for fp in read.get("footprints", [])}
    fp = footprints.get("UID_RT", {})
    runner.check(bool(fp), "UID_RT footprint present", actual=list(footprints))

    vias = fp.get("vias", [])
    runner.check(len(vias) == 1, "1 via survived", actual=len(vias))
    if vias:
        runner.check(
            vias[0].get("unique_id") == "VIAUID01",
            "via unique_id preserved",
            actual=vias[0].get("unique_id"),
            expected="VIAUID01",
        )

    regions = fp.get("regions", [])
    runner.check(len(regions) == 1, "1 region survived", actual=len(regions))
    if regions:
        runner.check(
            regions[0].get("unique_id") == "REGUID02",
            "region unique_id preserved",
            actual=regions[0].get("unique_id"),
            expected="REGUID02",
        )

    # Match the text we wrote by content (an auto .Designator string may also be
    # present) and assert its unique_id round-tripped.
    texts = fp.get("text", [])
    ref = next((t for t in texts if t.get("text") == "REF"), None)
    runner.check(ref is not None, "REF text survived", actual=texts)
    if ref is not None:
        runner.check(
            ref.get("unique_id") == "TXTUID03",
            "text unique_id preserved",
            actual=ref.get("unique_id"),
            expected="TXTUID03",
        )


def test_write_pcblib_common_indices_roundtrip(client, runner, lib_path):
    print("\n=== Test: write_pcblib net/polygon/component index round trip ===")
    # Coverage PR-R4: the common-header connectivity indices (net_index @3,
    # polygon_index @5, component_index @7) must be authorable via write_pcblib
    # and round-trip through read_pcblib for every primitive that carries them.
    # These are marginal for netless library footprints but complete round-trip
    # fidelity for a board-context primitive read into a library.
    footprint = {
        "name": "INDICES_RT",
        "pads": [
            {
                "designator": "1",
                "x": 0.0,
                "y": 0.0,
                "width": 0.6,
                "height": 0.5,
                "layer": "Top Layer",
                "net_index": 11,
                "polygon_index": 3,
                "component_index": 5,
            }
        ],
        "tracks": [
            {
                "x1": -1.0,
                "y1": 0.0,
                "x2": 1.0,
                "y2": 0.0,
                "width": 0.25,
                "layer": "Top Layer",
                "net_index": 11,
                "component_index": 5,
            }
        ],
        "text": [
            {
                "x": 0.0,
                "y": 1.5,
                "text": "NET",
                "height": 1.0,
                "layer": "Top Overlay",
                "net_index": 22,
                "component_index": 5,
            }
        ],
    }
    write = client.call_tool(
        "write_pcblib",
        {"filepath": lib_path, "footprints": [footprint], "append": False},
    )
    runner.check(
        not write.get("_isError"), "write_pcblib (indices) succeeded", actual=write
    )

    read = client.call_tool("read_pcblib", {"filepath": lib_path})
    runner.check(not read.get("_isError"), "read_pcblib succeeded", actual=read)
    footprints = {fp.get("name"): fp for fp in read.get("footprints", [])}
    fp = footprints.get("INDICES_RT", {})
    runner.check(bool(fp), "INDICES_RT footprint present", actual=list(footprints))

    pads = fp.get("pads", [])
    pad = next((p for p in pads if p.get("designator") == "1"), None)
    runner.check(pad is not None, "pad 1 survived", actual=[p.get("designator") for p in pads])
    if pad is not None:
        runner.check(pad.get("net_index") == 11, "pad net_index", actual=pad.get("net_index"), expected=11)
        runner.check(pad.get("polygon_index") == 3, "pad polygon_index", actual=pad.get("polygon_index"), expected=3)
        runner.check(pad.get("component_index") == 5, "pad component_index", actual=pad.get("component_index"), expected=5)

    tracks = fp.get("tracks", [])
    runner.check(len(tracks) == 1, "1 track survived", actual=len(tracks))
    if tracks:
        t = tracks[0]
        runner.check(t.get("net_index") == 11, "track net_index", actual=t.get("net_index"), expected=11)
        runner.check(t.get("component_index") == 5, "track component_index", actual=t.get("component_index"), expected=5)
        # An unset polygon_index round-trips as the 65535 "none" default.
        runner.check(t.get("polygon_index") == 65535, "track polygon_index default", actual=t.get("polygon_index"), expected=65535)

    texts = fp.get("text", [])
    ref = next((t for t in texts if t.get("text") == "NET"), None)
    runner.check(ref is not None, "NET text survived", actual=[t.get("text") for t in texts])
    if ref is not None:
        runner.check(ref.get("net_index") == 22, "text net_index", actual=ref.get("net_index"), expected=22)
        runner.check(ref.get("component_index") == 5, "text component_index", actual=ref.get("component_index"), expected=5)


def test_write_schlib_unique_id_roundtrip(client, runner, schlib_path):
    print("\n=== Test: write_schlib unique_id round trip ===")
    # Coverage PR-R1: a SchLib shape's identity GUID (unique_id) written via the
    # tool must survive read_schlib unchanged. Also proves the SchLib shape
    # allow-lists/parsers now accept unique_id. Covers a rectangle and a label.
    symbol = {
        "name": "UID_SYM",
        "designator_prefix": "U",
        "pins": [
            {
                "designator": "1",
                "name": "1",
                "x": -50,
                "y": 0,
                "length": 20,
                "orientation": "left",
                "electrical_type": "passive",
            }
        ],
        "rectangles": [
            {"x1": -30, "y1": -20, "x2": 30, "y2": 20, "unique_id": "RECTUID4"}
        ],
        "labels": [
            {"x": 0, "y": 25, "text": "LBL", "unique_id": "LBLUID05"}
        ],
    }
    write = client.call_tool(
        "write_schlib",
        {"filepath": schlib_path, "symbols": [symbol], "append": False},
    )
    runner.check(
        not write.get("_isError"), "write_schlib (unique_id) succeeded", actual=write
    )

    read = client.call_tool("read_schlib", {"filepath": schlib_path})
    runner.check(not read.get("_isError"), "read_schlib succeeded", actual=read)
    symbols = {s.get("name"): s for s in read.get("symbols", [])}
    sym = symbols.get("UID_SYM", {})
    runner.check(bool(sym), "UID_SYM symbol present", actual=list(symbols))

    rects = sym.get("rectangles", [])
    runner.check(len(rects) >= 1, "rectangle survived", actual=len(rects))
    if rects:
        runner.check(
            rects[0].get("unique_id") == "RECTUID4",
            "rectangle unique_id preserved",
            actual=rects[0].get("unique_id"),
            expected="RECTUID4",
        )

    labels = sym.get("labels", [])
    lbl = next((l for l in labels if l.get("text") == "LBL"), None)
    runner.check(lbl is not None, "LBL label survived", actual=labels)
    if lbl is not None:
        runner.check(
            lbl.get("unique_id") == "LBLUID05",
            "label unique_id preserved",
            actual=lbl.get("unique_id"),
            expected="LBLUID05",
        )


def test_write_schlib_pin_aux_data(client, runner, schlib_path):
    print("\n=== Test: write_schlib pin auxiliary data round trip ===")
    # Coverage PR-R3: the three pin auxiliary fields must be authorable via
    # write_schlib and survive read_schlib.
    #   - owner_part_display_mode: the pin binary record's own byte (Part 1).
    #   - symbol_line_width: per-component PinSymbolLineWidth stream (Part 2).
    #   - frac: per-component PinFrac stream, fractional off-grid coords (Part 3).
    # A default pin (P0) sets none of them, proving the omit-when-default path
    # (which writes no aux stream, keeping the golden byte-identical); the aux
    # pin (PA) sets all three and must read them back keyed by pin ordinal.
    symbol = {
        "name": "PINAUX",
        "pins": [
            {
                "designator": "0",
                "name": "P0",
                "x": -50,
                "y": 0,
                "length": 30,
                "orientation": "left",
            },
            {
                "designator": "1",
                "name": "PA",
                "x": -50,
                "y": 20,
                "length": 30,
                "orientation": "left",
                "owner_part_display_mode": 2,
                "symbol_line_width": 3,
                "frac": {"x": 50000, "y": -25000, "length": 12345},
            },
        ],
    }

    write = client.call_tool(
        "write_schlib",
        {"filepath": schlib_path, "symbols": [symbol], "append": False},
    )
    runner.check(
        not write.get("_isError"), "write_schlib (pin aux) succeeded", actual=write
    )

    read = client.call_tool("read_schlib", {"filepath": schlib_path})
    runner.check(not read.get("_isError"), "read_schlib succeeded", actual=read)

    symbols = {s.get("name"): s for s in read.get("symbols", [])}
    sym = symbols.get("PINAUX", {})
    runner.check(bool(sym), "PINAUX symbol present", actual=list(symbols))

    pins = {p.get("name"): p for p in sym.get("pins", [])}
    p0 = pins.get("P0", {})
    pa = pins.get("PA", {})

    # Default pin: every aux field stays at its default (serde omits them).
    runner.check(
        p0.get("owner_part_display_mode", 0) == 0,
        "default pin owner_part_display_mode",
        actual=p0.get("owner_part_display_mode", 0),
    )
    runner.check(
        p0.get("symbol_line_width", 0) == 0,
        "default pin symbol_line_width",
        actual=p0.get("symbol_line_width", 0),
    )
    runner.check(p0.get("frac") is None, "default pin has no frac", actual=p0.get("frac"))

    # Aux pin: all three survive, keyed by pin ordinal.
    runner.check(
        pa.get("owner_part_display_mode") == 2,
        "aux pin owner_part_display_mode",
        actual=pa.get("owner_part_display_mode"),
        expected=2,
    )
    runner.check(
        pa.get("symbol_line_width") == 3,
        "aux pin symbol_line_width",
        actual=pa.get("symbol_line_width"),
        expected=3,
    )
    frac = pa.get("frac") or {}
    runner.check(
        frac.get("x") == 50000 and frac.get("y") == -25000 and frac.get("length") == 12345,
        "aux pin frac (x, y, length) round-trip",
        actual=frac,
        expected={"x": 50000, "y": -25000, "length": 12345},
    )


def main():
    binary = find_binary()
    print(f"Using binary: {binary}")

    work = tempfile.mkdtemp(prefix="altium-it-")
    allowed = os.path.join(work, "libs")
    os.makedirs(allowed, exist_ok=True)
    # The committed read-only fixture lives in the repo, so allow its directory
    # too (the MCP write path cannot author vias/fills — reading the real sample
    # is the only way to exercise those collections).
    samples_dir = os.path.realpath(
        os.path.join(os.path.dirname(__file__), "..", "..", "scripts", "samples")
    )
    config_path = os.path.join(work, "config.json")
    with open(config_path, "w", encoding="utf-8") as f:
        json.dump(
            {"allowed_paths": [allowed, samples_dir], "logging": {"level": "warn"}}, f
        )

    lib_path = os.path.join(allowed, "RoundTrip.PcbLib")
    schlib_path = os.path.join(allowed, "RoundTrip.SchLib")
    outside_path = os.path.join(work, "outside.PcbLib")  # in `work`, not `allowed`
    sample_path = os.path.join(samples_dir, "footprints.PcbLib")
    schlib_sample_path = os.path.join(samples_dir, "symbols.SchLib")

    client = McpTestClient(binary, config_path)
    client.start()
    runner = TestRunner()
    try:
        test_initialise(client, runner)
        test_tools_list(client, runner)
        test_write_read_roundtrip(client, runner, lib_path)
        test_write_pcblib_auto_3d_body_opt_in(client, runner, lib_path)
        test_write_pcblib_via_fill(client, runner, lib_path)
        test_write_pcblib_flags_mask_keepout(client, runner, lib_path)
        test_write_pcblib_pad_thermal_relief(client, runner, lib_path)
        test_write_pcblib_via_thermal_power_plane(client, runner, lib_path)
        test_write_pcblib_pad_slot_hole(client, runner, lib_path)
        test_write_pcblib_via_slot_tolerances(client, runner, lib_path)
        test_write_pcblib_region_kind_net_name(client, runner, lib_path)
        test_write_pcblib_component_body_fields(client, runner, lib_path)
        test_write_pcblib_additional_parameters_roundtrip(client, runner, lib_path)
        test_write_pcblib_text_font_style(client, runner, lib_path)
        test_write_pcblib_unique_id_roundtrip(client, runner, lib_path)
        test_write_pcblib_common_indices_roundtrip(client, runner, lib_path)
        test_write_schlib_shapes(client, runner, schlib_path)
        test_write_schlib_fields(client, runner, schlib_path)
        test_write_schlib_display_flags(client, runner, schlib_path)
        test_write_schlib_parameter_display_fields(client, runner, schlib_path)
        test_write_schlib_utf8_text(client, runner, schlib_path)
        test_write_schlib_unique_id_roundtrip(client, runner, schlib_path)
        test_write_schlib_pin_aux_data(client, runner, schlib_path)
        test_read_pcblib_exposes_vias_fills(client, runner, sample_path)
        test_read_schlib_exposes_round_rects_polygons(client, runner, schlib_sample_path)
        test_unknown_tool(client, runner)
        test_unknown_method(client, runner)
        test_missing_params(client, runner)
        test_path_outside_allowed(client, runner, outside_path)
        test_ping(client, runner)
    finally:
        client.stop()

    return runner.summary()


if __name__ == "__main__":
    sys.exit(main())
