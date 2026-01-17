# Altium Designer MCP Server — Project TODO

> Everything discussed and planned for the altium-designer-mcp project.

---

## Executive Summary

**altium-designer-mcp** is an open-source Rust implementation of a Model Context Protocol
(MCP) server that enables AI assistants (Claude Code, Claude Desktop, VSCode Copilot) to
create, read, and manage Altium Designer component libraries with full IPC-7351B compliance.

### The Problem

Creating a single IPC-compliant Altium component manually takes **~1 hour**:

- Hunt through datasheets for package dimensions
- Look up IPC-7351B tables and calculate land patterns
- Manually place pads, silkscreen, courtyard in Altium
- Create matching schematic symbol
- Enter 40+ parameter fields (MPN, manufacturer, specs, suppliers...)
- Repeat hundreds of times per project

**Time cost for a typical library:**

| Components | Manual Time | With This Tool |
|------------|-------------|----------------|
| 100        | 2.5 weeks   | ~10 minutes    |
| 500        | 3 months    | ~1 hour        |
| 1000       | 6 months    | ~2 hours       |

### The Solution

A single Rust binary that:

1. **Reads existing Altium libraries** to learn your style preferences
2. **Calculates IPC-7351B compliant footprints** from package dimensions
3. **Generates complete components** (footprint + symbol + parameters)
4. **Writes native Altium files** (.PcbLib, .SchLib, .DbLib)
5. **Manages CSV-based component databases** for easy version control
6. **Integrates with Claude Code/VSCode** via MCP for natural language interaction

---

## Technology Stack

| Component          | Choice              | Rationale                                      |
|--------------------|---------------------|------------------------------------------------|
| **Language**       | Rust                | Single binary, zero deps, memory safe, fast    |
| **MCP SDK**        | `rmcp` (official)   | Anthropic's official Rust SDK for MCP          |
| **Async Runtime**  | Tokio               | Industry standard for Rust async               |
| **Serialization**  | Serde               | JSON/binary parsing                            |
| **OLE Parsing**    | Custom + `cfb` crate| Altium uses MS OLE compound documents          |

### Why Rust Over TypeScript/Python?

| Aspect           | TypeScript        | Python          | Rust          |
|------------------|-------------------|-----------------|---------------|
| Distribution     | npm + node bloat  | pip + venv conflicts | Single binary |
| Binary parsing   | Clunky buffers    | Slow            | Native        |
| Startup time     | ~500ms (Node)     | ~300ms          | <10ms         |
| Memory safety    | Runtime errors    | Runtime         | Compile       |
| Engineer appeal  | "Ugh, npm"        | "OK"            | "Nice!"       |

---

## Project Structure

```
altium-designer-mcp/
├── Cargo.toml
├── LICENSE                      # GPLv3
├── README.md
├── TODO.md                      # This file
├── CREDITS.md                   # Attribution to prior art
│
├── src/
│   ├── main.rs                  # Entry point, MCP server bootstrap
│   ├── lib.rs                   # Public API for library use
│   │
│   ├── mcp/                     # MCP protocol layer
│   │   ├── mod.rs
│   │   ├── server.rs            # rmcp server implementation
│   │   ├── tools.rs             # Tool definitions
│   │   └── resources.rs         # MCP resources (if needed)
│   │
│   ├── altium/                  # Altium file format handling
│   │   ├── mod.rs
│   │   ├── ole.rs               # OLE compound document parser
│   │   ├── pcblib/
│   │   │   ├── mod.rs
│   │   │   ├── reader.rs        # Read .PcbLib files
│   │   │   ├── writer.rs        # Write .PcbLib files
│   │   │   └── primitives.rs    # Pad, Track, Arc, Region, etc.
│   │   ├── schlib/
│   │   │   ├── mod.rs
│   │   │   ├── reader.rs        # Read .SchLib files
│   │   │   ├── writer.rs        # Write .SchLib files
│   │   │   └── primitives.rs    # Pin, Rectangle, Line, etc.
│   │   ├── dblib.rs             # DbLib XML generation
│   │   └── ascii.rs             # ASCII format support
│   │
│   ├── ipc7351/                 # IPC-7351B standard implementation
│   │   ├── mod.rs
│   │   ├── calculator.rs        # Land pattern calculations
│   │   ├── packages/
│   │   │   ├── mod.rs
│   │   │   ├── chip.rs          # 0201, 0402, 0603, 0805, 1206...
│   │   │   ├── soic.rs          # SOIC, SSOP, TSSOP, MSOP
│   │   │   ├── qfp.rs           # QFP, LQFP, TQFP
│   │   │   ├── qfn.rs           # QFN, DFN, SON
│   │   │   ├── bga.rs           # BGA, CSP
│   │   │   ├── sot.rs           # SOT-23, SOT-223, SOT-363...
│   │   │   └── discrete.rs      # MELF, SOD, SMA, SMB, SMC
│   │   ├── naming.rs            # IPC naming convention generator
│   │   ├── density.rs           # M/N/L density level handling
│   │   └── tolerances.rs        # Fabrication tolerances
│   │
│   ├── style/                   # Style extraction & application
│   │   ├── mod.rs
│   │   ├── extractor.rs         # Analyse existing libraries
│   │   ├── guide.rs             # StyleGuide struct definition
│   │   └── applicator.rs        # Apply style to new components
│   │
│   ├── symbols/                 # Schematic symbol generation
│   │   ├── mod.rs
│   │   ├── templates.rs         # R, C, L, D, Q, U templates
│   │   ├── generator.rs         # Symbol generation logic
│   │   └── pin_layout.rs        # IC pin arrangement algorithms
│   │
│   ├── database/                # CSV database management
│   │   ├── mod.rs
│   │   ├── schema.rs            # Column definitions
│   │   ├── csv_ops.rs           # CRUD operations
│   │   ├── validation.rs        # Data validation
│   │   └── batch_gen.rs         # Batch library generation
│   │
│   └── component_data/          # External data sources (future)
│       ├── mod.rs
│       ├── octopart.rs          # Octopart API client
│       ├── digikey.rs           # Digi-Key API client
│       └── mouser.rs            # Mouser API client
│
├── tests/
│   ├── ipc7351_tests.rs         # Calculation validation
│   ├── altium_roundtrip.rs      # Read→Write→Read validation
│   └── integration_tests.rs     # Full workflow tests
│
├── examples/
│   ├── create_resistor.rs
│   ├── batch_import.rs
│   └── style_extraction.rs
│
└── data/
    ├── ipc7351b_tables.json     # J-values, tolerances
    └── package_definitions.json  # Standard package dimensions
```

---

## MCP Tools Specification

### Category 1: Library Reading

```rust
/// Read and parse an Altium PcbLib file
#[tool(name = "read_pcblib")]
async fn read_pcblib(filepath: String) -> PcbLibContents;

/// Read and parse an Altium SchLib file
#[tool(name = "read_schlib")]
async fn read_schlib(filepath: String) -> SchLibContents;

/// List all component names in a library
#[tool(name = "list_components")]
async fn list_components(filepath: String) -> Vec<String>;

/// Get detailed information about a specific component
#[tool(name = "get_component")]
async fn get_component(filepath: String, component_name: String) -> ComponentDetails;
```

### Category 2: Style Management

```rust
/// Analyse an existing library and extract style preferences
#[tool(name = "extract_style_guide")]
async fn extract_style_guide(filepath: String) -> StyleGuide;

// StyleGuide contains:
// - silkscreen_line_width: f64 (mm)
// - silkscreen_font: String
// - courtyard_margin: f64 (mm)
// - courtyard_layer: String
// - pad_corner_radius: f64 (percentage)
// - assembly_line_width: f64 (mm)
// - pin1_marker_style: String
// - naming_convention: String

/// Save style guide to JSON file for reuse
#[tool(name = "save_style_guide")]
async fn save_style_guide(style_guide: StyleGuide, filepath: String);

/// Load previously saved style guide
#[tool(name = "load_style_guide")]
async fn load_style_guide(filepath: String) -> StyleGuide;
```

### Category 3: IPC-7351B Calculations

```rust
/// Calculate IPC-7351B compliant land pattern from package dimensions
#[tool(name = "calculate_footprint")]
async fn calculate_footprint(
    package_type: String,      // CHIP, SOIC, QFP, QFN, BGA, SOT, MELF
    body_length: f64,          // mm
    body_width: f64,           // mm
    lead_span: f64,            // toe-to-toe, mm
    lead_width: f64,           // mm
    pitch: Option<f64>,        // mm, for multi-pin packages
    pin_count: u32,
    density: Option<String>,   // "M" (Most), "N" (Nominal), "L" (Least)
) -> CalculatedFootprint;

/// Generate IPC-7351B compliant component name
#[tool(name = "get_ipc_name")]
async fn get_ipc_name(
    package_type: String,
    pitch: f64,
    body_length: f64,
    body_width: f64,
    height: f64,
    pin_count: u32,
    density: String,
) -> String;

/// List all supported IPC-7351B package types with descriptions
#[tool(name = "list_package_types")]
async fn list_package_types() -> Vec<PackageTypeInfo>;

/// Get J-values (solder fillet goals) for a density level
#[tool(name = "get_j_values")]
async fn get_j_values(density: String) -> JValues;
```

### Category 4: Component Generation

```rust
/// Create a complete footprint with all layers
#[tool(name = "create_footprint")]
async fn create_footprint(
    name: String,
    package_type: String,
    dimensions: PackageDimensions,
    density: String,
    style_guide: Option<StyleGuide>,
) -> Footprint;

/// Create a schematic symbol
#[tool(name = "create_symbol")]
async fn create_symbol(
    name: String,
    symbol_type: String,       // "resistor", "capacitor", "ic", etc.
    pins: Vec<PinDefinition>,
    style_guide: Option<StyleGuide>,
) -> Symbol;

/// Create complete component with footprint, symbol, and all parameters
#[tool(name = "create_component")]
async fn create_component(
    mpn: String,
    manufacturer: String,
    package_dims: PackageDimensions,
    parameters: HashMap<String, String>,
    pins: Vec<PinDefinition>,
    style_guide: Option<StyleGuide>,
) -> FullComponent;
```

### Category 5: Library Writing

```rust
/// Write footprints to a PcbLib file
#[tool(name = "write_pcblib")]
async fn write_pcblib(
    footprints: Vec<Footprint>,
    filepath: String,
    append: Option<bool>,
);

/// Write symbols to a SchLib file
#[tool(name = "write_schlib")]
async fn write_schlib(
    symbols: Vec<Symbol>,
    filepath: String,
    append: Option<bool>,
);

/// Add a component to existing libraries
#[tool(name = "add_to_library")]
async fn add_to_library(
    component: FullComponent,
    pcblib_path: String,
    schlib_path: String,
);
```

### Category 6: CSV Database Management

```rust
/// Create a new component database CSV with proper schema
#[tool(name = "create_database")]
async fn create_database(
    filepath: String,
    custom_columns: Option<Vec<String>>,
);

/// Add a component record to the database
#[tool(name = "add_component_to_db")]
async fn add_component_to_db(csv_path: String, record: ComponentRecord);

/// Query components from database with filtering
#[tool(name = "query_database")]
async fn query_database(
    csv_path: String,
    filter: Option<HashMap<String, String>>,
) -> Vec<ComponentRecord>;

/// Generate Altium DbLib file from CSV database
#[tool(name = "generate_dblib")]
async fn generate_dblib(
    csv_path: String,
    output_path: String,
    library_paths: Vec<String>,
);

/// Regenerate all libraries from CSV database (full rebuild)
#[tool(name = "regenerate_libraries")]
async fn regenerate_libraries(
    csv_path: String,
    output_dir: String,
    style_guide: Option<StyleGuide>,
) -> GenerationReport;
```

### Category 7: Component Data Lookup (Future)

```rust
/// Lookup component data from distributor APIs
#[tool(name = "lookup_component")]
async fn lookup_component(mpn: String) -> ComponentData;

/// Search for components by parameters
#[tool(name = "search_components")]
async fn search_components(
    category: String,
    specs: HashMap<String, String>,
) -> Vec<ComponentInfo>;
```

---

## CSV Database Schema

The master CSV file serves as the single source of truth:

```csv
ID,Part Number,Manufacturer,Description,Category,Value,Package,Package Type,Body Length,Body Width,Lead Span,Lead Width,Pitch,Pin Count,Footprint Ref,Symbol Ref,Datasheet,Supplier 1,Supplier PN 1,Unit Price,RoHS,Lifecycle,Tolerance,Power Rating,Voltage Rating,Temperature Range
1,RC0402FR-0710KL,Yageo,Thick Film Resistor,Resistor,10k,0402,CHIP,1.0,0.5,1.0,0.3,,2,RESC1005X40N,RES_2PIN,https://...,Digi-Key,311-10KLRCT-ND,0.01,Compliant,Active,1%,0.0625W,50V,-55C to +155C
```

### Standard Columns

| Column         | Description                          | Required    |
|----------------|--------------------------------------|-------------|
| ID             | Unique identifier                    | Yes         |
| Part Number    | Manufacturer part number (MPN)       | Yes         |
| Manufacturer   | Manufacturer name                    | Yes         |
| Description    | Human-readable description           | Yes         |
| Category       | Component category                   | Yes         |
| Value          | Component value (10k, 100nF, etc.)   | No          |
| Package        | Package name (0402, LQFP-48, etc.)   | Yes         |
| Package Type   | IPC package type (CHIP, QFP, etc.)   | Yes         |
| Body Length    | Package body length in mm            | Yes         |
| Body Width     | Package body width in mm             | Yes         |
| Lead Span      | Toe-to-toe dimension in mm           | Yes         |
| Lead Width     | Lead/terminal width in mm            | Yes         |
| Pitch          | Lead pitch in mm                     | For multi-pin |
| Pin Count      | Total number of pins                 | Yes         |
| Footprint Ref  | Reference to PcbLib footprint        | Generated   |
| Symbol Ref     | Reference to SchLib symbol           | Generated   |
| Datasheet      | URL to datasheet                     | Recommended |
| Supplier 1     | Primary supplier name                | Recommended |
| Supplier PN 1  | Supplier part number                 | Recommended |
| Unit Price     | Unit cost                            | Optional    |
| RoHS           | RoHS compliance status               | Recommended |
| Lifecycle      | Product lifecycle status             | Recommended |
| + Custom       | User-defined columns                 | Optional    |

---

## MCP Integration

### How It Works

1. You build and install the MCP server binary
2. Configure Claude Code to use it
3. Claude Code launches the server and discovers its tools
4. Claude can then call those tools on your behalf

### Claude Code CLI Setup

```bash
# Install the server
cargo install altium-designer-mcp

# Add to Claude Code (user scope - available everywhere)
claude mcp add --scope user --transport stdio altium -- altium-designer-mcp /path/to/libraries
```

Or via JSON:

```bash
claude mcp add-json altium '{
  "type": "stdio",
  "command": "altium-designer-mcp",
  "args": ["/path/to/your/libraries"],
  "env": {
    "OCTOPART_API_KEY": "your-key-here"
  }
}' --scope user
```

### Project-Scoped Config (Recommended)

Put `.mcp.json` in your component library Git repo:

```
my-altium-library/
├── .mcp.json                    <- MCP server config
├── libraries/
│   ├── Passives.PcbLib
│   ├── Passives.SchLib
│   └── ...
├── database/
│   └── components.csv
└── styles/
    └── company-style.json
```

**.mcp.json contents:**

```json
{
  "mcpServers": {
    "altium": {
      "type": "stdio",
      "command": "altium-designer-mcp",
      "args": ["./libraries", "--style", "./styles/company-style.json"],
      "env": {
        "OCTOPART_API_KEY": "${OCTOPART_API_KEY}"
      }
    }
  }
}
```

**Benefits:**

- Clone repo → everything works
- Team shares same config via git
- Config changes tracked with library
- Works on any machine with the server installed

### Claude Desktop

Edit `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "altium": {
      "command": "altium-designer-mcp",
      "args": ["/Users/engineer/altium-libraries"]
    }
  }
}
```

### VSCode

Create `.vscode/mcp.json` in your workspace:

```json
{
  "servers": {
    "altium": {
      "command": "altium-designer-mcp",
      "args": ["${workspaceFolder}/libraries"]
    }
  }
}
```

---

## Prior Art & Credits

This project builds upon the excellent work of others:

### AltiumSharp (C# / .NET)

- **Repository**: <https://github.com/issus/AltiumSharp>
- **Author**: issus, Tiago Trinidad (@Kronal)
- **License**: MIT
- **Contribution**: Understanding of Altium binary file formats, OLE structure,
  read/write implementations
- **Usage**: File format specifications and parsing logic will be ported to Rust

### pyAltiumLib (Python)

- **Repository**: <https://github.com/ChrisHoyer/pyAltiumLib>
- **Author**: Chris Hoyer
- **Contribution**: File structure documentation, rendering logic
- **Usage**: Reference for file format documentation

### python-altium

- **Repository**: <https://github.com/vadmium/python-altium>
- **Author**: vadmium
- **Contribution**: Detailed format.md documentation of Altium file structures
- **Usage**: Primary reference for ASCII and binary format specifications

### KiCad MCP Server

- **Repository**: <https://github.com/mixelpixx/KiCAD-MCP-Server>
- **Author**: mixelpixx
- **Contribution**: Proof of concept that EDA + MCP integration works and is valuable
- **Usage**: Architectural inspiration for tool organization

### IPC-7351B Standard

- **Publisher**: IPC (Institute for Printed Circuits)
- **Document**: IPC-7351B - Generic Requirements for Surface Mount Design and
  Land Pattern Standard
- **Usage**: All land pattern calculations follow this standard

---

## License

**GNU General Public License v3.0 (GPLv3)**

### Why GPLv3?

1. **Prevent monetization** - No one can take this code and sell it as proprietary
   software
2. **Ensure contributions flow back** - Any improvements must be shared with the
   community
3. **Protect engineer users** - The tool remains free and open forever
4. **Encourage collaboration** - Companies can use it, but must share improvements

### What GPLv3 Means For Users

**You CAN:**

- Use this tool for any purpose (personal, commercial, educational)
- Modify the code to suit your needs
- Distribute copies
- Use generated libraries in proprietary products (libraries are data, not code)

**You CANNOT:**

- Distribute modified versions without sharing source code
- Create proprietary forks
- Remove license notices

---

## Viability Analysis

### Strengths

- **Real problem** - The pain of manual component creation is genuine and widespread
- **Clear value proposition** - Time savings are measurable and significant
- **Good prior art** - Not starting from zero on file format understanding
- **Appropriate technology** - Rust is the right choice for binary parsing + distribution
- **Well-defined API** - MCP tools have clear boundaries

### Primary Risks

#### 1. Altium File Format Writing (CRITICAL RISK)

Reading Altium files is well-documented. **Writing files that Altium reliably accepts
is harder:**

- Undocumented fields may need specific values
- Version-specific quirks
- Internal consistency requirements (checksums, offsets, reference counts)

**Mitigation:** Consider ASCII format as fallback (simpler but requires user conversion).

#### 2. Altium Version Compatibility

- Altium releases major versions annually
- Format changes could break the tool
- No official API or documentation

**Mitigation:** Target specific versions, detect/warn on version mismatch.

#### 3. Symbol Generation Complexity

IC symbols with proper pin grouping (power, I/O, etc.) is a deep rabbit hole.

**Mitigation:** V1 uses simple rectangular symbols; V2 adds intelligent pin grouping.

### Alternative Architecture to Consider

**Phased approach to de-risk:**

```
Phase 1: CSV + DelphiScript Generator
─────────────────────────────────────
• MCP server manages CSV database only
• IPC calculations output to CSV
• Generate DelphiScript that creates components in Altium
• User runs script inside Altium
• Avoids file format complexity entirely

Phase 2: Native File I/O (if Phase 1 succeeds)
──────────────────────────────────────────────
• Add binary read/write once value is proven
• Can iterate on file format support over time
```

---

## Development Roadmap

**Timeline: ~1 year**

### Phase 1: Foundation (Months 1-2)

- [ ] Project scaffold with Cargo workspace
- [ ] Basic MCP server with rmcp SDK
- [ ] IPC-7351B calculator for chip components (0201-2512)
- [ ] IPC naming convention generator
- [ ] Unit tests for calculations

### Phase 2: Altium File I/O (Months 3-5)

- [ ] OLE compound document parser
- [ ] PcbLib reader (footprint extraction)
- [ ] PcbLib writer (footprint creation)
- [ ] SchLib reader (symbol extraction)
- [ ] SchLib writer (symbol creation)
- [ ] Round-trip validation tests
- [ ] ASCII format support as fallback

### Phase 3: Style System (Month 6)

- [ ] Style extractor from existing libraries
- [ ] StyleGuide JSON serialization
- [ ] Style applicator for new components
- [ ] Silkscreen, courtyard, assembly layer handling

### Phase 4: Package Types (Months 7-8)

- [ ] SOIC, SSOP, TSSOP, MSOP
- [ ] QFP, LQFP, TQFP
- [ ] QFN, DFN, SON (with thermal pad)
- [ ] BGA, CSP
- [ ] SOT family (SOT-23, SOT-223, SOT-363, etc.)
- [ ] Discrete (MELF, SOD, SMA/SMB/SMC)

### Phase 5: Database System (Month 9)

- [ ] CSV schema definition
- [ ] CRUD operations
- [ ] DbLib XML generator
- [ ] Batch library regeneration
- [ ] Validation and reporting

### Phase 6: Symbol Generation (Month 10)

- [ ] Passive templates (R, C, L)
- [ ] Semiconductor templates (D, Q, U)
- [ ] IC pin layout algorithms
- [ ] Multi-part symbol support

### Phase 7: Polish & Release (Months 11-12)

- [ ] Documentation
- [ ] Example libraries
- [ ] CI/CD pipeline
- [ ] Pre-built binaries for Win/Mac/Linux
- [ ] crates.io publication
- [ ] GitHub release
- [ ] Real-world testing with users

### Future Enhancements

- [ ] 3D model generation (STEP via CadQuery/Rust)
- [ ] Octopart/Digi-Key API integration
- [ ] Datasheet dimension extraction (PDF parsing)
- [ ] Web UI for non-CLI users
- [ ] Altium 365 API integration

---

## Minimum Viable Package Set (V1)

To ship something useful faster, V1 could focus on:

| Package Type | Coverage | % of Typical BOM |
|--------------|----------|------------------|
| CHIP         | 0201-2512 passives | ~40% |
| SOIC/TSSOP   | Standard ICs | ~20% |
| QFN          | Modern ICs | ~15% |
| SOT-23       | Small transistors/regulators | ~10% |

This covers ~85% of typical BOM components with 4 package families.

---

## Example Interactions

### Creating a Single Component

```
Engineer: Create a footprint for the TPS63000DRCR buck-boost converter.
          It's in a VSON-10 package (3x3mm body, 0.5mm pitch).
          Use my existing library style.

Claude:   [extract_style_guide] Analyzing your existing library...
          → Silkscreen: 0.15mm lines, Courtyard: 0.25mm margin

          [calculate_footprint] Computing IPC-7351B land pattern...
          → Package: SON50P300X300X80-11N
          → Pad size: 0.30mm x 0.85mm
          → Thermal pad: 1.65mm x 2.40mm

          [create_component] Generating complete component...
          [write_pcblib] Saved to ./libraries/PowerIC.PcbLib

          Done. Created TPS63000DRCR with IPC-7351B Nominal density footprint.
```

### Batch Import from BOM

```
Engineer: I have a BOM with 150 components. Can you create the library?

Claude:   [Analyzing BOM...]
          → 150 components, 43 unique packages, 12 categories

          [Processing...]
          ████████████████████████████████ 100%

          Created Libraries:
          • Passives.PcbLib (23 footprints)
          • Semiconductors.PcbLib (15 footprints)
          • Connectors.PcbLib (5 footprints)

          Database: components.csv (150 records)

          Warnings:
          - 3 components missing package dimensions
          - 2 components have obsolete lifecycle status

          Time: 2 minutes (estimated manual: ~150 hours)
```

---

## Open Questions to Resolve

1. **Binary vs ASCII format?**
   - ASCII is 10x simpler but has UX friction
   - Binary is complex but seamless
   - Decision: Support both, ASCII first?

2. **How to test file output?**
   - Need Altium Designer licenses for validation
   - CI/CD strategy for automated testing
   - Matrix of Altium versions to support

3. **DbLib vs Integrated Libraries?**
   - DbLib is simpler (CSV + references)
   - Integrated requires embedding in binary format
   - Decision: Start with DbLib workflow?

4. **Symbol generation scope for V1?**
   - Simple rectangular symbols (fast to implement)
   - vs. Intelligent pin grouping (complex)
   - Decision: Simple for V1, intelligent for V2

---

## Project Maintainers

- **Organization**: [The Embedded Society](https://github.com/embedded-society/)
- **Repository**: [altium-designer-mcp](https://github.com/embedded-society/altium-designer-mcp)
- **Contact**: <matejg03@gmail.com>

---

## Contributing

Areas needing help:

- Additional package type implementations
- Testing with real Altium libraries
- Documentation improvements
- Windows/Mac testing
- Performance optimisation

---

*This project is not affiliated with Altium Limited. Altium Designer is a trademark of Altium Limited.*
