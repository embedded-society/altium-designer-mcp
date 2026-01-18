# Style Guide

Code style conventions for altium-designer-mcp.

---

## General Rules

| Rule | Setting |
|------|--------|
| Indentation | 4 spaces (no tabs) |
| Max line length | 170 characters |
| Charset | UTF-8 |
| Final newline | Always |
| Trailing whitespace | Trim (except Markdown) |

These rules are enforced by `.editorconfig`. Install the EditorConfig plugin for your editor:

- **VS Code:** [EditorConfig for VS Code](https://marketplace.visualstudio.com/items?itemName=EditorConfig.EditorConfig)

VS Code also displays a ruler at 170 characters (configured in `.vscode/settings.json`).

---

## Language

Use **British English** in all documentation and user-facing text.

See `CONTRIBUTING.md` § British Spelling for the full spelling guide.

**Exceptions:**

- Code identifiers matching Rust/library conventions (e.g., `Color` in external APIs)
- Protocol-defined terms (e.g., `initialize` in MCP specification)
- Standard file names (e.g., `LICENSE` if required by tooling)
- External legal documents (e.g., GPL licence text)

---

## Single Source of Truth

Avoid duplicating information across files. Each piece of information should have one canonical location.

| Information | Canonical Source |
|-------------|------------------|
| British spelling | `CONTRIBUTING.md` § British Spelling |
| Build commands | `CONTRIBUTING.md` § Development Setup |
| Coding standards | `CONTRIBUTING.md` § Coding Standards |
| Commit conventions | `CONTRIBUTING.md` § Commit Messages |
| PR requirements | `CONTRIBUTING.md` § Pull Requests |
| Security policy | `SECURITY.md` |
| Formatting rules | `.editorconfig` |

**Guidelines:**

- Reference the canonical source instead of duplicating content
- If information must appear in multiple places (e.g., PR template checklists), keep it minimal
- When updating information, update the canonical source first
- Cross-reference using `filename` § Section Name format

---

## Rust

### Formatting

Use `rustfmt` with default settings. CI enforces this.

```bash
cargo fmt --all         # Format all code
cargo fmt --all --check # Check without modifying
```

### Linting

Use `clippy` with warnings as errors. CI enforces this.

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

### Naming Conventions

| Item | Convention | Example |
|------|------------|--------|
| Crates | snake_case | `altium_designer_mcp` |
| Modules | snake_case | `pcblib` |
| Types | PascalCase | `Footprint` |
| Functions | snake_case | `write_pcblib` |
| Constants | SCREAMING_SNAKE_CASE | `DEFAULT_LINE_WIDTH` |
| Variables | snake_case | `pad_width` |

### Documentation

- All public items must have doc comments (`///`)
- CI checks documentation builds without warnings

---

## YAML (GitHub Actions)

### Indentation

**4 spaces** for structure levels — aligned with project-wide convention.

```yaml
jobs:
    build:
        name: Build
        runs-on: ubuntu-latest

        steps:
            - name: Checkout
              uses: actions/checkout@v4

            - name: Build
              run: cargo build
```

### List Item Indentation

List items use **2-space continuation** from the `-` character (standard YAML behaviour):

```yaml
updates:
    - package-ecosystem: "github-actions"
      directory: "/"
      schedule:
        interval: "daily"
```

### Multi-line Scripts (`run: |`)

Shell script content inside `run: |` blocks uses **4-space indentation** for shell constructs (if/else, loops):

```yaml
            - name: Example step
              shell: bash
              run: |
                if [[ -n "$VAR" ]]; then
                    echo "Variable is set"
                else
                    echo "Variable is not set"
                fi
```

### Formatter

**Format-on-save is disabled** for YAML files in VS Code (configured in `.vscode/settings.json`).

---

## JSON

### Indentation

**4 spaces**.

```json
{
    "key": "value",
    "nested": {
        "item": 123
    }
}
```

### Formatter

VS Code uses the built-in JSON formatter (`vscode.json-language-features`).

---

## TOML

### Indentation

**4 spaces**.

```toml
[package]
name = "altium-designer-mcp"
version = "0.1.0"

[dependencies]
serde = { version = "1.0", features = ["derive"] }
```

### Formatter

Use [Even Better TOML](https://marketplace.visualstudio.com/items?itemName=tamasfe.even-better-toml) for VS Code. Column width is set to 170 characters (configured in `.vscode/settings.json`).

---

## Markdown

### Headings

Use ATX-style headings with blank lines before and after:

```markdown
## Section Title

Content here.
```

### Lists

Use `-` for unordered lists, `1.` for ordered lists.

### Code Blocks

Always specify the language:

````markdown
```rust
fn main() {
    println!("Hello!");
}
```
````

### Trailing Whitespace

Markdown files are exempt from trailing whitespace trimming (needed for line breaks).

---

## Commit Messages

See `CONTRIBUTING.md` § Commit Messages for conventions and allowed types.

---

## Primitives & Dimensions

- All dimensions in millimetres (mm)
- Use `f64` for dimensional values
- Document units in variable names or comments when not obvious
- Footprint primitives: Pad, Track, Arc, Region, Text, Fill, ComponentBody, Model3D
- Symbol primitives: Pin, Rectangle, Line, Polyline, Arc, Ellipse, Label, Parameter, FootprintModel

---

*Last updated: 2026-01-18*
