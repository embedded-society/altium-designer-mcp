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

    /// Validates that a `SchLib` coordinate is within the safe range. Graphic
    /// primitives carry f64 (off-grid) coordinates, so this takes f64 and also
    /// rejects non-finite (NaN/∞) values. Pins pass their i32 coordinates via
    /// `f64::from`.
    pub(crate) fn validate_schlib_coordinate(value: f64, field_name: &str) -> Result<(), String> {
        let max = f64::from(Self::MAX_SCHLIB_COORDINATE);
        if !value.is_finite() || value < -max || value > max {
            return Err(format!(
                "{field_name} value {value} exceeds the maximum safe range of ±{} units",
                Self::MAX_SCHLIB_COORDINATE
            ));
        }
        Ok(())
    }

    /// Validates all coordinates in a symbol before writing.
    #[allow(clippy::too_many_lines)] // a flat per-family checklist — splitting adds no clarity
    pub(crate) fn validate_symbol_coordinates(
        symbol: &crate::altium::schlib::Symbol,
    ) -> Result<(), String> {
        let name = &symbol.name;

        for (i, pin) in symbol.pins.iter().enumerate() {
            Self::validate_schlib_coordinate(
                f64::from(pin.x),
                &format!("Symbol '{name}' pin {i} x"),
            )?;
            Self::validate_schlib_coordinate(
                f64::from(pin.y),
                &format!("Symbol '{name}' pin {i} y"),
            )?;
            Self::validate_schlib_coordinate(
                f64::from(pin.length),
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

        for (i, rr) in symbol.round_rects.iter().enumerate() {
            Self::validate_schlib_coordinate(rr.x1, &format!("Symbol '{name}' round_rect {i} x1"))?;
            Self::validate_schlib_coordinate(rr.y1, &format!("Symbol '{name}' round_rect {i} y1"))?;
            Self::validate_schlib_coordinate(rr.x2, &format!("Symbol '{name}' round_rect {i} x2"))?;
            Self::validate_schlib_coordinate(rr.y2, &format!("Symbol '{name}' round_rect {i} y2"))?;
            Self::validate_schlib_coordinate(
                rr.corner_x_radius,
                &format!("Symbol '{name}' round_rect {i} corner_x_radius"),
            )?;
            Self::validate_schlib_coordinate(
                rr.corner_y_radius,
                &format!("Symbol '{name}' round_rect {i} corner_y_radius"),
            )?;
        }

        for (i, polygon) in symbol.polygons.iter().enumerate() {
            for (j, &(x, y)) in polygon.points.iter().enumerate() {
                Self::validate_schlib_coordinate(
                    x,
                    &format!("Symbol '{name}' polygon {i} point {j} x"),
                )?;
                Self::validate_schlib_coordinate(
                    y,
                    &format!("Symbol '{name}' polygon {i} point {j} y"),
                )?;
            }
        }

        for (i, pie) in symbol.pies.iter().enumerate() {
            Self::validate_schlib_coordinate(pie.x, &format!("Symbol '{name}' pie {i} x"))?;
            Self::validate_schlib_coordinate(pie.y, &format!("Symbol '{name}' pie {i} y"))?;
            Self::validate_schlib_coordinate(
                pie.radius,
                &format!("Symbol '{name}' pie {i} radius"),
            )?;
        }

        for (i, image) in symbol.images.iter().enumerate() {
            Self::validate_schlib_coordinate(image.x1, &format!("Symbol '{name}' image {i} x1"))?;
            Self::validate_schlib_coordinate(image.y1, &format!("Symbol '{name}' image {i} y1"))?;
            Self::validate_schlib_coordinate(image.x2, &format!("Symbol '{name}' image {i} x2"))?;
            Self::validate_schlib_coordinate(image.y2, &format!("Symbol '{name}' image {i} y2"))?;
        }

        for (i, frame) in symbol.text_frames.iter().enumerate() {
            Self::validate_schlib_coordinate(
                frame.x1,
                &format!("Symbol '{name}' text_frame {i} x1"),
            )?;
            Self::validate_schlib_coordinate(
                frame.y1,
                &format!("Symbol '{name}' text_frame {i} y1"),
            )?;
            Self::validate_schlib_coordinate(
                frame.x2,
                &format!("Symbol '{name}' text_frame {i} x2"),
            )?;
            Self::validate_schlib_coordinate(
                frame.y2,
                &format!("Symbol '{name}' text_frame {i} y2"),
            )?;
            Self::validate_schlib_coordinate(
                frame.text_margin,
                &format!("Symbol '{name}' text_frame {i} text_margin"),
            )?;
        }

        for (i, bezier) in symbol.beziers.iter().enumerate() {
            for (j, (x, y)) in [
                (bezier.x1, bezier.y1),
                (bezier.x2, bezier.y2),
                (bezier.x3, bezier.y3),
                (bezier.x4, bezier.y4),
            ]
            .into_iter()
            .enumerate()
            {
                Self::validate_schlib_coordinate(
                    x,
                    &format!("Symbol '{name}' bezier {i} point {j} x"),
                )?;
                Self::validate_schlib_coordinate(
                    y,
                    &format!("Symbol '{name}' bezier {i} point {j} y"),
                )?;
            }
        }

        for (i, ell_arc) in symbol.elliptical_arcs.iter().enumerate() {
            Self::validate_schlib_coordinate(
                ell_arc.x,
                &format!("Symbol '{name}' elliptical_arc {i} x"),
            )?;
            Self::validate_schlib_coordinate(
                ell_arc.y,
                &format!("Symbol '{name}' elliptical_arc {i} y"),
            )?;
            Self::validate_schlib_coordinate(
                ell_arc.radius,
                &format!("Symbol '{name}' elliptical_arc {i} radius"),
            )?;
            Self::validate_schlib_coordinate(
                ell_arc.secondary_radius,
                &format!("Symbol '{name}' elliptical_arc {i} secondary_radius"),
            )?;
        }

        for (i, text) in symbol.text.iter().enumerate() {
            Self::validate_schlib_coordinate(text.x, &format!("Symbol '{name}' text {i} x"))?;
            Self::validate_schlib_coordinate(text.y, &format!("Symbol '{name}' text {i} y"))?;
        }

        for (i, param) in symbol.parameters.iter().enumerate() {
            Self::validate_schlib_coordinate(param.x, &format!("Symbol '{name}' parameter {i} x"))?;
            Self::validate_schlib_coordinate(param.y, &format!("Symbol '{name}' parameter {i} y"))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::altium::pcblib::{Arc, Footprint, Layer, Pad, PadShape, Region, Track};
    use crate::altium::schlib::{Ellipse, Line, Pin, PinOrientation, Rectangle, RoundRect, Symbol};
    use crate::mcp::server::McpServer;

    // ---- validate_coordinate ------------------------------------------------

    #[test]
    fn validate_coordinate_accepts_finite_in_range() {
        assert!(McpServer::validate_coordinate(1234.5, "x").is_ok());
        assert!(McpServer::validate_coordinate(0.0, "x").is_ok());
    }

    #[test]
    fn validate_coordinate_rejects_nan_and_infinite() {
        assert!(McpServer::validate_coordinate(f64::NAN, "x")
            .unwrap_err()
            .contains("finite"));
        assert!(McpServer::validate_coordinate(f64::INFINITY, "x")
            .unwrap_err()
            .contains("finite"));
    }

    #[test]
    fn validate_coordinate_rejects_out_of_range() {
        assert!(McpServer::validate_coordinate(6000.0, "x")
            .unwrap_err()
            .contains("exceeds"));
    }

    // ---- pad_has_uniform_per_layer_data ------------------------------------

    #[test]
    fn pad_uniform_when_no_per_layer_data() {
        let pad = Pad::smd("1", 0.0, 0.0, 1.0, 1.0);
        assert!(McpServer::pad_has_uniform_per_layer_data(&pad));
    }

    #[test]
    fn pad_non_uniform_sizes_shapes_radii_offsets() {
        let base = Pad::smd("1", 0.0, 0.0, 1.0, 1.0);

        let mut sizes = base.clone();
        sizes.per_layer_sizes = Some(vec![(2.0, 1.0)]); // width differs from 1.0
        assert!(!McpServer::pad_has_uniform_per_layer_data(&sizes));

        let mut shapes = base.clone();
        shapes.per_layer_shapes = Some(vec![PadShape::Round]); // differs from primary
        assert!(!McpServer::pad_has_uniform_per_layer_data(&shapes));

        let mut radii = base.clone();
        radii.corner_radius_percent = Some(0);
        radii.per_layer_corner_radii = Some(vec![50]); // differs from 0
        assert!(!McpServer::pad_has_uniform_per_layer_data(&radii));

        let mut offsets = base;
        offsets.per_layer_offsets = Some(vec![(0.5, 0.0)]); // non-zero offset
        assert!(!McpServer::pad_has_uniform_per_layer_data(&offsets));
    }

    // ---- validate_footprint_coordinates ------------------------------------

    fn valid_footprint() -> Footprint {
        let mut fp = Footprint::new("F");
        let mut pad = Pad::smd("1", 0.0, 0.0, 1.0, 1.0);
        pad.hole_size = Some(0.5); // exercise the Some(hole) branch
        fp.add_pad(pad);
        fp.add_track(Track::new(-1.0, 0.0, 1.0, 0.0, 0.15, Layer::TopOverlay));
        fp.add_arc(Arc::circle(0.0, 2.0, 0.5, 0.1, Layer::TopOverlay));
        fp.add_region(Region::rectangle(-1.0, -1.0, 1.0, 1.0, Layer::TopCourtyard));
        fp
    }

    #[test]
    fn footprint_all_families_in_range_ok() {
        assert!(McpServer::validate_footprint_coordinates(&valid_footprint()).is_ok());
    }

    #[test]
    fn footprint_bad_pad_coordinate_reports_field() {
        let mut fp = Footprint::new("F");
        fp.add_pad(Pad::smd("1", 6000.0, 0.0, 1.0, 1.0));
        let err = McpServer::validate_footprint_coordinates(&fp).unwrap_err();
        assert!(err.contains("pad 0 x"), "{err}");
    }

    #[test]
    fn footprint_bad_track_coordinate_reports_field() {
        let mut fp = Footprint::new("F");
        fp.add_track(Track::new(0.0, 0.0, 6000.0, 0.0, 0.15, Layer::TopOverlay));
        let err = McpServer::validate_footprint_coordinates(&fp).unwrap_err();
        assert!(err.contains("track 0 x2"), "{err}");
    }

    #[test]
    fn footprint_bad_arc_radius_reports_field() {
        let mut fp = Footprint::new("F");
        fp.add_arc(Arc::circle(0.0, 0.0, 6000.0, 0.1, Layer::TopOverlay));
        let err = McpServer::validate_footprint_coordinates(&fp).unwrap_err();
        assert!(err.contains("arc 0 radius"), "{err}");
    }

    #[test]
    fn footprint_bad_region_vertex_reports_field() {
        let mut fp = Footprint::new("F");
        fp.add_region(Region::rectangle(
            -6000.0,
            -1.0,
            1.0,
            1.0,
            Layer::TopCourtyard,
        ));
        let err = McpServer::validate_footprint_coordinates(&fp).unwrap_err();
        assert!(err.contains("region 0 vertex"), "{err}");
    }

    // ---- validate_symbol_coordinates ---------------------------------------

    fn valid_symbol() -> Symbol {
        let mut sym = Symbol::new("S");
        sym.add_pin(Pin::new("1", "1", -20, 0, 10, PinOrientation::Left));
        sym.add_rectangle(Rectangle::new(-10, -5, 10, 5));
        sym.add_line(Line::new(0, 0, 20, 0));
        sym.add_ellipse(Ellipse::new(5, 5, 3, 2));
        sym.add_round_rect(RoundRect::new(0, 0, 10, 8, 2, 2));
        sym
    }

    #[test]
    fn symbol_common_families_in_range_ok() {
        assert!(McpServer::validate_symbol_coordinates(&valid_symbol()).is_ok());
    }

    #[test]
    fn symbol_bad_pin_coordinate_reports_field() {
        let mut sym = Symbol::new("S");
        sym.add_pin(Pin::new("1", "1", 40000, 0, 10, PinOrientation::Left));
        let err = McpServer::validate_symbol_coordinates(&sym).unwrap_err();
        assert!(err.contains("pin 0 x"), "{err}");
    }

    #[test]
    fn symbol_bad_rectangle_coordinate_reports_field() {
        let mut sym = Symbol::new("S");
        sym.add_rectangle(Rectangle::new(0, 0, 40000, 5));
        let err = McpServer::validate_symbol_coordinates(&sym).unwrap_err();
        assert!(err.contains("rectangle 0 x2"), "{err}");
    }
}
