//! Model Context Protocol (MCP) server implementation.
//!
//! This module implements the MCP specification for exposing Altium Designer
//! library management operations as tools to AI assistants. The server
//! communicates over stdio transport using JSON-RPC 2.0 messages.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                         MCP Server                          │
//! │                                                             │
//! │   ┌─────────────┐    ┌─────────────┐    ┌─────────────┐    │
//! │   │  Transport  │───▶│   Server    │───▶│   Tools     │    │
//! │   │   (stdio)   │    │  (lifecycle)│    │  (handlers) │    │
//! │   └─────────────┘    └─────────────┘    └─────────────┘    │
//! │          │                  │                  │            │
//! │          ▼                  ▼                  ▼            │
//! │   ┌─────────────────────────────────────────────────┐      │
//! │   │              JSON-RPC Messages                  │      │
//! │   └─────────────────────────────────────────────────┘      │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Protocol Version
//!
//! This implementation targets MCP protocol version 2024-11-05.

pub mod protocol;
pub mod server;
pub mod transport;

pub use protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse, MCP_PROTOCOL_VERSION};
pub use server::McpServer;
pub use transport::StdioTransport;
