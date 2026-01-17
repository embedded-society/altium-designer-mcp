//! altium-designer-mcp: MCP server for AI-assisted Altium Designer library management
//!
//! This library provides the core functionality for creating, reading, and managing
//! Altium Designer component libraries with full IPC-7351B compliance.
//!
//! # Architecture
//!
//! The MCP server enables AI assistants to:
//!
//! - Read existing Altium libraries (`.PcbLib`, `.SchLib`)
//! - Calculate IPC-7351B compliant footprints
//! - Generate complete components (footprint + symbol + parameters)
//! - Write native Altium files
//! - Manage CSV-based component databases
//!
//! # Modules
//!
//! - [`config`] — Configuration loading and validation
//! - [`error`] — Error types
//! - [`mcp`] — MCP protocol implementation
//! - `altium` — Altium file format handling (TODO)
//! - `ipc7351` — IPC-7351B land pattern calculations (TODO)
//! - `style` — Style extraction and application (TODO)
//! - `symbols` — Schematic symbol generation (TODO)
//! - `database` — CSV database management (TODO)

pub mod config;
pub mod error;
pub mod mcp;

// TODO: Implement these modules
// pub mod altium;
// pub mod ipc7351;
// pub mod style;
// pub mod symbols;
// pub mod database;
