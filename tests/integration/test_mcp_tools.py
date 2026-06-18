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


def main():
    binary = find_binary()
    print(f"Using binary: {binary}")

    work = tempfile.mkdtemp(prefix="altium-it-")
    allowed = os.path.join(work, "libs")
    os.makedirs(allowed, exist_ok=True)
    config_path = os.path.join(work, "config.json")
    with open(config_path, "w", encoding="utf-8") as f:
        json.dump({"allowed_paths": [allowed], "logging": {"level": "warn"}}, f)

    lib_path = os.path.join(allowed, "RoundTrip.PcbLib")
    outside_path = os.path.join(work, "outside.PcbLib")  # in `work`, not `allowed`

    client = McpTestClient(binary, config_path)
    client.start()
    runner = TestRunner()
    try:
        test_initialise(client, runner)
        test_tools_list(client, runner)
        test_write_read_roundtrip(client, runner, lib_path)
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
