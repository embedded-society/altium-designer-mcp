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
                Self::update_pcblib_component(filepath, component_name, fp_json, dry_run)
            }
            Some("schlib") => {
                let Some(sym_json) = arguments.get("symbol") else {
                    return ToolCallResult::error(
                        "Missing required parameter: symbol (required for .SchLib files)",
                    );
                };
                Self::update_schlib_component(filepath, component_name, sym_json, dry_run)
            }
            _ => ToolCallResult::error("Unknown file type. Expected .PcbLib or .SchLib extension."),
        }
    }

    /// Updates a footprint in-place within a `PcbLib` file.
    #[allow(clippy::too_many_lines)] // Includes parsing and dry_run logic
    pub(crate) fn update_pcblib_component(
        filepath: &str,
        component_name: &str,
        fp_json: &Value,
        dry_run: bool,
    ) -> ToolCallResult {
        use crate::altium::pcblib::{Footprint, PcbLib};

        // Read the library
        let mut library = match PcbLib::open(filepath) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
        };

        // Check component exists
        if library.get(component_name).is_none() {
            let available: Vec<_> = library.names().into_iter().take(10).collect();
            return ToolCallResult::error(format!(
                "Component '{component_name}' not found in library. Available: {}{}",
                available.join(", "),
                if library.len() > 10 { "..." } else { "" }
            ));
        }

        // Parse the replacement footprint
        let name = fp_json
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(component_name);
        let mut footprint = Footprint::new(name);

        if let Some(desc) = fp_json.get("description").and_then(Value::as_str) {
            footprint.description = desc.to_string();
        }

        // Parse pads
        if let Some(pads) = fp_json.get("pads").and_then(Value::as_array) {
            for (i, pad_json) in pads.iter().enumerate() {
                match Self::parse_pad(pad_json) {
                    Ok(pad) => footprint.add_pad(pad),
                    Err(e) => return ToolCallResult::error(format!("Pad {i}: {e}")),
                }
            }
        }

        // Parse tracks
        if let Some(tracks) = fp_json.get("tracks").and_then(Value::as_array) {
            for (i, track_json) in tracks.iter().enumerate() {
                match Self::parse_track(track_json) {
                    Ok(track) => footprint.add_track(track),
                    Err(e) => return ToolCallResult::error(format!("Track {i}: {e}")),
                }
            }
        }

        // Parse arcs
        if let Some(arcs) = fp_json.get("arcs").and_then(Value::as_array) {
            for (i, arc_json) in arcs.iter().enumerate() {
                match Self::parse_arc(arc_json) {
                    Ok(arc) => footprint.add_arc(arc),
                    Err(e) => return ToolCallResult::error(format!("Arc {i}: {e}")),
                }
            }
        }

        // Parse regions
        if let Some(regions) = fp_json.get("regions").and_then(Value::as_array) {
            for region_json in regions {
                if let Some(region) = Self::parse_region(region_json) {
                    footprint.add_region(region);
                }
            }
        }

        // Parse text
        if let Some(texts) = fp_json.get("texts").and_then(Value::as_array) {
            for text_json in texts {
                if let Some(text) = Self::parse_text(text_json) {
                    footprint.add_text(text);
                }
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

        // Create backup before destructive operation
        if let Err(e) = Self::create_backup(filepath) {
            return ToolCallResult::error(e);
        }

        // Write the library back
        if let Err(e) = library.save(filepath) {
            return ToolCallResult::error(format!("Failed to write library: {e}"));
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
            if old.pads.len() != new.pads.len() {
                changes.push(format!(
                    "pad_count: {} -> {}",
                    old.pads.len(),
                    new.pads.len()
                ));
            }
            if old.tracks.len() != new.tracks.len() {
                changes.push(format!(
                    "track_count: {} -> {}",
                    old.tracks.len(),
                    new.tracks.len()
                ));
            }
            if old.arcs.len() != new.arcs.len() {
                changes.push(format!(
                    "arc_count: {} -> {}",
                    old.arcs.len(),
                    new.arcs.len()
                ));
            }
            if old.regions.len() != new.regions.len() {
                changes.push(format!(
                    "region_count: {} -> {}",
                    old.regions.len(),
                    new.regions.len()
                ));
            }
            if old.text.len() != new.text.len() {
                changes.push(format!(
                    "text_count: {} -> {}",
                    old.text.len(),
                    new.text.len()
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

    /// Updates a symbol in-place within a `SchLib` file.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn update_schlib_component(
        filepath: &str,
        component_name: &str,
        sym_json: &Value,
        dry_run: bool,
    ) -> ToolCallResult {
        use crate::altium::schlib::{SchLib, Symbol};

        // Read the library
        let mut library = match SchLib::open(filepath) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
        };

        // Check component exists
        if library.get(component_name).is_none() {
            let available: Vec<_> = library.names().into_iter().take(10).collect();
            return ToolCallResult::error(format!(
                "Component '{component_name}' not found in library. Available: {}{}",
                available.join(", "),
                if library.len() > 10 { "..." } else { "" }
            ));
        }

        // Parse the replacement symbol
        let name = sym_json
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(component_name);
        let mut symbol = Symbol::new(name);

        if let Some(desc) = sym_json.get("description").and_then(Value::as_str) {
            symbol.description = desc.to_string();
        }

        if let Some(designator) = sym_json.get("designator").and_then(Value::as_str) {
            symbol.designator = designator.to_string();
        }

        // Parse pins
        if let Some(pins) = sym_json.get("pins").and_then(Value::as_array) {
            for pin_json in pins {
                if let Some(pin) = Self::parse_schlib_pin(pin_json) {
                    symbol.pins.push(pin);
                }
            }
        }

        // Parse rectangles
        if let Some(rects) = sym_json.get("rectangles").and_then(Value::as_array) {
            for rect_json in rects {
                if let Some(rect) = Self::parse_schlib_rectangle(rect_json) {
                    symbol.rectangles.push(rect);
                }
            }
        }

        // Parse lines
        if let Some(lines) = sym_json.get("lines").and_then(Value::as_array) {
            for line_json in lines {
                if let Some(line) = Self::parse_schlib_line(line_json) {
                    symbol.lines.push(line);
                }
            }
        }

        // Parse polylines
        if let Some(polylines) = sym_json.get("polylines").and_then(Value::as_array) {
            for polyline_json in polylines {
                if let Some(polyline) = Self::parse_schlib_polyline(polyline_json) {
                    symbol.polylines.push(polyline);
                }
            }
        }

        // Parse arcs
        if let Some(arcs) = sym_json.get("arcs").and_then(Value::as_array) {
            for arc_json in arcs {
                if let Some(arc) = Self::parse_schlib_arc(arc_json) {
                    symbol.arcs.push(arc);
                }
            }
        }

        // Parse ellipses
        if let Some(ellipses) = sym_json.get("ellipses").and_then(Value::as_array) {
            for ellipse_json in ellipses {
                if let Some(ellipse) = Self::parse_schlib_ellipse(ellipse_json) {
                    symbol.ellipses.push(ellipse);
                }
            }
        }

        // Parse parameters
        if let Some(params) = sym_json.get("parameters").and_then(Value::as_array) {
            for param_json in params {
                if let Some(param) = Self::parse_schlib_parameter(param_json) {
                    symbol.parameters.push(param);
                }
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
                        " (note: name in library key unchanged)".to_string()
                    }
                ),
            });

            return ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap());
        }

        // Perform the actual update
        library.update(component_name, symbol);

        // Create backup before destructive operation
        if let Err(e) = Self::create_backup(filepath) {
            return ToolCallResult::error(e);
        }

        // Write the library back
        if let Err(e) = library.save(filepath) {
            return ToolCallResult::error(format!("Failed to write library: {e}"));
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
                    " (note: name in library key unchanged, use rename_component to change key)".to_string()
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
            if old.pins.len() != new.pins.len() {
                changes.push(format!(
                    "pin_count: {} -> {}",
                    old.pins.len(),
                    new.pins.len()
                ));
            }
            if old.rectangles.len() != new.rectangles.len() {
                changes.push(format!(
                    "rectangle_count: {} -> {}",
                    old.rectangles.len(),
                    new.rectangles.len()
                ));
            }
            if old.lines.len() != new.lines.len() {
                changes.push(format!(
                    "line_count: {} -> {}",
                    old.lines.len(),
                    new.lines.len()
                ));
            }
            if old.polylines.len() != new.polylines.len() {
                changes.push(format!(
                    "polyline_count: {} -> {}",
                    old.polylines.len(),
                    new.polylines.len()
                ));
            }
            if old.arcs.len() != new.arcs.len() {
                changes.push(format!(
                    "arc_count: {} -> {}",
                    old.arcs.len(),
                    new.arcs.len()
                ));
            }
            if old.ellipses.len() != new.ellipses.len() {
                changes.push(format!(
                    "ellipse_count: {} -> {}",
                    old.ellipses.len(),
                    new.ellipses.len()
                ));
            }
            if old.labels.len() != new.labels.len() {
                changes.push(format!(
                    "label_count: {} -> {}",
                    old.labels.len(),
                    new.labels.len()
                ));
            }
            if old.text.len() != new.text.len() {
                changes.push(format!(
                    "text_count: {} -> {}",
                    old.text.len(),
                    new.text.len()
                ));
            }
            if old.parameters.len() != new.parameters.len() {
                changes.push(format!(
                    "parameter_count: {} -> {}",
                    old.parameters.len(),
                    new.parameters.len()
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
                Some(ext) => errors.push(format!("{path}: Unsupported file type '.{ext}'")),
                None => errors.push(format!("{path}: No file extension")),
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
            Some(ext) => ToolCallResult::error(format!(
                "Unsupported file type: .{ext}. Use .PcbLib or .SchLib"
            )),
            None => ToolCallResult::error("File has no extension. Use .PcbLib or .SchLib"),
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
            let available: Vec<&str> = library.iter().map(|fp| fp.name.as_str()).collect();
            return ToolCallResult::error(format!(
                "Component '{}' not found in library. Available components: {}",
                component_name,
                if available.len() <= 10 {
                    available.join(", ")
                } else {
                    format!(
                        "{} ... and {} more",
                        available[..10].join(", "),
                        available.len() - 10
                    )
                }
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
            let available: Vec<&str> = library.iter().map(|s| s.name.as_str()).collect();
            return ToolCallResult::error(format!(
                "Component '{}' not found in library. Available components: {}",
                component_name,
                if available.len() <= 10 {
                    available.join(", ")
                } else {
                    format!(
                        "{} ... and {} more",
                        available[..10].join(", "),
                        available.len() - 10
                    )
                }
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
            Some(ext) => {
                return ToolCallResult::error(format!(
                    "Unsupported file type: .{ext}. Use .PcbLib or .SchLib"
                ))
            }
            None => return ToolCallResult::error("File has no extension. Use .PcbLib or .SchLib"),
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
