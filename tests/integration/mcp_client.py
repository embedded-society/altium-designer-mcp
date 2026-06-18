#!/usr/bin/env python3
"""A small MCP test client for driving altium-designer-mcp over stdio.

Spawns the server binary, speaks JSON-RPC over stdin/stdout, and exposes
helpers for the integration tests. Unlike a POSIX-`select` based reader, this
uses a background reader thread + queue so it works on Windows as well as
Linux/macOS.
"""

import json
import os
import queue
import subprocess
import threading
import time


def find_binary():
    """Locate the built server binary, preferring release over debug."""
    name = "altium-designer-mcp" + (".exe" if os.name == "nt" else "")
    for profile in ("release", "debug"):
        candidate = os.path.realpath(os.path.join("target", profile, name))
        if os.path.exists(candidate):
            return candidate
    raise RuntimeError(
        "altium-designer-mcp binary not found in target/release or target/debug; "
        "run `cargo build` first"
    )


class McpTestClient:
    """Manages an MCP server subprocess and sends JSON-RPC requests."""

    def __init__(self, binary, config_path):
        self.binary = binary
        self.config_path = config_path
        self.process = None
        self.request_id = 0
        self._queue = queue.Queue()
        self._reader = None

    def start(self):
        """Start the server. The config path is passed positionally."""
        self.process = subprocess.Popen(
            [self.binary, self.config_path],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        )
        self._reader = threading.Thread(target=self._read_lines, daemon=True)
        self._reader.start()
        time.sleep(0.5)
        if self.process.poll() is not None:
            raise RuntimeError(f"server exited immediately (code {self.process.returncode})")
        print(f"Server started (PID {self.process.pid})")

    def _read_lines(self):
        for line in self.process.stdout:
            self._queue.put(line)
        self._queue.put(None)  # sentinel for EOF

    def stop(self):
        """Stop the server, killing it if it does not exit promptly."""
        if self.process and self.process.poll() is None:
            try:
                self.process.stdin.close()
            except (OSError, ValueError):
                # Best-effort shutdown: stdin may already be closed or invalid.
                # Ignore and continue waiting for process termination.
                pass
            try:
                self.process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()

    def _recv(self, timeout):
        try:
            line = self._queue.get(timeout=timeout)
        except queue.Empty as exc:
            raise RuntimeError("timeout waiting for server output") from exc
        if line is None:
            raise RuntimeError("server closed stdout (EOF)")
        return json.loads(line)

    def send(self, method, params=None, timeout=30):
        """Send a JSON-RPC request and return the parsed response."""
        self.request_id += 1
        request = {"jsonrpc": "2.0", "id": self.request_id, "method": method}
        if params is not None:
            request["params"] = params
        self.process.stdin.write(json.dumps(request) + "\n")
        self.process.stdin.flush()

        deadline = time.monotonic() + timeout
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise RuntimeError(f"timeout waiting for response to {method}")
            msg = self._recv(remaining)
            if "id" not in msg:  # skip notifications
                continue
            return msg

    def notify(self, method, params=None):
        """Send a JSON-RPC notification (no response expected)."""
        notification = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            notification["params"] = params
        self.process.stdin.write(json.dumps(notification) + "\n")
        self.process.stdin.flush()

    def call_tool(self, name, arguments=None):
        """Call an MCP tool and return the parsed content.

        Successful JSON results are parsed; non-JSON text is wrapped in
        {"_text": ...}; error results (isError=true) are wrapped in
        {"_error": text, "_isError": True}.
        """
        response = self.send("tools/call", {"name": name, "arguments": arguments or {}})
        if "error" in response:
            raise RuntimeError(f"tool {name} protocol error: {response['error']}")

        result = response["result"]
        text = result["content"][0]["text"]
        if result.get("isError", False):
            return {"_error": text, "_isError": True}
        try:
            return json.loads(text)
        except json.JSONDecodeError:
            return {"_text": text}


class TestRunner:
    """Tiny test runner that tracks pass/fail counts."""

    def __init__(self):
        self.passed = 0
        self.failed = 0
        self.total = 0

    def check(self, condition, description, actual=None, expected=None):
        self.total += 1
        if condition:
            print(f"  PASS: {description}")
            self.passed += 1
        else:
            detail = f" (expected {expected}, got {actual})" if expected is not None else (
                f" (got {actual})" if actual is not None else ""
            )
            print(f"  FAIL: {description}{detail}")
            self.failed += 1

    def summary(self):
        print()
        print("=" * 44)
        print(f"Results: {self.passed} passed, {self.failed} failed, {self.total} total")
        print("=" * 44)
        return 0 if self.failed == 0 else 1
