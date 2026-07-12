//! End-to-end integration tests driving the real server binary over stdio.
//!
//! These spawn the compiled `altium-designer-mcp` binary, speak JSON-RPC 2.0
//! over its stdin/stdout, and exercise tool dispatch, the request/response
//! envelope, create -> read round trips, and the error paths — none of which
//! the in-crate tests cover (they call the private handlers directly).
//!
//! This is the Rust port of the former `tests/integration/test_mcp_tools.py`.
//! Keeping it in Rust means the spawned binary is instrumented by
//! `cargo llvm-cov` like any other target, so these end-to-end paths land in
//! the coverage report. The `pyaltiumlib` readability oracle stays in Python
//! by design: its value is being an *independent* reader we did not write.
//!
//! Each test owns its own server process and temp directory, so the default
//! parallel test runner is safe.

// Test-only relaxations of the crate's pedantic/nursery lint gate. Long,
// literal-heavy round-trip assertions are the nature of an E2E suite.
#![allow(clippy::too_many_lines)]
#![allow(clippy::similar_names)]

use base64::Engine;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

// ============================================================================
// Test harness: a JSON-RPC client over a spawned server process.
// ============================================================================

/// The repo's read-only sample-fixture directory (`scripts/samples/`).
fn samples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("samples")
}

/// Owns a server subprocess and drives it over stdio with JSON-RPC.
struct Harness {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<String>,
    next_id: i64,
    tmp: tempfile::TempDir,
    libs: PathBuf,
    /// The stored `initialize` response, for the initialise test to assert on.
    init: Value,
}

impl Harness {
    /// Spawns a fresh server, performs the MCP initialise handshake, and
    /// returns a ready client. `allowed_paths` is the per-test `libs/` dir plus
    /// the shared read-only samples dir.
    fn start() -> Self {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let libs = tmp.path().join("libs");
        std::fs::create_dir_all(&libs).expect("create libs dir");

        let config = tmp.path().join("config.json");
        let cfg = json!({
            "allowed_paths": [
                libs.to_str().expect("utf-8 libs path"),
                samples_dir().to_str().expect("utf-8 samples path"),
            ],
            "logging": { "level": "warn" },
        });
        std::fs::write(&config, serde_json::to_string(&cfg).unwrap()).expect("write config");

        // The server takes the config path as its sole positional argument.
        let mut child = Command::new(env!("CARGO_BIN_EXE_altium-designer-mcp"))
            .arg(&config)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn server binary");

        let stdin = child.stdin.take().expect("child stdin");
        let stdout = child.stdout.take().expect("child stdout");

        // Background reader thread + channel so reads work identically on
        // Windows and Unix (no POSIX select on a pipe handle).
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(l) => {
                        if tx.send(l).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let mut h = Self { child, stdin, rx, next_id: 0, tmp, libs, init: Value::Null };

        let init = h.send(
            "initialize",
            Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "integration-test", "version": "1.0.0" },
            })),
        );
        h.notify("notifications/initialized");
        h.init = init;
        h
    }

    fn lib_path(&self) -> String {
        self.libs.join("RoundTrip.PcbLib").to_str().unwrap().to_owned()
    }

    fn schlib_path(&self) -> String {
        self.libs.join("RoundTrip.SchLib").to_str().unwrap().to_owned()
    }

    /// A path that exists in the temp dir but *outside* any allowed path.
    fn outside_path(&self) -> String {
        self.tmp.path().join("outside.PcbLib").to_str().unwrap().to_owned()
    }

    fn sample(name: &str) -> String {
        samples_dir().join(name).to_str().unwrap().to_owned()
    }

    /// Sends a JSON-RPC request and returns the first response bearing an `id`
    /// (notifications from the server are skipped).
    fn send(&mut self, method: &str, params: Option<Value>) -> Value {
        self.next_id += 1;
        let mut req = json!({ "jsonrpc": "2.0", "id": self.next_id, "method": method });
        if let Some(p) = params {
            req["params"] = p;
        }
        self.write_line(&req);

        loop {
            let line = self
                .rx
                .recv_timeout(Duration::from_secs(30))
                .unwrap_or_else(|_| panic!("timeout waiting for response to {method}"));
            let msg: Value = serde_json::from_str(&line).expect("server emitted valid JSON");
            if msg.get("id").is_some() {
                return msg;
            }
        }
    }

    /// Sends a JSON-RPC notification (no response expected).
    fn notify(&mut self, method: &str) {
        self.write_line(&json!({ "jsonrpc": "2.0", "method": method }));
    }

    /// Calls an MCP tool and normalises the result:
    /// - a successful JSON body is parsed and returned as-is;
    /// - an `isError` result becomes `{"_isError": true, "_error": <text>}`;
    /// - non-JSON success text becomes `{"_text": <text>}`.
    ///
    // Owned `Value` (not `&Value`) so call sites read `call_tool("x", json!({..}))`
    // rather than `&json!({..})`; the argument is logically the request's payload.
    #[allow(clippy::needless_pass_by_value)]
    fn call_tool(&mut self, name: &str, arguments: Value) -> Value {
        let resp = self.send("tools/call", Some(json!({ "name": name, "arguments": arguments })));
        assert!(
            resp.get("error").is_none(),
            "tool {name} protocol error: {}",
            resp["error"]
        );
        let result = &resp["result"];
        let text = result["content"][0]["text"].as_str().unwrap_or("").to_owned();
        if result.get("isError").and_then(Value::as_bool).unwrap_or(false) {
            return json!({ "_isError": true, "_error": text });
        }
        serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({ "_text": text }))
    }

    fn write_line(&mut self, msg: &Value) {
        writeln!(self.stdin, "{}", serde_json::to_string(msg).unwrap()).expect("write to server");
        self.stdin.flush().expect("flush server stdin");
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        // Best-effort shutdown; the OS reaps stdin on kill.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ============================================================================
// Small JSON accessors, kept terse so the round-trip assertions stay readable.
// ============================================================================

fn is_err(v: &Value) -> bool {
    v.get("_isError").and_then(Value::as_bool).unwrap_or(false)
}

/// Borrowed array at `key`, or an empty slice when absent/not an array.
fn arr<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

fn len_of(v: &Value, key: &str) -> usize {
    arr(v, key).len()
}

/// Finds the element of `items` whose string field `key` equals `name`.
fn find_by<'a>(items: &'a [Value], key: &str, name: &str) -> Option<&'a Value> {
    items.iter().find(|it| it.get(key).and_then(Value::as_str) == Some(name))
}

fn f(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(f64::NAN)
}

fn i(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(Value::as_i64).unwrap_or(i64::MIN)
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn b(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// True only when `key` is present *and* literally `false` — distinct from `!b`,
/// which also holds when the field is missing. Mirrors the Python `is False`
/// checks so a reader that drops the field is caught, not silently accepted.
fn is_false(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool) == Some(false)
}

/// True when `a` is within Altium's sub-micron fixed-point quantisation of `b`.
/// 1e-4 mm (0.1 µm) is well below any meaningful PCB tolerance.
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-4
}

/// `read_pcblib` serialises `PcbFlags` as a "|"-joined name string, e.g.
/// `"LOCKED | KEEPOUT"`. Returns whether `name` is one of the set bits.
fn has_flag(v: &Value, name: &str) -> bool {
    v.as_str().unwrap_or("").split('|').map(str::trim).any(|p| p == name)
}

// ============================================================================
// Protocol / dispatch tests
// ============================================================================

#[test]
fn initialise_returns_protocol_and_server_info() {
    let h = Harness::start();
    let result = &h.init["result"];
    assert_eq!(result["protocolVersion"], "2024-11-05");
    assert_eq!(result["serverInfo"]["name"], "altium-designer-mcp");
}

#[test]
fn tools_list_contains_core_tools() {
    let mut h = Harness::start();
    let resp = h.send("tools/list", None);
    assert!(resp.get("error").is_none(), "tools/list — no error");
    let names: Vec<&str> =
        arr(&resp["result"], "tools").iter().map(|t| s(t, "name")).collect();
    for expected in ["read_pcblib", "write_pcblib", "list_components", "get_component"] {
        assert!(names.contains(&expected), "tools/list contains {expected}");
    }
}

#[test]
fn unknown_tool_reports_tool_error() {
    let mut h = Harness::start();
    let resp = h.send("tools/call", Some(json!({ "name": "nope_not_a_tool", "arguments": {} })));
    assert!(resp.get("error").is_none(), "no protocol error");
    let result = &resp["result"];
    assert_eq!(result["isError"], true, "isError true");
    let text = result["content"][0]["text"].as_str().unwrap_or("");
    assert!(
        text.contains("Unknown tool") || text.to_lowercase().contains("unknown"),
        "mentions unknown tool: {text}"
    );
}

#[test]
fn unknown_method_returns_method_not_found() {
    let mut h = Harness::start();
    let resp = h.send("nonexistent/method", None);
    assert!(resp.get("error").is_some(), "has error field");
    assert_eq!(resp["error"]["code"], -32601, "method not found code");
}

#[test]
fn missing_required_params_errors() {
    let mut h = Harness::start();
    let resp = h.send("tools/call", Some(json!({ "name": "write_pcblib", "arguments": {} })));
    let has_error = resp.get("error").is_some();
    let has_tool_error =
        !has_error && resp["result"].get("isError").and_then(Value::as_bool).unwrap_or(false);
    assert!(has_error || has_tool_error, "error for missing args");
}

#[test]
fn path_outside_allowed_is_denied() {
    let mut h = Harness::start();
    let outside = h.outside_path();
    let footprint = json!({
        "name": "X",
        "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 0.5, "height": 0.5 }],
    });
    let result = h.call_tool(
        "write_pcblib",
        json!({ "filepath": outside, "footprints": [footprint], "append": false }),
    );
    assert!(is_err(&result), "write outside allowed rejected: {result}");
    assert!(s(&result, "_error").contains("Access denied"), "denial message");
}

#[test]
fn ping_succeeds() {
    let mut h = Harness::start();
    let resp = h.send("ping", None);
    assert!(resp.get("error").is_none(), "ping — no error");
    assert!(resp.get("result").is_some(), "ping has result");
}

// ============================================================================
// Write -> read round trips (PcbLib)
// ============================================================================

#[test]
fn write_read_roundtrip() {
    let mut h = Harness::start();
    let lib = h.lib_path();
    let footprint = json!({
        "name": "RESC0402",
        "description": "0402 chip resistor",
        "pads": [
            { "designator": "1", "x": -0.5, "y": 0.0, "width": 0.6, "height": 0.5 },
            { "designator": "2", "x": 0.5, "y": 0.0, "width": 0.6, "height": 0.5 },
        ],
    });
    let write = h.call_tool(
        "write_pcblib",
        json!({ "filepath": lib, "footprints": [footprint], "append": false }),
    );
    assert!(!is_err(&write), "write_pcblib succeeded: {write}");

    let read = h.call_tool("read_pcblib", json!({ "filepath": lib }));
    assert!(!is_err(&read), "read_pcblib succeeded");
    assert!(read.to_string().contains("RESC0402"), "read-back contains RESC0402");

    let listing = h.call_tool("list_components", json!({ "filepath": lib }));
    assert!(!is_err(&listing), "list_components succeeded");
    assert!(i(&listing, "total_count") >= 1, "list_components total_count >= 1");

    let got =
        h.call_tool("get_component", json!({ "filepath": lib, "component_name": "RESC0402" }));
    assert!(!is_err(&got), "get_component succeeded");
    assert!(got.to_string().contains("RESC0402"), "get_component returns RESC0402");
}

#[test]
fn write_pcblib_auto_3d_body_is_opt_in() {
    let mut h = Harness::start();
    let lib = h.lib_path();
    let fp = json!({
        "name": "NOBODY",
        "pads": [{ "designator": "1", "x": 0, "y": 0, "width": 1.0, "height": 1.0 }],
    });

    let default = h.call_tool(
        "write_pcblib",
        json!({ "filepath": lib, "footprints": [fp], "append": false }),
    );
    assert!(!is_err(&default), "write_pcblib (default) succeeded: {default}");
    let d_body = find_by(arr(&default, "bodies"), "name", "NOBODY").cloned().unwrap_or_default();
    assert_eq!(s(&d_body, "source"), "none", "default: no auto 3D body");

    let optin = h.call_tool(
        "write_pcblib",
        json!({ "filepath": lib, "footprints": [fp], "append": false, "auto_3d_body": true }),
    );
    assert!(!is_err(&optin), "write_pcblib (auto_3d_body) succeeded: {optin}");
    let o_body = find_by(arr(&optin, "bodies"), "name", "NOBODY").cloned().unwrap_or_default();
    assert_eq!(s(&o_body, "source"), "auto-extruded", "auto_3d_body:true adds an extruded body");
    assert!(b(&o_body, "assumed_height"), "auto body flagged assumed_height");
}

#[test]
fn write_pcblib_via_and_fill_roundtrip() {
    let mut h = Harness::start();
    let lib = h.lib_path();
    let footprint = json!({
        "name": "VIAFILL",
        "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 }],
        "vias": [{
            "x": 1.5, "y": 2.5, "diameter": 0.6, "hole_size": 0.3,
            "from_layer": "Top Layer", "to_layer": "Bottom Layer",
        }],
        "fills": [{
            "x1": -1.0, "y1": -2.0, "x2": 3.0, "y2": 4.0,
            "layer": "Top Layer", "rotation": 45.0,
        }],
    });
    let write = h.call_tool(
        "write_pcblib",
        json!({ "filepath": lib, "footprints": [footprint], "append": false }),
    );
    assert!(!is_err(&write), "write_pcblib (via+fill) succeeded: {write}");

    let read = h.call_tool("read_pcblib", json!({ "filepath": lib }));
    assert!(!is_err(&read), "read_pcblib succeeded");
    let fp = find_by(arr(&read, "footprints"), "name", "VIAFILL").expect("VIAFILL present");

    let vias = arr(fp, "vias");
    assert_eq!(vias.len(), 1, "1 via survived");
    assert!(near(f(&vias[0], "diameter"), 0.6), "via diameter");
    assert!(near(f(&vias[0], "hole_size"), 0.3), "via hole_size");

    let fills = arr(fp, "fills");
    assert_eq!(fills.len(), 1, "1 fill survived");
    let fl = &fills[0];
    assert!(
        near(f(fl, "x1"), -1.0) && near(f(fl, "y1"), -2.0) && near(f(fl, "x2"), 3.0)
            && near(f(fl, "y2"), 4.0),
        "fill corners"
    );
    assert!(near(f(fl, "rotation"), 45.0), "fill rotation");
}

#[test]
fn write_pcblib_flags_mask_keepout_roundtrip() {
    let mut h = Harness::start();
    let lib = h.lib_path();
    let footprint = json!({
        "name": "FLAGS_RT",
        "pads": [{
            "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0,
            "flags": "LOCKED", "solder_mask_expansion": 0.05,
            "solder_mask_expansion_mode": "manual",
        }],
        "tracks": [{
            "x1": -1.0, "y1": 0.0, "x2": 1.0, "y2": 0.0, "width": 0.15, "layer": "Top Overlay",
            "flags": "LOCKED", "solder_mask_expansion": 0.1, "keepout_restrictions": 3,
        }],
        "arcs": [{
            "x": 0.0, "y": 2.0, "radius": 0.5, "start_angle": 0.0, "end_angle": 90.0,
            "width": 0.15, "layer": "Top Overlay",
            "flags": "LOCKED", "solder_mask_expansion": 0.2, "keepout_restrictions": 5,
        }],
        "regions": [{
            "vertices": [{ "x": 0.0, "y": 0.0 }, { "x": 1.0, "y": 0.0 }, { "x": 0.0, "y": 1.0 }],
            "layer": "Top Courtyard", "flags": "KEEPOUT",
        }],
        "fills": [{
            "x1": -1.0, "y1": -1.0, "x2": 1.0, "y2": 1.0, "layer": "Top Layer",
            "flags": "LOCKED", "solder_mask_expansion": 0.05, "keepout_restrictions": 2,
        }],
        "text": [{
            "x": 0.0, "y": 3.0, "text": "REF", "height": 0.5, "layer": "Top Overlay",
            "flags": "LOCKED",
        }],
    });
    let write = h.call_tool(
        "write_pcblib",
        json!({ "filepath": lib, "footprints": [footprint], "append": false }),
    );
    assert!(!is_err(&write), "write_pcblib (flags) succeeded: {write}");

    let read = h.call_tool("read_pcblib", json!({ "filepath": lib }));
    let fp = find_by(arr(&read, "footprints"), "name", "FLAGS_RT").expect("FLAGS_RT present");

    let pads = arr(fp, "pads");
    assert_eq!(pads.len(), 1, "1 pad survived");
    assert!(has_flag(&pads[0]["flags"], "LOCKED"), "pad flags LOCKED");
    assert!(near(f(&pads[0], "solder_mask_expansion"), 0.05), "pad solder_mask_expansion");
    assert_eq!(s(&pads[0], "solder_mask_expansion_mode"), "manual", "pad mask mode");

    let tracks = arr(fp, "tracks");
    assert_eq!(tracks.len(), 1, "1 track survived");
    assert!(has_flag(&tracks[0]["flags"], "LOCKED"), "track flags LOCKED");
    assert!(near(f(&tracks[0], "solder_mask_expansion"), 0.1), "track solder_mask_expansion");
    assert_eq!(i(&tracks[0], "keepout_restrictions"), 3, "track keepout_restrictions");

    let arcs = arr(fp, "arcs");
    assert_eq!(arcs.len(), 1, "1 arc survived");
    assert!(has_flag(&arcs[0]["flags"], "LOCKED"), "arc flags LOCKED");
    assert!(near(f(&arcs[0], "solder_mask_expansion"), 0.2), "arc solder_mask_expansion");
    assert_eq!(i(&arcs[0], "keepout_restrictions"), 5, "arc keepout_restrictions");

    let regions = arr(fp, "regions");
    assert_eq!(regions.len(), 1, "1 region survived");
    assert!(has_flag(&regions[0]["flags"], "KEEPOUT"), "region flags KEEPOUT");

    let fills = arr(fp, "fills");
    assert_eq!(fills.len(), 1, "1 fill survived");
    assert!(has_flag(&fills[0]["flags"], "LOCKED"), "fill flags LOCKED");
    assert!(near(f(&fills[0], "solder_mask_expansion"), 0.05), "fill solder_mask_expansion");
    assert_eq!(i(&fills[0], "keepout_restrictions"), 2, "fill keepout_restrictions");

    let ref_text = find_by(arr(fp, "text"), "text", "REF").expect("REF text survived");
    assert!(has_flag(&ref_text["flags"], "LOCKED"), "text flags LOCKED");
}

#[test]
fn write_pcblib_pad_thermal_relief_roundtrip() {
    let mut h = Harness::start();
    let lib = h.lib_path();
    let footprint = json!({
        "name": "PAD_RELIEF_RT",
        "pads": [{
            "designator": "1", "x": 0.0, "y": 0.0, "width": 1.6, "height": 1.6,
            "hole_size": 0.8, "layer": "Multi-Layer",
            "power_plane_connect_style": "direct", "relief_conductor_width": 0.3,
            "relief_entries": 2, "relief_air_gap": 0.2,
            "power_plane_relief_expansion": 0.6, "power_plane_clearance": 0.7,
        }],
    });
    let write = h.call_tool(
        "write_pcblib",
        json!({ "filepath": lib, "footprints": [footprint], "append": false }),
    );
    assert!(!is_err(&write), "write_pcblib (pad relief) succeeded: {write}");

    let read = h.call_tool("read_pcblib", json!({ "filepath": lib }));
    let fp = find_by(arr(&read, "footprints"), "name", "PAD_RELIEF_RT").expect("present");
    let pads = arr(fp, "pads");
    assert_eq!(pads.len(), 1, "1 pad survived");
    let p = &pads[0];
    assert_eq!(s(p, "power_plane_connect_style"), "direct", "connect_style");
    assert!(near(f(p, "relief_conductor_width"), 0.3), "relief_conductor_width");
    assert_eq!(i(p, "relief_entries"), 2, "relief_entries");
    assert!(near(f(p, "relief_air_gap"), 0.2), "relief_air_gap");
    assert!(near(f(p, "power_plane_relief_expansion"), 0.6), "relief_expansion");
    assert!(near(f(p, "power_plane_clearance"), 0.7), "power_plane_clearance");
}

#[test]
fn write_pcblib_via_thermal_power_plane_roundtrip() {
    let mut h = Harness::start();
    let lib = h.lib_path();
    let footprint = json!({
        "name": "VIA_RELIEF_RT",
        "vias": [{
            "x": 1.0, "y": 2.0, "diameter": 0.8, "hole_size": 0.4,
            "from_layer": "Top Layer", "to_layer": "Bottom Layer",
            "power_plane_connect_style": "direct",
            "power_plane_relief_expansion": 0.6, "power_plane_clearance": 0.7,
            "paste_mask_expansion": 0.05, "net_index": 42,
            "flags": "TENTING_TOP | TENTING_BOTTOM | KEEPOUT | LOCKED",
        }],
    });
    let write = h.call_tool(
        "write_pcblib",
        json!({ "filepath": lib, "footprints": [footprint], "append": false }),
    );
    assert!(!is_err(&write), "write_pcblib (via relief) succeeded: {write}");

    let read = h.call_tool("read_pcblib", json!({ "filepath": lib }));
    let fp = find_by(arr(&read, "footprints"), "name", "VIA_RELIEF_RT").expect("present");
    let vias = arr(fp, "vias");
    assert_eq!(vias.len(), 1, "1 via survived");
    let v = &vias[0];
    assert_eq!(s(v, "power_plane_connect_style"), "direct", "connect_style");
    assert!(near(f(v, "power_plane_relief_expansion"), 0.6), "relief_expansion");
    assert!(near(f(v, "power_plane_clearance"), 0.7), "power_plane_clearance");
    assert!(near(f(v, "paste_mask_expansion"), 0.05), "paste_mask_expansion");
    assert_eq!(i(v, "net_index"), 42, "net_index");
    for flag in ["LOCKED", "KEEPOUT", "TENTING_TOP", "TENTING_BOTTOM"] {
        assert!(has_flag(&v["flags"], flag), "via flag {flag}");
    }
}

#[test]
fn write_pcblib_pad_slot_hole_roundtrip() {
    let mut h = Harness::start();
    let lib = h.lib_path();
    let footprint = json!({
        "name": "PAD_SLOT_RT",
        "pads": [{
            "designator": "1", "x": 0.0, "y": 0.0, "width": 2.0, "height": 1.2,
            "hole_size": 0.8, "layer": "Multi-Layer",
            "hole_shape": "slot", "hole_slot_length": 1.5, "hole_rotation": 45.0,
            "hole_positive_tolerance": 0.05, "hole_negative_tolerance": 0.02,
        }],
    });
    let write = h.call_tool(
        "write_pcblib",
        json!({ "filepath": lib, "footprints": [footprint], "append": false }),
    );
    assert!(!is_err(&write), "write_pcblib (pad slot) succeeded: {write}");

    let read = h.call_tool("read_pcblib", json!({ "filepath": lib }));
    let fp = find_by(arr(&read, "footprints"), "name", "PAD_SLOT_RT").expect("present");
    let pads = arr(fp, "pads");
    assert_eq!(pads.len(), 1, "1 pad survived");
    let p = &pads[0];
    assert_eq!(s(p, "hole_shape"), "slot", "hole_shape");
    assert!(near(f(p, "hole_slot_length"), 1.5), "hole_slot_length");
    assert!(near(f(p, "hole_rotation"), 45.0), "hole_rotation");
    assert!(near(f(p, "hole_positive_tolerance"), 0.05), "hole_positive_tolerance");
    assert!(near(f(p, "hole_negative_tolerance"), 0.02), "hole_negative_tolerance");
}

#[test]
fn write_pcblib_via_slot_tolerances_roundtrip() {
    let mut h = Harness::start();
    let lib = h.lib_path();
    let footprint = json!({
        "name": "VIA_TOL_RT",
        "vias": [{
            "x": 1.0, "y": 2.0, "diameter": 0.8, "hole_size": 0.4,
            "hole_positive_tolerance": 0.05, "hole_negative_tolerance": 0.02,
        }],
    });
    let write = h.call_tool(
        "write_pcblib",
        json!({ "filepath": lib, "footprints": [footprint], "append": false }),
    );
    assert!(!is_err(&write), "write_pcblib (via tol) succeeded: {write}");

    let read = h.call_tool("read_pcblib", json!({ "filepath": lib }));
    let fp = find_by(arr(&read, "footprints"), "name", "VIA_TOL_RT").expect("present");
    let vias = arr(fp, "vias");
    assert_eq!(vias.len(), 1, "1 via survived");
    assert!(near(f(&vias[0], "hole_positive_tolerance"), 0.05), "via hole_positive_tolerance");
    assert!(near(f(&vias[0], "hole_negative_tolerance"), 0.02), "via hole_negative_tolerance");
}

#[test]
fn write_pcblib_region_kind_net_name_roundtrip() {
    let mut h = Harness::start();
    let lib = h.lib_path();
    let footprint = json!({
        "name": "REGION_FIELDS_RT",
        "regions": [{
            "vertices": [
                { "x": -1.0, "y": -1.0 }, { "x": 1.0, "y": -1.0 },
                { "x": 1.0, "y": 1.0 }, { "x": -1.0, "y": 1.0 },
            ],
            "layer": "Top Layer", "kind": "cutout", "name": "POUR_A",
            "net_index": 7, "cavity_height": 0.254,
            "sub_poly_index": 3, "union_index": 2, "is_shape_based": true,
        }],
    });
    let write = h.call_tool(
        "write_pcblib",
        json!({ "filepath": lib, "footprints": [footprint], "append": false }),
    );
    assert!(!is_err(&write), "write_pcblib (region) succeeded: {write}");

    let read = h.call_tool("read_pcblib", json!({ "filepath": lib }));
    let fp = find_by(arr(&read, "footprints"), "name", "REGION_FIELDS_RT").expect("present");
    let regions = arr(fp, "regions");
    assert_eq!(regions.len(), 1, "1 region survived");
    let rg = &regions[0];
    assert_eq!(s(rg, "kind"), "cutout", "region kind");
    assert_eq!(s(rg, "name"), "POUR_A", "region name");
    assert_eq!(i(rg, "net_index"), 7, "region net_index");
    assert!(near(f(rg, "cavity_height"), 0.254), "region cavity_height");
    assert_eq!(i(rg, "sub_poly_index"), 3, "region sub_poly_index");
    assert_eq!(i(rg, "union_index"), 2, "region union_index");
    assert!(b(rg, "is_shape_based"), "region is_shape_based");
}

#[test]
fn write_pcblib_component_body_fields_roundtrip() {
    let mut h = Harness::start();
    let lib = h.lib_path();
    let footprint = json!({
        "name": "BODY_FIELDS_RT",
        "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 }],
        "component_bodies": [{
            "overall_height": 1.5, "standoff_height": 0.2, "layer": "Mechanical 13",
            "outline": [
                { "x": -1.0, "y": -1.0 }, { "x": 1.0, "y": -1.0 },
                { "x": 1.0, "y": 1.0 }, { "x": -1.0, "y": 1.0 },
            ],
            "body_color_3d": 0xFF_0000, "body_opacity_3d": 0.5, "body_projection": 1,
            "model_2d_rotation": 90.0, "is_shape_based": true, "kind": 2, "name": "BODY_A",
        }],
    });
    let write = h.call_tool(
        "write_pcblib",
        json!({ "filepath": lib, "footprints": [footprint], "append": false }),
    );
    assert!(!is_err(&write), "write_pcblib (component body) succeeded: {write}");

    let read = h.call_tool("read_pcblib", json!({ "filepath": lib }));
    let fp = find_by(arr(&read, "footprints"), "name", "BODY_FIELDS_RT").expect("present");
    let bodies = arr(fp, "component_bodies");
    assert_eq!(bodies.len(), 1, "1 component body survived");
    let bd = &bodies[0];
    assert_eq!(s(bd, "layer"), "Mechanical 13", "body layer (layer-reader fix)");
    assert_eq!(i(bd, "body_color_3d"), 0xFF_0000, "body_color_3d");
    assert!(near(f(bd, "body_opacity_3d"), 0.5), "body_opacity_3d");
    assert_eq!(i(bd, "body_projection"), 1, "body_projection");
    assert!(near(f(bd, "model_2d_rotation"), 90.0), "model_2d_rotation");
    assert!(b(bd, "is_shape_based"), "is_shape_based");
    assert_eq!(i(bd, "kind"), 2, "body kind");
    assert_eq!(s(bd, "name"), "BODY_A", "body name");
}

#[test]
fn write_pcblib_additional_parameters_roundtrip() {
    let mut h = Harness::start();
    let lib = h.lib_path();
    // Keys the writer does NOT emit itself, so a custom value can survive.
    let region_extra = [["LAYER", "TOP"], ["ISBOARDCUTOUT", "FALSE"], ["LAYERSTACKID", "7"]];
    let body_extra = [["WELDINGSPOT", "42"], ["CUSTOMTAG", "xyz"]];
    let footprint = json!({
        "name": "ADDL_PARAMS_RT",
        "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 }],
        "regions": [{
            "vertices": [
                { "x": -1.0, "y": -1.0 }, { "x": 1.0, "y": -1.0 },
                { "x": 1.0, "y": 1.0 }, { "x": -1.0, "y": 1.0 },
            ],
            "layer": "Top Layer",
            "additional_parameters": region_extra,
        }],
        "component_bodies": [{
            "overall_height": 1.0, "layer": "Top 3D Body",
            "additional_parameters": body_extra,
        }],
    });
    let write = h.call_tool(
        "write_pcblib",
        json!({ "filepath": lib, "footprints": [footprint], "append": false }),
    );
    assert!(!is_err(&write), "write_pcblib (additional_parameters) succeeded: {write}");

    let read = h.call_tool("read_pcblib", json!({ "filepath": lib }));
    let fp = find_by(arr(&read, "footprints"), "name", "ADDL_PARAMS_RT").expect("present");

    let region_pairs = pairs(&arr(fp, "regions")[0], "additional_parameters");
    for [k, v] in region_extra {
        assert!(
            region_pairs.contains(&(k.to_owned(), v.to_owned())),
            "region additional_parameters preserved: {k}={v}"
        );
    }

    let body_pairs = pairs(&arr(fp, "component_bodies")[0], "additional_parameters");
    for [k, v] in body_extra {
        assert!(
            body_pairs.contains(&(k.to_owned(), v.to_owned())),
            "body additional_parameters preserved: {k}={v}"
        );
    }
    // No canonical key the writer emits itself may appear twice after a RMW.
    let keys: Vec<&String> = body_pairs.iter().map(|(k, _)| k).collect();
    for (k, _) in &body_pairs {
        assert!(keys.iter().filter(|x| **x == k).count() == 1, "body key {k} not duplicated");
    }
}

#[test]
fn write_pcblib_text_font_style_roundtrip() {
    let mut h = Harness::start();
    let lib = h.lib_path();
    let footprint = json!({
        "name": "TEXT_STYLE_RT",
        "text": [{
            "x": 0.0, "y": 0.0, "text": "REF", "height": 1.0, "layer": "Top Overlay",
            "kind": "true_type", "font_name": "Times New Roman", "bold": true, "italic": true,
            "mirror": true, "justification": "top_right", "flags": "LOCKED",
        }],
    });
    let write = h.call_tool(
        "write_pcblib",
        json!({ "filepath": lib, "footprints": [footprint], "append": false }),
    );
    assert!(!is_err(&write), "write_pcblib (text style) succeeded: {write}");

    let read = h.call_tool("read_pcblib", json!({ "filepath": lib }));
    let fp = find_by(arr(&read, "footprints"), "name", "TEXT_STYLE_RT").expect("present");
    let re = find_by(arr(fp, "text"), "text", "REF").expect("REF text survived");
    assert_eq!(s(re, "kind"), "true_type", "text kind");
    assert_eq!(s(re, "font_name"), "Times New Roman", "text font_name");
    assert!(b(re, "bold"), "text bold");
    assert!(b(re, "italic"), "text italic");
    assert!(b(re, "mirror"), "text mirror");
    assert_eq!(s(re, "justification"), "top_right", "text justification");
}

#[test]
fn write_pcblib_text_inverted_rect_roundtrip() {
    let mut h = Harness::start();
    let lib = h.lib_path();
    let footprint = json!({
        "name": "TEXT_INVRECT_RT",
        "text": [{
            "x": 0.0, "y": 0.0, "text": "KO", "height": 1.0, "layer": "Top Overlay",
            "is_inverted": true, "inverted_border": 0.0254, "use_inverted_rectangle": true,
            "inverted_rect_width": 0.254, "inverted_rect_height": 0.127,
            "inverted_rect_text_offset": 0.0508,
        }],
    });
    let write = h.call_tool(
        "write_pcblib",
        json!({ "filepath": lib, "footprints": [footprint], "append": false }),
    );
    assert!(!is_err(&write), "write_pcblib (inverted rect) succeeded: {write}");

    let read = h.call_tool("read_pcblib", json!({ "filepath": lib }));
    let fp = find_by(arr(&read, "footprints"), "name", "TEXT_INVRECT_RT").expect("present");
    let ko = find_by(arr(fp, "text"), "text", "KO").expect("KO text survived");
    assert!(b(ko, "is_inverted"), "text is_inverted");
    assert!(b(ko, "use_inverted_rectangle"), "text use_inverted_rectangle");
    assert!(near(f(ko, "inverted_border"), 0.0254), "text inverted_border");
    assert!(near(f(ko, "inverted_rect_width"), 0.254), "text inverted_rect_width");
    assert!(near(f(ko, "inverted_rect_height"), 0.127), "text inverted_rect_height");
    assert!(near(f(ko, "inverted_rect_text_offset"), 0.0508), "text inverted_rect_text_offset");
}

#[test]
fn write_pcblib_unique_id_roundtrip() {
    let mut h = Harness::start();
    let lib = h.lib_path();
    let footprint = json!({
        "name": "UID_RT",
        "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 }],
        "vias": [{ "x": 1.5, "y": 2.5, "diameter": 0.6, "hole_size": 0.3, "unique_id": "VIAUID01" }],
        "regions": [{
            "vertices": [{ "x": -1.0, "y": -1.0 }, { "x": 1.0, "y": -1.0 }, { "x": 1.0, "y": 1.0 }],
            "layer": "Top Layer", "unique_id": "REGUID02",
        }],
        "text": [{
            "x": 0.0, "y": 3.0, "text": "REF", "height": 1.0, "layer": "Top Overlay",
            "unique_id": "TXTUID03",
        }],
    });
    let write = h.call_tool(
        "write_pcblib",
        json!({ "filepath": lib, "footprints": [footprint], "append": false }),
    );
    assert!(!is_err(&write), "write_pcblib (unique_id) succeeded: {write}");

    let read = h.call_tool("read_pcblib", json!({ "filepath": lib }));
    let fp = find_by(arr(&read, "footprints"), "name", "UID_RT").expect("present");
    assert_eq!(s(&arr(fp, "vias")[0], "unique_id"), "VIAUID01", "via unique_id preserved");
    assert_eq!(s(&arr(fp, "regions")[0], "unique_id"), "REGUID02", "region unique_id preserved");
    let re = find_by(arr(fp, "text"), "text", "REF").expect("REF text survived");
    assert_eq!(s(re, "unique_id"), "TXTUID03", "text unique_id preserved");
}

#[test]
fn write_pcblib_common_indices_roundtrip() {
    let mut h = Harness::start();
    let lib = h.lib_path();
    let footprint = json!({
        "name": "INDICES_RT",
        "pads": [{
            "designator": "1", "x": 0.0, "y": 0.0, "width": 0.6, "height": 0.5,
            "layer": "Top Layer", "net_index": 11, "polygon_index": 3, "component_index": 5,
        }],
        "tracks": [{
            "x1": -1.0, "y1": 0.0, "x2": 1.0, "y2": 0.0, "width": 0.25, "layer": "Top Layer",
            "net_index": 11, "component_index": 5,
        }],
        "text": [{
            "x": 0.0, "y": 1.5, "text": "NET", "height": 1.0, "layer": "Top Overlay",
            "net_index": 22, "component_index": 5,
        }],
    });
    let write = h.call_tool(
        "write_pcblib",
        json!({ "filepath": lib, "footprints": [footprint], "append": false }),
    );
    assert!(!is_err(&write), "write_pcblib (indices) succeeded: {write}");

    let read = h.call_tool("read_pcblib", json!({ "filepath": lib }));
    let fp = find_by(arr(&read, "footprints"), "name", "INDICES_RT").expect("present");

    let pad = find_by(arr(fp, "pads"), "designator", "1").expect("pad 1 survived");
    assert_eq!(i(pad, "net_index"), 11, "pad net_index");
    assert_eq!(i(pad, "polygon_index"), 3, "pad polygon_index");
    assert_eq!(i(pad, "component_index"), 5, "pad component_index");

    let tracks = arr(fp, "tracks");
    assert_eq!(tracks.len(), 1, "1 track survived");
    assert_eq!(i(&tracks[0], "net_index"), 11, "track net_index");
    assert_eq!(i(&tracks[0], "component_index"), 5, "track component_index");
    assert_eq!(i(&tracks[0], "polygon_index"), 65535, "track polygon_index default");

    let re = find_by(arr(fp, "text"), "text", "NET").expect("NET text survived");
    assert_eq!(i(re, "net_index"), 22, "text net_index");
    assert_eq!(i(re, "component_index"), 5, "text component_index");
}

#[test]
fn update_pcblib_preserves_vias_fills() {
    let mut h = Harness::start();
    let lib = h.lib_path();
    let mut footprint = json!({
        "name": "UPD_VIA_FILL",
        "description": "before",
        "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 }],
        "vias": [{ "x": 2.0, "y": 0.0, "diameter": 0.6, "hole_size": 0.3 }],
        "fills": [{ "x1": -1.0, "y1": -1.0, "x2": 1.0, "y2": 1.0, "layer": "Top Layer" }],
    });
    let write = h.call_tool(
        "write_pcblib",
        json!({ "filepath": lib, "footprints": [footprint.clone()], "append": false }),
    );
    assert!(!is_err(&write), "write_pcblib (via+fill) succeeded: {write}");

    footprint["description"] = json!("after");
    let upd = h.call_tool(
        "update_component",
        json!({ "filepath": lib, "component_name": "UPD_VIA_FILL", "footprint": footprint }),
    );
    assert!(!is_err(&upd), "update_component succeeded: {upd}");

    let read = h.call_tool("read_pcblib", json!({ "filepath": lib }));
    let fp = find_by(arr(&read, "footprints"), "name", "UPD_VIA_FILL").expect("present");
    assert_eq!(len_of(fp, "vias"), 1, "via survived update_component");
    assert_eq!(len_of(fp, "fills"), 1, "fill survived update_component");
    assert_eq!(s(fp, "description"), "after", "description was updated");
}

#[test]
fn compare_components_duplicates_and_depth() {
    let mut h = Harness::start();
    let lib = h.lib_path();
    let region = || json!({
        "vertices": [
            { "x": -1.0, "y": -1.0 }, { "x": 1.0, "y": -1.0 },
            { "x": 1.0, "y": 1.0 }, { "x": -1.0, "y": 1.0 },
        ],
        "layer": "Top Courtyard",
    });
    let fp_a = json!({
        "name": "CMP_A",
        "pads": [
            { "designator": "9", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 },
            { "designator": "9", "x": 2.0, "y": 0.0, "width": 1.0, "height": 1.0 },
        ],
        "regions": [region()],
        "text": [{ "x": 0.0, "y": -2.0, "text": "REF", "height": 1.0, "layer": "Top Overlay" }],
    });
    let fp_b = json!({
        "name": "CMP_B",
        "pads": [
            { "designator": "9", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 },
            { "designator": "9", "x": 2.0, "y": 0.0, "width": 1.5, "height": 1.0 },
        ],
        "regions": [region()],
        "text": [{ "x": 3.0, "y": -2.0, "text": "REF", "height": 1.0, "layer": "Top Overlay" }],
    });
    let write = h.call_tool(
        "write_pcblib",
        json!({ "filepath": lib, "footprints": [fp_a, fp_b], "append": false }),
    );
    assert!(!is_err(&write), "write_pcblib (compare fixtures) succeeded: {write}");

    let cmp = h.call_tool(
        "compare_components",
        json!({
            "filepath_a": lib, "component_a": "CMP_A",
            "filepath_b": lib, "component_b": "CMP_B",
        }),
    );
    assert!(!is_err(&cmp), "compare_components succeeded: {cmp}");
    assert_eq!(cmp["identical"], false, "differences detected");

    let diffs = arr(&cmp, "differences");
    let pad_diffs = find_by(diffs, "field", "pads").map_or(&[][..], |d| arr(d, "differences"));
    assert!(
        pad_diffs.iter().any(|d| s(d, "status") == "modified" && i(d, "occurrence") == 1),
        "duplicate-designator pad diff reported with occurrence index"
    );
    let text_diffs = find_by(diffs, "field", "text").map_or(&[][..], |d| arr(d, "differences"));
    assert!(
        text_diffs.iter().any(|d| {
            s(d, "status") == "modified"
                && arr(d, "changes").iter().any(|c| s(c, "property") == "position")
        }),
        "moved same-content text reported as position change"
    );
}

// ============================================================================
// Write -> read round trips (SchLib)
// ============================================================================

#[test]
fn write_schlib_shapes_roundtrip() {
    let mut h = Harness::start();
    let lib = h.schlib_path();
    let symbol = json!({
        "name": "SHAPES",
        "pins": [{
            "designator": "1", "name": "P1", "x": -50, "y": 0, "length": 30, "orientation": "left",
        }],
        "round_rects": [{
            "x1": 0, "y1": 0, "x2": 40, "y2": 30, "corner_x_radius": 5, "corner_y_radius": 7,
            "fill_color": 0x11_2233,
        }],
        "polygons": [{
            "points": [{ "x": 0, "y": 0 }, { "x": 20, "y": 0 }, { "x": 10, "y": 20 }],
            "fill_color": 0x44_5566, "line_style": 2, "transparent": true, "is_not_accessible": false,
        }],
        "ellipses": [{ "x": 60, "y": 60, "radius_x": 15, "radius_y": 10, "fill_color": 0x77_8899 }],
        "arcs": [{
            "x": 80, "y": 80, "radius": 25, "start_angle": 30, "end_angle": 270,
            "is_not_accessible": false,
        }],
        "lines": [{ "x1": 0, "y1": 0, "x2": 30, "y2": 0, "is_not_accessible": false }],
        "pies": [{
            "x": 100, "y": 100, "radius": 20, "start_angle": 45, "end_angle": 315,
            "fill_color": 0x22_3344, "filled": true, "transparent": true,
        }],
        "images": [{
            "x1": -20, "y1": -10, "x2": 20, "y2": 10, "file_name": "pic.png",
            "embed_image": true, "keep_aspect": true, "show_border": true,
        }],
        "text_frames": [{
            "x1": -10, "y1": -5, "x2": 10, "y2": 5, "text": "Frame text",
            "area_color": 0xB0_FFFF, "text_color": 0x80_0000, "text_margin": 0.2,
            "line_width": 1, "is_solid": true, "alignment": 2,
        }],
        "labels": [{ "x": 5, "y": 45, "text": "HELLO" }],
    });
    let write = h.call_tool(
        "write_schlib",
        json!({ "filepath": lib, "symbols": [symbol], "append": false }),
    );
    assert!(!is_err(&write), "write_schlib succeeded: {write}");

    let read = h.call_tool("read_schlib", json!({ "filepath": lib }));
    assert!(!is_err(&read), "read_schlib succeeded: {read}");
    let sym = find_by(arr(&read, "symbols"), "name", "SHAPES").expect("SHAPES symbol present");

    let rrs = arr(sym, "round_rects");
    assert_eq!(rrs.len(), 1, "1 round_rect survived");
    assert!(near(f(&rrs[0], "corner_x_radius"), 5.0), "round_rect corner_x_radius");
    assert!(near(f(&rrs[0], "corner_y_radius"), 7.0), "round_rect corner_y_radius");
    assert!(near(f(&rrs[0], "x2"), 40.0) && near(f(&rrs[0], "y2"), 30.0), "round_rect geometry");

    let pgs = arr(sym, "polygons");
    assert_eq!(pgs.len(), 1, "1 polygon survived");
    assert_eq!(len_of(&pgs[0], "points"), 3, "polygon has 3 vertices");
    assert_eq!(i(&pgs[0], "line_style"), 2, "polygon line_style round-trips");
    assert!(b(&pgs[0], "transparent"), "polygon transparent round-trips");
    assert!(is_false(&pgs[0], "is_not_accessible"), "polygon is_not_accessible=false round-trips");

    let els = arr(sym, "ellipses");
    assert_eq!(els.len(), 1, "1 ellipse survived");
    assert!(near(f(&els[0], "radius_x"), 15.0) && near(f(&els[0], "radius_y"), 10.0), "ellipse radii");

    let arcs = arr(sym, "arcs");
    assert_eq!(arcs.len(), 1, "1 arc survived");
    assert!(near(f(&arcs[0], "radius"), 25.0), "arc radius");
    assert!(near(f(&arcs[0], "start_angle"), 30.0) && near(f(&arcs[0], "end_angle"), 270.0), "arc angles");
    assert!(is_false(&arcs[0], "is_not_accessible"), "arc is_not_accessible=false round-trips");

    let lines = arr(sym, "lines");
    assert_eq!(lines.len(), 1, "1 line survived");
    assert!(is_false(&lines[0], "is_not_accessible"), "line is_not_accessible=false round-trips");

    let pies = arr(sym, "pies");
    assert_eq!(pies.len(), 1, "1 pie survived");
    assert!(near(f(&pies[0], "radius"), 20.0), "pie radius");
    assert!(near(f(&pies[0], "start_angle"), 45.0) && near(f(&pies[0], "end_angle"), 315.0), "pie angles");
    assert!(b(&pies[0], "filled"), "pie filled round-trips");
    assert!(b(&pies[0], "transparent"), "pie transparent round-trips");

    let images = arr(sym, "images");
    assert_eq!(images.len(), 1, "1 image survived");
    assert_eq!(s(&images[0], "file_name"), "pic.png", "image file_name");
    assert!(b(&images[0], "embed_image"), "image embed_image round-trips");
    assert!(b(&images[0], "keep_aspect"), "image keep_aspect round-trips");
    assert!(b(&images[0], "show_border"), "image show_border round-trips");

    let tfs = arr(sym, "text_frames");
    assert_eq!(tfs.len(), 1, "1 text_frame survived");
    let tf = &tfs[0];
    assert_eq!(s(tf, "text"), "Frame text", "text_frame text");
    assert_eq!(i(tf, "area_color"), 0xB0_FFFF, "text_frame area_color round-trips");
    assert_eq!(i(tf, "text_color"), 0x80_0000, "text_frame text_color round-trips");
    assert!(near(f(tf, "text_margin"), 0.2), "text_frame text_margin round-trips");
    assert!(b(tf, "is_solid"), "text_frame is_solid round-trips");
    assert_eq!(i(tf, "alignment"), 2, "text_frame alignment round-trips");
    assert!(b(tf, "word_wrap"), "text_frame word_wrap defaults true");
    assert!(b(tf, "clip_to_rect"), "text_frame clip_to_rect defaults true");
    assert!(b(tf, "show_border"), "text_frame show_border defaults true");

    let labels = arr(sym, "labels");
    assert_eq!(labels.len(), 1, "1 label survived");
    assert_eq!(s(&labels[0], "text"), "HELLO", "label text");
}

#[test]
fn write_schlib_fields_roundtrip() {
    let mut h = Harness::start();
    let lib = h.schlib_path();
    let symbol = json!({
        "name": "FIELDS",
        "pins": [
            {
                "designator": "1", "name": "CLK", "x": -50, "y": 0, "length": 30,
                "orientation": "left", "description": "clock input", "colour": 0x00_FF00,
                "graphically_locked": true, "swap_id_group": "grpA",
                "part_and_sequence": "|1&2|", "default_value": "0",
            },
            {
                "designator": "2", "name": "OC", "x": -50, "y": 20, "length": 30,
                "orientation": "left", "electrical_type": "open_collector",
            },
        ],
        "rectangles": [{ "x1": 0, "y1": 0, "x2": 40, "y2": 30, "line_style": 2, "transparent": true }],
        "round_rects": [{
            "x1": 0, "y1": 40, "x2": 40, "y2": 70, "corner_x_radius": 5, "corner_y_radius": 7,
            "line_style": 1, "transparent": true,
        }],
        "lines": [{ "x1": 0, "y1": 80, "x2": 40, "y2": 80, "line_style": 2 }],
        "polylines": [{
            "points": [{ "x": 0, "y": 90 }, { "x": 20, "y": 90 }, { "x": 20, "y": 110 }],
            "line_style": 1, "start_line_shape": 2, "end_line_shape": 3, "line_shape_size": 4,
            "transparent": true,
        }],
        "arcs": [{ "x": 80, "y": 80, "radius": 25, "fill_color": 0x11_2233 }],
        "ellipses": [{ "x": 60, "y": 120, "radius_x": 15, "radius_y": 10, "transparent": true }],
    });
    let write = h.call_tool(
        "write_schlib",
        json!({ "filepath": lib, "symbols": [symbol], "append": false }),
    );
    assert!(!is_err(&write), "write_schlib (fields) succeeded: {write}");

    let read = h.call_tool("read_schlib", json!({ "filepath": lib }));
    let sym = find_by(arr(&read, "symbols"), "name", "FIELDS").expect("FIELDS symbol present");

    let clk = find_by(arr(sym, "pins"), "name", "CLK").expect("CLK pin");
    assert_eq!(s(clk, "description"), "clock input", "pin description");
    assert_eq!(i(clk, "colour"), 0x00_FF00, "pin colour");
    assert!(b(clk, "graphically_locked"), "pin graphically_locked");
    assert_eq!(s(clk, "swap_id_group"), "grpA", "pin swap_id_group");
    assert_eq!(s(clk, "part_and_sequence"), "|1&2|", "pin part_and_sequence");
    assert_eq!(s(clk, "default_value"), "0", "pin default_value");

    let oc = find_by(arr(sym, "pins"), "name", "OC").expect("OC pin");
    assert_eq!(s(oc, "electrical_type"), "open_collector", "pin electrical_type");

    let rects = arr(sym, "rectangles");
    assert_eq!(rects.len(), 1, "1 rectangle survived");
    assert_eq!(i(&rects[0], "line_style"), 2, "rectangle line_style");
    assert!(b(&rects[0], "transparent"), "rectangle transparent");

    let rrs = arr(sym, "round_rects");
    assert_eq!(rrs.len(), 1, "1 round_rect survived");
    assert_eq!(i(&rrs[0], "line_style"), 1, "round_rect line_style");
    assert!(b(&rrs[0], "transparent"), "round_rect transparent");

    let lines = arr(sym, "lines");
    assert_eq!(lines.len(), 1, "1 line survived");
    assert_eq!(i(&lines[0], "line_style"), 2, "line line_style");

    let pls = arr(sym, "polylines");
    assert_eq!(pls.len(), 1, "1 polyline survived");
    assert_eq!(i(&pls[0], "line_style"), 1, "polyline line_style");
    assert_eq!(i(&pls[0], "start_line_shape"), 2, "polyline start_line_shape");
    assert_eq!(i(&pls[0], "end_line_shape"), 3, "polyline end_line_shape");
    assert_eq!(i(&pls[0], "line_shape_size"), 4, "polyline line_shape_size");
    assert!(b(&pls[0], "transparent"), "polyline transparent");

    let arcs = arr(sym, "arcs");
    assert_eq!(arcs.len(), 1, "1 arc survived");
    assert_eq!(i(&arcs[0], "fill_color"), 0x11_2233, "arc fill_color");

    let els = arr(sym, "ellipses");
    assert_eq!(els.len(), 1, "1 ellipse survived");
    assert!(b(&els[0], "transparent"), "ellipse transparent");
}

#[test]
fn write_schlib_display_flags_roundtrip() {
    let mut h = Harness::start();
    let lib = h.schlib_path();
    // The four universal display/lock flags, merged into every shape below.
    let flags = json!({
        "graphically_locked": true, "disabled": true, "dimmed": true,
        "owner_part_display_mode": 1,
    });
    let with_flags = |mut base: Value| -> Value {
        let obj = base.as_object_mut().unwrap();
        for (k, v) in flags.as_object().unwrap() {
            obj.insert(k.clone(), v.clone());
        }
        base
    };
    let symbol = json!({
        "name": "FLAGS",
        "pins": [{
            "designator": "1", "name": "P1", "x": -50, "y": 0, "length": 30, "orientation": "left",
        }],
        "rectangles": [with_flags(json!({ "x1": 0, "y1": 0, "x2": 40, "y2": 30 }))],
        "round_rects": [with_flags(json!({
            "x1": 0, "y1": 40, "x2": 40, "y2": 70, "corner_x_radius": 5, "corner_y_radius": 7,
        }))],
        "ellipses": [with_flags(json!({ "x": 60, "y": 120, "radius_x": 15, "radius_y": 10 }))],
        "lines": [with_flags(json!({ "x1": 0, "y1": 80, "x2": 40, "y2": 80 }))],
        "polylines": [with_flags(json!({
            "points": [{ "x": 0, "y": 90 }, { "x": 20, "y": 90 }, { "x": 20, "y": 110 }],
        }))],
        "polygons": [with_flags(json!({
            "points": [{ "x": 0, "y": 0 }, { "x": 20, "y": 0 }, { "x": 10, "y": 20 }],
        }))],
        "arcs": [with_flags(json!({ "x": 80, "y": 80, "radius": 25 }))],
        "labels": [with_flags(json!({ "x": 0, "y": 130, "text": "L" }))],
        "parameters": [with_flags(json!({ "name": "Value", "value": "1k" }))],
    });
    let write = h.call_tool(
        "write_schlib",
        json!({ "filepath": lib, "symbols": [symbol], "append": false }),
    );
    assert!(!is_err(&write), "write_schlib (display flags) succeeded: {write}");

    let read = h.call_tool("read_schlib", json!({ "filepath": lib }));
    let sym = find_by(arr(&read, "symbols"), "name", "FLAGS").expect("FLAGS symbol present");

    for (coll, label) in [
        ("rectangles", "rectangle"),
        ("round_rects", "round_rect"),
        ("ellipses", "ellipse"),
        ("lines", "line"),
        ("polylines", "polyline"),
        ("polygons", "polygon"),
        ("arcs", "arc"),
        ("labels", "label"),
        ("parameters", "parameter"),
    ] {
        let shapes = arr(sym, coll);
        assert_eq!(shapes.len(), 1, "1 {label} survived");
        let sh = &shapes[0];
        assert!(b(sh, "graphically_locked"), "{label} graphically_locked");
        assert!(b(sh, "disabled"), "{label} disabled");
        assert!(b(sh, "dimmed"), "{label} dimmed");
        assert_eq!(i(sh, "owner_part_display_mode"), 1, "{label} owner_part_display_mode");
    }
}

#[test]
fn write_schlib_parameter_display_fields_roundtrip() {
    let mut h = Harness::start();
    let lib = h.schlib_path();
    let symbol = json!({
        "name": "PARAMFIELDS",
        "pins": [{
            "designator": "1", "name": "P1", "x": -50, "y": 0, "length": 30, "orientation": "left",
        }],
        "parameters": [{
            "name": "Value", "value": "10k", "read_only_state": 1, "param_type": 2,
            "unique_id": "WXYZ7890", "orientation": 3, "show_name": true, "hide_name": true,
            "description": "Resistance", "is_configurable": true,
        }],
    });
    let write = h.call_tool(
        "write_schlib",
        json!({ "filepath": lib, "symbols": [symbol], "append": false }),
    );
    assert!(!is_err(&write), "write_schlib (parameter display fields) succeeded: {write}");

    let read = h.call_tool("read_schlib", json!({ "filepath": lib }));
    let sym = find_by(arr(&read, "symbols"), "name", "PARAMFIELDS").expect("present");
    let val = find_by(arr(sym, "parameters"), "name", "Value").expect("Value parameter");
    assert_eq!(i(val, "read_only_state"), 1, "parameter read_only_state");
    assert_eq!(i(val, "param_type"), 2, "parameter param_type");
    assert_eq!(s(val, "unique_id"), "WXYZ7890", "parameter unique_id preserved");
    assert_eq!(i(val, "orientation"), 3, "parameter orientation");
    assert!(b(val, "show_name"), "parameter show_name");
    assert!(b(val, "hide_name"), "parameter hide_name");
    assert_eq!(s(val, "description"), "Resistance", "parameter description");
    assert!(b(val, "is_configurable"), "parameter is_configurable");
}

#[test]
fn write_schlib_utf8_text_roundtrip() {
    let mut h = Harness::start();
    let lib = h.schlib_path();
    let omega = "10kΩ"; // Greek capital omega — not in Windows-1252
    let cyrillic = "Привет";
    let cjk = "抄抗器";
    let symbol = json!({
        "name": "UTF8SYM",
        "designator": cjk,
        "pins": [{
            "designator": "1", "name": "P1", "x": -50, "y": 0, "length": 30, "orientation": "left",
        }],
        "labels": [{ "x": 0, "y": 40, "text": cyrillic }],
        "parameters": [{ "name": "Value", "value": omega }],
    });
    let write = h.call_tool(
        "write_schlib",
        json!({ "filepath": lib, "symbols": [symbol], "append": false }),
    );
    assert!(!is_err(&write), "write_schlib (UTF-8 text) succeeded: {write}");

    let read = h.call_tool("read_schlib", json!({ "filepath": lib }));
    let sym = find_by(arr(&read, "symbols"), "name", "UTF8SYM").expect("UTF8SYM present");
    let val = find_by(arr(sym, "parameters"), "name", "Value").expect("Value parameter");
    assert_eq!(s(val, "value"), omega, "parameter Ω value survives UTF-8 round-trip");
    assert!(
        arr(sym, "labels").iter().any(|l| s(l, "text") == cyrillic),
        "label Cyrillic value survives UTF-8 round-trip"
    );
    assert_eq!(s(sym, "designator"), cjk, "designator CJK value survives UTF-8 round-trip");
}

#[test]
fn write_schlib_unique_id_roundtrip() {
    let mut h = Harness::start();
    let lib = h.schlib_path();
    let symbol = json!({
        "name": "UID_SYM",
        "designator_prefix": "U",
        "pins": [{
            "designator": "1", "name": "1", "x": -50, "y": 0, "length": 20,
            "orientation": "left", "electrical_type": "passive",
        }],
        "rectangles": [{ "x1": -30, "y1": -20, "x2": 30, "y2": 20, "unique_id": "RECTUID4" }],
        "labels": [{ "x": 0, "y": 25, "text": "LBL", "unique_id": "LBLUID05" }],
    });
    let write = h.call_tool(
        "write_schlib",
        json!({ "filepath": lib, "symbols": [symbol], "append": false }),
    );
    assert!(!is_err(&write), "write_schlib (unique_id) succeeded: {write}");

    let read = h.call_tool("read_schlib", json!({ "filepath": lib }));
    let sym = find_by(arr(&read, "symbols"), "name", "UID_SYM").expect("present");
    let rects = arr(sym, "rectangles");
    assert!(!rects.is_empty(), "rectangle survived");
    assert_eq!(s(&rects[0], "unique_id"), "RECTUID4", "rectangle unique_id preserved");
    let lbl = find_by(arr(sym, "labels"), "text", "LBL").expect("LBL label survived");
    assert_eq!(s(lbl, "unique_id"), "LBLUID05", "label unique_id preserved");
}

#[test]
fn write_schlib_pin_aux_data_roundtrip() {
    let mut h = Harness::start();
    let lib = h.schlib_path();
    let symbol = json!({
        "name": "PINAUX",
        "pins": [
            {
                "designator": "0", "name": "P0", "x": -50, "y": 0, "length": 30,
                "orientation": "left",
            },
            {
                "designator": "1", "name": "PA", "x": -50, "y": 20, "length": 30,
                "orientation": "left", "owner_part_display_mode": 2, "symbol_line_width": 3,
                "frac": { "x": 50000, "y": -25000, "length": 12345 },
            },
        ],
    });
    let write = h.call_tool(
        "write_schlib",
        json!({ "filepath": lib, "symbols": [symbol], "append": false }),
    );
    assert!(!is_err(&write), "write_schlib (pin aux) succeeded: {write}");

    let read = h.call_tool("read_schlib", json!({ "filepath": lib }));
    let sym = find_by(arr(&read, "symbols"), "name", "PINAUX").expect("present");
    let p0 = find_by(arr(sym, "pins"), "name", "P0").expect("P0 pin");
    let pa = find_by(arr(sym, "pins"), "name", "PA").expect("PA pin");

    assert_eq!(p0.get("owner_part_display_mode").and_then(Value::as_i64).unwrap_or(0), 0, "default pin owner_part_display_mode");
    assert_eq!(p0.get("symbol_line_width").and_then(Value::as_i64).unwrap_or(0), 0, "default pin symbol_line_width");
    assert!(p0.get("frac").map_or(true, Value::is_null), "default pin has no frac");

    assert_eq!(i(pa, "owner_part_display_mode"), 2, "aux pin owner_part_display_mode");
    assert_eq!(i(pa, "symbol_line_width"), 3, "aux pin symbol_line_width");
    let frac = &pa["frac"];
    assert!(
        i(frac, "x") == 50000 && i(frac, "y") == -25000 && i(frac, "length") == 12345,
        "aux pin frac (x, y, length) round-trip"
    );
}

#[test]
fn write_schlib_embedded_image_bytes_roundtrip() {
    let mut h = Harness::start();
    let lib = h.schlib_path();
    let img_b64 = base64::engine::general_purpose::STANDARD.encode(b"BM_fake_bitmap_payload_0123456789");
    let symbol = json!({
        "name": "EMBIMG",
        "images": [
            {
                "x1": -5, "y1": -3, "x2": 5, "y2": 3, "keep_aspect": true, "embed_image": true,
                "file_name": "C:\\img\\embed.bmp", "image_data": img_b64,
            },
            { "x1": 0, "y1": 10, "x2": 5, "y2": 13, "file_name": "linked.png" },
        ],
    });
    let write = h.call_tool(
        "write_schlib",
        json!({ "filepath": lib, "symbols": [symbol], "append": false }),
    );
    assert!(!is_err(&write), "write_schlib (embedded image) succeeded: {write}");

    let read = h.call_tool("read_schlib", json!({ "filepath": lib }));
    let sym = find_by(arr(&read, "symbols"), "name", "EMBIMG").expect("present");
    let embedded =
        find_by(arr(sym, "images"), "file_name", "C:\\img\\embed.bmp").expect("embedded image");
    let linked = find_by(arr(sym, "images"), "file_name", "linked.png").expect("linked image");
    assert!(b(embedded, "embed_image"), "embedded image keeps embed_image");
    assert_eq!(s(embedded, "image_data"), img_b64, "embedded image bytes round-trip as base64");
    assert!(linked.get("image_data").map_or(true, Value::is_null), "linked image carries no image_data");
}

#[test]
fn update_schlib_preserves_pies_images() {
    let mut h = Harness::start();
    let lib = h.schlib_path();
    let payload_b64 = base64::engine::general_purpose::STANDARD.encode(b"BM_pieimg_payload");
    let mut symbol = json!({
        "name": "PIEIMG",
        "part_count": 2,
        "pies": [{ "x": 0, "y": 0, "radius": 5, "start_angle": 30.0, "end_angle": 210.0 }],
        "images": [{
            "x1": -5, "y1": -3, "x2": 5, "y2": 3, "file_name": "logo.bmp",
            "embed_image": true, "image_data": payload_b64,
        }],
        "footprints": [{ "name": "RESC1608X55N" }],
    });
    let write = h.call_tool(
        "write_schlib",
        json!({ "filepath": lib, "symbols": [symbol.clone()], "append": false }),
    );
    assert!(!is_err(&write), "write_schlib (pie+image) succeeded: {write}");

    symbol["description"] = json!("after");
    let upd = h.call_tool(
        "update_component",
        json!({ "filepath": lib, "component_name": "PIEIMG", "symbol": symbol }),
    );
    assert!(!is_err(&upd), "update_component succeeded: {upd}");

    let read = h.call_tool("read_schlib", json!({ "filepath": lib }));
    let sym = find_by(arr(&read, "symbols"), "name", "PIEIMG").expect("present");
    assert_eq!(len_of(sym, "pies"), 1, "pie survived update_component");
    assert_eq!(len_of(sym, "images"), 1, "image survived update_component");
    assert_eq!(
        s(&arr(sym, "images")[0], "image_data"),
        payload_b64,
        "embedded image bytes survived update_component (RMW preservation)"
    );
    assert_eq!(len_of(sym, "footprints"), 1, "footprint link survived");
    assert_eq!(i(sym, "part_count"), 2, "part_count survived");
    assert_eq!(s(sym, "description"), "after", "description was updated");
}

#[test]
fn bulk_rename_chained_no_loss() {
    let mut h = Harness::start();
    let lib = h.schlib_path();
    let write = h.call_tool(
        "write_schlib",
        json!({
            "filepath": lib,
            "symbols": [{ "name": "AA" }, { "name": "AAA" }],
            "append": false,
        }),
    );
    assert!(!is_err(&write), "write_schlib (AA, AAA) succeeded: {write}");

    let rn = h.call_tool(
        "bulk_rename",
        json!({ "filepath": lib, "pattern": "^A", "replacement": "" }),
    );
    assert!(!is_err(&rn), "bulk_rename succeeded: {rn}");

    let read = h.call_tool("read_schlib", json!({ "filepath": lib }));
    let mut names: Vec<&str> = arr(&read, "symbols").iter().map(|sm| s(sm, "name")).collect();
    names.sort_unstable();
    // AA->A and AAA->AA: two distinct symbols, neither clobbered.
    assert_eq!(names, ["A", "AA"], "both symbols survive the chained rename");
}

// ============================================================================
// Read fidelity of committed sample fixtures (collections the write path
// cannot author).
// ============================================================================

#[test]
fn read_pcblib_exposes_vias_and_fills() {
    let mut h = Harness::start();
    let sample = Harness::sample("footprints.PcbLib");
    let read = h.call_tool("read_pcblib", json!({ "filepath": sample }));
    assert!(!is_err(&read), "read_pcblib succeeded: {read}");
    let footprints = arr(&read, "footprints");

    let vias_fp = find_by(footprints, "name", "VIAS").expect("VIAS footprint");
    assert!(vias_fp.get("vias").is_some(), "VIAS footprint has 'vias' field");
    assert_eq!(len_of(vias_fp, "vias"), 2, "VIAS footprint exposes 2 vias");

    let fills_fp = find_by(footprints, "name", "FILLS").expect("FILLS footprint");
    assert!(fills_fp.get("fills").is_some(), "FILLS footprint has 'fills' field");
    assert_eq!(len_of(fills_fp, "fills"), 2, "FILLS footprint exposes 2 fills");

    let shapes_fp = find_by(footprints, "name", "PAD_SHAPES").expect("PAD_SHAPES footprint");
    assert!(shapes_fp.get("vias").is_some(), "PAD_SHAPES footprint has 'vias' field");
    assert!(shapes_fp.get("fills").is_some(), "PAD_SHAPES footprint has 'fills' field");

    let got = h.call_tool("get_component", json!({ "filepath": sample, "component_name": "VIAS" }));
    assert!(!is_err(&got), "get_component(VIAS) succeeded: {got}");
    assert_eq!(len_of(&got["component"], "vias"), 2, "get_component(VIAS) exposes 2 vias");
}

#[test]
fn read_schlib_exposes_round_rects_and_polygons() {
    let mut h = Harness::start();
    let sample = Harness::sample("symbols.SchLib");
    let read = h.call_tool("read_schlib", json!({ "filepath": sample }));
    assert!(!is_err(&read), "read_schlib succeeded: {read}");
    let symbols = arr(&read, "symbols");

    let rr = find_by(symbols, "name", "ROUNDRECTS").expect("ROUNDRECTS symbol");
    assert!(rr.get("round_rects").is_some(), "ROUNDRECTS has 'round_rects' field");
    assert_eq!(len_of(rr, "round_rects"), 1, "ROUNDRECTS exposes 1 round_rect");

    let pg = find_by(symbols, "name", "POLYGONS").expect("POLYGONS symbol");
    assert!(pg.get("polygons").is_some(), "POLYGONS has 'polygons' field");
    assert_eq!(len_of(pg, "polygons"), 2, "POLYGONS exposes 2 polygons");

    let pins = find_by(symbols, "name", "PINS_ETYPE").expect("PINS_ETYPE symbol");
    assert!(pins.get("round_rects").is_some(), "PINS_ETYPE has 'round_rects' field");
    assert!(pins.get("polygons").is_some(), "PINS_ETYPE has 'polygons' field");
}

/// Collects an `additional_parameters` array of `[key, value]` pairs.
fn pairs(v: &Value, key: &str) -> Vec<(String, String)> {
    arr(v, key)
        .iter()
        .filter_map(|p| {
            let a = p.as_array()?;
            Some((a.first()?.as_str()?.to_owned(), a.get(1)?.as_str()?.to_owned()))
        })
        .collect()
}
