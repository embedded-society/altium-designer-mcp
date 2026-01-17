# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **Major architectural refactor**: Shifted from calculator-based to primitive-based approach
    - Removed IPC-7351B calculators from the tool (AI now handles calculations)
    - Tool now provides pure file I/O and primitive placement
    - Any footprint type can now be created (not limited to pre-programmed packages)

### Removed

- `src/ipc7351/` module (chip.rs, sot.rs, density.rs, naming.rs, error.rs)
- Calculator-based MCP tools (`calculate_footprint`, `list_package_types`, `get_ipc_name`)
- Old approach preserved in `stale/calculator-approach` branch

### Added

- `src/altium/` module for Altium file I/O
    - `pcblib/mod.rs` - PcbLib reading and writing with OLE compound document handling
    - `pcblib/primitives.rs` - Pad, Track, Arc, Region, Text, Model3D types
    - `error.rs` - Altium-specific error types
- New primitive-based MCP tools
    - `read_pcblib` - Read footprints from .PcbLib files
    - `write_pcblib` - Write footprints with primitive definitions
    - `list_components` - List component names in a library

### Documentation

- Rewrote README.md with new architecture vision
- Rewrote docs/ARCHITECTURE.md with responsibility split diagram
- Rewrote docs/AI_WORKFLOW.md with primitive-based workflow
- Updated docs/VISION.md with simplified approach
- Updated .claude/CLAUDE.md with new project structure
- Updated TODO.md with current status
