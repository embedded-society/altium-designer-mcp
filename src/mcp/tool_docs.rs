//! Generates `docs/TOOLS.md` from [`McpServer::get_tool_definitions`], so the
//! human-readable tool reference cannot drift from the schema the server
//! actually serves over `tools/list`.
//!
//! The committed `docs/TOOLS.md` is a build artifact of the tool definitions:
//! a unit test ([`tests::tools_md_in_sync`]) fails if the file is out of date,
//! and `UPDATE_DOCS=1 cargo test --lib tools_md_in_sync` regenerates it. Tool
//! descriptions, parameter schemas, and the per-tool `example` all live in
//! `tool_definitions.rs` (the single source of truth); nothing here is authored
//! by hand.

use std::fmt::Write as _;

use serde_json::Value;

use crate::mcp::server::McpServer;

/// Path to the generated doc, relative to the crate manifest dir.
const TOOLS_MD_REL: &str = "docs/TOOLS.md";

const HEADER: &str = "<!-- GENERATED — do not edit by hand.
     Source of truth: src/mcp/tool_definitions.rs
     Regenerate: UPDATE_DOCS=1 cargo test --lib tools_md_in_sync -->

# MCP Tools Reference

Every tool the **altium-designer-mcp** server exposes, rendered from the tool
definitions served over `tools/list`. Coordinates are millimetres for `.PcbLib`
footprints and schematic units (10 units = 1 grid square) for `.SchLib` symbols.
";

/// Renders the full Markdown reference for every registered tool.
pub fn render_tools_markdown() -> String {
    let mut out = String::from(HEADER);
    let tools = McpServer::get_tool_definitions();
    let _ = writeln!(out, "\n_{} tools._", tools.len());

    for tool in &tools {
        let _ = writeln!(out, "\n## `{}`", tool.name);
        if let Some(desc) = &tool.description {
            let _ = writeln!(out, "\n{}", desc.trim());
        }
        if let Some(example) = &tool.example {
            out.push_str("\n**Example**\n\n```json\n");
            out.push_str(&serde_json::to_string_pretty(example).unwrap_or_default());
            out.push_str("\n```\n");
        }
        out.push_str(&render_params(&tool.input_schema));
    }
    out
}

/// Renders the parameter table for one tool's input schema.
fn render_params(schema: &Value) -> String {
    let props = schema.get("properties").and_then(Value::as_object);
    let Some(props) = props else {
        return "\n_No parameters._\n".to_string();
    };
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    // Sort property names so the table is deterministic regardless of the
    // serde_json map backing (BTreeMap vs preserve_order IndexMap).
    let mut names: Vec<&String> = props.keys().collect();
    names.sort();

    let mut out = String::from(
        "\n**Parameters**\n\n| Name | Type | Required | Description |\n| --- | --- | --- | --- |\n",
    );
    for name in names {
        let p = &props[name];
        let req = if required.contains(&name.as_str()) {
            "yes"
        } else {
            "no"
        };
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} |",
            name,
            schema_type(p),
            req,
            describe(p)
        );
    }
    out
}

/// Human-readable type for a schema property (`string`, `array<object>`, …).
fn schema_type(p: &Value) -> String {
    if p.get("enum").is_some() {
        return "enum".to_string();
    }
    match p.get("type").and_then(Value::as_str) {
        Some("array") => {
            let item = p
                .get("items")
                .and_then(|i| i.get("type"))
                .and_then(Value::as_str)
                .unwrap_or("any");
            format!("array<{item}>")
        }
        Some(t) => t.to_string(),
        None => "any".to_string(),
    }
}

/// Description cell: the schema `description`, with enum values and any default
/// appended so the table is self-contained.
fn describe(p: &Value) -> String {
    let mut s = p
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .replace('\n', " ")
        .replace('|', "\\|");
    if let Some(vals) = p.get("enum").and_then(Value::as_array) {
        let joined = vals
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        if !joined.is_empty() {
            let _ = write!(s, " (one of: {joined})");
        }
    }
    if let Some(def) = p.get("default") {
        let _ = write!(s, " (default: `{def}`)");
    }
    s.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::{render_tools_markdown, TOOLS_MD_REL};
    use crate::mcp::server::McpServer;
    use serde_json::Value;

    /// Every per-tool `example` must be a valid call for that tool: it must name
    /// the right tool, use only documented top-level arguments (the same
    /// contract the strict-deserialization allow-lists enforce at runtime), and
    /// supply every required argument. A stale or hand-typo'd example would
    /// otherwise ship in docs/TOOLS.md and mislead an agent.
    #[test]
    fn examples_are_schema_valid() {
        let mut problems: Vec<String> = Vec::new();
        for tool in McpServer::get_tool_definitions() {
            let Some(example) = &tool.example else {
                continue;
            };
            let t = &tool.name;
            if example.get("name").and_then(Value::as_str) != Some(t.as_str()) {
                problems.push(format!(
                    "{t}: example names the wrong tool (or omits `name`)"
                ));
            }
            let Some(args) = example.get("arguments").and_then(Value::as_object) else {
                problems.push(format!("{t}: example has no `arguments` object"));
                continue;
            };
            let Some(props) = tool
                .input_schema
                .get("properties")
                .and_then(Value::as_object)
            else {
                continue;
            };
            for key in args.keys() {
                if !props.contains_key(key) {
                    problems.push(format!("{t}: argument `{key}` is not in the input schema"));
                }
            }
            if let Some(required) = tool.input_schema.get("required").and_then(Value::as_array) {
                for req in required.iter().filter_map(Value::as_str) {
                    if !args.contains_key(req) {
                        problems.push(format!("{t}: missing required argument `{req}`"));
                    }
                }
            }
        }
        assert!(
            problems.is_empty(),
            "tool examples disagree with their schemas:\n  {}",
            problems.join("\n  ")
        );
    }

    /// The internal `example` must never leak onto the `tools/list` wire — it is
    /// not part of the MCP tool schema. Guards the `#[serde(skip)]`.
    #[test]
    fn example_field_is_not_serialized() {
        let tool = McpServer::get_tool_definitions()
            .into_iter()
            .find(|t| t.example.is_some())
            .expect("at least one tool carries an example");
        let wire = serde_json::to_value(&tool).expect("serialize ToolDefinition");
        assert!(
            wire.get("example").is_none(),
            "`example` must be #[serde(skip)] — it leaked into tools/list output"
        );
        assert!(
            wire.get("inputSchema").is_some(),
            "the real schema is still serialized (camelCase)"
        );
    }

    /// Guards `docs/TOOLS.md` against drift from `tool_definitions.rs`. If a
    /// tool's schema, description, or example changes and the doc isn't
    /// regenerated, this fails. Regenerate with:
    ///   `UPDATE_DOCS=1 cargo test --lib tools_md_in_sync`
    #[test]
    fn tools_md_in_sync() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(TOOLS_MD_REL);
        let generated = render_tools_markdown();

        if std::env::var_os("UPDATE_DOCS").is_some() {
            std::fs::write(&path, &generated).expect("write docs/TOOLS.md");
            return;
        }

        let committed = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        assert_eq!(
            generated.replace("\r\n", "\n"),
            committed,
            "docs/TOOLS.md is out of date with tool_definitions.rs. \
             Regenerate: UPDATE_DOCS=1 cargo test --lib tools_md_in_sync"
        );
    }
}
