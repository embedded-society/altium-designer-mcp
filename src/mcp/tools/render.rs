//! Footprint/symbol ASCII rendering tools, split from `server.rs`.

use serde_json::{json, Value};

use crate::mcp::server::{McpServer, ToolCallResult};

impl McpServer {
    // ==================== Rendering Tools ====================

    /// Renders an ASCII art visualisation of a footprint.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub(crate) fn call_render_footprint(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::PcbLib;

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        let Some(component_name) = arguments.get("component_name").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: component_name");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        // Parse optional parameters
        let scale = arguments
            .get("scale")
            .and_then(Value::as_f64)
            .unwrap_or(2.0);
        let max_width = arguments
            .get("max_width")
            .and_then(Value::as_u64)
            .unwrap_or(80) as usize;
        let max_height = arguments
            .get("max_height")
            .and_then(Value::as_u64)
            .unwrap_or(40) as usize;

        if scale <= 0.0 {
            return ToolCallResult::error("scale must be greater than 0");
        }

        // Read the library
        let library = match PcbLib::open(filepath) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
        };

        // Find the footprint
        let Some(footprint) = library.get(component_name) else {
            let available: Vec<_> = library.iter().take(5).map(|f| &f.name).collect();
            let hint = if available.is_empty() {
                "Library is empty".to_string()
            } else {
                format!(
                    "Available footprints include: {}{}",
                    available
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    if library.len() > 5 {
                        format!(" (and {} more)", library.len() - 5)
                    } else {
                        String::new()
                    }
                )
            };
            return ToolCallResult::error(format!(
                "Footprint '{component_name}' not found in library. {hint}"
            ));
        };

        // Render the footprint
        let ascii_art = Self::render_footprint_ascii(footprint, scale, max_width, max_height);

        let result = json!({
            "status": "success",
            "filepath": filepath,
            "component_name": component_name,
            "scale": scale,
            "render": ascii_art,
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Renders a footprint as ASCII art.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        clippy::similar_names,
        clippy::too_many_lines,
        clippy::float_cmp,
        clippy::needless_range_loop
    )]
    pub(crate) fn render_footprint_ascii(
        footprint: &crate::altium::pcblib::Footprint,
        scale: f64,
        max_width: usize,
        max_height: usize,
    ) -> String {
        use std::fmt::Write;

        // Find bounding box
        let (mut min_x, mut max_x, mut min_y, mut max_y) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);

        for pad in &footprint.pads {
            let half_w = pad.width / 2.0;
            let half_h = pad.height / 2.0;
            min_x = min_x.min(pad.x - half_w);
            max_x = max_x.max(pad.x + half_w);
            min_y = min_y.min(pad.y - half_h);
            max_y = max_y.max(pad.y + half_h);
        }

        for track in &footprint.tracks {
            min_x = min_x.min(track.x1.min(track.x2) - track.width / 2.0);
            max_x = max_x.max(track.x1.max(track.x2) + track.width / 2.0);
            min_y = min_y.min(track.y1.min(track.y2) - track.width / 2.0);
            max_y = max_y.max(track.y1.max(track.y2) + track.width / 2.0);
        }

        for arc in &footprint.arcs {
            min_x = min_x.min(arc.x - arc.radius);
            max_x = max_x.max(arc.x + arc.radius);
            min_y = min_y.min(arc.y - arc.radius);
            max_y = max_y.max(arc.y + arc.radius);
        }

        // Handle empty footprint
        if min_x == f64::MAX {
            return "Empty footprint (no primitives)".to_string();
        }

        // Add margin
        let margin = 0.5;
        min_x -= margin;
        max_x += margin;
        min_y -= margin;
        max_y += margin;

        // Calculate canvas size
        let width_mm = max_x - min_x;
        let height_mm = max_y - min_y;
        let mut canvas_width = (width_mm * scale).ceil() as usize;
        let mut canvas_height = (height_mm * scale).ceil() as usize;

        // Clamp to max dimensions
        if canvas_width > max_width {
            canvas_width = max_width;
        }
        if canvas_height > max_height {
            canvas_height = max_height;
        }

        // Ensure minimum size
        canvas_width = canvas_width.max(10);
        canvas_height = canvas_height.max(5);

        // Calculate actual scale after clamping
        let actual_scale_x = canvas_width as f64 / width_mm;
        let actual_scale_y = canvas_height as f64 / height_mm;

        // Create canvas (y is inverted for display)
        let mut canvas = vec![vec![' '; canvas_width]; canvas_height];

        // Helper to convert coordinates
        let to_canvas = |x: f64, y: f64| -> (usize, usize) {
            let cx = ((x - min_x) * actual_scale_x).round() as usize;
            let cy =
                canvas_height.saturating_sub(1) - ((y - min_y) * actual_scale_y).round() as usize;
            (cx.min(canvas_width - 1), cy.min(canvas_height - 1))
        };

        // Draw tracks (as lines)
        for track in &footprint.tracks {
            Self::draw_line(
                &mut canvas,
                to_canvas(track.x1, track.y1),
                to_canvas(track.x2, track.y2),
                '-',
            );
        }

        // Draw arcs (simplified as circles at centre)
        for arc in &footprint.arcs {
            let (cx, cy) = to_canvas(arc.x, arc.y);
            if cx < canvas_width && cy < canvas_height {
                canvas[cy][cx] = 'o';
            }
        }

        // Draw pads (as rectangles with full designator)
        for pad in &footprint.pads {
            let half_w = pad.width / 2.0;
            let half_h = pad.height / 2.0;
            let (x1, y1) = to_canvas(pad.x - half_w, pad.y - half_h);
            let (x2, y2) = to_canvas(pad.x + half_w, pad.y + half_h);

            // Fill pad area
            let (min_cy, max_cy) = (y1.min(y2), y1.max(y2));
            let (min_cx, max_cx) = (x1.min(x2), x1.max(x2));

            for cy in min_cy..=max_cy {
                for cx in min_cx..=max_cx {
                    if cy < canvas_height && cx < canvas_width {
                        canvas[cy][cx] = '#';
                    }
                }
            }

            // Place full designator centred on pad
            let (cx, cy) = to_canvas(pad.x, pad.y);
            if cy < canvas_height {
                let desig = &pad.designator;
                let start_x = cx.saturating_sub(desig.len() / 2);
                for (i, ch) in desig.chars().enumerate() {
                    let x = start_x + i;
                    if x < canvas_width {
                        canvas[cy][x] = ch;
                    }
                }
            }
        }

        // Draw origin crosshair
        let (ox, oy) = to_canvas(0.0, 0.0);
        if ox < canvas_width && oy < canvas_height {
            canvas[oy][ox] = '+';
        }

        // Build output string
        let mut output = String::new();
        let _ = writeln!(
            output,
            "Footprint: {} ({:.2} x {:.2} mm)",
            footprint.name,
            width_mm - margin * 2.0,
            height_mm - margin * 2.0
        );
        let _ = writeln!(
            output,
            "Pads: {}, Tracks: {}, Arcs: {}",
            footprint.pads.len(),
            footprint.tracks.len(),
            footprint.arcs.len()
        );
        output.push_str(&"-".repeat(canvas_width + 2));
        output.push('\n');

        for row in &canvas {
            output.push('|');
            for &ch in row {
                output.push(ch);
            }
            output.push('|');
            output.push('\n');
        }

        output.push_str(&"-".repeat(canvas_width + 2));
        output.push('\n');
        output.push_str("Legend: # = pad, - = track, o = arc, + = origin\n");

        output
    }

    /// Draws a line on the canvas using Bresenham's algorithm.
    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    pub(crate) fn draw_line(
        canvas: &mut [Vec<char>],
        (x0, y0): (usize, usize),
        (x1, y1): (usize, usize),
        ch: char,
    ) {
        let dx = (x1 as isize - x0 as isize).abs();
        let dy = (y1 as isize - y0 as isize).abs();
        let sx: isize = if x0 < x1 { 1 } else { -1 };
        let sy: isize = if y0 < y1 { 1 } else { -1 };
        let mut err = dx - dy;

        let mut x = x0 as isize;
        let mut y = y0 as isize;

        let height = canvas.len();
        let width = if height > 0 { canvas[0].len() } else { 0 };

        loop {
            if (x as usize) < width && (y as usize) < height {
                canvas[y as usize][x as usize] = ch;
            }

            if x == x1 as isize && y == y1 as isize {
                break;
            }

            let e2 = 2 * err;
            if e2 > -dy {
                err -= dy;
                x += sx;
            }
            if e2 < dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Renders an ASCII art visualisation of a schematic symbol.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub(crate) fn call_render_symbol(&self, arguments: &Value) -> ToolCallResult {
        use crate::altium::SchLib;

        let Some(filepath) = arguments.get("filepath").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: filepath");
        };

        let Some(component_name) = arguments.get("component_name").and_then(Value::as_str) else {
            return ToolCallResult::error("Missing required parameter: component_name");
        };

        // Validate path is within allowed directories
        if let Err(e) = self.validate_path(filepath) {
            return ToolCallResult::error(e);
        }

        // Parse optional parameters
        let scale = arguments
            .get("scale")
            .and_then(Value::as_f64)
            .unwrap_or(1.0);
        let max_width = arguments
            .get("max_width")
            .and_then(Value::as_u64)
            .unwrap_or(80) as usize;
        let max_height = arguments
            .get("max_height")
            .and_then(Value::as_u64)
            .unwrap_or(40) as usize;
        let part_id = arguments
            .get("part_id")
            .and_then(Value::as_i64)
            .unwrap_or(1) as i32;

        if scale <= 0.0 {
            return ToolCallResult::error("scale must be greater than 0");
        }

        // Read the library
        let library = match SchLib::open(filepath) {
            Ok(lib) => lib,
            Err(e) => return ToolCallResult::error(format!("Failed to read library: {e}")),
        };

        // Find the symbol
        let Some(symbol) = library.get(component_name) else {
            let available: Vec<_> = library.iter().take(5).map(|s| s.name.as_str()).collect();
            let hint = if available.is_empty() {
                "Library is empty".to_string()
            } else {
                format!(
                    "Available symbols include: {}{}",
                    available.join(", "),
                    if library.len() > 5 {
                        format!(" (and {} more)", library.len() - 5)
                    } else {
                        String::new()
                    }
                )
            };
            return ToolCallResult::error(format!(
                "Symbol '{component_name}' not found in library. {hint}"
            ));
        };

        // Render the symbol
        let ascii_art = Self::render_symbol_ascii(symbol, scale, max_width, max_height, part_id);

        let result = json!({
            "status": "success",
            "filepath": filepath,
            "component_name": component_name,
            "scale": scale,
            "part_id": part_id,
            "render": ascii_art,
        });

        ToolCallResult::text(serde_json::to_string_pretty(&result).unwrap())
    }

    /// Renders a schematic symbol as ASCII art.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        clippy::similar_names,
        clippy::too_many_lines,
        clippy::float_cmp,
        clippy::needless_range_loop
    )]
    pub(crate) fn render_symbol_ascii(
        symbol: &crate::altium::schlib::Symbol,
        scale: f64,
        max_width: usize,
        max_height: usize,
        part_id: i32,
    ) -> String {
        use crate::altium::schlib::PinOrientation;
        use std::fmt::Write;

        // Find bounding box (in schematic units)
        let (mut min_x, mut max_x, mut min_y, mut max_y) = (i32::MAX, i32::MIN, i32::MAX, i32::MIN);

        // Helper to check if primitive belongs to requested part
        let matches_part = |owner_part_id: i32| -> bool {
            part_id == 0 || owner_part_id == part_id || owner_part_id == 0
        };

        // Calculate bounding box from pins (include pin length)
        for pin in &symbol.pins {
            if !matches_part(pin.owner_part_id) {
                continue;
            }
            let (px, py) = (pin.x, pin.y);
            let (end_x, end_y) = match pin.orientation {
                PinOrientation::Right => (px + pin.length, py),
                PinOrientation::Left => (px - pin.length, py),
                PinOrientation::Up => (px, py + pin.length),
                PinOrientation::Down => (px, py - pin.length),
            };
            min_x = min_x.min(px).min(end_x);
            max_x = max_x.max(px).max(end_x);
            min_y = min_y.min(py).min(end_y);
            max_y = max_y.max(py).max(end_y);
        }

        // Calculate bounding box from rectangles
        for rect in &symbol.rectangles {
            if !matches_part(rect.owner_part_id) {
                continue;
            }
            min_x = min_x.min(rect.x1).min(rect.x2);
            max_x = max_x.max(rect.x1).max(rect.x2);
            min_y = min_y.min(rect.y1).min(rect.y2);
            max_y = max_y.max(rect.y1).max(rect.y2);
        }

        // Calculate bounding box from lines
        for line in &symbol.lines {
            if !matches_part(line.owner_part_id) {
                continue;
            }
            min_x = min_x.min(line.x1).min(line.x2);
            max_x = max_x.max(line.x1).max(line.x2);
            min_y = min_y.min(line.y1).min(line.y2);
            max_y = max_y.max(line.y1).max(line.y2);
        }

        // Calculate bounding box from polylines
        for polyline in &symbol.polylines {
            if !matches_part(polyline.owner_part_id) {
                continue;
            }
            for &(x, y) in &polyline.points {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }

        // Calculate bounding box from arcs
        for arc in &symbol.arcs {
            if !matches_part(arc.owner_part_id) {
                continue;
            }
            min_x = min_x.min(arc.x - arc.radius);
            max_x = max_x.max(arc.x + arc.radius);
            min_y = min_y.min(arc.y - arc.radius);
            max_y = max_y.max(arc.y + arc.radius);
        }

        // Calculate bounding box from ellipses
        for ellipse in &symbol.ellipses {
            if !matches_part(ellipse.owner_part_id) {
                continue;
            }
            min_x = min_x.min(ellipse.x - ellipse.radius_x);
            max_x = max_x.max(ellipse.x + ellipse.radius_x);
            min_y = min_y.min(ellipse.y - ellipse.radius_y);
            max_y = max_y.max(ellipse.y + ellipse.radius_y);
        }

        // Handle empty symbol
        if min_x == i32::MAX {
            return "Empty symbol (no primitives)".to_string();
        }

        // Add margin (10 schematic units = 1 grid)
        let margin = 10;
        min_x -= margin;
        max_x += margin;
        min_y -= margin;
        max_y += margin;

        // Calculate canvas size (scale is chars per 10 schematic units)
        let width_units = f64::from(max_x - min_x);
        let height_units = f64::from(max_y - min_y);
        let mut canvas_width = ((width_units / 10.0) * scale).ceil() as usize;
        let mut canvas_height = ((height_units / 10.0) * scale).ceil() as usize;

        // Clamp to max dimensions
        canvas_width = canvas_width.clamp(10, max_width);
        canvas_height = canvas_height.clamp(5, max_height);

        // Calculate actual scale after clamping
        let actual_scale_x = canvas_width as f64 / width_units;
        let actual_scale_y = canvas_height as f64 / height_units;

        // Create canvas (y is inverted for display)
        let mut canvas = vec![vec![' '; canvas_width]; canvas_height];

        // Helper to convert schematic coordinates to canvas coordinates
        let to_canvas = |x: i32, y: i32| -> (usize, usize) {
            let cx = (f64::from(x - min_x) * actual_scale_x).round() as usize;
            let cy = canvas_height.saturating_sub(1)
                - (f64::from(y - min_y) * actual_scale_y).round() as usize;
            (cx.min(canvas_width - 1), cy.min(canvas_height - 1))
        };

        // Draw rectangles (as box outlines or filled)
        for rect in &symbol.rectangles {
            if !matches_part(rect.owner_part_id) {
                continue;
            }
            let (x1, y1) = to_canvas(rect.x1, rect.y1);
            let (x2, y2) = to_canvas(rect.x2, rect.y2);
            let (min_cx, max_cx) = (x1.min(x2), x1.max(x2));
            let (min_cy, max_cy) = (y1.min(y2), y1.max(y2));

            // Draw top and bottom edges
            for cx in min_cx..=max_cx {
                if cx < canvas_width {
                    if min_cy < canvas_height {
                        canvas[min_cy][cx] = '-';
                    }
                    if max_cy < canvas_height {
                        canvas[max_cy][cx] = '-';
                    }
                }
            }
            // Draw left and right edges
            for cy in min_cy..=max_cy {
                if cy < canvas_height {
                    if min_cx < canvas_width {
                        canvas[cy][min_cx] = '|';
                    }
                    if max_cx < canvas_width {
                        canvas[cy][max_cx] = '|';
                    }
                }
            }
            // Draw corners
            if min_cy < canvas_height && min_cx < canvas_width {
                canvas[min_cy][min_cx] = '+';
            }
            if min_cy < canvas_height && max_cx < canvas_width {
                canvas[min_cy][max_cx] = '+';
            }
            if max_cy < canvas_height && min_cx < canvas_width {
                canvas[max_cy][min_cx] = '+';
            }
            if max_cy < canvas_height && max_cx < canvas_width {
                canvas[max_cy][max_cx] = '+';
            }
        }

        // Draw lines
        for line in &symbol.lines {
            if !matches_part(line.owner_part_id) {
                continue;
            }
            Self::draw_line(
                &mut canvas,
                to_canvas(line.x1, line.y1),
                to_canvas(line.x2, line.y2),
                '-',
            );
        }

        // Draw polylines
        for polyline in &symbol.polylines {
            if !matches_part(polyline.owner_part_id) {
                continue;
            }
            for i in 0..polyline.points.len().saturating_sub(1) {
                let (x1, y1) = polyline.points[i];
                let (x2, y2) = polyline.points[i + 1];
                Self::draw_line(&mut canvas, to_canvas(x1, y1), to_canvas(x2, y2), '-');
            }
        }

        // Draw arcs (simplified as circles at centre)
        for arc in &symbol.arcs {
            if !matches_part(arc.owner_part_id) {
                continue;
            }
            let (cx, cy) = to_canvas(arc.x, arc.y);
            if cx < canvas_width && cy < canvas_height {
                canvas[cy][cx] = 'o';
            }
        }

        // Draw ellipses (simplified as circles at centre)
        for ellipse in &symbol.ellipses {
            if !matches_part(ellipse.owner_part_id) {
                continue;
            }
            let (cx, cy) = to_canvas(ellipse.x, ellipse.y);
            if cx < canvas_width && cy < canvas_height {
                canvas[cy][cx] = 'O';
            }
        }

        // Draw pins with full designators
        for pin in &symbol.pins {
            if !matches_part(pin.owner_part_id) {
                continue;
            }
            let (px, py) = (pin.x, pin.y);
            let (end_x, end_y) = match pin.orientation {
                PinOrientation::Right => (px + pin.length, py),
                PinOrientation::Left => (px - pin.length, py),
                PinOrientation::Up => (px, py + pin.length),
                PinOrientation::Down => (px, py - pin.length),
            };

            // Draw pin line
            Self::draw_line(&mut canvas, to_canvas(px, py), to_canvas(end_x, end_y), '~');

            // Draw full designator at connection point
            let (cx, cy) = to_canvas(px, py);
            if cy < canvas_height {
                let desig = &pin.designator;

                // Place designator based on pin orientation
                match pin.orientation {
                    PinOrientation::Right => {
                        // Pin points right, place designator to the left (inside symbol)
                        for (i, ch) in desig.chars().enumerate() {
                            let x = cx.saturating_sub(desig.len() - 1 - i);
                            if x < canvas_width {
                                canvas[cy][x] = ch;
                            }
                        }
                    }
                    PinOrientation::Left => {
                        // Pin points left, place designator to the right (inside symbol)
                        for (i, ch) in desig.chars().enumerate() {
                            let x = cx + i;
                            if x < canvas_width {
                                canvas[cy][x] = ch;
                            }
                        }
                    }
                    PinOrientation::Up | PinOrientation::Down => {
                        // Vertical pins: place designator horizontally centred
                        let start_x = cx.saturating_sub(desig.len() / 2);
                        for (i, ch) in desig.chars().enumerate() {
                            let x = start_x + i;
                            if x < canvas_width {
                                canvas[cy][x] = ch;
                            }
                        }
                    }
                }
            }
        }

        // Draw origin crosshair
        let (ox, oy) = to_canvas(0, 0);
        if ox < canvas_width && oy < canvas_height && canvas[oy][ox] == ' ' {
            canvas[oy][ox] = '+';
        }

        // Count primitives for summary
        let pin_count = symbol
            .pins
            .iter()
            .filter(|p| matches_part(p.owner_part_id))
            .count();
        let rect_count = symbol
            .rectangles
            .iter()
            .filter(|r| matches_part(r.owner_part_id))
            .count();
        let line_count = symbol
            .lines
            .iter()
            .filter(|l| matches_part(l.owner_part_id))
            .count();

        // Build output string
        let mut output = String::new();
        let _ = writeln!(
            output,
            "Symbol: {} (part {}/{})",
            symbol.name,
            if part_id == 0 { 1 } else { part_id },
            symbol.part_count
        );
        let _ = writeln!(
            output,
            "Pins: {pin_count}, Rectangles: {rect_count}, Lines: {line_count}"
        );
        output.push_str(&"-".repeat(canvas_width + 2));
        output.push('\n');

        for row in &canvas {
            output.push('|');
            for &ch in row {
                output.push(ch);
            }
            output.push('|');
            output.push('\n');
        }

        output.push_str(&"-".repeat(canvas_width + 2));
        output.push('\n');
        output
            .push_str("Legend: |-+ = rectangle, ~ = pin line, o = arc, O = ellipse, + = origin\n");

        output
    }
}
