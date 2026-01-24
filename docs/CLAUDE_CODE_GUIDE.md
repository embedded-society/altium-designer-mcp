# Using altium-designer-mcp with Claude Code

This guide explains how to set up and use the Altium Designer MCP server with Claude Code
for AI-assisted component library creation.

---

## Overview

Claude Code can use this MCP server to:

- Read existing Altium libraries and analyse their structure
- Create new footprints with IPC-7351B compliant land patterns
- Create schematic symbols with proper pin definitions
- Match the style of existing libraries
- Generate entire component libraries from specifications

**Note:** While Altium Designer only runs on Windows, you can use this MCP server on any
platform to generate library files that can then be opened in Altium Designer on Windows.

---

## Installation

### Prerequisites

- [Rust 1.75+](https://rustup.rs/) (for building from source)
- [Claude Code](https://docs.anthropic.com/en/docs/claude-code) CLI installed

### Step 1: Clone and Build

See [CONTRIBUTING.md § Development Setup](../CONTRIBUTING.md#development-setup) for build instructions.

**Binary location after build:**

- **Windows:** `target\release\altium-designer-mcp.exe`
- **Linux/macOS:** `target/release/altium-designer-mcp`

### Step 2: Create Configuration File

See [README.md § Configuration](../README.md#configuration) for configuration options.

**Config file location:**

- **Windows:** `%USERPROFILE%\.altium-designer-mcp\config.json`
- **Linux/macOS:** `~/.altium-designer-mcp/config.json`

Create the directory and config file for your platform:

**Windows (PowerShell):**

```powershell
mkdir $env:USERPROFILE\.altium-designer-mcp -ErrorAction SilentlyContinue
```

**Linux/macOS:**

```bash
mkdir -p ~/.altium-designer-mcp
```

### Step 3: Configure Claude Code

Claude Code uses a `.mcp.json` file in your project root to configure MCP servers.

#### Option A: Project-Level Configuration (Recommended)

Create a `.mcp.json` file in your Altium project's root directory:

#### Windows

```json
{
    "mcpServers": {
        "altium": {
            "command": "C:\\path\\to\\altium-designer-mcp\\target\\release\\altium-designer-mcp.exe",
            "args": ["%USERPROFILE%\\.altium-designer-mcp\\config.json"]
        }
    }
}
```

#### Linux

```json
{
    "mcpServers": {
        "altium": {
            "command": "/path/to/altium-designer-mcp/target/release/altium-designer-mcp",
            "args": ["/home/yourname/.altium-designer-mcp/config.json"]
        }
    }
}
```

#### macOS

```json
{
    "mcpServers": {
        "altium": {
            "command": "/path/to/altium-designer-mcp/target/release/altium-designer-mcp",
            "args": ["/Users/yourname/.altium-designer-mcp/config.json"]
        }
    }
}
```

#### Option B: Global Configuration via CLI

You can also add the MCP server globally using the Claude Code CLI:

**Windows (PowerShell):**

```powershell
claude mcp add altium C:\path\to\altium-designer-mcp\target\release\altium-designer-mcp.exe -- %USERPROFILE%\.altium-designer-mcp\config.json
```

**Linux / macOS:**

```bash
claude mcp add altium /path/to/altium-designer-mcp/target/release/altium-designer-mcp -- ~/.altium-designer-mcp/config.json
```

To verify it was added:

```bash
claude mcp list
```

---

## Using with Claude Code CLI

### Starting Claude Code

Navigate to your Altium project directory and run:

```bash
claude
```

Claude Code will automatically detect and load the MCP server from:

1. The `.mcp.json` file in the current directory (if present)
2. Your global MCP configuration

### Verify MCP is Loaded

Ask Claude Code:

```
What MCP tools do you have available?
```

Or use the CLI command:

```bash
claude mcp list
```

You should see the Altium tools listed:

**Core Tools:**

- `read_pcblib` — Read footprints from a PcbLib file
- `write_pcblib` — Write footprints to a PcbLib file
- `read_schlib` — Read symbols from a SchLib file
- `write_schlib` — Write symbols to a SchLib file
- `list_components` — List component names in a library
- `extract_style` — Extract styling information from a library

**Library Management:**

- `delete_component` — Delete components from a library
- `copy_component` — Duplicate a component within a library
- `validate_library` — Check a library for common issues
- `export_library` — Export library to JSON or CSV format
- `diff_libraries` — Compare two library versions

**Batch Operations:**

- `batch_update` — Perform library-wide updates (track widths, layer renaming)

**SchLib Tools:**

- `manage_schlib_parameters` — Manage component parameters (Value, Manufacturer, etc.)
- `manage_schlib_footprints` — Manage footprint links in symbols

**Visualisation:**

- `render_footprint` — Generate ASCII art preview of a footprint

---

## Example Workflows

### 1. Create a Single Footprint

```
Create an IPC-7351B compliant 0603 chip resistor footprint and save it to
./MyLibrary.PcbLib
```

Claude Code will:

1. Calculate the land pattern using IPC-7351B
2. Generate pad coordinates, silkscreen, and courtyard
3. Call `write_pcblib` to create the file

### 2. Create a Matching Schematic Symbol

```
Now create a matching schematic symbol for the 0603 resistor and save it to
./MyLibrary.SchLib. Use designator "R?" and link it to the RESC1608X55N footprint.
```

### 3. Analyse an Existing Library

```
Read ./ExistingLibrary.PcbLib and describe the footprints it contains.
What silkscreen style does it use?
```

Claude Code will:

1. Call `read_pcblib` to read the library
2. Analyse the primitives
3. Describe the styling conventions

### 4. Match an Existing Style

```
Extract the style from ./CompanyLibrary.PcbLib and create a new 0805 capacitor
footprint that matches the same style conventions.
```

Claude Code will:

1. Call `extract_style` to analyse the existing library
2. Apply the same track widths, pad shapes, and layer usage
3. Create a style-matched footprint

### 5. Create a Complete Component Library

```
Create a chip resistor library with footprints and symbols for:
- 0201, 0402, 0603, 0805, 1206, 2010, 2512

Use IPC-7351B nominal density. Save to ./ChipResistors.PcbLib and
./ChipResistors.SchLib
```

Claude Code will batch-create all components using IPC-7351B calculations.

### 6. Create from Datasheet Specifications

```
Create a footprint for a QFN-24 package with:
- Body: 4mm x 4mm
- 24 pins, 0.5mm pitch
- Thermal pad: 2.5mm x 2.5mm
- Use IPC-7351B nominal density

Save to ./ICs.PcbLib
```

---

## Example Prompts

### Basic Component Creation

```
Create an 0805 chip capacitor footprint with IPC-7351B nominal land pattern.
```

```
Create a 2-pin polarised capacitor schematic symbol.
```

### Working with Existing Libraries

```
List all components in ./MyLibrary.PcbLib
```

```
Read ./Passives.SchLib and show me the pin configuration for the RESISTOR symbol.
```

### Style Matching

```
Analyse the silkscreen style in ./ExistingLib.PcbLib - what line width does it use?
```

```
Create a new footprint matching the style of ./CompanyStandard.PcbLib
```

### Batch Creation

```
Create a complete SMD inductor library with sizes: 0402, 0603, 0805, 1008, 1206
```

```
Create schematic symbols for all footprints in ./Passives.PcbLib
```

---

## Tips for Best Results

### 1. Be Specific About Standards

```
Use IPC-7351B nominal density (not maximum or minimum)
```

### 2. Specify Layer Preferences

```
Put silkscreen on Top Overlay, courtyard on Top Courtyard layer
```

### 3. Request Style Analysis First

```
First analyse ./ExistingLib.PcbLib, then create new components matching that style
```

### 4. Provide Datasheet Details

When creating custom packages, provide:

- Body dimensions (L x W x H)
- Pin pitch
- Pin count and arrangement
- Thermal pad dimensions (if applicable)

### 5. Use Append Mode for Incremental Building

```
Add an 0402 resistor footprint to the existing ./Passives.PcbLib (append mode)
```

---

## Troubleshooting

### "Access denied" Error

The file path is outside `allowed_paths`. Update your config.json to include the
directory where you want to create libraries.

### MCP Server Not Found

Verify the path in your Claude Code configuration points to the correct binary:

- Windows: `.exe` extension required
- Check the path exists and is executable

### Library Won't Open in Altium

- Ensure you're using a recent version of Altium Designer (19+)
- Check that the file was created successfully (non-zero file size)
- Try opening with File > Open rather than drag-and-drop

### Style Extraction Shows Unexpected Values

The `extract_style` tool analyses all primitives in the library. If a library has
mixed styles, you may see multiple values for each property.

---

## Platform-Specific Notes

### Windows

This is the primary platform since Altium Designer runs on Windows. You can:

- Generate libraries directly in your Altium project folder
- Use Windows paths with backslashes in config.json (escape them: `\\`)
- Run Claude Code in PowerShell, CMD, or Windows Terminal

### Linux

Generate libraries on Linux and transfer to Windows for use in Altium:

- Use a shared folder, cloud sync, or version control
- File format is binary-compatible across platforms
- Consider using Wine with Altium Designer (community-supported)

### macOS

Same approach as Linux:

- Generate libraries and transfer to Windows
- Use Apple Silicon or Intel Mac - both work
- File format is binary-compatible

---

## Security Notes

The MCP server validates all file paths against the `allowed_paths` configuration.
This prevents the AI from accessing or modifying files outside designated directories.

Always configure `allowed_paths` to include only the directories where you want
to allow library operations.

---

## Next Steps

- Read [AI_WORKFLOW.md](AI_WORKFLOW.md) for detailed IPC-7351B reference
- See [ARCHITECTURE.md](ARCHITECTURE.md) for technical details
- Check sample files in `scripts/` folder for examples
