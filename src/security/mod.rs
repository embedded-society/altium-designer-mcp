//! Security and safety controls.
//!
//! This module provides controls that protect against runaway or abusive use
//! of the file-mutating tools:
//!
//! - **Rate limiting**: a token bucket that caps the rate of destructive
//!   operations (writes, deletes, renames, batch updates), so a looping AI
//!   client cannot thrash the disk with repeated full-file rewrites + backups.
//!
//! - **Audit logging**: an append-only JSON-lines record of destructive
//!   operations, so there is a durable trail of which components were
//!   written/deleted/renamed and with what outcome.
//!
//! Path validation (keeping operations inside the configured `allowed_paths`)
//! lives with the server in [`crate::mcp`]; this module is for the additional,
//! self-contained safety primitives.

pub mod audit;
pub mod rate_limit;

pub use audit::{AuditEvent, AuditLogger, AuditOutcome};
pub use rate_limit::{RateLimiter, RateLimiterStats};
