//! Per-domain MCP tool handlers and helpers, split out of `server.rs`.
//!
//! Each submodule adds an `impl McpServer` block. Method resolution is
//! independent of which file an `impl` lives in, so the dispatch in `server.rs`
//! (and the in-crate tests) call these methods unchanged via `Self::`/`self.`.
//! Helpers reached across modules are `pub(crate)`.

mod batch;
mod compare;
mod components;
mod diff;
mod library_ops;
mod maintenance;
mod parsing;
mod query_update;
mod read_write;
mod render;
mod schlib_manage;
mod step;
mod validation;
