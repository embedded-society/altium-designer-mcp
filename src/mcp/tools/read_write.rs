//! Read/write/list/style tools. Split from `server.rs`.

use serde_json::{json, Value};

use crate::mcp::server::{ErrorContext, McpServer, ToolCallResult};

impl McpServer {
    // ==================== Tool Handlers ====================

    /// Reads a `PcbLib` file and returns its contents.
    /// Supports pagination via limit/offset and filtering by `component_name`.
    #[allow(clippy::cast_possible_truncation)]
    #[allow(clippy::too_many_lines)] // Complex formatting logic for compact mode
    pub(crate) fn call_read_pcblib(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::pcblib::primitives::PadStackMode;
        use crate::altium::PcbLib;

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        // Parse optional pagination/filter parameters
        let component_name = arguments.get("component_name").and_then(Value::as_str);
        let limit = arguments
            .get("limit")
            .and_then(Value::as_u64)
            .map(|v| v as usize);
        let offset = arguments
            .get("offset")
            .and_then(Value::as_u64)
            .map_or(0, |v| v as usize);

        // Parse compact parameter (default: true - omit redundant per-layer data)
        let compact = arguments
            .get("compact")
            .and_then(Value::as_bool)
            .unwrap_or(true);

        match PcbLib::open(filepath) {
            Ok(library) => {
                let total_count = library.len();

                // Apply filtering and pagination
                let footprints: Vec<_> = library
                    .iter()
                    .filter(|fp| {
                        // If component_name specified, only include matching
                        component_name.map_or(true, |name| fp.name == name)
                    })
                    .skip(offset)
                    .take(limit.unwrap_or(usize::MAX))
                    .map(|fp| {
                        // If compact mode, strip per-layer data when it's redundant
                        let pads: Vec<Value> = if compact {
                            fp.pads
                                .iter()
                                .map(|pad| {
                                    let mut pad_json = serde_json::to_value(pad).unwrap();
                                    // Remove per-layer data if stack_mode is Simple OR all values are uniform
                                    let should_strip = pad.stack_mode == PadStackMode::Simple
                                        || Self::pad_has_uniform_per_layer_data(pad);
                                    if should_strip {
                                        if let Value::Object(ref mut obj) = pad_json {
                                            obj.remove("per_layer_sizes");
                                            obj.remove("per_layer_shapes");
                                            obj.remove("per_layer_corner_radii");
                                            obj.remove("per_layer_offsets");
                                            // Downgrade stack_mode to simple if we stripped uniform data
                                            if pad.stack_mode != PadStackMode::Simple {
                                                obj.insert(
                                                    "stack_mode".to_string(),
                                                    json!("simple"),
                                                );
                                            }
                                        }
                                    }
                                    pad_json
                                })
                                .collect()
                        } else {
                            fp.pads
                                .iter()
                                .map(|p| serde_json::to_value(p).unwrap())
                                .collect()
                        };

                        json!({
                            "name": fp.name,
                            "description": fp.description,
                            "pads": pads,
                            "tracks": fp.tracks,
                            "arcs": fp.arcs,
                            "regions": fp.regions,
                            "text": fp.text,
                            "model_3d": fp.model_3d,
                            "component_bodies": fp.component_bodies,
                        })
                    })
                    .collect();

                let returned_count = footprints.len();
                let has_more = if component_name.is_some() {
                    false // Single component fetch, no pagination
                } else {
                    offset + returned_count < total_count
                };

                let result = json!({
                    "status": "success",
                    "filepath": filepath,
                    "units": "mm",
                    "total_count": total_count,
                    "returned_count": returned_count,
                    "offset": offset,
                    "has_more": has_more,
                    "compact": compact,
                    "footprints": footprints,
                });

                ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e.to_string(),
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Writes footprints to a `PcbLib` file.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn call_write_pcblib(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::pcblib::{Footprint, Model3D, PcbLib};

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        let Some(footprints_json) = arguments.get("footprints").and_then(Value::as_array) else {
            return ToolCallResult::error("Missing required parameter: footprints");
        };

        // Collect and validate footprint names for duplicates
        let new_names: Vec<&str> = footprints_json
            .iter()
            .filter_map(|fp| fp.get("name").and_then(Value::as_str))
            .collect();

        // Check for duplicates within the new footprints
        {
            let mut seen = std::collections::HashSet::new();
            for name in &new_names {
                if !seen.insert(*name) {
                    return ToolCallResult::error_with_context(
                        ErrorContext::new(
                            "write_pcblib",
                            format!("Duplicate footprint name: '{name}'"),
                        )
                        .with_filepath(filepath)
                        .with_component(*name)
                        .with_details("Each footprint in the request must have a unique name"),
                    );
                }
            }
        }

        // Validate footprint names
        // Note: OLE storage names are limited to 31 characters, but the library layer
        // handles this by truncating storage names while preserving full names in PATTERN.
        #[allow(clippy::items_after_statements)]
        const INVALID_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
        for name in &new_names {
            if name.is_empty() {
                return ToolCallResult::error("Footprint name cannot be empty");
            }
            if let Some(c) = name.chars().find(|c| INVALID_CHARS.contains(c)) {
                return ToolCallResult::error(format!(
                    "Footprint name '{name}' contains invalid character '{c}'. \
                     Names cannot contain: / \\ : * ? \" < > |",
                ));
            }
        }

        let append = arguments
            .get("append")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // If append mode and file exists, read existing library; otherwise create new
        let mut library = if append && std::path::Path::new(filepath).exists() {
            match PcbLib::open(filepath) {
                Ok(lib) => lib,
                Err(e) => {
                    return ToolCallResult::error_with_context(
                        ErrorContext::new(
                            "write_pcblib",
                            format!("Failed to read existing library: {e}"),
                        )
                        .with_filepath(filepath)
                        .with_details(
                            "The library file exists but could not be opened for appending",
                        ),
                    );
                }
            }
        } else {
            PcbLib::new()
        };

        // Check for duplicates with existing footprints in append mode
        if append {
            let existing_names: std::collections::HashSet<_> =
                library.names().into_iter().collect();
            for name in &new_names {
                if existing_names.contains(*name) {
                    return ToolCallResult::error(format!(
                        "Footprint '{name}' already exists in the library"
                    ));
                }
            }
        }

        for fp_json in footprints_json {
            let name = fp_json
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Unnamed");
            let mut footprint = Footprint::new(name);

            if let Some(desc) = fp_json.get("description").and_then(Value::as_str) {
                footprint.description = desc.to_string();
            }

            // Parse pads
            if let Some(pads) = fp_json.get("pads").and_then(Value::as_array) {
                for (i, pad_json) in pads.iter().enumerate() {
                    match Self::parse_pad(pad_json) {
                        Ok(pad) => footprint.add_pad(pad),
                        Err(e) => {
                            return ToolCallResult::error_with_context(
                                ErrorContext::new("write_pcblib", e)
                                    .with_filepath(filepath)
                                    .with_component(name)
                                    .with_details(format!("Failed to parse pad at index {i}")),
                            )
                        }
                    }
                }
            }

            // Parse tracks
            if let Some(tracks) = fp_json.get("tracks").and_then(Value::as_array) {
                for (i, track_json) in tracks.iter().enumerate() {
                    match Self::parse_track(track_json) {
                        Ok(track) => footprint.add_track(track),
                        Err(e) => {
                            return ToolCallResult::error_with_context(
                                ErrorContext::new("write_pcblib", e)
                                    .with_filepath(filepath)
                                    .with_component(name)
                                    .with_details(format!("Failed to parse track at index {i}")),
                            )
                        }
                    }
                }
            }

            // Parse arcs
            if let Some(arcs) = fp_json.get("arcs").and_then(Value::as_array) {
                for (i, arc_json) in arcs.iter().enumerate() {
                    match Self::parse_arc(arc_json) {
                        Ok(arc) => footprint.add_arc(arc),
                        Err(e) => {
                            return ToolCallResult::error_with_context(
                                ErrorContext::new("write_pcblib", e)
                                    .with_filepath(filepath)
                                    .with_component(name)
                                    .with_details(format!("Failed to parse arc at index {i}")),
                            )
                        }
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
            if let Some(texts) = fp_json.get("text").and_then(Value::as_array) {
                for text_json in texts {
                    if let Some(text) = Self::parse_text(text_json) {
                        footprint.add_text(text);
                    }
                }
            }

            // Parse 3D model
            if let Some(model_json) = fp_json.get("step_model") {
                if let Some(model_path) = model_json.get("filepath").and_then(Value::as_str) {
                    let embed = model_json
                        .get("embed")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);

                    if embed {
                        // The embed source is read from disk at save time
                        // (prepare_3d_models_for_writing -> std::fs::read), far from
                        // this handler. Validate it against the allow-list now so a
                        // caller cannot embed an arbitrary file (e.g. "../../etc/passwd")
                        // into the library. External references (embed=false) are only
                        // stored as a string and never read, so they are not gated here.
                        if let Err(e) = self.validate_path(model_path) {
                            return ToolCallResult::error(e);
                        }

                        // Embedded model - use Model3D which will read the file on save
                        footprint.model_3d = Some(Model3D {
                            filepath: model_path.to_string(),
                            x_offset: model_json
                                .get("x_offset")
                                .and_then(Value::as_f64)
                                .unwrap_or(0.0),
                            y_offset: model_json
                                .get("y_offset")
                                .and_then(Value::as_f64)
                                .unwrap_or(0.0),
                            z_offset: model_json
                                .get("z_offset")
                                .and_then(Value::as_f64)
                                .unwrap_or(0.0),
                            rotation: model_json
                                .get("rotation")
                                .and_then(Value::as_f64)
                                .unwrap_or(0.0),
                        });
                    } else {
                        // External reference only - create ComponentBody directly
                        // Preserve the full path for external references so organized subfolders work
                        use crate::altium::pcblib::{ComponentBody, Layer};
                        footprint.add_component_body(ComponentBody {
                            model_id: String::new(),            // No GUID for external reference
                            model_name: model_path.to_string(), // Preserve full path
                            embedded: false,
                            rotation_x: 0.0,
                            rotation_y: 0.0,
                            rotation_z: model_json
                                .get("rotation")
                                .and_then(Value::as_f64)
                                .unwrap_or(0.0),
                            z_offset: model_json
                                .get("z_offset")
                                .and_then(Value::as_f64)
                                .unwrap_or(0.0),
                            overall_height: 0.0,
                            standoff_height: 0.0,
                            layer: Layer::Top3DBody,
                            outline: Vec::new(),
                            unique_id: None,
                        });
                    }
                }
            }

            // Validate coordinates before adding
            if let Err(e) = Self::validate_footprint_coordinates(&footprint) {
                return ToolCallResult::error(e);
            }

            library.add(footprint);
        }

        // Create backup before destructive operation (if file exists)
        if let Err(e) = Self::create_backup(filepath) {
            return ToolCallResult::error(e);
        }

        match library.save(filepath) {
            Ok(()) => {
                let mut result = json!({
                    "status": "success",
                    "filepath": filepath,
                    "footprint_count": library.len(),
                    "footprint_names": library.names(),
                });

                // Run post-write validation
                if let Some(validation) = Self::post_write_validation_pcblib(filepath) {
                    result["validation"] = validation;
                }

                ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e.to_string(),
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Reads a `SchLib` file and returns its contents.
    /// Supports pagination via limit/offset and filtering by `component_name`.
    #[allow(clippy::cast_possible_truncation)]
    pub(crate) fn call_read_schlib(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::SchLib;

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        // Parse optional pagination/filter parameters
        let component_name = arguments.get("component_name").and_then(Value::as_str);
        let limit = arguments
            .get("limit")
            .and_then(Value::as_u64)
            .map(|v| v as usize);
        let offset = arguments
            .get("offset")
            .and_then(Value::as_u64)
            .map_or(0, |v| v as usize);

        match SchLib::open(filepath) {
            Ok(library) => {
                let total_count = library.len();

                // Apply filtering and pagination
                let symbols: Vec<_> = library
                    .iter()
                    .filter(|symbol| {
                        // If component_name specified, only include matching
                        component_name.map_or(true, |filter| symbol.name == filter)
                    })
                    .skip(offset)
                    .take(limit.unwrap_or(usize::MAX))
                    .map(|symbol| {
                        json!({
                            "name": symbol.name,
                            "description": symbol.description,
                            "designator": symbol.designator,
                            "part_count": symbol.part_count,
                            "pins": symbol.pins,
                            "rectangles": symbol.rectangles,
                            "lines": symbol.lines,
                            "polylines": symbol.polylines,
                            "arcs": symbol.arcs,
                            "ellipses": symbol.ellipses,
                            "labels": symbol.labels,
                            "parameters": symbol.parameters,
                            "footprints": symbol.footprints,
                        })
                    })
                    .collect();

                let returned_count = symbols.len();
                let has_more = if component_name.is_some() {
                    false // Single component fetch, no pagination
                } else {
                    offset + returned_count < total_count
                };

                let result = json!({
                    "status": "success",
                    "filepath": filepath,
                    "units": "schematic units (10 = 1 grid)",
                    "total_count": total_count,
                    "returned_count": returned_count,
                    "offset": offset,
                    "has_more": has_more,
                    "symbols": symbols,
                });

                ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e.to_string(),
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Writes symbols to a `SchLib` file.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn call_write_schlib(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::schlib::{FootprintModel, SchLib, Symbol};

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        let Some(symbols_json) = arguments.get("symbols").and_then(Value::as_array) else {
            return ToolCallResult::error("Missing required parameter: symbols");
        };

        // Collect and validate symbol names
        let new_names: Vec<&str> = symbols_json
            .iter()
            .filter_map(|sym| sym.get("name").and_then(Value::as_str))
            .collect();

        // Check for duplicates within the new symbols
        {
            let mut seen = std::collections::HashSet::new();
            for name in &new_names {
                if !seen.insert(*name) {
                    return ToolCallResult::error_with_context(
                        ErrorContext::new(
                            "write_schlib",
                            format!("Duplicate symbol name: '{name}'"),
                        )
                        .with_filepath(filepath)
                        .with_component(*name)
                        .with_details("Each symbol in the request must have a unique name"),
                    );
                }
            }
        }

        // Validate symbol names
        // Note: OLE storage names are limited to 31 characters, but the library layer
        // handles this by truncating storage names while preserving full names in LIBREFERENCE.
        #[allow(clippy::items_after_statements)]
        const INVALID_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
        for name in &new_names {
            if name.is_empty() {
                return ToolCallResult::error("Symbol name cannot be empty");
            }
            if let Some(c) = name.chars().find(|c| INVALID_CHARS.contains(c)) {
                return ToolCallResult::error(format!(
                    "Symbol name '{name}' contains invalid character '{c}'. \
                     Names cannot contain: / \\ : * ? \" < > |",
                ));
            }
        }

        let append = arguments
            .get("append")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // If append mode and file exists, read existing library; otherwise create new
        let mut library = if append && std::path::Path::new(filepath).exists() {
            match SchLib::open(filepath) {
                Ok(lib) => lib,
                Err(e) => {
                    return ToolCallResult::error_with_context(
                        ErrorContext::new(
                            "write_schlib",
                            format!("Failed to read existing library: {e}"),
                        )
                        .with_filepath(filepath)
                        .with_details(
                            "The library file exists but could not be opened for appending",
                        ),
                    );
                }
            }
        } else {
            SchLib::new()
        };

        // Check for duplicates with existing symbols in append mode
        if append {
            for name in &new_names {
                if library.get(name).is_some() {
                    return ToolCallResult::error(format!(
                        "Symbol '{name}' already exists in the library"
                    ));
                }
            }
        }

        for sym_json in symbols_json {
            let name = sym_json
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Unnamed");
            let mut symbol = Symbol::new(name);

            if let Some(desc) = sym_json.get("description").and_then(Value::as_str) {
                symbol.description = desc.to_string();
            }

            if let Some(desig) = sym_json.get("designator_prefix").and_then(Value::as_str) {
                symbol.designator = format!("{desig}?");
            }

            // Parse part_count for multi-part symbols (e.g., dual op-amp)
            if let Some(part_count) = sym_json.get("part_count").and_then(Value::as_u64) {
                #[allow(clippy::cast_possible_truncation)]
                {
                    symbol.part_count = part_count.clamp(1, 255) as u32;
                }
            }

            // Parse pins
            if let Some(pins) = sym_json.get("pins").and_then(Value::as_array) {
                for pin_json in pins {
                    if let Some(pin) = Self::parse_schlib_pin(pin_json) {
                        symbol.add_pin(pin);
                    }
                }
            }

            // Parse rectangles
            if let Some(rects) = sym_json.get("rectangles").and_then(Value::as_array) {
                for rect_json in rects {
                    if let Some(rect) = Self::parse_schlib_rectangle(rect_json) {
                        symbol.add_rectangle(rect);
                    }
                }
            }

            // Parse lines
            if let Some(lines) = sym_json.get("lines").and_then(Value::as_array) {
                for line_json in lines {
                    if let Some(line) = Self::parse_schlib_line(line_json) {
                        symbol.add_line(line);
                    }
                }
            }

            // Parse polylines
            if let Some(polylines) = sym_json.get("polylines").and_then(Value::as_array) {
                for polyline_json in polylines {
                    if let Some(polyline) = Self::parse_schlib_polyline(polyline_json) {
                        symbol.add_polyline(polyline);
                    }
                }
            }

            // Parse arcs
            if let Some(arcs) = sym_json.get("arcs").and_then(Value::as_array) {
                for arc_json in arcs {
                    if let Some(arc) = Self::parse_schlib_arc(arc_json) {
                        symbol.add_arc(arc);
                    }
                }
            }

            // Parse ellipses
            if let Some(ellipses) = sym_json.get("ellipses").and_then(Value::as_array) {
                for ellipse_json in ellipses {
                    if let Some(ellipse) = Self::parse_schlib_ellipse(ellipse_json) {
                        symbol.add_ellipse(ellipse);
                    }
                }
            }

            // Parse labels
            if let Some(labels) = sym_json.get("labels").and_then(Value::as_array) {
                for label_json in labels {
                    if let Some(label) = Self::parse_schlib_label(label_json) {
                        symbol.add_label(label);
                    }
                }
            }

            // Parse text annotations
            if let Some(texts) = sym_json.get("text").and_then(Value::as_array) {
                for text_json in texts {
                    if let Some(text) = Self::parse_schlib_text(text_json) {
                        symbol.add_text(text);
                    }
                }
            }

            // Parse parameters
            if let Some(params) = sym_json.get("parameters").and_then(Value::as_array) {
                for param_json in params {
                    if let Some(param) = Self::parse_schlib_parameter(param_json) {
                        symbol.add_parameter(param);
                    }
                }
            }

            // Parse footprint references
            if let Some(footprints) = sym_json.get("footprints").and_then(Value::as_array) {
                for fp_json in footprints {
                    if let Some(fp_name) = fp_json.get("name").and_then(Value::as_str) {
                        let mut fp = FootprintModel::new(fp_name);
                        if let Some(desc) = fp_json.get("description").and_then(Value::as_str) {
                            fp.description = desc.to_string();
                        }
                        symbol.add_footprint(fp);
                    }
                }
            }

            // Validate coordinates before adding
            if let Err(e) = Self::validate_symbol_coordinates(&symbol) {
                return ToolCallResult::error(e);
            }

            library.add(symbol);
        }

        // Create backup before destructive operation (if file exists)
        if let Err(e) = Self::create_backup(filepath) {
            return ToolCallResult::error(e);
        }

        match library.save(filepath) {
            Ok(()) => {
                let symbol_names: Vec<_> = library.iter().map(|s| s.name.clone()).collect();
                let mut result = json!({
                    "status": "success",
                    "filepath": filepath,
                    "symbol_count": library.len(),
                    "symbol_names": symbol_names,
                });

                // Run post-write validation
                if let Some(validation) = Self::post_write_validation_schlib(filepath) {
                    result["validation"] = validation;
                }

                ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e.to_string(),
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Lists component names in a library file.
    #[allow(clippy::cast_possible_truncation, clippy::too_many_lines)]
    pub(crate) fn call_list_components(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::{PcbLib, SchLib};

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        // Parse optional pagination parameters
        let limit = arguments
            .get("limit")
            .and_then(Value::as_u64)
            .map(|v| v as usize);
        let offset = arguments
            .get("offset")
            .and_then(Value::as_u64)
            .map_or(0, |v| v as usize);

        // Parse include_metadata parameter (default: false)
        let include_metadata = arguments
            .get("include_metadata")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // Try to determine file type from extension
        let path = std::path::Path::new(filepath);
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        match extension.as_deref() {
            Some("pcblib") => match PcbLib::open(filepath) {
                Ok(library) => {
                    let total_count = library.len();

                    // Apply pagination and optionally include metadata
                    let components: Vec<Value> = if include_metadata {
                        library
                            .iter()
                            .skip(offset)
                            .take(limit.unwrap_or(usize::MAX))
                            .map(|fp| {
                                json!({
                                    "name": fp.name,
                                    "description": fp.description,
                                    "pad_count": fp.pads.len(),
                                    "track_count": fp.tracks.len(),
                                    "arc_count": fp.arcs.len(),
                                    "region_count": fp.regions.len(),
                                    "text_count": fp.text.len(),
                                    "has_3d_model": fp.model_3d.is_some() || !fp.component_bodies.is_empty(),
                                })
                            })
                            .collect()
                    } else {
                        library
                            .names()
                            .into_iter()
                            .skip(offset)
                            .take(limit.unwrap_or(usize::MAX))
                            .map(|n| json!(n))
                            .collect()
                    };

                    let returned_count = components.len();
                    let has_more = offset + returned_count < total_count;

                    let result = json!({
                        "status": "success",
                        "filepath": filepath,
                        "file_type": "PcbLib",
                        "total_count": total_count,
                        "returned_count": returned_count,
                        "offset": offset,
                        "has_more": has_more,
                        "include_metadata": include_metadata,
                        "components": components,
                    });
                    ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
                }
                Err(e) => {
                    let result = json!({
                        "status": "error",
                        "filepath": filepath,
                        "error": e.to_string(),
                    });
                    ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
                }
            },
            Some("schlib") => match SchLib::open(filepath) {
                Ok(library) => {
                    let total_count = library.len();

                    // Apply pagination and optionally include metadata
                    let components: Vec<Value> = if include_metadata {
                        library
                            .iter()
                            .skip(offset)
                            .take(limit.unwrap_or(usize::MAX))
                            .map(|s| {
                                json!({
                                    "name": s.name,
                                    "description": s.description,
                                    "designator": s.designator,
                                    "part_count": s.part_count,
                                    "pin_count": s.pins.len(),
                                    "footprint_count": s.footprints.len(),
                                })
                            })
                            .collect()
                    } else {
                        library
                            .iter()
                            .map(|s| json!(s.name.clone()))
                            .skip(offset)
                            .take(limit.unwrap_or(usize::MAX))
                            .collect()
                    };

                    let returned_count = components.len();
                    let has_more = offset + returned_count < total_count;

                    let result = json!({
                        "status": "success",
                        "filepath": filepath,
                        "file_type": "SchLib",
                        "total_count": total_count,
                        "returned_count": returned_count,
                        "offset": offset,
                        "has_more": has_more,
                        "include_metadata": include_metadata,
                        "components": components,
                    });
                    ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
                }
                Err(e) => {
                    let result = json!({
                        "status": "error",
                        "filepath": filepath,
                        "error": e.to_string(),
                    });
                    ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
                }
            },
            _ => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": "Unknown file type. Expected .PcbLib or .SchLib extension.",
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Extracts style information from a library file.
    pub(crate) fn call_extract_style(&self, arguments: &Value) -> ToolCallResult {
        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        let path = std::path::Path::new(filepath);
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase);

        match extension.as_deref() {
            Some("pcblib") => Self::extract_pcblib_style(filepath),
            Some("schlib") => Self::extract_schlib_style(filepath),
            _ => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": "Unknown file type. Expected .PcbLib or .SchLib extension.",
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Extracts style from a `PcbLib` file.
    pub(crate) fn extract_pcblib_style(filepath: &str) -> ToolCallResult {
        use crate::altium::PcbLib;
        use std::collections::HashMap;

        match PcbLib::open(filepath) {
            Ok(library) => {
                // Track widths by layer
                let mut track_widths: HashMap<String, Vec<f64>> = HashMap::new();
                // Pad shapes count
                let mut pad_shapes: HashMap<String, usize> = HashMap::new();
                // Text heights
                let mut text_heights: Vec<f64> = Vec::new();
                // Layers used
                let mut layers_used: HashMap<String, usize> = HashMap::new();

                for fp in library.iter() {
                    // Analyse tracks
                    for track in &fp.tracks {
                        let layer_name = track.layer.as_str().to_string();
                        track_widths
                            .entry(layer_name.clone())
                            .or_default()
                            .push(track.width);
                        *layers_used.entry(layer_name).or_insert(0) += 1;
                    }

                    // Analyse pads
                    for pad in &fp.pads {
                        let shape_name = format!("{:?}", pad.shape);
                        *pad_shapes.entry(shape_name).or_insert(0) += 1;
                        let layer_name = pad.layer.as_str().to_string();
                        *layers_used.entry(layer_name).or_insert(0) += 1;
                    }

                    // Analyse text
                    for text in &fp.text {
                        text_heights.push(text.height);
                        let layer_name = text.layer.as_str().to_string();
                        *layers_used.entry(layer_name).or_insert(0) += 1;
                    }

                    // Analyse regions
                    for region in &fp.regions {
                        let layer_name = region.layer.as_str().to_string();
                        *layers_used.entry(layer_name).or_insert(0) += 1;
                    }
                }

                // Calculate statistics for track widths
                #[allow(clippy::cast_precision_loss)]
                let track_width_stats: HashMap<String, Value> = track_widths
                    .into_iter()
                    .map(|(layer, widths)| {
                        let min = widths.iter().copied().fold(f64::INFINITY, f64::min);
                        let max = widths.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                        let avg = widths.iter().sum::<f64>() / widths.len() as f64;
                        let most_common = Self::most_common_f64(&widths);
                        (
                            layer,
                            json!({
                                "min_mm": min,
                                "max_mm": max,
                                "avg_mm": avg,
                                "most_common_mm": most_common,
                                "count": widths.len()
                            }),
                        )
                    })
                    .collect();

                // Calculate text height stats
                let text_height_stats = if text_heights.is_empty() {
                    json!(null)
                } else {
                    let min = text_heights.iter().copied().fold(f64::INFINITY, f64::min);
                    let max = text_heights
                        .iter()
                        .copied()
                        .fold(f64::NEG_INFINITY, f64::max);
                    let most_common = Self::most_common_f64(&text_heights);
                    json!({
                        "min_mm": min,
                        "max_mm": max,
                        "most_common_mm": most_common,
                        "count": text_heights.len()
                    })
                };

                let result = json!({
                    "status": "success",
                    "filepath": filepath,
                    "file_type": "PcbLib",
                    "footprint_count": library.len(),
                    "style": {
                        "track_widths_by_layer": track_width_stats,
                        "pad_shapes": pad_shapes,
                        "text_heights": text_height_stats,
                        "layers_used": layers_used
                    }
                });

                ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e.to_string(),
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Extracts style from a `SchLib` file.
    pub(crate) fn extract_schlib_style(filepath: &str) -> ToolCallResult {
        use crate::altium::SchLib;
        use std::collections::HashMap;

        match SchLib::open(filepath) {
            Ok(library) => {
                // Line widths
                let mut line_widths: Vec<u8> = Vec::new();
                // Pin lengths
                let mut pin_lengths: Vec<i32> = Vec::new();
                // Colours used
                let mut line_colors: HashMap<String, usize> = HashMap::new();
                let mut fill_colors: HashMap<String, usize> = HashMap::new();
                // Rectangle stats
                let mut rect_filled_count = 0usize;
                let mut rect_unfilled_count = 0usize;

                for symbol in library.iter() {
                    // Analyse pins
                    for pin in &symbol.pins {
                        pin_lengths.push(pin.length);
                    }

                    // Analyse rectangles
                    for rect in &symbol.rectangles {
                        line_widths.push(rect.line_width);
                        let line_color = format!("#{:06X}", rect.line_color);
                        let fill_color = format!("#{:06X}", rect.fill_color);
                        *line_colors.entry(line_color).or_insert(0) += 1;
                        *fill_colors.entry(fill_color).or_insert(0) += 1;
                        if rect.filled {
                            rect_filled_count += 1;
                        } else {
                            rect_unfilled_count += 1;
                        }
                    }

                    // Analyse lines
                    for line in &symbol.lines {
                        line_widths.push(line.line_width);
                        let color = format!("#{:06X}", line.color);
                        *line_colors.entry(color).or_insert(0) += 1;
                    }
                }

                // Calculate stats
                let pin_length_stats = if pin_lengths.is_empty() {
                    json!(null)
                } else {
                    let min = *pin_lengths.iter().min().unwrap();
                    let max = *pin_lengths.iter().max().unwrap();
                    let most_common = Self::most_common(&pin_lengths);
                    json!({
                        "min_units": min,
                        "max_units": max,
                        "most_common_units": most_common,
                        "count": pin_lengths.len()
                    })
                };

                let line_width_stats = if line_widths.is_empty() {
                    json!(null)
                } else {
                    let min = *line_widths.iter().min().unwrap();
                    let max = *line_widths.iter().max().unwrap();
                    let most_common = Self::most_common(&line_widths);
                    json!({
                        "min": min,
                        "max": max,
                        "most_common": most_common,
                        "count": line_widths.len()
                    })
                };

                let result = json!({
                    "status": "success",
                    "filepath": filepath,
                    "file_type": "SchLib",
                    "symbol_count": library.len(),
                    "style": {
                        "pin_lengths": pin_length_stats,
                        "line_widths": line_width_stats,
                        "line_colors": line_colors,
                        "fill_colors": fill_colors,
                        "rectangles": {
                            "filled_count": rect_filled_count,
                            "unfilled_count": rect_unfilled_count
                        }
                    }
                });

                ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
            }
            Err(e) => {
                let result = json!({
                    "status": "error",
                    "filepath": filepath,
                    "error": e.to_string(),
                });
                ToolCallResult::error(serde_json::to_string_pretty(&result).unwrap())
            }
        }
    }

    /// Finds the most common value in a slice of hashable, copyable values.
    ///
    /// Returns the default value if the slice is empty.
    pub(crate) fn most_common<T>(values: &[T]) -> T
    where
        T: std::hash::Hash + Eq + Copy + Default,
    {
        use std::collections::HashMap;
        let mut counts: HashMap<T, usize> = HashMap::new();
        for &v in values {
            *counts.entry(v).or_insert(0) += 1;
        }
        counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map_or_else(T::default, |(key, _)| key)
    }

    /// Finds the most common value in a slice of f64, rounded to 2 decimal places.
    ///
    /// Since f64 doesn't implement Hash/Eq, values are quantized to centesimal
    /// precision (0.01) for grouping purposes.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    pub(crate) fn most_common_f64(values: &[f64]) -> f64 {
        use std::collections::HashMap;
        let mut counts: HashMap<i64, usize> = HashMap::new();
        for &v in values {
            // Round to 2 decimal places for grouping
            let key = (v * 100.0).round() as i64;
            *counts.entry(key).or_insert(0) += 1;
        }
        counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map_or(0.0, |(key, _)| key as f64 / 100.0)
    }
}
