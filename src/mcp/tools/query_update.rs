//! Update/search/get/exists tools. Split from `server.rs`.

use serde_json::{json, Value};

use crate::mcp::server::{McpServer, ToolCallResult};

impl McpServer {
    /// Updates a component in-place within an Altium library file.
    pub(crate) fn call_update_component(&self, arguments: &Value) -> ToolCallResult {
        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        let Some(component_name) = arguments.get("component_name").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: component_name");
        };

        let dry_run = arguments
            .get("dry_run")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Determine file type from extension
        let path = std::path::Path::new(filepath);
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        match extension.as_deref() {
            Some("pcblib") => {
                let Some(fp_json) = arguments.get("footprint") else {
                    return ToolCallResult::error(
                        "Missing required parameter: footprint (required for .PcbLib files)",
                    );
                };
                self.update_pcblib_component(filepath, component_name, fp_json, dry_run)
            }
            Some("schlib") => {
                let Some(sym_json) = arguments.get("symbol") else {
                    return ToolCallResult::error(
                        "Missing required parameter: symbol (required for .SchLib files)",
                    );
                };
                self.update_schlib_component(filepath, component_name, sym_json, dry_run)
            }
            _ => ToolCallResult::error(super::unsupported_file_type(filepath)),
        }
    }

    /// Updates a footprint in-place within a `PcbLib` file.
    #[allow(clippy::too_many_lines)] // Includes parsing and dry_run logic
    pub(crate) fn update_pcblib_component(
        &self,
        filepath: &str,
        component_name: &str,
        fp_json: &Value,
        dry_run: bool,
    ) -> ToolCallResult {
        use crate::altium::pcblib::PcbLib;

        // Read the library
        let mut library = match PcbLib::open(filepath) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
        };

        // The component as the library spells it: the caller may name it in
        // another case, and that spelling must not leak into the file.
        let Some(stored_name) = library.get(component_name).map(|f| f.name.clone()) else {
            return ToolCallResult::error(super::component_not_found(
                component_name,
                &library.names(),
            ));
        };
        let component_name = stored_name.as_str();

        // Parse the replacement footprint — the same parser as write_pcblib,
        // so an update accepts exactly what a write does. A replacement
        // without a name keeps the stored one.
        let keys = crate::mcp::tools::allowed_keys::PcbLibKeys::new();
        let footprint = match self.parse_footprint_json(
            fp_json,
            &keys,
            "update_component",
            filepath,
            component_name,
        ) {
            Ok(footprint) => footprint,
            Err(result) => return result,
        };
        let name = footprint.name.clone();
        let name = name.as_str();

        // A replacement may carry a new name, which makes this a rename too —
        // and a rename onto a name another footprint already holds would leave
        // two components answering to it. Refuse, as rename_component does.
        if name != component_name {
            if let Err(e) = Self::validate_ole_name(name) {
                return ToolCallResult::error(e);
            }
            // The footprint's own name in another case resolves to itself
            // and is a legitimate rename; any other holder is a clash.
            if let Some(existing) = library
                .get(name)
                .filter(|f| !crate::altium::same_name(&f.name, component_name))
            {
                return ToolCallResult::error(Self::taken_name_error(
                    format!(
                        "Cannot rename '{component_name}' to '{name}': a footprint with that name already exists"
                    ),
                    name,
                    &existing.name,
                ));
            }
        }

        // Get the old component for comparison
        let old = library.get(component_name).cloned();

        if dry_run {
            // Build preview of changes
            let changes = Self::preview_footprint_changes(old.as_ref(), &footprint);

            let result = json!({
                "status": "dry_run",
                "filepath": filepath,
                "file_type": "PcbLib",
                "component_name": component_name,
                "new_name": name,
                "would_rename": name != component_name,
                "changes": changes,
                "message": format!(
                    "Would update component '{component_name}'{}",
                    if name == component_name {
                        String::new()
                    } else {
                        format!(" and rename to '{name}'")
                    }
                ),
            });

            return ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap());
        }

        // Perform the actual update
        library.update(component_name, footprint);

        if let Err(resp) = Self::backup_then_save(filepath, &mut library) {
            return resp;
        }

        let mut result = json!({
            "status": "success",
            "filepath": filepath,
            "file_type": "PcbLib",
            "component_name": component_name,
            "new_name": name,
            "renamed": name != component_name,
            "old_description": old.as_ref().map(|f| &f.description),
            "component_count": library.len(),
            "message": format!(
                "Updated component '{component_name}' in '{filepath}'{}",
                if name == component_name {
                    String::new()
                } else {
                    format!(" (renamed to '{name}')")
                }
            ),
        });

        // Run post-write validation
        if let Some(validation) = Self::post_write_validation_pcblib(filepath) {
            result["validation"] = validation;
        }

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Compares two footprints and returns a list of changes for `dry_run` preview.
    pub(crate) fn preview_footprint_changes(
        old: Option<&crate::altium::pcblib::Footprint>,
        new: &crate::altium::pcblib::Footprint,
    ) -> Vec<String> {
        let mut changes = Vec::new();

        if let Some(old) = old {
            if old.description != new.description {
                changes.push(format!(
                    "description: '{}' -> '{}'",
                    old.description, new.description
                ));
            }
            // Every primitive kind, from the enum, so the preview cannot miss
            // an added or removed kind.
            for kind in crate::altium::pcblib::PrimitiveKind::WRITE_ORDER {
                let (old_len, new_len) = (old.count_of(kind), new.count_of(kind));
                if old_len != new_len {
                    changes.push(format!("{}_count: {old_len} -> {new_len}", kind.name()));
                }
            }
        } else {
            changes.push("component will be created".to_string());
        }

        if changes.is_empty() {
            changes.push("no structural changes detected".to_string());
        }

        changes
    }

    /// Updates a symbol in-place within a `SchLib` file.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn update_schlib_component(
        &self,
        filepath: &str,
        component_name: &str,
        sym_json: &Value,
        dry_run: bool,
    ) -> ToolCallResult {
        use crate::altium::schlib::SchLib;

        // Read the library
        let mut library = match SchLib::open(filepath) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
        };

        // The component as the library spells it: the caller may name it in
        // another case, and that spelling must not leak into the file.
        let Some(stored_name) = library.get(component_name).map(|s| s.name.clone()) else {
            return ToolCallResult::error(super::component_not_found(
                component_name,
                &library.names(),
            ));
        };
        let component_name = stored_name.as_str();

        // Parse the replacement symbol — the same parser as write_schlib, so
        // an update accepts exactly what a write does. A replacement without
        // a name keeps the stored one.
        let keys = crate::mcp::tools::allowed_keys::SchLibKeys::new();
        let symbol = match self.parse_symbol_json(
            sym_json,
            &keys,
            "update_component",
            filepath,
            component_name,
        ) {
            Ok(symbol) => symbol,
            Err(result) => return result,
        };
        let name = symbol.name.clone();
        let name = name.as_str();

        // A replacement may carry a new name, which makes this a rename too —
        // and a rename onto a name another symbol already holds would leave
        // two components answering to it. Refuse, as rename_component does.
        if name != component_name {
            if let Err(e) = Self::validate_ole_name(name) {
                return ToolCallResult::error(e);
            }
            // The symbol's own name in another case resolves to itself and
            // is a legitimate rename; any other holder is a clash.
            if let Some(existing) = library
                .get(name)
                .filter(|s| !crate::altium::same_name(&s.name, component_name))
            {
                return ToolCallResult::error(Self::taken_name_error(
                    format!(
                        "Cannot rename '{component_name}' to '{name}': a symbol with that name already exists"
                    ),
                    name,
                    &existing.name,
                ));
            }
        }

        // Get the old component for comparison
        let old = library.get(component_name).cloned();

        if dry_run {
            // Build preview of changes
            let changes = Self::preview_symbol_changes(old.as_ref(), &symbol);

            let result = json!({
                "status": "dry_run",
                "filepath": filepath,
                "file_type": "SchLib",
                "component_name": component_name,
                "new_name": name,
                "would_rename": name != component_name,
                "changes": changes,
                "message": format!(
                    "Would update component '{component_name}'{}",
                    if name == component_name {
                        String::new()
                    } else {
                        format!(" and rename it to '{name}'")
                    }
                ),
            });

            return ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap());
        }

        // Perform the actual update
        library.update(component_name, symbol);

        if let Err(resp) = Self::backup_then_save(filepath, &mut library) {
            return resp;
        }

        let mut result = json!({
            "status": "success",
            "filepath": filepath,
            "file_type": "SchLib",
            "component_name": component_name,
            "new_name": name,
            "renamed": name != component_name,
            "old_description": old.as_ref().map(|s| &s.description),
            "component_count": library.len(),
            "message": format!(
                "Updated component '{component_name}' in '{filepath}'{}",
                if name == component_name {
                    String::new()
                } else {
                    format!(" and changed its saved name to '{name}' (use rename_component if you also need the in-session lookup key updated)")
                }
            ),
        });

        // Run post-write validation
        if let Some(validation) = Self::post_write_validation_schlib(filepath) {
            result["validation"] = validation;
        }

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Compares two symbols and returns a list of changes for `dry_run` preview.
    pub(crate) fn preview_symbol_changes(
        old: Option<&crate::altium::schlib::Symbol>,
        new: &crate::altium::schlib::Symbol,
    ) -> Vec<String> {
        let mut changes = Vec::new();

        if let Some(old) = old {
            if old.description != new.description {
                changes.push(format!(
                    "description: '{}' -> '{}'",
                    old.description, new.description
                ));
            }
            if old.designator != new.designator {
                changes.push(format!(
                    "designator: '{}' -> '{}'",
                    old.designator, new.designator
                ));
            }
            if old.part_count != new.part_count {
                changes.push(format!(
                    "part_count: {} -> {}",
                    old.part_count, new.part_count
                ));
            }
            // Every record kind, from the enum, so the preview cannot miss an
            // added or removed kind; footprint links are not a kind.
            for kind in crate::altium::schlib::SchPrimitiveKind::WRITE_ORDER {
                let (old_len, new_len) = (old.count_of(kind), new.count_of(kind));
                if old_len != new_len {
                    changes.push(format!("{}_count: {old_len} -> {new_len}", kind.name()));
                }
            }
            if old.footprints.len() != new.footprints.len() {
                changes.push(format!(
                    "footprint_count: {} -> {}",
                    old.footprints.len(),
                    new.footprints.len()
                ));
            }
        } else {
            changes.push("component will be created".to_string());
        }

        if changes.is_empty() {
            changes.push("no structural changes detected".to_string());
        }

        changes
    }

    /// Searches for components across multiple libraries using regex or glob patterns.
    pub(crate) fn call_search_components(&self, arguments: &Value) -> ToolCallResult {
        let Some(filepaths) = arguments.get("filepaths").and_then(Value::as_array) else {
            return ToolCallResult::error("Missing required parameter: filepaths");
        };

        let paths: Vec<&str> = filepaths.iter().filter_map(Value::as_str).collect();

        if paths.is_empty() {
            return ToolCallResult::error("filepaths must contain at least one path");
        }

        let Some(pattern) = arguments.get("pattern").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: pattern");
        };

        let pattern_type = arguments
            .get("pattern_type")
            .and_then(Value::as_str)
            .unwrap_or("glob");

        if !["glob", "regex"].contains(&pattern_type) {
            return ToolCallResult::error("pattern_type must be one of: 'glob', 'regex'");
        }

        // Validate all paths
        for path in &paths {
            if let Err(e) = self.validate_path(path) {
                return ToolCallResult::error(e);
            }
        }

        // Convert glob to regex if needed
        let regex_pattern = if pattern_type == "glob" {
            Self::glob_to_regex(pattern)
        } else {
            pattern.to_string()
        };

        // Compile the regex
        let regex = match regex::Regex::new(&format!("(?i)^{regex_pattern}$")) {
            Ok(r) => r,
            Err(e) => return ToolCallResult::error(format!("Invalid pattern: {e}")),
        };

        let mut matches: Vec<Value> = Vec::new();
        let mut searched_count = 0;
        let mut errors: Vec<String> = Vec::new();

        for path in &paths {
            let ext = std::path::Path::new(path)
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_lowercase);

            match ext.as_deref() {
                Some("pcblib") => match Self::search_pcblib(path, &regex) {
                    Ok((names, count)) => {
                        for name in names {
                            matches.push(json!({
                                "name": name,
                                "library": path,
                                "type": "PcbLib"
                            }));
                        }
                        searched_count += count;
                    }
                    Err(e) => errors.push(format!("{path}: {e}")),
                },
                Some("schlib") => match Self::search_schlib(path, &regex) {
                    Ok((names, count)) => {
                        for name in names {
                            matches.push(json!({
                                "name": name,
                                "library": path,
                                "type": "SchLib"
                            }));
                        }
                        searched_count += count;
                    }
                    Err(e) => errors.push(format!("{path}: {e}")),
                },
                _ => errors.push(super::unsupported_file_type(path)),
            }
        }

        let result = json!({
            "status": if errors.is_empty() { "success" } else { "partial" },
            "pattern": pattern,
            "pattern_type": pattern_type,
            "libraries_searched": paths.len(),
            "components_searched": searched_count,
            "matches_found": matches.len(),
            "matches": matches,
            "errors": if errors.is_empty() { Value::Null } else { json!(errors) },
            "message": format!(
                "Found {} matches for '{}' across {} libraries ({} components searched)",
                matches.len(),
                pattern,
                paths.len(),
                searched_count
            ),
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Converts a glob pattern to a regex pattern.
    pub(crate) fn glob_to_regex(glob: &str) -> String {
        let mut regex = String::with_capacity(glob.len() * 2);
        for c in glob.chars() {
            match c {
                '*' => regex.push_str(".*"),
                '?' => regex.push('.'),
                '.' | '+' | '^' | '$' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\' => {
                    regex.push('\\');
                    regex.push(c);
                }
                _ => regex.push(c),
            }
        }
        regex
    }

    /// Searches a `PcbLib` for component names matching the regex.
    pub(crate) fn search_pcblib(
        path: &str,
        regex: &regex::Regex,
    ) -> Result<(Vec<String>, usize), String> {
        use crate::altium::PcbLib;

        let library = PcbLib::open(path).map_err(|e| format!("Failed to read: {e}"))?;
        let total = library.len();
        let matching: Vec<String> = library
            .iter()
            .filter(|fp| regex.is_match(&fp.name))
            .map(|fp| fp.name.clone())
            .collect();

        Ok((matching, total))
    }

    /// Searches a `SchLib` for component names matching the regex.
    pub(crate) fn search_schlib(
        path: &str,
        regex: &regex::Regex,
    ) -> Result<(Vec<String>, usize), String> {
        use crate::altium::SchLib;

        let library = SchLib::open(path).map_err(|e| format!("Failed to read: {e}"))?;
        let total = library.len();
        let matching: Vec<String> = library
            .iter()
            .filter(|s| regex.is_match(&s.name))
            .map(|s| s.name.clone())
            .collect();

        Ok((matching, total))
    }

    /// Gets a single component by name from an Altium library.
    pub(crate) fn call_get_component(&self, arguments: &Value) -> ToolCallResult {
        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        let Some(component_name) = arguments.get("component_name").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: component_name");
        };

        // Validate path
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        let ext = std::path::Path::new(filepath)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        match ext.as_deref() {
            Some("pcblib") => Self::get_pcblib_component(filepath, component_name),
            Some("schlib") => Self::get_schlib_component(filepath, component_name),
            _ => ToolCallResult::error(super::unsupported_file_type(filepath)),
        }
    }

    /// Gets a single footprint from a `PcbLib` file.
    pub(crate) fn get_pcblib_component(filepath: &str, component_name: &str) -> ToolCallResult {
        use crate::altium::PcbLib;

        let library = match PcbLib::open(filepath) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
        };

        let Some(footprint) = library.get(component_name) else {
            return ToolCallResult::error(super::component_not_found(
                component_name,
                &library.names(),
            ));
        };

        let result = json!({
            "status": "success",
            "filepath": filepath,
            "component_name": component_name,
            "type": "PcbLib",
            "units": "mm",
            "component": footprint,
            "message": format!("Retrieved footprint '{}' from '{}'", component_name, filepath),
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Gets a single symbol from a `SchLib` file.
    pub(crate) fn get_schlib_component(filepath: &str, component_name: &str) -> ToolCallResult {
        use crate::altium::SchLib;

        let library = match SchLib::open(filepath) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
        };

        let Some(symbol) = library.get(component_name) else {
            return ToolCallResult::error(super::component_not_found(
                component_name,
                &library.names(),
            ));
        };

        let result = json!({
            "status": "success",
            "filepath": filepath,
            "component_name": component_name,
            "type": "SchLib",
            "units": "schematic units (10 = 1 grid)",
            "component": symbol,
            "message": format!("Retrieved symbol '{}' from '{}'", component_name, filepath),
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Checks if one or more components exist in an Altium library.
    pub(crate) fn call_component_exists(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::{PcbLib, SchLib};

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        let Some(names) = arguments.get("component_names").and_then(Value::as_array) else {
            return ToolCallResult::error("Missing required parameter: component_names");
        };

        // Convert names to strings
        let names: Vec<&str> = names.iter().filter_map(Value::as_str).collect();

        if names.is_empty() {
            return ToolCallResult::error(
                "component_names array is empty or contains non-string values",
            );
        }

        // Validate path
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        let ext = std::path::Path::new(filepath)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        let results: Vec<Value> = match ext.as_deref() {
            Some("pcblib") => {
                let library = match PcbLib::open(filepath) {
                    Ok(lib) => lib,
                    Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
                };
                names
                    .iter()
                    .map(|name| {
                        json!({
                            "name": *name,
                            "exists": library.get(name).is_some(),
                        })
                    })
                    .collect()
            }
            Some("schlib") => {
                let library = match SchLib::open(filepath) {
                    Ok(lib) => lib,
                    Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
                };
                names
                    .iter()
                    .map(|name| {
                        json!({
                            "name": *name,
                            "exists": library.get(name).is_some(),
                        })
                    })
                    .collect()
            }
            _ => return ToolCallResult::error(super::unsupported_file_type(filepath)),
        };

        let all_exist = results
            .iter()
            .all(|r| r["exists"].as_bool().unwrap_or(false));
        let exists_count = results
            .iter()
            .filter(|r| r["exists"].as_bool().unwrap_or(false))
            .count();

        let result = json!({
            "status": "success",
            "filepath": filepath,
            "checked_count": results.len(),
            "exists_count": exists_count,
            "all_exist": all_exist,
            "results": results,
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::tools::test_support::{
        create_test_pcblib, create_test_schlib, create_test_server, get_result_text,
        parse_result_json, test_temp_dir,
    };

    // ==================== search_components ====================

    #[test]
    fn search_components_glob_across_both_library_types() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let pcb = dir.path().join("Search.PcbLib");
        let sch = dir.path().join("Search.SchLib");
        create_test_pcblib(&pcb);
        create_test_schlib(&sch);

        let result = server.call_search_components(&json!({
            "filepaths": [pcb.to_string_lossy(), sch.to_string_lossy()],
            "pattern": "C*",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["pattern_type"], "glob");
        assert_eq!(parsed["libraries_searched"], 2);
        assert_eq!(parsed["components_searched"], 4);
        // CHIP_0402, CHIP_0603 (PcbLib) and CAPACITOR (SchLib) match "C*".
        assert_eq!(parsed["matches_found"], 3);
        let matches = parsed["matches"].as_array().unwrap();
        assert!(matches
            .iter()
            .any(|m| m["name"] == "CHIP_0402" && m["type"] == "PcbLib"));
        assert!(matches
            .iter()
            .any(|m| m["name"] == "CAPACITOR" && m["type"] == "SchLib"));
        assert_eq!(parsed["errors"], Value::Null);
    }

    #[test]
    fn search_components_regex_mode_is_case_insensitive() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let sch = dir.path().join("Regex.SchLib");
        create_test_schlib(&sch);

        let result = server.call_search_components(&json!({
            "filepaths": [sch.to_string_lossy()],
            "pattern": "res.stor",
            "pattern_type": "regex",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["matches_found"], 1);
        assert_eq!(parsed["matches"][0]["name"], "RESISTOR");
    }

    #[test]
    fn search_components_partial_status_on_unsupported_file() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let sch = dir.path().join("Ok.SchLib");
        create_test_schlib(&sch);
        let txt = dir.path().join("bad.txt");
        std::fs::write(&txt, b"x").unwrap();

        let result = server.call_search_components(&json!({
            "filepaths": [sch.to_string_lossy(), txt.to_string_lossy()],
            "pattern": "*",
        }));
        assert!(!result.is_error);
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "partial");
        assert_eq!(parsed["matches_found"], 2);
        let errors = parsed["errors"].as_array().unwrap();
        assert_eq!(errors.len(), 1);
        assert!(errors[0]
            .as_str()
            .unwrap()
            .contains("Unsupported file type"));
    }

    #[test]
    fn search_components_rejects_bad_arguments() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let sch = dir.path().join("Bad.SchLib");
        create_test_schlib(&sch);

        let result = server.call_search_components(&json!({}));
        assert!(result.is_error);
        assert_eq!(
            get_result_text(&result),
            "Missing required parameter: filepaths"
        );

        let result = server.call_search_components(&json!({ "filepaths": [] }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("at least one path"));

        let result = server.call_search_components(&json!({
            "filepaths": [sch.to_string_lossy()],
            "pattern": "x",
            "pattern_type": "fuzzy",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("pattern_type must be one of"));

        let result = server.call_search_components(&json!({
            "filepaths": [sch.to_string_lossy()],
            "pattern": "(unclosed",
            "pattern_type": "regex",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Invalid pattern"));
    }

    #[test]
    fn glob_to_regex_escapes_metacharacters() {
        assert_eq!(McpServer::glob_to_regex("CHIP_*"), "CHIP_.*");
        assert_eq!(McpServer::glob_to_regex("R?"), "R.");
        assert_eq!(McpServer::glob_to_regex("a.b+c"), "a\\.b\\+c");
    }

    // ==================== get_component ====================

    #[test]
    fn get_component_pcblib_returns_full_footprint() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Get.PcbLib");
        create_test_pcblib(&path);

        let result = server.call_get_component(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "CHIP_0402",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["type"], "PcbLib");
        assert_eq!(parsed["units"], "mm");
        assert_eq!(parsed["component"]["name"], "CHIP_0402");
        assert_eq!(parsed["component"]["description"], "0402 chip resistor");
        let pads = parsed["component"]["pads"].as_array().unwrap();
        assert_eq!(pads.len(), 2);
        assert_eq!(pads[0]["designator"], "1");
    }

    #[test]
    fn get_component_schlib_returns_full_symbol() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Get.SchLib");
        create_test_schlib(&path);

        let result = server.call_get_component(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "RESISTOR",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["type"], "SchLib");
        assert_eq!(parsed["component"]["name"], "RESISTOR");
        assert_eq!(parsed["component"]["designator"], "R?");
        assert_eq!(parsed["component"]["pins"].as_array().unwrap().len(), 2);
        assert_eq!(
            parsed["component"]["rectangles"].as_array().unwrap().len(),
            1
        );
    }

    #[test]
    fn get_component_not_found_lists_available() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("GetErr.PcbLib");
        create_test_pcblib(&path);

        let result = server.call_get_component(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "GHOST",
        }));
        assert!(result.is_error);
        let text = get_result_text(&result);
        assert!(text.contains("'GHOST' not found"));
        assert!(text.contains("CHIP_0402"));
        assert!(text.contains("CHIP_0603"));
    }

    #[test]
    fn get_component_rejects_bad_arguments() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());

        let result = server.call_get_component(&json!({}));
        assert!(result.is_error);
        assert_eq!(
            get_result_text(&result),
            "Missing required parameter: filepath"
        );

        let txt = dir.path().join("x.txt");
        let result = server.call_get_component(&json!({
            "filepath": txt.to_string_lossy(),
            "component_name": "A",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Unsupported file type"));
    }

    // ==================== component_exists ====================

    #[test]
    fn component_exists_reports_per_name_status() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Exists.PcbLib");
        create_test_pcblib(&path);

        let result = server.call_component_exists(&json!({
            "filepath": path.to_string_lossy(),
            "component_names": ["CHIP_0402", "GHOST", "CHIP_0603"],
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["checked_count"], 3);
        assert_eq!(parsed["exists_count"], 2);
        assert_eq!(parsed["all_exist"], false);
        assert_eq!(parsed["results"][0]["exists"], true);
        assert_eq!(parsed["results"][1]["name"], "GHOST");
        assert_eq!(parsed["results"][1]["exists"], false);
        assert_eq!(parsed["results"][2]["exists"], true);
    }

    #[test]
    fn component_exists_schlib_all_exist() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Exists.SchLib");
        create_test_schlib(&path);

        let result = server.call_component_exists(&json!({
            "filepath": path.to_string_lossy(),
            "component_names": ["RESISTOR", "CAPACITOR"],
        }));
        assert!(!result.is_error);
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["all_exist"], true);
        assert_eq!(parsed["exists_count"], 2);
    }

    #[test]
    fn component_exists_rejects_bad_arguments() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("ExistsBad.PcbLib");
        create_test_pcblib(&path);

        let result = server.call_component_exists(&json!({
            "filepath": path.to_string_lossy(),
        }));
        assert!(result.is_error);
        assert_eq!(
            get_result_text(&result),
            "Missing required parameter: component_names"
        );

        let result = server.call_component_exists(&json!({
            "filepath": path.to_string_lossy(),
            "component_names": [],
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("empty"));

        let txt = dir.path().join("x.txt");
        let result = server.call_component_exists(&json!({
            "filepath": txt.to_string_lossy(),
            "component_names": ["A"],
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Unsupported file type"));
    }

    // ==================== update_component ====================

    #[test]
    fn update_component_pcblib_replaces_footprint() {
        use crate::altium::PcbLib;

        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Update.PcbLib");
        create_test_pcblib(&path);

        let result = server.call_update_component(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "CHIP_0402",
            "footprint": {
                "description": "reworked 0402",
                "pads": [
                    { "designator": "1", "x": -0.55, "y": 0.0, "width": 0.65, "height": 0.55 },
                    { "designator": "2", "x": 0.55, "y": 0.0, "width": 0.65, "height": 0.55 },
                    { "designator": "3", "x": 0.0, "y": 0.6, "width": 0.4, "height": 0.4 }
                ],
                "tracks": [
                    { "x1": -1.0, "y1": -0.6, "x2": 1.0, "y2": -0.6, "width": 0.15, "layer": "Top Overlay" }
                ],
            },
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["file_type"], "PcbLib");
        assert_eq!(parsed["renamed"], false);
        assert_eq!(parsed["old_description"], "0402 chip resistor");
        assert_eq!(parsed["component_count"], 2);

        let lib = PcbLib::open(&path).unwrap();
        let fp = lib.get("CHIP_0402").unwrap();
        assert_eq!(fp.description, "reworked 0402");
        assert_eq!(fp.pads.len(), 3);
        assert_eq!(fp.tracks.len(), 1);
    }

    /// The natural edit loop — `get_component` → `update_component` with the
    /// echoed JSON — keeps the footprint's kind-85 identity and interleaved
    /// stream order, mirroring the create path's replay.
    #[test]
    fn update_component_replays_footprint_fidelity_fields() {
        use crate::altium::pcblib::{Footprint, Layer, Pad, PcbLib, PrimitiveKind, Track};

        let dir = test_temp_dir();
        let server = create_test_server(dir.path());

        // Track before pad: an interleaving the grouped default would lose.
        let mut fp = Footprint::new("FID");
        fp.guid = Some("{11111111-2222-3333-4444-555555555555}".to_string());
        fp.add_track(Track::new(-1.0, -1.0, 1.0, -1.0, 0.2, Layer::TopOverlay));
        fp.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
        let mut lib = PcbLib::new();
        lib.add(fp);
        let path = dir.path().join("FidUpdate.PcbLib");
        lib.save(&path).unwrap();

        let fetched = parse_result_json(&server.call_get_component(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "FID",
        })));
        assert_eq!(
            fetched["component"]["primitive_order"],
            json!(["track", "pad"]),
            "get_component echoes the interleaved order"
        );

        let result = server.call_update_component(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "FID",
            "footprint": fetched["component"],
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));

        let reopened = PcbLib::open(&path).unwrap();
        let fp = reopened.get("FID").unwrap();
        assert_eq!(
            fp.guid.as_deref(),
            Some("{11111111-2222-3333-4444-555555555555}")
        );
        assert_eq!(
            fp.primitive_order,
            vec![PrimitiveKind::Track, PrimitiveKind::Pad]
        );
    }

    /// The `SchLib` flavour: designator position/identity and the interleaved
    /// record order survive the get → update loop.
    #[test]
    fn update_component_replays_symbol_fidelity_fields() {
        use crate::altium::schlib::{Line, Pin, PinOrientation, SchLib, SchPrimitiveKind, Symbol};

        let dir = test_temp_dir();
        let server = create_test_server(dir.path());

        let mut symbol = Symbol::new("FID");
        symbol.designator = "U?".to_string();
        symbol.designator_x = 5.5;
        symbol.designator_y = -3.5;
        symbol.add_line(Line::new(0.0, 0.0, 10.0, 10.0));
        symbol.add_pin(Pin::new("IN", "1", 0, 0, 10, PinOrientation::Left));
        let mut lib = SchLib::new();
        lib.add(symbol);
        let path = dir.path().join("FidUpdate.SchLib");
        lib.save(&path).unwrap();

        let fetched = parse_result_json(&server.call_get_component(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "FID",
        })));

        let result = server.call_update_component(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "FID",
            "symbol": fetched["component"],
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));

        let reopened = SchLib::open(&path).unwrap();
        let symbol = reopened.get("FID").unwrap();
        assert!((symbol.designator_x - 5.5).abs() < 1e-9);
        assert!((symbol.designator_y - (-3.5)).abs() < 1e-9);
        assert!(symbol.designator_unique_id.is_some());
        assert_eq!(
            symbol.primitive_order,
            vec![SchPrimitiveKind::Line, SchPrimitiveKind::Pin]
        );
    }

    /// A replacement carrying another component's name is a collision, not a
    /// rename — refused on both formats, in dry-run too, while renaming onto a
    /// free name still works.
    #[test]
    fn update_component_refuses_to_rename_onto_an_existing_name() {
        use crate::altium::{PcbLib, SchLib};

        let dir = test_temp_dir();
        let server = create_test_server(dir.path());

        let pcb = dir.path().join("Collide.PcbLib");
        create_test_pcblib(&pcb); // CHIP_0402 + CHIP_0603
        for dry_run in [true, false] {
            let result = server.call_update_component(&json!({
                "filepath": pcb.to_string_lossy(),
                "component_name": "CHIP_0402",
                "dry_run": dry_run,
                "footprint": {
                    "name": "CHIP_0603",
                    "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 0.6, "height": 0.5 }],
                },
            }));
            assert!(result.is_error, "dry_run={dry_run}");
            assert!(get_result_text(&result).contains("already exists"));
        }
        let lib = PcbLib::open(&pcb).unwrap();
        assert_eq!(lib.len(), 2, "nothing changed");
        assert!(lib.get("CHIP_0402").is_some() && lib.get("CHIP_0603").is_some());

        // Renaming onto a free name is still a rename.
        let result = server.call_update_component(&json!({
            "filepath": pcb.to_string_lossy(),
            "component_name": "CHIP_0402",
            "footprint": {
                "name": "CHIP_0402_V2",
                "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 0.6, "height": 0.5 }],
            },
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        assert_eq!(parse_result_json(&result)["renamed"], true);
        let lib = PcbLib::open(&pcb).unwrap();
        assert!(lib.get("CHIP_0402").is_none() && lib.get("CHIP_0402_V2").is_some());

        let sch = dir.path().join("Collide.SchLib");
        create_test_schlib(&sch); // RESISTOR + CAPACITOR
        let result = server.call_update_component(&json!({
            "filepath": sch.to_string_lossy(),
            "component_name": "RESISTOR",
            "symbol": { "name": "CAPACITOR", "pins": [] },
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("already exists"));
        let lib = SchLib::open(&sch).unwrap();
        assert_eq!(lib.len(), 2, "nothing changed");

        // A rename onto a name with a control character is refused too, on both formats.
        for (path, key, body) in [
            (
                &pcb,
                "footprint",
                json!({ "name": "BAD\tNAME", "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 0.6, "height": 0.5 }] }),
            ),
            (&sch, "symbol", json!({ "name": "BAD\tNAME", "pins": [] })),
        ] {
            let result = server.call_update_component(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": if key == "footprint" { "CHIP_0603" } else { "RESISTOR" },
                key: body,
            }));
            assert!(result.is_error, "{key}");
            assert!(
                get_result_text(&result).contains("control character"),
                "{}",
                get_result_text(&result)
            );
        }
    }

    /// A malformed `primitive_order` in a replacement is ignored with the
    /// default grouped order rather than failing the update, on both formats —
    /// the same advisory treatment the create path gives it.
    #[test]
    fn update_component_ignores_a_malformed_primitive_order() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());

        let pcb = dir.path().join("Order.PcbLib");
        create_test_pcblib(&pcb);
        let result = server.call_update_component(&json!({
            "filepath": pcb.to_string_lossy(),
            "component_name": "CHIP_0402",
            "footprint": {
                "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 0.6, "height": 0.5 }],
                "primitive_order": ["not_a_kind"],
            },
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));

        let sch = dir.path().join("Order.SchLib");
        create_test_schlib(&sch);
        let result = server.call_update_component(&json!({
            "filepath": sch.to_string_lossy(),
            "component_name": "RESISTOR",
            "symbol": { "pins": [], "primitive_order": 42 },
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
    }

    /// The caller may name the component in another case: a replacement
    /// without a name keeps the stored spelling (no rename happens, none is
    /// reported), and an explicit name in another case is the one case-only
    /// rename that is allowed.
    #[test]
    fn update_component_keeps_the_stored_spelling_unless_renamed() {
        use crate::altium::{PcbLib, SchLib};
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());

        let pcb = dir.path().join("Spelling.PcbLib");
        create_test_pcblib(&pcb); // CHIP_0402 + CHIP_0603
        let result = server.call_update_component(&json!({
            "filepath": pcb.to_string_lossy(),
            "component_name": "chip_0402",
            "footprint": { "description": "touched", "pads": [] },
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["renamed"], false, "{parsed}");
        assert_eq!(parsed["component_name"], "CHIP_0402");
        assert_eq!(
            PcbLib::open(&pcb).unwrap().names(),
            ["CHIP_0402", "CHIP_0603"]
        );

        let result = server.call_update_component(&json!({
            "filepath": pcb.to_string_lossy(),
            "component_name": "chip_0402",
            "footprint": { "name": "Chip_0402", "pads": [] },
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        assert_eq!(parse_result_json(&result)["renamed"], true);
        assert_eq!(
            PcbLib::open(&pcb).unwrap().names(),
            ["Chip_0402", "CHIP_0603"]
        );

        let sch = dir.path().join("Spelling.SchLib");
        create_test_schlib(&sch); // RESISTOR + CAPACITOR
        let result = server.call_update_component(&json!({
            "filepath": sch.to_string_lossy(),
            "component_name": "resistor",
            "symbol": { "pins": [] },
            "dry_run": true,
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        assert_eq!(parse_result_json(&result)["would_rename"], false);
        let result = server.call_update_component(&json!({
            "filepath": sch.to_string_lossy(),
            "component_name": "resistor",
            "symbol": { "pins": [] },
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        assert_eq!(parse_result_json(&result)["renamed"], false);
        assert_eq!(
            SchLib::open(&sch).unwrap().names(),
            ["RESISTOR", "CAPACITOR"]
        );
    }

    #[test]
    fn update_component_pcblib_dry_run_previews_changes() {
        use crate::altium::PcbLib;

        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("UpdateDry.PcbLib");
        create_test_pcblib(&path);

        let result = server.call_update_component(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "CHIP_0402",
            "footprint": {
                "description": "changed",
                "pads": [
                    { "designator": "1", "x": -0.5, "y": 0.0, "width": 0.6, "height": 0.5 }
                ],
            },
            "dry_run": true,
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "dry_run");
        assert_eq!(parsed["would_rename"], false);
        let changes = parsed["changes"].as_array().unwrap();
        assert!(changes
            .iter()
            .any(|c| c.as_str().unwrap().starts_with("description:")));
        assert!(changes
            .iter()
            .any(|c| c.as_str().unwrap() == "pad_count: 2 -> 1"));

        // Nothing was written.
        let lib = PcbLib::open(&path).unwrap();
        assert_eq!(lib.get("CHIP_0402").unwrap().pads.len(), 2);
    }

    #[test]
    fn update_component_schlib_replaces_symbol() {
        use crate::altium::SchLib;

        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Update.SchLib");
        create_test_schlib(&path);

        let result = server.call_update_component(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "RESISTOR",
            "symbol": {
                "description": "Precision resistor",
                "designator": "R?",
                "part_count": 2,
                "pins": [
                    { "designator": "1", "name": "1", "x": -30, "y": 0, "length": 10, "orientation": "left" },
                    { "designator": "2", "name": "2", "x": 30, "y": 0, "length": 10, "orientation": "right" }
                ],
                "rectangles": [
                    { "x1": -20, "y1": -10, "x2": 20, "y2": 10 }
                ],
                "lines": [
                    { "x1": -20, "y1": 0, "x2": 20, "y2": 0 }
                ],
                "parameters": [
                    { "name": "Tolerance", "value": "0.1%" }
                ],
                "labels": [
                    { "x": 0, "y": 15, "text": "precision" }
                ],
            },
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["file_type"], "SchLib");
        assert_eq!(parsed["renamed"], false);
        assert_eq!(parsed["old_description"], "Generic resistor");

        let lib = SchLib::open(&path).unwrap();
        let sym = lib.get("RESISTOR").unwrap();
        assert_eq!(sym.description, "Precision resistor");
        assert_eq!(sym.part_count, 2);
        assert_eq!(sym.pins.len(), 2);
        assert_eq!(sym.pins[0].x, -30);
        assert_eq!(sym.lines.len(), 1);
        assert_eq!(sym.labels.len(), 1);
        assert_eq!(sym.parameters[0].name, "Tolerance");
        assert_eq!(sym.parameters[0].value, "0.1%");
    }

    #[test]
    fn update_component_schlib_dry_run_previews_family_counts() {
        use crate::altium::SchLib;

        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("UpdateDry.SchLib");
        create_test_schlib(&path);

        let result = server.call_update_component(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "CAPACITOR",
            "symbol": {
                "designator": "C?",
                "pins": [
                    { "designator": "1", "name": "1", "x": -20, "y": 0, "length": 10 }
                ],
            },
            "dry_run": true,
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "dry_run");
        let changes = parsed["changes"].as_array().unwrap();
        assert!(changes
            .iter()
            .any(|c| c.as_str().unwrap() == "pin_count: 2 -> 1"));

        // Nothing was written.
        let lib = SchLib::open(&path).unwrap();
        assert_eq!(lib.get("CAPACITOR").unwrap().pins.len(), 2);
    }

    #[test]
    fn update_component_error_paths() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let pcb = dir.path().join("UpdErr.PcbLib");
        let sch = dir.path().join("UpdErr.SchLib");
        create_test_pcblib(&pcb);
        create_test_schlib(&sch);

        let result = server.call_update_component(&json!({}));
        assert!(result.is_error);
        assert_eq!(
            get_result_text(&result),
            "Missing required parameter: filepath"
        );

        // PcbLib update requires a footprint payload.
        let result = server.call_update_component(&json!({
            "filepath": pcb.to_string_lossy(),
            "component_name": "CHIP_0402",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Missing required parameter: footprint"));

        // SchLib update requires a symbol payload.
        let result = server.call_update_component(&json!({
            "filepath": sch.to_string_lossy(),
            "component_name": "RESISTOR",
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("Missing required parameter: symbol"));

        // Unknown component lists the available ones.
        let result = server.call_update_component(&json!({
            "filepath": pcb.to_string_lossy(),
            "component_name": "GHOST",
            "footprint": { "pads": [] },
        }));
        assert!(result.is_error);
        let text = get_result_text(&result);
        assert!(text.contains("'GHOST' not found"));
        assert!(text.contains("CHIP_0402"));

        // Out-of-range geometry is rejected before save.
        let result = server.call_update_component(&json!({
            "filepath": pcb.to_string_lossy(),
            "component_name": "CHIP_0402",
            "footprint": {
                "pads": [
                    { "designator": "1", "x": 999_999.0, "y": 0.0, "width": 0.6, "height": 0.5 }
                ],
            },
        }));
        assert!(result.is_error);
    }

    // ==================== update_component: full-family parse + preview ====================

    mod update_families {
        use super::*;
        use serde_json::Value;

        /// A footprint payload carrying one of every 2D primitive family.
        fn rich_footprint() -> Value {
            json!({
                "description": "rich",
                "pads": [
                    { "designator": "1", "x": -0.5, "y": 0.0, "width": 0.6, "height": 0.5 },
                    { "designator": "2", "x": 0.5, "y": 0.0, "width": 0.6, "height": 0.5 }
                ],
                "tracks": [{ "x1": -1.0, "y1": -0.6, "x2": 1.0, "y2": -0.6, "width": 0.15, "layer": "Top Overlay" }],
                "arcs": [{ "x": 0.0, "y": 0.0, "radius": 0.5, "start_angle": 0.0, "end_angle": 90.0, "width": 0.1, "layer": "Top Overlay" }],
                "regions": [{ "layer": "Top Layer", "kind": "copper",
                    "vertices": [ {"x": -0.5,"y": -0.5}, {"x": 0.5,"y": -0.5}, {"x": 0.0,"y": 0.5} ] }],
                "vias": [{ "x": 0.0, "y": 0.0, "diameter": 0.6, "hole_size": 0.3 }],
                "fills": [{ "x1": -0.3, "y1": -0.3, "x2": 0.3, "y2": 0.3, "layer": "Top Layer" }],
                "component_bodies": [{ "model_name": "CHIP", "overall_height": 0.5, "standoff_height": 0.0,
                    "outline": [ {"x": -0.5,"y": -0.25}, {"x": 0.5,"y": -0.25}, {"x": 0.5,"y": 0.25}, {"x": -0.5,"y": 0.25} ] }],
                "text": [{ "x": 0.0, "y": 0.7, "text": "R1", "height": 0.3, "layer": "Top Overlay" }]
            })
        }

        /// A symbol payload carrying one of every schematic primitive family.
        fn rich_symbol() -> Value {
            json!({
                "designator": "R?",
                "pins": [
                    { "designator": "1", "name": "1", "x": -20, "y": 0, "length": 10, "orientation": "left" },
                    { "designator": "2", "name": "2", "x": 20, "y": 0, "length": 10, "orientation": "right" }
                ],
                "polylines": [{ "points": [ {"x": -10,"y": 0}, {"x": 0,"y": 5}, {"x": 10,"y": 0} ] }],
                "arcs": [{ "x": 0, "y": 0, "radius": 8, "start_angle": 0.0, "end_angle": 180.0 }],
                "ellipses": [{ "x": 0, "y": 0, "radius_x": 6, "radius_y": 4 }],
                "round_rects": [{ "x1": -10, "y1": -6, "x2": 10, "y2": 6, "corner_x_radius": 2, "corner_y_radius": 2 }],
                "polygons": [{ "points": [ {"x": -5,"y": -5}, {"x": 5,"y": -5}, {"x": 0,"y": 5} ] }],
                "labels": [{ "x": 0, "y": 12, "text": "R" }],
                "ieee_symbols": [{ "x": 0, "y": -12, "symbol": 3, "rotation": 90, "is_mirrored": true }],
                "pies": [{ "x": 0, "y": 0, "radius": 5, "start_angle": 0.0, "end_angle": 90.0 }],
                "images": [{ "x1": -8, "y1": -8, "x2": 8, "y2": 8, "file_name": "img.png" }],
                "text_frames": [{ "x1": -12, "y1": -14, "x2": 12, "y2": -10, "text": "frame" }]
            })
        }

        fn change_strings(parsed: &Value) -> Vec<String> {
            parsed["changes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c.as_str().unwrap_or("").to_string())
                .collect()
        }

        #[test]
        fn update_pcblib_parses_all_primitive_families() {
            use crate::altium::PcbLib;
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("UpdRich.PcbLib");
            create_test_pcblib(&path);

            let r = server.call_update_component(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "CHIP_0402",
                "footprint": rich_footprint(),
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let p = parse_result_json(&r);
            assert_eq!(p["status"], "success");
            assert_eq!(p["old_description"], "0402 chip resistor");

            let lib = PcbLib::open(&path).unwrap();
            let fp = lib.get("CHIP_0402").unwrap();
            assert_eq!(fp.tracks.len(), 1);
            assert_eq!(fp.arcs.len(), 1);
            assert_eq!(fp.regions.len(), 1);
            assert_eq!(fp.vias.len(), 1);
            assert_eq!(fp.fills.len(), 1);
            assert_eq!(fp.component_bodies.len(), 1);
            assert_eq!(fp.text.len(), 1);
        }

        #[test]
        fn update_pcblib_dry_run_previews_all_family_counts() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("UpdRichDry.PcbLib");
            create_test_pcblib(&path);
            let r = server.call_update_component(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "CHIP_0402",
                "dry_run": true,
                "footprint": rich_footprint(),
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let p = parse_result_json(&r);
            assert_eq!(p["status"], "dry_run");
            let changes = change_strings(&p);
            for expected in [
                "track_count: 0 -> 1",
                "arc_count: 0 -> 1",
                "region_count: 0 -> 1",
                "text_count: 0 -> 1",
                "via_count: 0 -> 1",
                "fill_count: 0 -> 1",
                "component_body_count: 0 -> 1",
            ] {
                assert!(
                    changes.iter().any(|c| c == expected),
                    "missing {expected}: {changes:?}"
                );
            }
        }

        /// The preview helpers take `Option<&old>` and report a creation when
        /// it is `None`. Both `update_*_component` entry points reject an
        /// unknown name before they get here, so this arm is only reachable by
        /// calling the helper directly — which pins the contract for any future
        /// caller that does allow create-on-update.
        #[test]
        fn previewing_against_no_previous_component_reports_a_creation() {
            use crate::altium::{pcblib::Footprint, schlib::Symbol};

            let changes = McpServer::preview_footprint_changes(None, &Footprint::new("NEW"));
            assert_eq!(changes, ["component will be created"], "{changes:?}");

            let changes = McpServer::preview_symbol_changes(None, &Symbol::new("NEW"));
            assert_eq!(changes, ["component will be created"], "{changes:?}");
        }

        #[test]
        fn dry_run_that_changes_nothing_says_so_rather_than_listing_nothing() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            // Re-submitting CHIP_0402's own description and pad count leaves
            // every compared field equal, so the preview must not come back
            // as an empty change list.
            let pcb = dir.path().join("NoopDry.PcbLib");
            create_test_pcblib(&pcb);
            let r = server.call_update_component(&json!({
                "filepath": pcb.to_string_lossy(),
                "component_name": "CHIP_0402",
                "dry_run": true,
                "footprint": {
                    "description": "0402 chip resistor",
                    "pads": [
                        { "designator": "1", "x": -0.5, "y": 0.0, "width": 0.6, "height": 0.5 },
                        { "designator": "2", "x": 0.5, "y": 0.0, "width": 0.6, "height": 0.5 },
                    ],
                },
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let changes = change_strings(&parse_result_json(&r));
            assert_eq!(changes, ["no structural changes detected"], "{changes:?}");

            let sch = dir.path().join("NoopDry.SchLib");
            create_test_schlib(&sch);
            let (description, designator, pin_count, rects) = {
                use crate::altium::SchLib;
                let lib = SchLib::open(&sch).unwrap();
                let s = lib.get("RESISTOR").unwrap();
                (
                    s.description.clone(),
                    s.designator.clone(),
                    s.pins.len(),
                    s.rectangles.clone(),
                )
            };
            let pins: Vec<Value> = (0..pin_count)
                .map(|i| {
                    json!({
                        "designator": format!("{}", i + 1),
                        "name": format!("P{}", i + 1),
                        "x": 0, "y": 0, "length": 10, "orientation": "left",
                    })
                })
                .collect();
            let r = server.call_update_component(&json!({
                "filepath": sch.to_string_lossy(),
                "component_name": "RESISTOR",
                "dry_run": true,
                "symbol": {
                    "description": description,
                    "designator": designator,
                    "pins": pins,
                    "rectangles": rects,
                },
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let changes = change_strings(&parse_result_json(&r));
            assert_eq!(changes, ["no structural changes detected"], "{changes:?}");
        }

        #[test]
        fn update_reports_the_new_name_when_the_component_is_renamed() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            let pcb = dir.path().join("Rename.PcbLib");
            create_test_pcblib(&pcb);
            let r = server.call_update_component(&json!({
                "filepath": pcb.to_string_lossy(),
                "component_name": "CHIP_0402",
                "footprint": { "name": "CHIP_0402_NEW", "description": "renamed" },
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let p = parse_result_json(&r);
            assert_eq!(p["renamed"], true);
            assert!(
                get_result_text(&r).contains("CHIP_0402_NEW"),
                "{}",
                get_result_text(&r)
            );

            // The SchLib side says the same thing in its own words, in both the
            // preview and the committed update.
            let sch = dir.path().join("Rename.SchLib");
            create_test_schlib(&sch);
            let renaming = json!({
                "filepath": sch.to_string_lossy(),
                "component_name": "RESISTOR",
                "symbol": { "name": "RESISTOR_NEW", "description": "renamed" },
            });
            let mut preview = renaming.clone();
            preview["dry_run"] = json!(true);
            let r = server.call_update_component(&preview);
            assert!(!r.is_error, "{}", get_result_text(&r));
            assert_eq!(parse_result_json(&r)["would_rename"], true);
            assert!(get_result_text(&r).contains("RESISTOR_NEW"));

            let r = server.call_update_component(&renaming);
            assert!(!r.is_error, "{}", get_result_text(&r));
            assert_eq!(parse_result_json(&r)["renamed"], true);
            assert!(get_result_text(&r).contains("RESISTOR_NEW"));
        }

        #[test]
        fn a_missing_name_lists_only_the_first_ten_candidates() {
            use crate::altium::{
                pcblib::{Footprint, Pad},
                schlib::Symbol,
                PcbLib, SchLib,
            };
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            // Twelve components: the error must name ten and count the rest,
            // rather than spilling the whole library into the message.
            let pcb = dir.path().join("Many.PcbLib");
            let mut lib = PcbLib::new();
            for i in 0..12 {
                let mut fp = Footprint::new(format!("FP_{i:02}"));
                fp.add_pad(Pad::smd("1", 0.0, 0.0, 0.6, 0.5));
                lib.add(fp);
            }
            lib.save(&pcb).unwrap();

            let r = server.call_get_component(&json!({
                "filepath": pcb.to_string_lossy(),
                "component_name": "ABSENT",
            }));
            assert!(r.is_error);
            let msg = get_result_text(&r);
            assert!(msg.contains("and 2 more"), "{msg}");
            assert!(msg.contains("FP_09"), "{msg}");
            assert!(!msg.contains("FP_11"), "{msg}");

            let sch = dir.path().join("Many.SchLib");
            let mut lib = SchLib::new();
            for i in 0..12 {
                lib.add(Symbol::new(format!("SYM_{i:02}")));
            }
            lib.save(&sch).unwrap();

            let r = server.call_get_component(&json!({
                "filepath": sch.to_string_lossy(),
                "component_name": "ABSENT",
            }));
            assert!(r.is_error);
            let msg = get_result_text(&r);
            assert!(msg.contains("and 2 more"), "{msg}");
            assert!(msg.contains("SYM_09"), "{msg}");
            assert!(!msg.contains("SYM_11"), "{msg}");
        }

        #[test]
        fn update_schlib_parses_all_primitive_families() {
            use crate::altium::SchLib;
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("UpdRich.SchLib");
            create_test_schlib(&path);

            let r = server.call_update_component(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "RESISTOR",
                "symbol": rich_symbol(),
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let p = parse_result_json(&r);
            assert_eq!(p["status"], "success");
            assert_eq!(p["old_description"], "Generic resistor");

            let lib = SchLib::open(&path).unwrap();
            let sym = lib.get("RESISTOR").unwrap();
            assert_eq!(sym.polylines.len(), 1);
            assert_eq!(sym.arcs.len(), 1);
            assert_eq!(sym.ellipses.len(), 1);
            assert_eq!(sym.round_rects.len(), 1);
            assert_eq!(sym.polygons.len(), 1);
            assert_eq!(sym.labels.len(), 1);
            assert_eq!(sym.pies.len(), 1);
        }

        #[test]
        fn update_schlib_dry_run_previews_designator_and_families() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("UpdRichDry.SchLib");
            create_test_schlib(&path);
            let r = server.call_update_component(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "RESISTOR",
                "dry_run": true,
                "symbol": {
                    "designator": "RN?",
                    "part_count": 4,
                    "pins": [{ "designator": "1", "name": "1", "x": -20, "y": 0, "length": 10 }],
                    "lines": [{ "x1": -10, "y1": 0, "x2": 10, "y2": 0 }],
                    "polylines": [{ "points": [ {"x":-5,"y":0}, {"x":5,"y":0} ] }],
                    "arcs": [{ "x": 0, "y": 0, "radius": 5 }],
                    "labels": [{ "x": 0, "y": 10, "text": "R" }],
                    "parameters": [{ "name": "Tol", "value": "1%" }],
                },
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let p = parse_result_json(&r);
            assert_eq!(p["status"], "dry_run");
            let changes = change_strings(&p);
            for expected in [
                "designator: 'R?' -> 'RN?'",
                "part_count: 1 -> 4",
                "pin_count: 2 -> 1",
                "line_count: 0 -> 1",
                "polyline_count: 0 -> 1",
                "arc_count: 0 -> 1",
                "rectangle_count: 1 -> 0",
                "label_count: 0 -> 1",
                "parameter_count: 0 -> 1",
            ] {
                assert!(
                    changes.iter().any(|c| c == expected),
                    "missing {expected}: {changes:?}"
                );
            }
        }
    }

    // ==================== rejection paths across the four tools ==============

    mod rejections {
        use crate::mcp::tools::test_support::{
            create_test_pcblib, create_test_schlib, create_test_server, get_result_text,
            parse_result_json, test_temp_dir,
        };
        use serde_json::json;
        use tempfile::TempDir;

        fn write_garbage(path: &std::path::Path) {
            std::fs::write(path, b"not an OLE compound document").unwrap();
        }

        /// Fails the library's next save — and ONLY the save — by occupying
        /// the deterministic temp path `save_atomic` must create beside the
        /// target (`<name>.pcblib.tmp` / `<name>.schlib.tmp`) with a
        /// directory: `File::create` over a directory fails on every platform,
        /// while the `.bak` backup (a plain copy) is untouched. Same mechanism
        /// as `BlockedSave` in `library_ops.rs`. Permissions cannot do this
        /// portably: a read-only FILE only blocks the rename-over on Windows
        /// (on Unix that permission belongs to the parent directory), and a
        /// read-only DIRECTORY fails the backup before the save is reached.
        fn block_save(path: &std::path::Path, blocked: bool) {
            let tmp_ext = if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("schlib"))
            {
                "schlib.tmp"
            } else {
                "pcblib.tmp"
            };
            let tmp = path.with_extension(tmp_ext);
            if blocked {
                std::fs::create_dir(&tmp).expect("occupy the save temp path");
            } else {
                let _ = std::fs::remove_dir(&tmp);
            }
        }

        struct Fixtures {
            dir: TempDir,
        }

        impl Fixtures {
            fn new() -> Self {
                let dir = test_temp_dir();
                create_test_pcblib(&dir.path().join("Lib.PcbLib"));
                create_test_schlib(&dir.path().join("Lib.SchLib"));
                write_garbage(&dir.path().join("Bad.PcbLib"));
                write_garbage(&dir.path().join("Bad.SchLib"));
                Self { dir }
            }

            fn path(&self, name: &str) -> String {
                self.dir.path().join(name).to_string_lossy().into_owned()
            }
        }

        fn assert_error_mentions(result: &crate::mcp::server::ToolCallResult, needle: &str) {
            let text = get_result_text(result);
            assert!(result.is_error, "expected an error, got: {text}");
            assert!(
                text.contains(needle),
                "expected the error to mention {needle:?}, got: {text}"
            );
        }

        // ---- update_component ---------------------------------------------------

        #[test]
        fn update_component_names_each_missing_argument_and_bad_extension() {
            let fx = Fixtures::new();
            let outside = test_temp_dir();
            let server = create_test_server(fx.dir.path());

            let escaped = server.call_update_component(&json!({
                "filepath": outside.path().join("X.PcbLib").to_string_lossy(),
                "component_name": "A", "footprint": {},
            }));
            assert!(escaped.is_error, "{}", get_result_text(&escaped));

            let no_component = server.call_update_component(&json!({
                "filepath": fx.path("Lib.PcbLib"), "footprint": {},
            }));
            assert_error_mentions(&no_component, "component_name");

            // The replacement body is keyed by file type, so each dispatch arm
            // demands its own key rather than a generic one.
            let no_footprint = server.call_update_component(&json!({
                "filepath": fx.path("Lib.PcbLib"), "component_name": "CHIP_0402",
            }));
            assert_error_mentions(&no_footprint, "footprint");

            let no_symbol = server.call_update_component(&json!({
                "filepath": fx.path("Lib.SchLib"), "component_name": "RESISTOR",
            }));
            assert_error_mentions(&no_symbol, "symbol");

            let wrong_ext = server.call_update_component(&json!({
                "filepath": fx.path("Lib.txt"), "component_name": "A", "footprint": {},
            }));
            assert_error_mentions(&wrong_ext, "Unsupported file type");
        }

        #[test]
        fn update_component_reports_unreadable_libraries_and_missing_components() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());

            let bad_pcb = server.call_update_component(&json!({
                "filepath": fx.path("Bad.PcbLib"), "component_name": "A", "footprint": {},
            }));
            assert_error_mentions(&bad_pcb, "Failed to read library");

            let bad_sch = server.call_update_component(&json!({
                "filepath": fx.path("Bad.SchLib"), "component_name": "A", "symbol": {},
            }));
            assert_error_mentions(&bad_sch, "Failed to read library");

            // The rejection lists what the library does hold, so the caller can
            // correct the name without a second call.
            let missing_fp = server.call_update_component(&json!({
                "filepath": fx.path("Lib.PcbLib"), "component_name": "GHOST", "footprint": {},
            }));
            assert_error_mentions(&missing_fp, "Available");

            let missing_sym = server.call_update_component(&json!({
                "filepath": fx.path("Lib.SchLib"), "component_name": "GHOST", "symbol": {},
            }));
            assert_error_mentions(&missing_sym, "Available");
        }

        #[test]
        fn update_component_reports_which_primitive_failed_to_parse() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());

            // The update path IS the create path's parser, so a malformed
            // primitive is named and indexed the same way rather than dropped,
            // under the updating tool's name.
            let cases: [(&str, serde_json::Value, &str); 5] = [
                (
                    "pads",
                    json!([{ "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 }]),
                    "Failed to parse pad at index 0",
                ),
                (
                    "tracks",
                    json!([{ "y1": 0.0, "x2": 1.0, "y2": 0.0, "width": 0.2 }]),
                    "Failed to parse track at index 0",
                ),
                (
                    "arcs",
                    json!([{ "x": 0.0, "y": 0.0, "radius": 1.0, "start_angle": 0.0, "end_angle": 90.0 }]),
                    "Failed to parse arc at index 0",
                ),
                (
                    "vias",
                    json!([{ "x": 0.0, "y": 0.0, "diameter": 0.0, "hole_size": 0.3 }]),
                    "Failed to parse via at index 0",
                ),
                (
                    "fills",
                    json!([{ "y1": 0.0, "x2": 1.0, "y2": 1.0 }]),
                    "Failed to parse fill at index 0",
                ),
            ];

            for (key, payload, expected) in cases {
                let mut footprint = json!({ "name": "CHIP_0402" });
                footprint[key] = payload;
                let r = server.call_update_component(&json!({
                    "filepath": fx.path("Lib.PcbLib"),
                    "component_name": "CHIP_0402",
                    "footprint": footprint,
                }));
                assert_error_mentions(&r, expected);
                assert_error_mentions(&r, "update_component");
            }
        }

        #[test]
        fn update_component_reports_a_failed_write_for_both_types() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());

            let pcb = fx.path("Lib.PcbLib");
            block_save(std::path::Path::new(&pcb), true);
            let pcb_result = server.call_update_component(&json!({
                "filepath": &pcb, "component_name": "CHIP_0402",
                "footprint": { "name": "CHIP_0402", "description": "changed" },
            }));
            block_save(std::path::Path::new(&pcb), false);
            assert!(pcb_result.is_error, "{}", get_result_text(&pcb_result));

            let sch = fx.path("Lib.SchLib");
            block_save(std::path::Path::new(&sch), true);
            let sch_result = server.call_update_component(&json!({
                "filepath": &sch, "component_name": "RESISTOR",
                "symbol": { "name": "RESISTOR", "description": "changed" },
            }));
            block_save(std::path::Path::new(&sch), false);
            assert!(sch_result.is_error, "{}", get_result_text(&sch_result));
        }

        #[test]
        fn update_component_dry_run_describes_the_change_without_writing() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());
            let pcb = fx.path("Lib.PcbLib");

            let r = server.call_update_component(&json!({
                "filepath": &pcb, "component_name": "CHIP_0402",
                "footprint": { "name": "CHIP_0402_NEW", "description": "renamed" },
                "dry_run": true,
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let parsed = parse_result_json(&r);
            assert_eq!(parsed["would_rename"], true);
            assert!(!parsed["changes"].as_array().unwrap().is_empty());

            // Nothing was written, so the original name is still the one there.
            let lib = crate::altium::PcbLib::open(&pcb).unwrap();
            assert!(lib.get("CHIP_0402").is_some());
            assert!(lib.get("CHIP_0402_NEW").is_none());
        }

        // ---- search_components --------------------------------------------------

        #[test]
        fn search_names_its_missing_arguments_and_bad_pattern() {
            let fx = Fixtures::new();
            let outside = test_temp_dir();
            let server = create_test_server(fx.dir.path());

            let no_paths = server.call_search_components(&json!({ "pattern": "*" }));
            assert_error_mentions(&no_paths, "filepaths");

            // Present but carrying nothing usable.
            let empty_paths = server.call_search_components(&json!({
                "filepaths": [], "pattern": "*",
            }));
            assert_error_mentions(&empty_paths, "at least one path");

            let no_pattern = server.call_search_components(&json!({
                "filepaths": [fx.path("Lib.PcbLib")],
            }));
            assert_error_mentions(&no_pattern, "pattern");

            let bad_type = server.call_search_components(&json!({
                "filepaths": [fx.path("Lib.PcbLib")], "pattern": "*", "pattern_type": "fuzzy",
            }));
            assert_error_mentions(&bad_type, "must be one of");

            let escaped = server.call_search_components(&json!({
                "filepaths": [outside.path().join("X.PcbLib").to_string_lossy()], "pattern": "*",
            }));
            assert!(escaped.is_error, "{}", get_result_text(&escaped));

            let bad_regex = server.call_search_components(&json!({
                "filepaths": [fx.path("Lib.PcbLib")], "pattern": "CHIP[", "pattern_type": "regex",
            }));
            assert_error_mentions(&bad_regex, "Invalid pattern");
        }

        #[test]
        fn search_collects_per_library_errors_instead_of_failing_the_whole_call() {
            // One unreadable library among several must not lose the results
            // from the others, so the failures come back alongside the matches.
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());

            let r = server.call_search_components(&json!({
                "filepaths": [
                    fx.path("Lib.PcbLib"),
                    fx.path("Bad.PcbLib"),
                    fx.path("Bad.SchLib"),
                    fx.path("Notes.txt"),
                    fx.path("NoExtension"),
                ],
                "pattern": "*",
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let parsed = parse_result_json(&r);
            assert_eq!(parsed["status"], "partial");
            assert!(parsed["matches_found"].as_u64().unwrap() >= 2);

            let errors = parsed["errors"].as_array().unwrap();
            let joined = errors
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
                .join("\n");
            assert!(joined.contains("Unsupported file type"), "{joined}");
            assert!(joined.contains("no file extension"), "{joined}");
        }

        // ---- get_component ------------------------------------------------------

        #[test]
        fn get_component_names_its_missing_arguments_and_bad_extension() {
            let fx = Fixtures::new();
            let outside = test_temp_dir();
            let server = create_test_server(fx.dir.path());

            let no_name = server.call_get_component(&json!({ "filepath": fx.path("Lib.PcbLib") }));
            assert_error_mentions(&no_name, "component_name");

            let escaped = server.call_get_component(&json!({
                "filepath": outside.path().join("X.PcbLib").to_string_lossy(),
                "component_name": "A",
            }));
            assert!(escaped.is_error, "{}", get_result_text(&escaped));

            let no_ext = server.call_get_component(&json!({
                "filepath": fx.path("Lib"), "component_name": "A",
            }));
            assert_error_mentions(&no_ext, "no file extension");

            let wrong_ext = server.call_get_component(&json!({
                "filepath": fx.path("Lib.txt"), "component_name": "A",
            }));
            assert_error_mentions(&wrong_ext, "Unsupported file type");
        }

        #[test]
        fn get_component_reports_unreadable_libraries_and_lists_what_is_there() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());

            for lib in ["Bad.PcbLib", "Bad.SchLib"] {
                let r = server.call_get_component(&json!({
                    "filepath": fx.path(lib), "component_name": "A",
                }));
                assert_error_mentions(&r, "Failed to read library");
            }

            for (lib, present) in [("Lib.PcbLib", "CHIP_0402"), ("Lib.SchLib", "RESISTOR")] {
                let missing = server.call_get_component(&json!({
                    "filepath": fx.path(lib), "component_name": "GHOST",
                }));
                assert_error_mentions(&missing, present);

                let found = server.call_get_component(&json!({
                    "filepath": fx.path(lib), "component_name": present,
                }));
                assert!(!found.is_error, "{}", get_result_text(&found));
            }
        }

        // ---- component_exists ---------------------------------------------------

        #[test]
        fn component_exists_names_its_missing_arguments_and_bad_extension() {
            let fx = Fixtures::new();
            let outside = test_temp_dir();
            let server = create_test_server(fx.dir.path());

            let no_names = server.call_component_exists(&json!({
                "filepath": fx.path("Lib.PcbLib"),
            }));
            assert_error_mentions(&no_names, "component_names");

            // Present but holding nothing readable as a name.
            let empty_names = server.call_component_exists(&json!({
                "filepath": fx.path("Lib.PcbLib"), "component_names": [1, 2],
            }));
            assert_error_mentions(&empty_names, "empty or contains non-string");

            let escaped = server.call_component_exists(&json!({
                "filepath": outside.path().join("X.PcbLib").to_string_lossy(),
                "component_names": ["A"],
            }));
            assert!(escaped.is_error, "{}", get_result_text(&escaped));

            let no_ext = server.call_component_exists(&json!({
                "filepath": fx.path("Lib"), "component_names": ["A"],
            }));
            assert_error_mentions(&no_ext, "no file extension");

            let wrong_ext = server.call_component_exists(&json!({
                "filepath": fx.path("Lib.txt"), "component_names": ["A"],
            }));
            assert_error_mentions(&wrong_ext, "Unsupported file type");
        }

        #[test]
        fn component_exists_answers_per_name_for_both_library_types() {
            let fx = Fixtures::new();
            let server = create_test_server(fx.dir.path());

            for lib in ["Bad.PcbLib", "Bad.SchLib"] {
                let r = server.call_component_exists(&json!({
                    "filepath": fx.path(lib), "component_names": ["A"],
                }));
                assert_error_mentions(&r, "Failed to read library");
            }

            for (lib, present) in [("Lib.PcbLib", "CHIP_0402"), ("Lib.SchLib", "RESISTOR")] {
                let r = server.call_component_exists(&json!({
                    "filepath": fx.path(lib), "component_names": [present, "GHOST"],
                }));
                assert!(!r.is_error, "{}", get_result_text(&r));
                let parsed = parse_result_json(&r);
                let results = parsed["results"].as_array().unwrap();
                assert_eq!(results[0]["exists"], true, "{parsed}");
                assert_eq!(results[1]["exists"], false, "{parsed}");
            }
        }
    }

    /// A component is found by its name regardless of case, as Altium and
    /// the file's own directory find it; a rename onto another component's
    /// name in a different case is refused, onto its own is allowed.
    #[test]
    fn names_resolve_regardless_of_case() {
        use crate::altium::PcbLib;

        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let pcb = dir.path().join("Case.PcbLib");
        create_test_pcblib(&pcb); // CHIP_0402 + CHIP_0603

        let result = server.call_get_component(&json!({
            "filepath": pcb.to_string_lossy(),
            "component_name": "chip_0402",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        assert_eq!(parse_result_json(&result)["component"]["name"], "CHIP_0402");

        let result = server.call_update_component(&json!({
            "filepath": pcb.to_string_lossy(),
            "component_name": "CHIP_0402",
            "footprint": {
                "name": "chip_0603",
                "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 0.6, "height": 0.5 }],
            },
        }));
        assert!(result.is_error);
        assert!(
            get_result_text(&result)
                .contains("as 'CHIP_0603' (component names are case-insensitive)"),
            "{}",
            get_result_text(&result)
        );

        let result = server.call_update_component(&json!({
            "filepath": pcb.to_string_lossy(),
            "component_name": "CHIP_0402",
            "footprint": {
                "name": "Chip_0402",
                "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 0.6, "height": 0.5 }],
            },
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        assert_eq!(parse_result_json(&result)["renamed"], true);
        assert_eq!(
            PcbLib::open(&pcb).unwrap().names(),
            ["Chip_0402", "CHIP_0603"]
        );
    }
}
