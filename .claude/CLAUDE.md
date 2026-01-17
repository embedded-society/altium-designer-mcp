# altium-designer-mcp

MCP server for AI-assisted Altium Designer component library management with IPC-7351B compliance.

## Project Overview

This Rust MCP server enables AI assistants (Claude Code, Claude Desktop, VSCode Copilot) to create, read, and manage Altium Designer component libraries.

### Key Features (Planned)

- **IPC-7351B Calculations**: Generate compliant land patterns from package dimensions
- **Altium File I/O**: Read and write .PcbLib, .SchLib, .DbLib files
- **Style Management**: Extract and apply consistent component styles
- **CSV Database**: Manage component data with version-controlled CSV files
- **Symbol Generation**: Create schematic symbols from pin definitions

### Current Status

This is an early-stage project with placeholder implementations. The MCP server infrastructure is in place, but the actual Altium file I/O and IPC calculations are not yet implemented.

## Architecture

```
src/
├── main.rs          # CLI entry point
├── lib.rs           # Library root
├── config/          # Configuration loading
├── error.rs         # Error types
└── mcp/             # MCP server implementation
    ├── mod.rs
    ├── protocol.rs  # JSON-RPC 2.0 types
    ├── server.rs    # MCP server with tool handlers
    └── transport.rs # stdio transport
```

## Building

```bash
cargo build --release
```

## Running

```bash
# With library path
./target/release/altium-designer-mcp /path/to/libraries

# With config file
./target/release/altium-designer-mcp --config config.json
```

## MCP Tools (Currently Implemented)

1. **list_package_types** - Lists supported IPC-7351B package families
2. **calculate_footprint** - Calculates footprint from dimensions (placeholder)
3. **get_ipc_name** - Generates IPC-7351B compliant name (placeholder)

## Development Guidelines

### Code Style

- Use `cargo fmt` before committing
- All code must pass `cargo clippy` with pedantic lints
- No `unsafe` code allowed
- Tests required for new functionality

### Module Organization

- Keep modules focused and single-purpose
- Use `mod.rs` for module re-exports
- Document public APIs with rustdoc comments

### Error Handling

- Use `thiserror` for error types
- Provide meaningful error messages
- Never expose internal details in user-facing errors

### Testing

```bash
cargo test
cargo clippy -- -D warnings
```

## License

GPL-3.0-or-later
