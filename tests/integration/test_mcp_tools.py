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
        pts = polygons[0].get("points", [])
        runner.check(len(pts) == 3, "polygon has 3 vertices", actual=len(pts))

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
        test_write_schlib_shapes(client, runner, schlib_path)
        test_write_schlib_fields(client, runner, schlib_path)
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
