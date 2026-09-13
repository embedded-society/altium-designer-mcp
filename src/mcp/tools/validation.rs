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

    /// Validates a component name for every tool that creates one (write,
    /// copy, rename, bulk rename, update, import): non-empty and free of
    /// ASCII control characters. Punctuation is Altium's business, not ours: an
    /// AD21-authored library names footprints `EC10*10.5` and `L1210/3225`
    /// (#507), and the library layer stores such a name under a sanitised,
    /// 31-unit storage name exactly as Altium does while the full name
    /// travels in PATTERN/LIBREFERENCE.
    /// The error for a name the library already holds: `message` as given
    /// when the spelling is the same, and with the existing spelling named
    /// when only the case differs — the two are one storage to the OLE
    /// directory and one component to Altium, so the clash is real even
    /// though the strings are not equal.
    pub(crate) fn taken_name_error(message: String, requested: &str, existing: &str) -> String {
        if existing == requested {
            message
        } else {
            format!("{message} as '{existing}' (component names are case-insensitive)")
        }
    }

    pub(crate) fn validate_ole_name(name: &str) -> Result<(), String> {
        if name.is_empty() {
            return Err("Component name cannot be empty".to_string());
        }
        // Only the C0 range and DEL: a non-1252 name travels in wire form,
        // whose bytes 0x80-0x9F decode to C1 controls that are part of it.
        if let Some(c) = name.chars().find(char::is_ascii_control) {
            return Err(format!(
                "Component name '{}' contains the control character U+{:04X}",
                name.escape_default(),
                u32::from(c)
            ));
        }
        Ok(())
    }

    /// Validates every coordinate and size a footprint will write, kind by
    /// kind from the enum, so a primitive kind cannot go unchecked: a value
    /// past the safe range would otherwise saturate on save and land in the
    /// file silently wrong.
    pub(crate) fn validate_footprint_coordinates(
        footprint: &crate::altium::pcblib::Footprint,
    ) -> Result<(), String> {
        for kind in crate::altium::pcblib::PrimitiveKind::WRITE_ORDER {
            Self::validate_footprint_kind(footprint, kind)?;
        }
        Ok(())
    }

    /// The range checks of one primitive kind.
    fn validate_footprint_kind(
        footprint: &crate::altium::pcblib::Footprint,
        kind: crate::altium::pcblib::PrimitiveKind,
    ) -> Result<(), String> {
        use crate::altium::pcblib::PrimitiveKind;

        let name = &footprint.name;
        let check = |value: f64, index: usize, field: &str| {
            Self::validate_coordinate(
                value,
                &format!("Footprint '{name}' {} {index} {field}", kind.name()),
            )
        };
        match kind {
            PrimitiveKind::Pad => {
                for (i, pad) in footprint.pads.iter().enumerate() {
                    check(pad.x, i, "x")?;
                    check(pad.y, i, "y")?;
                    check(pad.width, i, "width")?;
                    check(pad.height, i, "height")?;
                    if let Some(hole) = pad.hole_size {
                        check(hole, i, "hole_size")?;
                    }
                }
            }
            PrimitiveKind::Via => {
                for (i, via) in footprint.vias.iter().enumerate() {
                    check(via.x, i, "x")?;
                    check(via.y, i, "y")?;
                    check(via.diameter, i, "diameter")?;
                    check(via.hole_size, i, "hole_size")?;
                }
            }
            PrimitiveKind::Track => {
                for (i, track) in footprint.tracks.iter().enumerate() {
                    check(track.x1, i, "x1")?;
                    check(track.y1, i, "y1")?;
                    check(track.x2, i, "x2")?;
                    check(track.y2, i, "y2")?;
                    check(track.width, i, "width")?;
                }
            }
            PrimitiveKind::Arc => {
                for (i, arc) in footprint.arcs.iter().enumerate() {
                    check(arc.x, i, "x")?;
                    check(arc.y, i, "y")?;
                    check(arc.radius, i, "radius")?;
                    check(arc.width, i, "width")?;
                }
            }
            PrimitiveKind::Region => {
                for (i, region) in footprint.regions.iter().enumerate() {
                    for (j, vertex) in region.vertices.iter().enumerate() {
                        check(vertex.x, i, &format!("vertex {j} x"))?;
                        check(vertex.y, i, &format!("vertex {j} y"))?;
                    }
                }
            }
            PrimitiveKind::Text => {
                for (i, text) in footprint.text.iter().enumerate() {
                    check(text.x, i, "x")?;
                    check(text.y, i, "y")?;
                    check(text.height, i, "height")?;
                }
            }
            PrimitiveKind::Fill => {
                for (i, fill) in footprint.fills.iter().enumerate() {
                    check(fill.x1, i, "x1")?;
                    check(fill.y1, i, "y1")?;
                    check(fill.x2, i, "x2")?;
                    check(fill.y2, i, "y2")?;
                }
            }
            PrimitiveKind::ComponentBody => {
                for (i, body) in footprint.component_bodies.iter().enumerate() {
                    for (j, (x, y)) in body.outline.iter().enumerate() {
                        check(*x, i, &format!("outline {j} x"))?;
                        check(*y, i, &format!("outline {j} y"))?;
                    }
                    check(body.overall_height, i, "overall_height")?;
                    check(body.standoff_height, i, "standoff_height")?;
                    check(body.z_offset, i, "z_offset")?;
                }
            }
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

        for (i, ieee) in symbol.ieee_symbols.iter().enumerate() {
            Self::validate_schlib_coordinate(
                ieee.x,
                &format!("Symbol '{name}' ieee_symbol {i} x"),
            )?;
            Self::validate_schlib_coordinate(
                ieee.y,
                &format!("Symbol '{name}' ieee_symbol {i} y"),
            )?;
            Self::validate_schlib_coordinate(
                ieee.scale_factor,
                &format!("Symbol '{name}' ieee_symbol {i} scale_factor"),
            )?;
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
    use crate::altium::pcblib::{Arc, Footprint, Layer, Pad, Region, Track};
    use crate::altium::schlib::{Ellipse, Line, Pin, PinOrientation, Rectangle, RoundRect, Symbol};
    use crate::mcp::server::McpServer;

    // ---- validate_coordinate ------------------------------------------------

    /// Well past `MAX_SCHLIB_COORDINATE`, so any family that range-checks its
    /// coordinates rejects it.
    const FAR: f64 = 99_999.0;

    #[test]
    #[allow(clippy::too_many_lines)] // a flat per-family case list, like the code it covers
    fn every_symbol_shape_family_has_its_coordinates_range_checked() {
        // A schematic coordinate past the safe range saturates on save, so a
        // shape drawn far off-sheet would be written silently wrong. Each
        // family is checked by its own loop, so each needs its own case or a
        // family could go unchecked without any test noticing.
        use crate::mcp::tools::test_support::{create_test_server, get_result_text, test_temp_dir};
        use serde_json::json;

        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Far.SchLib");

        let cases: [(&str, serde_json::Value, &str); 14] = [
            (
                "rectangles",
                json!([{ "x1": 0, "y1": 0, "x2": FAR, "y2": 10 }]),
                "rectangle 0 x2",
            ),
            (
                "lines",
                json!([{ "x1": 0, "y1": 0, "x2": FAR, "y2": 0 }]),
                "line 0 x2",
            ),
            (
                "polylines",
                json!([{ "points": [{ "x": 0, "y": 0 }, { "x": FAR, "y": 0 }] }]),
                "polyline 0 point 1 x",
            ),
            (
                "arcs",
                json!([{ "x": 0, "y": 0, "radius": FAR, "start_angle": 0, "end_angle": 90 }]),
                "arc 0 radius",
            ),
            (
                "ellipses",
                json!([{ "x": 0, "y": 0, "radius_x": FAR, "radius_y": 5 }]),
                "ellipse 0 radius_x",
            ),
            (
                "labels",
                json!([{ "x": FAR, "y": 0, "text": "L" }]),
                "label 0 x",
            ),
            (
                "round_rects",
                json!([{
                    "x1": 0, "y1": 0, "x2": 10, "y2": 10,
                    "corner_x_radius": FAR, "corner_y_radius": 2,
                }]),
                "round_rect 0 corner_x_radius",
            ),
            (
                "polygons",
                json!([{ "points": [{ "x": 0, "y": 0 }, { "x": 10, "y": 0 }, { "x": FAR, "y": 10 }] }]),
                "polygon 0 point 2 x",
            ),
            (
                "pies",
                json!([{ "x": 0, "y": 0, "radius": FAR, "start_angle": 0, "end_angle": 90 }]),
                "pie 0 radius",
            ),
            (
                "images",
                json!([{ "x1": 0, "y1": 0, "x2": FAR, "y2": 10, "file_name": "logo.bmp" }]),
                "image 0 x2",
            ),
            (
                "text_frames",
                json!([{ "x1": 0, "y1": 0, "x2": FAR, "y2": 10, "text": "F" }]),
                "text_frame 0 x2",
            ),
            (
                "beziers",
                json!([{
                    "x1": 0, "y1": 0, "x2": 10, "y2": 0,
                    "x3": 20, "y3": 0, "x4": FAR, "y4": 0,
                }]),
                "bezier 0 point 3 x",
            ),
            (
                "elliptical_arcs",
                json!([{
                    "x": 0, "y": 0, "radius": FAR, "secondary_radius": 5,
                    "start_angle": 0, "end_angle": 90,
                }]),
                "elliptical_arc 0 radius",
            ),
            (
                "ieee_symbols",
                json!([{ "x": FAR, "y": 0, "symbol": 1 }]),
                "ieee_symbol 0 x",
            ),
        ];

        for (family, payload, expected) in cases {
            let mut symbol = json!({ "name": "FAR" });
            symbol[family] = payload;
            let r = server.call_write_schlib(&json!({
                "filepath": path.to_string_lossy(),
                "symbols": [symbol],
            }));
            let text = get_result_text(&r);
            assert!(r.is_error, "{family} was not range-checked: {text}");
            assert!(
                text.contains(expected),
                "{family}: expected the error to name {expected:?}, got: {text}"
            );
        }
    }

    /// Every footprint primitive kind has its coordinates and sizes
    /// range-checked — a kind the validator skipped would saturate on save.
    #[test]
    fn every_footprint_primitive_kind_has_its_coordinates_range_checked() {
        use crate::altium::pcblib::PrimitiveKind;
        use crate::mcp::tools::test_support::{create_test_server, get_result_text, test_temp_dir};
        use serde_json::json;

        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Far.PcbLib");

        let cases: [(PrimitiveKind, &str, serde_json::Value, &str); PrimitiveKind::COUNT] = [
            (
                PrimitiveKind::Pad,
                "pads",
                json!([{ "designator": "1", "x": FAR, "y": 0, "width": 1, "height": 1 }]),
                "pad 0 x",
            ),
            (
                PrimitiveKind::Via,
                "vias",
                json!([{ "x": 0, "y": FAR, "diameter": 0.6, "hole_size": 0.3 }]),
                "via 0 y",
            ),
            (
                PrimitiveKind::Track,
                "tracks",
                json!([{ "x1": 0, "y1": 0, "x2": FAR, "y2": 0, "width": 0.2, "layer": "TopOverlay" }]),
                "track 0 x2",
            ),
            (
                PrimitiveKind::Arc,
                "arcs",
                json!([{ "x": 0, "y": 0, "radius": FAR, "start_angle": 0, "end_angle": 360, "width": 0.2, "layer": "TopOverlay" }]),
                "arc 0 radius",
            ),
            (
                PrimitiveKind::Text,
                "text",
                json!([{ "x": 0, "y": 0, "text": "T", "height": FAR, "layer": "TopOverlay" }]),
                "text 0 height",
            ),
            (
                PrimitiveKind::Region,
                "regions",
                json!([{ "vertices": [{ "x": 0, "y": 0 }, { "x": 1, "y": 0 }, { "x": FAR, "y": 1 }], "layer": "TopCourtyard" }]),
                "region 0 vertex 2 x",
            ),
            (
                PrimitiveKind::Fill,
                "fills",
                json!([{ "x1": 0, "y1": 0, "x2": 1, "y2": FAR, "layer": "TopOverlay" }]),
                "fill 0 y2",
            ),
            (
                PrimitiveKind::ComponentBody,
                "component_bodies",
                json!([{ "outline": [[0, 0], [1, 0], [1, FAR]], "overall_height": 1 }]),
                "component_body 0 outline 2 y",
            ),
        ];
        let covered: std::collections::BTreeSet<&str> =
            cases.iter().map(|(kind, ..)| kind.name()).collect();
        assert_eq!(covered.len(), PrimitiveKind::COUNT, "one case per kind");

        for (_, family, payload, expected) in cases {
            let mut footprint = json!({ "name": "FAR" });
            footprint[family] = payload;
            let r = server.call_write_pcblib(&json!({
                "filepath": path.to_string_lossy(),
                "footprints": [footprint],
            }));
            let text = get_result_text(&r);
            assert!(r.is_error, "{family} was not range-checked: {text}");
            assert!(
                text.contains(expected),
                "{family}: expected the error to name {expected:?}, got: {text}"
            );
        }
    }

    #[test]
    #[allow(clippy::too_many_lines)] // a flat per-field case list, like the code it covers
    fn each_coordinate_within_a_family_is_checked_on_its_own() {
        // The families above are checked one field at a time, so covering one
        // corner of a rectangle says nothing about the other three. A field
        // that slipped through would saturate on save while its neighbours
        // were caught — a shape anchored correctly at one end and folded flat
        // at the other.
        use crate::mcp::tools::test_support::{create_test_server, get_result_text, test_temp_dir};
        use serde_json::json;

        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Corners.SchLib");

        let cases: [(&str, serde_json::Value, &str); 17] = [
            (
                // Pin coordinates are integers, so this mirrors FAR rather
                // than casting it.
                "pins",
                json!([{ "name": "1", "designator": "1", "x": 0, "y": 99_999, "length": 10, "orientation": "left" }]),
                "pin 0 y",
            ),
            (
                "pins",
                json!([{ "name": "1", "designator": "1", "x": 0, "y": 0, "length": 99_999, "orientation": "left" }]),
                "pin 0 length",
            ),
            (
                "rectangles",
                json!([{ "x1": FAR, "y1": 0, "x2": 10, "y2": 10 }]),
                "rectangle 0 x1",
            ),
            (
                "rectangles",
                json!([{ "x1": 0, "y1": FAR, "x2": 10, "y2": 10 }]),
                "rectangle 0 y1",
            ),
            (
                "rectangles",
                json!([{ "x1": 0, "y1": 0, "x2": 10, "y2": FAR }]),
                "rectangle 0 y2",
            ),
            (
                "polylines",
                json!([{ "points": [{ "x": 0, "y": 0 }, { "x": 0, "y": FAR }] }]),
                "polyline 0 point 1 y",
            ),
            (
                "ellipses",
                json!([{ "x": 0, "y": 0, "radius_x": 5, "radius_y": FAR }]),
                "ellipse 0 radius_y",
            ),
            (
                "round_rects",
                json!([{
                    "x1": 0, "y1": 0, "x2": 10, "y2": 10,
                    "corner_x_radius": 2, "corner_y_radius": FAR,
                }]),
                "round_rect 0 corner_y_radius",
            ),
            (
                "polygons",
                json!([{ "points": [{ "x": 0, "y": 0 }, { "x": 10, "y": 0 }, { "x": 5, "y": FAR }] }]),
                "polygon 0 point 2 y",
            ),
            (
                "text_frames",
                json!([{ "x1": 0, "y1": FAR, "x2": 10, "y2": 10, "text": "F" }]),
                "text_frame 0 y1",
            ),
            (
                "text_frames",
                json!([{ "x1": 0, "y1": 0, "x2": 10, "y2": FAR, "text": "F" }]),
                "text_frame 0 y2",
            ),
            (
                "text_frames",
                json!([{ "x1": 0, "y1": 0, "x2": 10, "y2": 10, "text": "F", "text_margin": FAR }]),
                "text_frame 0 text_margin",
            ),
            (
                "beziers",
                json!([{
                    "x1": 0, "y1": 0, "x2": 10, "y2": 0,
                    "x3": 20, "y3": 0, "x4": 30, "y4": FAR,
                }]),
                "bezier 0 point 3 y",
            ),
            (
                "elliptical_arcs",
                json!([{
                    "x": 0, "y": FAR, "radius": 5, "secondary_radius": 5,
                    "start_angle": 0, "end_angle": 90,
                }]),
                "elliptical_arc 0 y",
            ),
            (
                "elliptical_arcs",
                json!([{
                    "x": 0, "y": 0, "radius": 5, "secondary_radius": FAR,
                    "start_angle": 0, "end_angle": 90,
                }]),
                "elliptical_arc 0 secondary_radius",
            ),
            (
                "ieee_symbols",
                json!([{ "x": 0, "y": FAR, "symbol": 1 }]),
                "ieee_symbol 0 y",
            ),
            (
                "ieee_symbols",
                json!([{ "x": 0, "y": 0, "symbol": 1, "scale_factor": FAR }]),
                "ieee_symbol 0 scale_factor",
            ),
        ];

        for (family, payload, expected) in cases {
            let mut symbol = json!({ "name": "FAR" });
            symbol[family] = payload;
            let r = server.call_write_schlib(&json!({
                "filepath": path.to_string_lossy(),
                "symbols": [symbol],
            }));
            let text = get_result_text(&r);
            assert!(r.is_error, "{expected} was not range-checked: {text}");
            assert!(
                text.contains(expected),
                "expected the error to name {expected:?}, got: {text}"
            );
        }
    }

    #[test]
    fn a_footprint_text_height_out_of_range_is_caught() {
        // The PcbLib side has the same per-field shape; text height is the one
        // field there that is neither a coordinate nor a width.
        use crate::mcp::tools::test_support::{create_test_server, get_result_text, test_temp_dir};
        use serde_json::json;

        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let r = server.call_write_pcblib(&json!({
            "filepath": dir.path().join("Tall.PcbLib").to_string_lossy(),
            "footprints": [{
                "name": "FP",
                "pads": [{ "designator": "1", "x": 0.0, "y": 0.0, "width": 1.0, "height": 1.0 }],
                "text": [{ "x": 0.0, "y": 0.0, "text": "REF", "height": 99_999.0, "layer": "Top Overlay" }],
            }],
        }));
        let text = get_result_text(&r);
        assert!(r.is_error, "{text}");
        assert!(text.contains("text 0 height"), "{text}");
    }

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
