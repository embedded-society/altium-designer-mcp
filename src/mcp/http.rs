//! The MCP Streamable HTTP transport (protocol revisions 2025-03-26 and later).
//!
//! One endpoint, `/mcp`, carries every message:
//!
//! - `POST` takes one JSON-RPC message. A request is answered with its
//!   JSON-RPC response as `application/json`; a notification with `202
//!   Accepted`. The `initialize` request opens a session, whose id comes back
//!   in the `Mcp-Session-Id` header and must accompany every later message.
//! - `GET` would open a server-to-client event stream; this server sends no
//!   unsolicited messages, so it answers `405 Method Not Allowed`, as the
//!   specification allows.
//! - `DELETE` ends the session named in `Mcp-Session-Id`.
//!
//! The server writes files, so it is guarded before a message is read: the
//! listener binds to loopback unless told otherwise and then insists on a
//! bearer token; a browser's `Origin` must be a local one or explicitly
//! allowed, which is what stops a web page from reaching the server through
//! DNS rebinding; bodies are capped; and every session draws on one rate
//! limiter. Messages are handled one at a time, off the async runtime, so two
//! clients can never write the same library at once.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use http_body_util::{BodyExt, Full, Limited};
use hyper::body::{Bytes, Incoming};
use hyper::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE};
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

use crate::mcp::protocol::SUPPORTED_PROTOCOL_VERSIONS;
use crate::mcp::server::McpServer;

/// The path the transport answers on.
pub const ENDPOINT: &str = "/mcp";

/// The largest message body accepted. A whole library written in one
/// `write_pcblib` call is the biggest legitimate message.
pub const MAX_BODY_BYTES: usize = 32 * 1024 * 1024;

/// The environment variable the bearer token is read from — never a
/// command-line argument, which other users of the machine can list.
pub const TOKEN_ENV: &str = "ALTIUM_DESIGNER_MCP_HTTP_TOKEN";

const SESSION_HEADER: &str = "mcp-session-id";
const PROTOCOL_HEADER: &str = "mcp-protocol-version";

/// How the HTTP transport listens and whom it lets in.
#[derive(Debug, Clone)]
pub struct HttpOptions {
    /// The address to listen on.
    pub addr: SocketAddr,
    /// The bearer token every request must carry, if any. Required when
    /// `addr` is not a loopback address.
    pub token: Option<String>,
    /// Browser origins allowed besides the local ones (`http://localhost`,
    /// `http://127.0.0.1`, `http://[::1]`, any port), compared exactly.
    pub allowed_origins: Vec<String>,
}

impl HttpOptions {
    /// Refuses a configuration that would expose the server unauthenticated.
    ///
    /// # Errors
    ///
    /// Returns a message when `addr` is not a loopback address and no token is
    /// set, or when the token is empty.
    pub fn validate(&self) -> Result<(), String> {
        if self.token.as_deref().is_some_and(str::is_empty) {
            return Err(format!("{TOKEN_ENV} is set but empty"));
        }
        if !self.addr.ip().is_loopback() && self.token.is_none() {
            return Err(format!(
                "listening on {} would expose the server beyond this machine; set {TOKEN_ENV} \
                 to a secret bearer token first, and put a TLS proxy in front of it",
                self.addr
            ));
        }
        Ok(())
    }
}

/// Builds a fresh server for a new session.
pub type ServerFactory = Arc<dyn Fn() -> McpServer + Send + Sync>;

/// The transport's shared state.
struct State {
    options: HttpOptions,
    factory: ServerFactory,
    sessions: Mutex<HashMap<String, McpServer>>,
}

/// Binds `options.addr` and serves until Ctrl+C.
///
/// # Errors
///
/// Returns an error when the options are refused or the address cannot be
/// bound.
pub async fn serve(options: HttpOptions, factory: ServerFactory) -> std::io::Result<()> {
    options.validate().map_err(std::io::Error::other)?;
    let listener = TcpListener::bind(options.addr).await?;
    tracing::info!(addr = %listener.local_addr()?, "Streamable HTTP transport listening on {ENDPOINT}");
    tokio::select! {
        result = serve_on(listener, options, factory) => result,
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received Ctrl+C, shutting down the HTTP transport");
            Ok(())
        }
    }
}

/// Serves connections accepted on `listener` — the part of [`serve`] tests
/// drive on an ephemeral port.
///
/// # Errors
///
/// Returns an error when the options are refused or accepting fails.
pub async fn serve_on(
    listener: TcpListener,
    options: HttpOptions,
    factory: ServerFactory,
) -> std::io::Result<()> {
    options.validate().map_err(std::io::Error::other)?;
    let state = Arc::new(State {
        options,
        factory,
        sessions: Mutex::new(HashMap::new()),
    });
    loop {
        let (stream, peer) = listener.accept().await?;
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            let service = hyper::service::service_fn(move |req| {
                let state = Arc::clone(&state);
                async move { Ok::<_, std::convert::Infallible>(handle(&state, req).await) }
            });
            if let Err(e) = hyper::server::conn::http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service)
                .await
            {
                tracing::debug!(%peer, error = %e, "HTTP connection ended with an error");
            }
        });
    }
}

/// The most of a refused request's body read before answering. A refusal is
/// sent before the body is read, and a socket closed with unread data in it
/// is reset — on macOS before the client has read the answer — so up to this
/// much is read and dropped first. A larger body is not worth reading for a
/// refusal; its sender may then see the reset instead.
const REFUSAL_DRAIN_BYTES: usize = 64 * 1024;

/// Answers one HTTP request.
async fn handle(state: &Arc<State>, req: Request<Incoming>) -> Response<Full<Bytes>> {
    if let Some(refusal) = refusal_before_body(state, &req) {
        let _ = Limited::new(req.into_body(), REFUSAL_DRAIN_BYTES)
            .collect()
            .await;
        return refusal;
    }
    match *req.method() {
        Method::POST => post(state, req).await,
        _ => delete(state, req.headers()),
    }
}

/// Everything a request can be refused for from its method, path and headers
/// alone: the path, the bearer token, the origin, the method, and a POST's
/// media types and protocol version.
fn refusal_before_body(
    state: &Arc<State>,
    req: &Request<Incoming>,
) -> Option<Response<Full<Bytes>>> {
    if req.uri().path() != ENDPOINT {
        return Some(status(
            StatusCode::NOT_FOUND,
            "no MCP endpoint here; use /mcp",
        ));
    }
    if let Some(refusal) = check_access(&state.options, req.headers()) {
        return Some(refusal);
    }
    match *req.method() {
        Method::POST => refusal_of_post_headers(req.headers()),
        Method::DELETE => None,
        _ => {
            let mut response = status(
                StatusCode::METHOD_NOT_ALLOWED,
                "this server sends no event stream; POST messages, DELETE to end a session",
            );
            response
                .headers_mut()
                .insert("allow", HeaderValue::from_static("POST, DELETE"));
            Some(response)
        }
    }
}

/// The bearer token and the browser origin, checked before anything else.
fn check_access(options: &HttpOptions, headers: &HeaderMap) -> Option<Response<Full<Bytes>>> {
    if let Some(token) = &options.token {
        let presented = headers
            .get(hyper::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));
        if !presented.is_some_and(|p| constant_time_eq(p.as_bytes(), token.as_bytes())) {
            let mut response = status(StatusCode::UNAUTHORIZED, "a valid bearer token is required");
            response
                .headers_mut()
                .insert("www-authenticate", HeaderValue::from_static("Bearer"));
            return Some(response);
        }
    }
    if let Some(origin) = headers.get(hyper::header::ORIGIN) {
        let allowed = origin
            .to_str()
            .is_ok_and(|o| origin_is_allowed(o, &options.allowed_origins));
        if !allowed {
            return Some(status(StatusCode::FORBIDDEN, "origin not allowed"));
        }
    }
    None
}

/// Whether a browser `Origin` may reach the server: a local one on any port,
/// or one listed exactly (ignoring case).
fn origin_is_allowed(origin: &str, allowed: &[String]) -> bool {
    if allowed.iter().any(|a| a.eq_ignore_ascii_case(origin)) {
        return true;
    }
    let Some(rest) = origin
        .strip_prefix("http://")
        .or_else(|| origin.strip_prefix("https://"))
    else {
        return false;
    };
    let host = if rest.starts_with('[') {
        rest.split_once(']')
            .map_or(rest, |(h, _)| h)
            .trim_start_matches('[')
    } else {
        rest.split(':').next().unwrap_or(rest)
    };
    host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "::1"
}

/// Compares two byte strings in time independent of where they differ.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// A POST's media types and protocol version, checked before its body is read.
fn refusal_of_post_headers(headers: &HeaderMap) -> Option<Response<Full<Bytes>>> {
    if !header_allows(headers, CONTENT_TYPE, false) {
        return Some(status(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "send application/json",
        ));
    }
    if headers.contains_key(ACCEPT) && !header_allows(headers, ACCEPT, true) {
        return Some(status(
            StatusCode::NOT_ACCEPTABLE,
            "this server answers with application/json",
        ));
    }
    if let Some(version) = headers.get(PROTOCOL_HEADER) {
        let known = version
            .to_str()
            .is_ok_and(|v| SUPPORTED_PROTOCOL_VERSIONS.contains(&v));
        if !known {
            return Some(status(
                StatusCode::BAD_REQUEST,
                "unsupported MCP-Protocol-Version",
            ));
        }
    }
    None
}

/// Handles a `POST` whose headers passed: reads and answers the message.
async fn post(state: &Arc<State>, req: Request<Incoming>) -> Response<Full<Bytes>> {
    let headers = req.headers().clone();
    let body = match Limited::new(req.into_body(), MAX_BODY_BYTES)
        .collect()
        .await
    {
        Ok(collected) => collected.to_bytes(),
        Err(_) => return status(StatusCode::PAYLOAD_TOO_LARGE, "message too large"),
    };
    let Ok(text) = String::from_utf8(body.to_vec()) else {
        return status(StatusCode::BAD_REQUEST, "the message is not UTF-8");
    };
    let is_initialize = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .is_some_and(|v| v.get("method").and_then(serde_json::Value::as_str) == Some("initialize"));
    let session = headers
        .get(SESSION_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    if !is_initialize {
        let Some(id) = &session else {
            return status(
                StatusCode::BAD_REQUEST,
                "missing Mcp-Session-Id; initialize first",
            );
        };
        if !state.sessions.lock().is_ok_and(|s| s.contains_key(id)) {
            return status(
                StatusCode::NOT_FOUND,
                "unknown or ended session; initialize again",
            );
        }
    }

    // Tool calls read and write files: run them off the runtime, one at a
    // time.
    let state = Arc::clone(state);
    let outcome = tokio::task::spawn_blocking(move || {
        if is_initialize {
            let mut server = (state.factory)();
            let reply = server.handle_message_text(&text);
            let opened = reply.as_deref().is_some_and(|r| r.contains("\"result\""));
            let id = opened.then(|| uuid::Uuid::new_v4().simple().to_string());
            if let Some(id) = &id {
                if let Ok(mut sessions) = state.sessions.lock() {
                    sessions.insert(id.clone(), server);
                }
            }
            (reply, id)
        } else {
            let id = session.unwrap_or_default();
            let reply = state
                .sessions
                .lock()
                .ok()
                .and_then(|mut sessions| {
                    sessions.get_mut(&id).map(|s| s.handle_message_text(&text))
                })
                .flatten();
            (reply, None)
        }
    })
    .await;

    match outcome {
        Ok((Some(reply), new_session)) => {
            let mut response = Response::new(Full::new(Bytes::from(reply)));
            response
                .headers_mut()
                .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            if let Some(id) = new_session.and_then(|id| HeaderValue::from_str(&id).ok()) {
                response.headers_mut().insert(SESSION_HEADER, id);
            }
            response
        }
        Ok((None, _)) => {
            let mut response = Response::new(Full::new(Bytes::new()));
            *response.status_mut() = StatusCode::ACCEPTED;
            response
        }
        Err(e) => {
            tracing::error!(error = %e, "an MCP message handler panicked");
            status(StatusCode::INTERNAL_SERVER_ERROR, "internal error")
        }
    }
}

/// Handles a `DELETE`: ends the session.
fn delete(state: &Arc<State>, headers: &HeaderMap) -> Response<Full<Bytes>> {
    let Some(id) = headers.get(SESSION_HEADER).and_then(|v| v.to_str().ok()) else {
        return status(StatusCode::BAD_REQUEST, "missing Mcp-Session-Id");
    };
    let removed = state
        .sessions
        .lock()
        .ok()
        .and_then(|mut sessions| sessions.remove(id));
    if removed.is_some() {
        let mut response = Response::new(Full::new(Bytes::new()));
        *response.status_mut() = StatusCode::NO_CONTENT;
        response
    } else {
        status(StatusCode::NOT_FOUND, "unknown or ended session")
    }
}

/// Whether a `Content-Type` (or, with `wildcards`, an `Accept`) header admits
/// JSON.
fn header_allows(headers: &HeaderMap, name: hyper::header::HeaderName, wildcards: bool) -> bool {
    headers.get_all(name).iter().any(|value| {
        value.to_str().is_ok_and(|v| {
            v.split(',').any(|item| {
                let media = item.split(';').next().unwrap_or("").trim();
                media.eq_ignore_ascii_case("application/json")
                    || (wildcards
                        && (media == "*/*" || media.eq_ignore_ascii_case("application/*")))
            })
        })
    })
}

/// A plain-text response with the given status.
fn status(code: StatusCode, message: &'static str) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from_static(message.as_bytes())));
    *response.status_mut() = code;
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_origins_are_allowed_and_others_are_not() {
        let none: Vec<String> = Vec::new();
        for ok in [
            "http://localhost",
            "http://localhost:3000",
            "https://127.0.0.1:8443",
            "http://[::1]:9000",
        ] {
            assert!(origin_is_allowed(ok, &none), "{ok}");
        }
        for bad in [
            "http://evil.example",
            "http://localhost.evil.example",
            "null",
            "file://",
            "http://127.0.0.2",
        ] {
            assert!(!origin_is_allowed(bad, &none), "{bad}");
        }
        let listed = vec!["https://claude.ai".to_string()];
        assert!(origin_is_allowed("https://Claude.ai", &listed));
    }

    #[test]
    fn a_non_loopback_bind_needs_a_token() {
        let mut options = HttpOptions {
            addr: "0.0.0.0:8080".parse().unwrap(),
            token: None,
            allowed_origins: Vec::new(),
        };
        assert!(options.validate().is_err());
        options.token = Some(String::new());
        assert!(options.validate().is_err(), "an empty token is no token");
        options.token = Some("secret".to_string());
        assert!(options.validate().is_ok());
        options.addr = "127.0.0.1:8080".parse().unwrap();
        options.token = None;
        assert!(options.validate().is_ok(), "loopback needs none");
    }

    #[test]
    fn token_comparison_needs_an_exact_match() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secreT"));
        assert!(!constant_time_eq(b"secret", b"secret2"));
    }
}
