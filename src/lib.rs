//! altium-designer-mcp: MCP server for AI-assisted Altium Designer library management
//!
//! This library provides file I/O and primitive placement tools that enable AI assistants
//! to create and manage Altium Designer component libraries.
//!
//! # Architecture
//!
//! The MCP server provides low-level primitives. The AI handles the intelligence:
//!
//! - **Altium File I/O**: Read/write `.PcbLib`, `.SchLib` files directly
//! - **Primitive Placement**: Pads, tracks, arcs, regions, text on standard Altium layers
//! - **STEP Model Attachment**: Link existing STEP files to footprints
//!
//! The AI (not this tool) handles:
//! - IPC-7351B calculations and compliance
//! - Package-specific layout decisions
//! - Style and design choices
//!
//! # Modules
//!
//! - [`config`] — Configuration loading and validation
//! - [`error`] — Error types
//! - [`mcp`] — MCP protocol implementation
//! - `altium` — Altium file format handling (TODO)

pub mod altium;
pub mod config;
pub mod error;
pub mod mcp;
