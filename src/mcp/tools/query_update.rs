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

        // Parse vias, fills and component bodies. The create path (call_write_pcblib)
        // handles these, but this update path used to omit them entirely, so a
        // read-modify-write of a footprint carrying any via / fill / 3D body silently
        // DROPPED it. Mirror the create path exactly.
        if let Some(vias) = fp_json.get("vias").and_then(Value::as_array) {
            for (i, via_json) in vias.iter().enumerate() {
                match Self::parse_via(via_json) {
                    Ok(via) => footprint.add_via(via),
                    Err(e) => return ToolCallResult::error(format!("Via {i}: {e}")),
                }
            }
        }
        if let Some(fills) = fp_json.get("fills").and_then(Value::as_array) {
            for (i, fill_json) in fills.iter().enumerate() {
                match Self::parse_fill(fill_json) {
                    Ok(fill) => footprint.add_fill(fill),
                    Err(e) => return ToolCallResult::error(format!("Fill {i}: {e}")),
                }
            }
        }
        if let Some(bodies) = fp_json.get("component_bodies").and_then(Value::as_array) {
            for body_json in bodies {
                footprint.add_component_body(Self::parse_component_body_json(body_json));
            }
        }

        // Parse text. Accept both "text" (the create-path key) and the legacy
        // "texts" so reusing the create schema for an update no longer silently
        // drops text primitives.
        if let Some(texts) = fp_json
            .get("text")
            .or_else(|| fp_json.get("texts"))
            .and_then(Value::as_array)
        {
            for text_json in texts {
                if let Some(text) = Self::parse_text(text_json) {
                    footprint.add_text(text);
                }
            }
        }

        // Reject out-of-range / non-finite geometry before it can saturate in
        // from_mm() on save. The create path validates here; this path skipped
        // it (parallels the #102 update_pad/update_primitive bug). Runs in
        // dry-run too so previews report the error.
        if let Err(e) = Self::validate_footprint_coordinates(&footprint) {
            return ToolCallResult::error(e);
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

        if let Err(resp) = Self::backup_then_save(filepath, || library.save(filepath)) {
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
            if old.vias.len() != new.vias.len() {
                changes.push(format!(
                    "via_count: {} -> {}",
                    old.vias.len(),
                    new.vias.len()
                ));
            }
            if old.fills.len() != new.fills.len() {
                changes.push(format!(
                    "fill_count: {} -> {}",
                    old.fills.len(),
                    new.fills.len()
                ));
            }
            if old.component_bodies.len() != new.component_bodies.len() {
                changes.push(format!(
                    "component_body_count: {} -> {}",
                    old.component_bodies.len(),
                    new.component_bodies.len()
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

        // Parse part_count and the other symbol header fields (mirrors the
        // create path for part_count; the rest are returned by get_component,
        // so a read-modify-write must not reset them to defaults — omitting
        // part_count here silently collapsed a multi-part symbol to one part).
        if let Some(part_count) = sym_json.get("part_count").and_then(Value::as_u64) {
            #[allow(clippy::cast_possible_truncation)]
            {
                symbol.part_count = part_count.clamp(1, 255) as u32;
            }
        }
        if let Some(v) = sym_json.get("display_mode_count").and_then(Value::as_u64) {
            symbol.display_mode_count = u32::try_from(v).unwrap_or(symbol.display_mode_count);
        }
        if let Some(v) = sym_json.get("current_part_id").and_then(Value::as_u64) {
            symbol.current_part_id = u32::try_from(v).unwrap_or(symbol.current_part_id);
        }
        if let Some(v) = sym_json.get("part_id_locked").and_then(Value::as_bool) {
            symbol.part_id_locked = v;
        }
        if let Some(v) = sym_json.get("source_library_name").and_then(Value::as_str) {
            symbol.source_library_name = v.to_string();
        }
        if let Some(v) = sym_json.get("target_file_name").and_then(Value::as_str) {
            symbol.target_file_name = v.to_string();
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

        // Parse round_rects, polygons, labels and text. The create path
        // (call_write_schlib) handles all of these; this update path omitted them,
        // so a read-modify-write of a symbol carrying any of them silently DROPPED
        // it. Mirror the create path.
        if let Some(round_rects) = sym_json.get("round_rects").and_then(Value::as_array) {
            for rr_json in round_rects {
                if let Some(rr) = Self::parse_schlib_round_rect(rr_json) {
                    symbol.round_rects.push(rr);
                }
            }
        }
        if let Some(polygons) = sym_json.get("polygons").and_then(Value::as_array) {
            for polygon_json in polygons {
                if let Some(polygon) = Self::parse_schlib_polygon(polygon_json) {
                    symbol.polygons.push(polygon);
                }
            }
        }
        if let Some(labels) = sym_json.get("labels").and_then(Value::as_array) {
            for label_json in labels {
                if let Some(label) = Self::parse_schlib_label(label_json) {
                    symbol.labels.push(label);
                }
            }
        }
        if let Some(texts) = sym_json.get("text").and_then(Value::as_array) {
            for text_json in texts {
                if let Some(text) = Self::parse_schlib_text(text_json) {
                    symbol.text.push(text);
                }
            }
        }

        // Parse pies and images (mirror the create path — both were added to
        // call_write_schlib without this path, recreating exactly the
        // dropped-primitive bug documented above).
        if let Some(pies) = sym_json.get("pies").and_then(Value::as_array) {
            for pie_json in pies {
                if let Some(pie) = Self::parse_schlib_pie(pie_json) {
                    symbol.pies.push(pie);
                }
            }
        }
        if let Some(images) = sym_json.get("images").and_then(Value::as_array) {
            for image_json in images {
                if let Some(image) = Self::parse_schlib_image(image_json) {
                    symbol.images.push(image);
                }
            }
        }
        if let Some(text_frames) = sym_json.get("text_frames").and_then(Value::as_array) {
            for frame_json in text_frames {
                if let Some(text_frame) = Self::parse_schlib_text_frame(frame_json) {
                    symbol.text_frames.push(text_frame);
                }
            }
        }

        // Beziers and elliptical arcs (mirror the create path, which authors
        // them through the same parse helpers — the JSON keys equal the serde
        // field names, so a get_component echo parses identically).
        if let Some(beziers) = sym_json.get("beziers").and_then(Value::as_array) {
            for bezier_json in beziers {
                if let Some(bezier) = Self::parse_schlib_bezier(bezier_json) {
                    symbol.beziers.push(bezier);
                }
            }
        }
        if let Some(ell_arcs) = sym_json.get("elliptical_arcs").and_then(Value::as_array) {
            for ell_arc_json in ell_arcs {
                if let Some(ell_arc) = Self::parse_schlib_elliptical_arc(ell_arc_json) {
                    symbol.elliptical_arcs.push(ell_arc);
                }
            }
        }

        // Parse footprint references. serde shape (get_component echo) and the
        // create-path shape ({name, description, library_path}) both
        // deserialise, since every other FootprintModel field has a default.
        if let Some(footprints) = sym_json.get("footprints").and_then(Value::as_array) {
            for fp_json in footprints {
                if let Ok(fp) = serde_json::from_value(fp_json.clone()) {
                    symbol.footprints.push(fp);
                }
            }
        }

        // Reject out-of-range geometry before save (the create path validates
        // here; this path skipped it entirely). Runs in dry-run too.
        if let Err(e) = Self::validate_symbol_coordinates(&symbol) {
            return ToolCallResult::error(e);
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
                        format!(" and change its saved name to '{name}' (use rename_component if you also need the in-session lookup key updated)")
                    }
                ),
            });

            return ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap());
        }

        // Perform the actual update
        library.update(component_name, symbol);

        if let Err(resp) = Self::backup_then_save(filepath, || library.save(filepath)) {
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
            // Every primitive family, so the dry-run preview can never miss an
            // added/removed family (a new family = one line here).
            let family_counts = [
                ("pin_count", old.pins.len(), new.pins.len()),
                (
                    "rectangle_count",
                    old.rectangles.len(),
                    new.rectangles.len(),
                ),
                ("line_count", old.lines.len(), new.lines.len()),
                ("polyline_count", old.polylines.len(), new.polylines.len()),
                ("arc_count", old.arcs.len(), new.arcs.len()),
                ("ellipse_count", old.ellipses.len(), new.ellipses.len()),
                ("label_count", old.labels.len(), new.labels.len()),
                ("text_count", old.text.len(), new.text.len()),
                (
                    "parameter_count",
                    old.parameters.len(),
                    new.parameters.len(),
                ),
                (
                    "round_rect_count",
                    old.round_rects.len(),
                    new.round_rects.len(),
                ),
                ("polygon_count", old.polygons.len(), new.polygons.len()),
                ("pie_count", old.pies.len(), new.pies.len()),
                ("image_count", old.images.len(), new.images.len()),
                (
                    "text_frame_count",
                    old.text_frames.len(),
                    new.text_frames.len(),
                ),
                ("bezier_count", old.beziers.len(), new.beziers.len()),
                (
                    "elliptical_arc_count",
                    old.elliptical_arcs.len(),
                    new.elliptical_arcs.len(),
                ),
                (
                    "footprint_count",
                    old.footprints.len(),
                    new.footprints.len(),
                ),
            ];
            for (label, old_len, new_len) in family_counts {
                if old_len != new_len {
                    changes.push(format!("{label}: {old_len} -> {new_len}"));
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
                "text": [{ "x": 0, "y": -12, "text": "note" }],
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
}
