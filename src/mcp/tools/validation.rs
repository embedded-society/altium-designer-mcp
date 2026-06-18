//! Coordinate/primitive validation helpers, split from `server.rs`.

use crate::mcp::server::McpServer;

impl McpServer {
    // ==================== Coordinate Validation ====================

    /// Maximum coordinate value in mm that can be safely converted to Altium internal units.
    /// Internal units use i32: max value ~5456 mm (`i32::MAX` / 393700.7874).
    /// We use 5000 mm (~200 inches) as a conservative limit.
    const MAX_COORDINATE_MM: f64 = 5000.0;

    /// Validates that a coordinate is within the safe range for Altium internal units.
    pub(crate) fn validate_coordinate(value: f64, field_name: &str) -> Result<(), String> {
        if !value.is_finite() {
            return Err(format!(
                "{field_name} must be a finite number, got: {value}"
            ));
        }
        if value.abs() > Self::MAX_COORDINATE_MM {
            return Err(format!(
                "{field_name} value {value} mm exceeds the maximum safe range of ±{} mm",
                Self::MAX_COORDINATE_MM
            ));
        }
        Ok(())
    }

    /// Checks if a pad's per-layer data is uniform (all layers have same values).
    ///
    /// When all per-layer values are identical, the data is redundant and can be
    /// omitted in compact mode, even if the pad was stored with `FullStack` mode.
    pub(crate) fn pad_has_uniform_per_layer_data(pad: &crate::altium::pcblib::Pad) -> bool {
        // Check per_layer_sizes - all should match primary width/height
        let sizes_uniform = pad.per_layer_sizes.as_ref().map_or(true, |sizes| {
            sizes
                .iter()
                .all(|&(w, h)| (w - pad.width).abs() < 0.001 && (h - pad.height).abs() < 0.001)
        });

        // Check per_layer_shapes - all should match primary shape
        let shapes_uniform = pad
            .per_layer_shapes
            .as_ref()
            .map_or(true, |shapes| shapes.iter().all(|s| *s == pad.shape));

        // Check per_layer_corner_radii - all should match primary corner_radius_percent
        let primary_radius = pad.corner_radius_percent.unwrap_or(0);
        let radii_uniform = pad
            .per_layer_corner_radii
            .as_ref()
            .map_or(true, |radii| radii.iter().all(|&r| r == primary_radius));

        // Check per_layer_offsets - all should be zero (no offset)
        let offsets_uniform = pad.per_layer_offsets.as_ref().map_or(true, |offsets| {
            offsets
                .iter()
                .all(|&(x, y)| x.abs() < 0.001 && y.abs() < 0.001)
        });

        sizes_uniform && shapes_uniform && radii_uniform && offsets_uniform
    }

    /// Validates all coordinates in a footprint before writing.
    pub(crate) fn validate_footprint_coordinates(
        footprint: &crate::altium::pcblib::Footprint,
    ) -> Result<(), String> {
        let name = &footprint.name;

        for (i, pad) in footprint.pads.iter().enumerate() {
            Self::validate_coordinate(pad.x, &format!("Footprint '{name}' pad {i} x"))?;
            Self::validate_coordinate(pad.y, &format!("Footprint '{name}' pad {i} y"))?;
            Self::validate_coordinate(pad.width, &format!("Footprint '{name}' pad {i} width"))?;
            Self::validate_coordinate(pad.height, &format!("Footprint '{name}' pad {i} height"))?;
            if let Some(hole) = pad.hole_size {
                Self::validate_coordinate(hole, &format!("Footprint '{name}' pad {i} hole_size"))?;
            }
        }

        for (i, track) in footprint.tracks.iter().enumerate() {
            Self::validate_coordinate(track.x1, &format!("Footprint '{name}' track {i} x1"))?;
            Self::validate_coordinate(track.y1, &format!("Footprint '{name}' track {i} y1"))?;
            Self::validate_coordinate(track.x2, &format!("Footprint '{name}' track {i} x2"))?;
            Self::validate_coordinate(track.y2, &format!("Footprint '{name}' track {i} y2"))?;
            Self::validate_coordinate(track.width, &format!("Footprint '{name}' track {i} width"))?;
        }

        for (i, arc) in footprint.arcs.iter().enumerate() {
            Self::validate_coordinate(arc.x, &format!("Footprint '{name}' arc {i} x"))?;
            Self::validate_coordinate(arc.y, &format!("Footprint '{name}' arc {i} y"))?;
            Self::validate_coordinate(arc.radius, &format!("Footprint '{name}' arc {i} radius"))?;
            Self::validate_coordinate(arc.width, &format!("Footprint '{name}' arc {i} width"))?;
        }

        for (i, region) in footprint.regions.iter().enumerate() {
            for (j, vertex) in region.vertices.iter().enumerate() {
                Self::validate_coordinate(
                    vertex.x,
                    &format!("Footprint '{name}' region {i} vertex {j} x"),
                )?;
                Self::validate_coordinate(
                    vertex.y,
                    &format!("Footprint '{name}' region {i} vertex {j} y"),
                )?;
            }
        }

        for (i, text) in footprint.text.iter().enumerate() {
            Self::validate_coordinate(text.x, &format!("Footprint '{name}' text {i} x"))?;
            Self::validate_coordinate(text.y, &format!("Footprint '{name}' text {i} y"))?;
            Self::validate_coordinate(text.height, &format!("Footprint '{name}' text {i} height"))?;
        }

        Ok(())
    }

    /// Maximum coordinate value for `SchLib` (uses i16 internally).
    /// `i16::MAX` = 32767, but we use 32000 as a conservative limit.
    const MAX_SCHLIB_COORDINATE: i32 = 32000;

    /// Validates that a `SchLib` coordinate is within the safe range for i16.
    pub(crate) fn validate_schlib_coordinate(value: i32, field_name: &str) -> Result<(), String> {
        if value.abs() > Self::MAX_SCHLIB_COORDINATE {
            return Err(format!(
                "{field_name} value {value} exceeds the maximum safe range of ±{} units",
                Self::MAX_SCHLIB_COORDINATE
            ));
        }
        Ok(())
    }

    /// Validates all coordinates in a symbol before writing.
    pub(crate) fn validate_symbol_coordinates(
        symbol: &crate::altium::schlib::Symbol,
    ) -> Result<(), String> {
        let name = &symbol.name;

        for (i, pin) in symbol.pins.iter().enumerate() {
            Self::validate_schlib_coordinate(pin.x, &format!("Symbol '{name}' pin {i} x"))?;
            Self::validate_schlib_coordinate(pin.y, &format!("Symbol '{name}' pin {i} y"))?;
            Self::validate_schlib_coordinate(
                pin.length,
                &format!("Symbol '{name}' pin {i} length"),
            )?;
        }

        for (i, rect) in symbol.rectangles.iter().enumerate() {
            Self::validate_schlib_coordinate(
                rect.x1,
                &format!("Symbol '{name}' rectangle {i} x1"),
            )?;
            Self::validate_schlib_coordinate(
                rect.y1,
                &format!("Symbol '{name}' rectangle {i} y1"),
            )?;
            Self::validate_schlib_coordinate(
                rect.x2,
                &format!("Symbol '{name}' rectangle {i} x2"),
            )?;
            Self::validate_schlib_coordinate(
                rect.y2,
                &format!("Symbol '{name}' rectangle {i} y2"),
            )?;
        }

        for (i, line) in symbol.lines.iter().enumerate() {
            Self::validate_schlib_coordinate(line.x1, &format!("Symbol '{name}' line {i} x1"))?;
            Self::validate_schlib_coordinate(line.y1, &format!("Symbol '{name}' line {i} y1"))?;
            Self::validate_schlib_coordinate(line.x2, &format!("Symbol '{name}' line {i} x2"))?;
            Self::validate_schlib_coordinate(line.y2, &format!("Symbol '{name}' line {i} y2"))?;
        }

        for (i, polyline) in symbol.polylines.iter().enumerate() {
            for (j, &(x, y)) in polyline.points.iter().enumerate() {
                Self::validate_schlib_coordinate(
                    x,
                    &format!("Symbol '{name}' polyline {i} point {j} x"),
                )?;
                Self::validate_schlib_coordinate(
                    y,
                    &format!("Symbol '{name}' polyline {i} point {j} y"),
                )?;
            }
        }

        for (i, arc) in symbol.arcs.iter().enumerate() {
            Self::validate_schlib_coordinate(arc.x, &format!("Symbol '{name}' arc {i} x"))?;
            Self::validate_schlib_coordinate(arc.y, &format!("Symbol '{name}' arc {i} y"))?;
            Self::validate_schlib_coordinate(
                arc.radius,
                &format!("Symbol '{name}' arc {i} radius"),
            )?;
        }

        for (i, ellipse) in symbol.ellipses.iter().enumerate() {
            Self::validate_schlib_coordinate(ellipse.x, &format!("Symbol '{name}' ellipse {i} x"))?;
            Self::validate_schlib_coordinate(ellipse.y, &format!("Symbol '{name}' ellipse {i} y"))?;
            Self::validate_schlib_coordinate(
                ellipse.radius_x,
                &format!("Symbol '{name}' ellipse {i} radius_x"),
            )?;
            Self::validate_schlib_coordinate(
                ellipse.radius_y,
                &format!("Symbol '{name}' ellipse {i} radius_y"),
            )?;
        }

        for (i, label) in symbol.labels.iter().enumerate() {
            Self::validate_schlib_coordinate(label.x, &format!("Symbol '{name}' label {i} x"))?;
            Self::validate_schlib_coordinate(label.y, &format!("Symbol '{name}' label {i} y"))?;
        }

        Ok(())
    }
}
