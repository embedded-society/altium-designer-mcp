//! Shared fixtures for the colocated MCP tool handler tests.
//!
//! Compiled only under `cfg(test)`. Mirrors the helpers in the `server.rs`
//! test module so each `tools/*.rs` file can host its own focused test module
//! without duplicating fixture code.

use serde_json::Value;
use tempfile::TempDir;

use crate::altium::pcblib::{Footprint, Pad, PcbLib};
use crate::altium::schlib::{Pin, PinOrientation, Rectangle, SchLib, Symbol};
use crate::mcp::server::{McpServer, ToolCallResult, ToolContent};

/// Creates a temporary directory inside `.tmp/` for test isolation.
/// The directory is automatically cleaned up when the returned `TempDir` is dropped.
///
/// Uses an absolute path (canonicalised from the constant `.tmp`, which cargo
/// resolves against the crate root) to avoid issues with parallel test
/// execution. Deriving it from a constant rather than `current_dir()` also
/// avoids the spurious `rust/path-injection` taint CodeQL raises on this
/// test-only helper.
pub fn test_temp_dir() -> TempDir {
    std::fs::create_dir_all(".tmp").expect("Failed to create .tmp directory");
    let tmp_root = std::path::Path::new(".tmp")
        .canonicalize()
        .expect("Failed to canonicalise .tmp");
    tempfile::tempdir_in(tmp_root).expect("Failed to create temp dir")
}

/// Helper to create a server with a temp directory as the only allowed path.
pub fn create_test_server(temp_path: &std::path::Path) -> McpServer {
    McpServer::new(vec![temp_path.to_path_buf()])
}

/// Helper to create a test `PcbLib` with two sample footprints.
pub fn create_test_pcblib(path: &std::path::Path) {
    let mut lib = PcbLib::new();

    let mut fp1 = Footprint::new("CHIP_0402");
    fp1.description = "0402 chip resistor".to_string();
    fp1.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
    fp1.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));
    lib.add(fp1);

    let mut fp2 = Footprint::new("CHIP_0603");
    fp2.description = "0603 chip resistor".to_string();
    fp2.add_pad(Pad::smd("1", -0.8, 0.0, 0.8, 0.8));
    fp2.add_pad(Pad::smd("2", 0.8, 0.0, 0.8, 0.8));
    lib.add(fp2);

    lib.save(path).expect("Failed to create test PcbLib");
}

/// Helper to create a test `SchLib` with two sample symbols.
pub fn create_test_schlib(path: &std::path::Path) {
    let mut lib = SchLib::new();

    let mut sym1 = Symbol::new("RESISTOR");
    sym1.description = "Generic resistor".to_string();
    sym1.designator = "R?".to_string();
    sym1.add_pin(Pin::new("1", "1", -20, 0, 10, PinOrientation::Left));
    sym1.add_pin(Pin::new("2", "2", 20, 0, 10, PinOrientation::Right));
    sym1.add_rectangle(Rectangle::new(-10, -5, 10, 5));
    lib.add(sym1);

    let mut sym2 = Symbol::new("CAPACITOR");
    sym2.description = "Generic capacitor".to_string();
    sym2.designator = "C?".to_string();
    sym2.add_pin(Pin::new("1", "1", -20, 0, 10, PinOrientation::Left));
    sym2.add_pin(Pin::new("2", "2", 20, 0, 10, PinOrientation::Right));
    lib.add(sym2);

    lib.save(path).expect("Failed to create test SchLib");
}

/// Helper to extract the text payload from a tool result.
pub fn get_result_text(result: &ToolCallResult) -> &str {
    match &result.content[0] {
        ToolContent::Text { text } => text,
    }
}

/// Parses the JSON payload of a tool result, panicking with the raw text on
/// failure so a malformed response is easy to diagnose.
pub fn parse_result_json(result: &ToolCallResult) -> Value {
    let text = get_result_text(result);
    serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("tool result is not valid JSON ({e}): {text}"))
}
