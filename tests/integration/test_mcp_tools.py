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
    outside_path = os.path.join(work, "outside.PcbLib")  # in `work`, not `allowed`
    sample_path = os.path.join(samples_dir, "pads.PcbLib")
    schlib_sample_path = os.path.join(samples_dir, "symbols.SchLib")

    client = McpTestClient(binary, config_path)
    client.start()
    runner = TestRunner()
    try:
        test_initialise(client, runner)
        test_tools_list(client, runner)
        test_write_read_roundtrip(client, runner, lib_path)
        test_write_pcblib_auto_3d_body_opt_in(client, runner, lib_path)
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
