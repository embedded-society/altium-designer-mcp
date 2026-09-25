//! The Streamable HTTP transport, driven over real TCP on an ephemeral port:
//! the session lifecycle, every refusal, and the rate limiter the sessions
//! share.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use altium_designer_mcp::mcp::http::{self, HttpOptions, ServerFactory};
use altium_designer_mcp::mcp::server::McpServer;
use altium_designer_mcp::security::RateLimiter;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// A parsed HTTP response: status, lower-cased headers, body.
struct Reply {
    status: u16,
    headers: HashMap<String, String>,
    body: String,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or_else(|e| panic!("{e}: {}", self.body))
    }
}

/// Starts the transport on 127.0.0.1 with the given token, allowed origins and
/// rate limiter, and returns its address.
async fn start(
    token: Option<&str>,
    origins: &[&str],
    limiter: RateLimiter,
    dir: &std::path::Path,
) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let limiter = Arc::new(limiter);
    let root = dir.to_path_buf();
    let factory: ServerFactory = Arc::new(move || {
        McpServer::new(vec![root.clone()]).with_shared_rate_limiter(Arc::clone(&limiter))
    });
    let options = HttpOptions {
        addr,
        token: token.map(str::to_string),
        allowed_origins: origins.iter().map(|o| (*o).to_string()).collect(),
    };
    tokio::spawn(http::serve_on(listener, options, factory));
    addr
}

/// Sends one HTTP/1.1 request and reads the whole response.
async fn send(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Reply {
    send_bytes(addr, method, path, headers, body.as_bytes()).await
}

/// As [`send`], with a body that need not be text.
async fn send_bytes(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Reply {
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (name, value) in headers {
        request.push_str(name);
        request.push_str(": ");
        request.push_str(value);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("write head");
    stream.write_all(body).await.expect("write body");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.expect("read");
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .expect("status line");
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    Reply {
        status,
        headers,
        body: body.to_string(),
    }
}

/// POSTs a JSON-RPC message with the headers a conforming client sends.
async fn post(
    addr: SocketAddr,
    session: Option<&str>,
    message: &Value,
    extra: &[(&str, &str)],
) -> Reply {
    let mut headers = vec![
        ("Content-Type", "application/json"),
        ("Accept", "application/json, text/event-stream"),
    ];
    if let Some(id) = session {
        headers.push(("Mcp-Session-Id", id));
    }
    headers.extend_from_slice(extra);
    send(addr, "POST", "/mcp", &headers, &message.to_string()).await
}

fn initialize() -> Value {
    json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": {"name": "http-test", "version": "0"}}})
}

/// Initialises a session and returns its id.
async fn open_session(addr: SocketAddr, extra: &[(&str, &str)]) -> String {
    let reply = post(addr, None, &initialize(), extra).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let id = reply
        .headers
        .get("mcp-session-id")
        .expect("session id")
        .clone();
    let done = post(
        addr,
        Some(&id),
        &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        extra,
    )
    .await;
    assert_eq!(done.status, 202, "a notification is accepted with no body");
    id
}

#[tokio::test]
async fn a_session_initialises_lists_tools_and_ends() {
    let dir = tempfile::tempdir().unwrap();
    let addr = start(None, &[], RateLimiter::unlimited(), dir.path()).await;

    let reply = post(addr, None, &initialize(), &[]).await;
    assert_eq!(reply.status, 200);
    assert_eq!(
        reply.headers.get("content-type").map(String::as_str),
        Some("application/json")
    );
    assert_eq!(reply.json()["result"]["protocolVersion"], "2025-06-18");
    let id = reply
        .headers
        .get("mcp-session-id")
        .expect("session id")
        .clone();

    let init_done = post(
        addr,
        Some(&id),
        &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        &[],
    )
    .await;
    assert_eq!(init_done.status, 202);

    let tools = post(
        addr,
        Some(&id),
        &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        &[("MCP-Protocol-Version", "2025-06-18")],
    )
    .await;
    assert_eq!(tools.status, 200);
    assert_eq!(
        tools.json()["result"]["tools"].as_array().map(Vec::len),
        Some(34)
    );

    let end = send(addr, "DELETE", "/mcp", &[("Mcp-Session-Id", &id)], "").await;
    assert_eq!(end.status, 204);
    let after = post(
        addr,
        Some(&id),
        &json!({"jsonrpc": "2.0", "id": 3, "method": "ping"}),
        &[],
    )
    .await;
    assert_eq!(after.status, 404, "an ended session is unknown");
}

#[tokio::test]
async fn a_tool_call_writes_a_library_through_http() {
    let dir = tempfile::tempdir().unwrap();
    let addr = start(None, &[], RateLimiter::unlimited(), dir.path()).await;
    let id = open_session(addr, &[]).await;
    let lib = dir.path().join("Http.PcbLib");
    let call = json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {
    "name": "write_pcblib",
    "arguments": {"filepath": lib.to_string_lossy(), "footprints": [
        {"name": "R0402", "pads": [{"designator": "1", "x": 0.0, "y": 0.0, "width": 0.5, "height": 0.5}]}
    ]}}});
    let reply = post(addr, Some(&id), &call, &[]).await;
    assert_eq!(reply.status, 200);
    assert_ne!(reply.json()["result"]["isError"], true, "{}", reply.body);
    assert!(lib.exists(), "the library was written");
}

#[tokio::test]
async fn every_session_draws_on_one_rate_limiter() {
    let dir = tempfile::tempdir().unwrap();
    // One mutating call in total, no refill.
    let addr = start(None, &[], RateLimiter::new(1, 0.0), dir.path()).await;
    let write = |n: u32| {
        let lib = dir.path().join(format!("L{n}.PcbLib"));
        json!({"jsonrpc": "2.0", "id": n, "method": "tools/call", "params": {
        "name": "write_pcblib",
        "arguments": {"filepath": lib.to_string_lossy(), "footprints": [
            {"name": "F", "pads": [{"designator": "1", "x": 0.0, "y": 0.0, "width": 0.5, "height": 0.5}]}
        ]}}})
    };
    let first = open_session(addr, &[]).await;
    let second = open_session(addr, &[]).await;
    let ok = post(addr, Some(&first), &write(1), &[]).await;
    assert_ne!(ok.json()["result"]["isError"], true, "{}", ok.body);
    let refused = post(addr, Some(&second), &write(2), &[]).await;
    assert!(
        refused.body.to_ascii_lowercase().contains("rate limit"),
        "a second session cannot get round the limit: {}",
        refused.body
    );
}

#[tokio::test]
async fn requests_without_a_session_or_with_a_bad_one_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let addr = start(None, &[], RateLimiter::unlimited(), dir.path()).await;
    let ping = json!({"jsonrpc": "2.0", "id": 9, "method": "ping"});
    assert_eq!(post(addr, None, &ping, &[]).await.status, 400);
    assert_eq!(post(addr, Some("nope"), &ping, &[]).await.status, 404);
    assert_eq!(
        send(addr, "DELETE", "/mcp", &[("Mcp-Session-Id", "nope")], "")
            .await
            .status,
        404
    );
}

#[tokio::test]
async fn malformed_requests_are_refused_with_the_right_status() {
    let dir = tempfile::tempdir().unwrap();
    let addr = start(None, &[], RateLimiter::unlimited(), dir.path()).await;

    let get = send(addr, "GET", "/mcp", &[("Accept", "text/event-stream")], "").await;
    assert_eq!(get.status, 405);
    assert_eq!(
        get.headers.get("allow").map(String::as_str),
        Some("POST, DELETE")
    );

    assert_eq!(
        send(
            addr,
            "POST",
            "/other",
            &[("Content-Type", "application/json")],
            "{}"
        )
        .await
        .status,
        404
    );

    let text = send(
        addr,
        "POST",
        "/mcp",
        &[("Content-Type", "text/plain")],
        "{}",
    )
    .await;
    assert_eq!(text.status, 415);

    let accept = send(
        addr,
        "POST",
        "/mcp",
        &[
            ("Content-Type", "application/json"),
            ("Accept", "text/html"),
        ],
        &initialize().to_string(),
    )
    .await;
    assert_eq!(accept.status, 406);

    let version = post(
        addr,
        None,
        &initialize(),
        &[("MCP-Protocol-Version", "1999-01-01")],
    )
    .await;
    assert_eq!(version.status, 400);

    let huge = format!("\"{}\"", " ".repeat(http::MAX_BODY_BYTES));
    let too_large = send(
        addr,
        "POST",
        "/mcp",
        &[("Content-Type", "application/json")],
        &huge,
    )
    .await;
    assert_eq!(too_large.status, 413);
}

#[tokio::test]
async fn a_token_is_required_when_one_is_set() {
    let dir = tempfile::tempdir().unwrap();
    let addr = start(Some("s3cret"), &[], RateLimiter::unlimited(), dir.path()).await;
    let none = post(addr, None, &initialize(), &[]).await;
    assert_eq!(none.status, 401);
    assert_eq!(
        none.headers.get("www-authenticate").map(String::as_str),
        Some("Bearer")
    );
    assert_eq!(
        post(
            addr,
            None,
            &initialize(),
            &[("Authorization", "Bearer wrong")]
        )
        .await
        .status,
        401
    );
    assert_eq!(
        post(
            addr,
            None,
            &initialize(),
            &[("Authorization", "Bearer s3cret")]
        )
        .await
        .status,
        200
    );
}

#[tokio::test]
async fn a_foreign_browser_origin_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let addr = start(
        None,
        &["https://claude.ai"],
        RateLimiter::unlimited(),
        dir.path(),
    )
    .await;
    assert_eq!(
        post(
            addr,
            None,
            &initialize(),
            &[("Origin", "http://evil.example")]
        )
        .await
        .status,
        403
    );
    assert_eq!(
        post(
            addr,
            None,
            &initialize(),
            &[("Origin", "http://localhost:5173")]
        )
        .await
        .status,
        200
    );
    assert_eq!(
        post(
            addr,
            None,
            &initialize(),
            &[("Origin", "https://claude.ai")]
        )
        .await
        .status,
        200
    );
}

/// A body that is not UTF-8 is refused before it is parsed: the transport
/// answers 400 rather than letting the bytes reach the JSON reader.
#[tokio::test]
async fn a_body_that_is_not_utf8_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let addr = start(None, &[], RateLimiter::unlimited(), dir.path()).await;

    let reply = send_bytes(
        addr,
        "POST",
        "/mcp",
        &[
            ("Content-Type", "application/json"),
            ("Accept", "application/json"),
        ],
        &[b'{', 0xff, 0xfe, b'}'],
    )
    .await;
    assert_eq!(reply.status, 400);
    assert!(reply.body.contains("UTF-8"), "{}", reply.body);
}

/// `serve` refuses options that would expose the server before it binds
/// anything: a non-loopback address without a bearer token.
#[tokio::test]
async fn serve_refuses_options_that_would_expose_the_server() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let factory: ServerFactory = Arc::new(move || McpServer::new(vec![root.clone()]));
    let options = HttpOptions {
        addr: "0.0.0.0:0".parse().expect("addr"),
        token: None,
        allowed_origins: Vec::new(),
    };

    let error = http::serve(options, factory)
        .await
        .expect_err("a non-loopback address without a token is refused");
    assert!(error.to_string().contains("beyond this machine"), "{error}");
}
