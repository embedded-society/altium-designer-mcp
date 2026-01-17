//! Integration tests for MCP protocol handling.
//!
//! These tests verify the MCP server's JSON-RPC 2.0 protocol implementation,
//! including request/response handling, error responses, and lifecycle management.

use altium_designer_mcp::mcp::protocol::{parse_message, IncomingMessage, RequestId};

// =============================================================================
// Protocol Parsing Tests
// =============================================================================

#[test]
fn test_parse_initialize_request() {
    let json = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "test-client",
                "version": "1.0.0"
            }
        }
    }"#;

    let result = parse_message(json);
    assert!(result.is_ok());

    if let IncomingMessage::Request(req) = result.unwrap() {
        assert_eq!(req.method, "initialize");
        assert_eq!(req.id, RequestId::Number(1));
    } else {
        panic!("Expected Request");
    }
}

#[test]
fn test_parse_tools_list_request() {
    let json = r#"{
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    }"#;

    let result = parse_message(json);
    assert!(result.is_ok());

    if let IncomingMessage::Request(req) = result.unwrap() {
        assert_eq!(req.method, "tools/list");
        assert_eq!(req.id, RequestId::Number(2));
    } else {
        panic!("Expected Request");
    }
}

#[test]
fn test_parse_notification() {
    let json = r#"{
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    }"#;

    let result = parse_message(json);
    assert!(result.is_ok());

    if let IncomingMessage::Notification(notif) = result.unwrap() {
        assert_eq!(notif.method, "notifications/initialized");
    } else {
        panic!("Expected Notification");
    }
}

#[test]
fn test_parse_invalid_json() {
    let json = "not valid json";

    let result = parse_message(json);
    assert!(result.is_err());
}

#[test]
fn test_parse_missing_jsonrpc_version() {
    let json = r#"{
        "id": 1,
        "method": "test"
    }"#;

    let result = parse_message(json);
    assert!(result.is_err());
}
