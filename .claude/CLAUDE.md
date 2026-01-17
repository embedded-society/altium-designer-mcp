# altium-designer-mcp — AI Assistant Context

## The Vision (Read First!)

See `docs/VISION.md` for the full architecture:

| Component | Data Flow |
|-----------|-----------|
| **IPC-7351B** | Dimensions → Calculations → Land Pattern |
| **Altium I/O** | MCP Server ↔ .PcbLib/.SchLib Files |

**Core principle:** AI handles repetitive calculations. Engineers focus on design decisions.

## What Is This Project?

An MCP server for AI-assisted Altium Designer component library management with IPC-7351B compliance.

**IPC-7351B First:** All footprints follow the standard.
**Style Extraction:** Learn from existing library components.

## Quick Reference

| Resource | Location |
|----------|----------|
| Vision | `docs/VISION.md` |
| Architecture | `docs/ARCHITECTURE.md` |
| AI Workflow | `docs/AI_WORKFLOW.md` |

## Critical Rules

### NEVER Do These

1. **NEVER write arbitrary files outside library paths**
2. **NEVER expose internal file paths in error messages**
3. **NEVER skip IPC validation**
4. **NEVER ignore user's style configuration**
5. **NEVER push to main**

### ALWAYS Do These

1. **Use IPC-7351B formulas** for land patterns
2. **Validate dimensions** before calculation
3. **Extract style** from reference components when available
4. **Use sensible defaults** when config is missing
5. **Sanitise paths** in error messages

## Implementation Pattern

```rust
// CORRECT: IPC-7351B calculation with validation
pub fn calculate_chip_footprint(
    body_length: f64,
    body_width: f64,
    terminal_length: f64,
    density: DensityLevel,
) -> Result<Footprint, IpcError> {
    // Validate inputs
    if terminal_length >= body_length {
        return Err(IpcError::InvalidDimensions(
            "Terminal length must be less than body length".into()
        ));
    }

    // IPC-7351B calculations
    let (toe, heel, side) = density.solder_goals();
    let pad_width = terminal_length + toe + heel;
    let pad_height = body_width + 2.0 * side;

    Ok(Footprint {
        pads: vec![
            Pad::new(1, -pad_span/2.0, 0.0, pad_width, pad_height),
            Pad::new(2, pad_span/2.0, 0.0, pad_width, pad_height),
        ],
        courtyard: Courtyard::from_bounds(...),
        silkscreen: Silkscreen::chip_outline(...),
    })
}
```

## Project Structure

```
src/
├── lib.rs              # Library crate root
├── main.rs             # CLI entry point
├── error.rs            # Top-level error types
├── config/             # Configuration
│   ├── mod.rs          # Module exports
│   └── settings.rs     # Config file parsing
├── mcp/                # MCP server
│   ├── mod.rs          # Module exports
│   ├── server.rs       # JSON-RPC server
│   ├── protocol.rs     # MCP protocol types
│   └── transport.rs    # Stdio transport
├── ipc7351/            # IPC-7351B calculations (planned)
│   ├── mod.rs          # Module exports
│   ├── chip.rs         # Chip resistor/capacitor patterns
│   ├── soic.rs         # SOIC/SOP patterns
│   ├── qfn.rs          # QFN/DFN patterns
│   └── naming.rs       # IPC naming conventions
├── altium/             # Altium file I/O (planned)
│   ├── mod.rs          # Module exports
│   ├── cfb.rs          # OLE compound file handling
│   ├── pcblib.rs       # .PcbLib reading/writing
│   └── schlib.rs       # .SchLib reading/writing
├── style/              # Style management (planned)
│   ├── mod.rs          # Module exports
│   ├── extract.rs      # Extract style from existing lib
│   └── apply.rs        # Apply style to components
├── symbols/            # Symbol generation (planned)
│   └── generator.rs    # Schematic symbol creation
└── database/           # CSV database (planned)
    ├── schema.rs       # Database schema
    └── csv_io.rs       # CSV reading/writing
```

## Off Limits

**`CODE_OF_CONDUCT.md`** — Do not modify.
