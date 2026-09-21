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
            return ToolCallResult::error(super::component_not_found(
                component_name,
                &library.names(),
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

    /// Renders a footprint as ASCII art: every primitive kind, each with its
    /// own marker, the origin as `+` where nothing else is drawn.
    pub(crate) fn render_footprint_ascii(
        footprint: &crate::altium::pcblib::Footprint,
        scale: f64,
        max_width: usize,
        max_height: usize,
    ) -> String {
        use std::fmt::Write;

        let bounds = Self::footprint_bounds(footprint);
        if bounds.is_empty() {
            return "Empty footprint (no primitives)".to_string();
        }
        // scale is cells per mm; 0.5 mm of margin all round.
        let mut canvas = Canvas::new(&bounds, 0.5, scale, max_width, max_height);
        Self::draw_footprint(&mut canvas, footprint);

        let mut output = String::new();
        let _ = writeln!(
            output,
            "Footprint: {} ({:.2} x {:.2} mm)",
            footprint.name,
            bounds.width(),
            bounds.height()
        );
        let _ = writeln!(
            output,
            "Pads: {}, Tracks: {}, Arcs: {}, Vias: {}, Fills: {}, Regions: {}, Text: {}, Bodies: {}",
            footprint.pads.len(),
            footprint.tracks.len(),
            footprint.arcs.len(),
            footprint.vias.len(),
            footprint.fills.len(),
            footprint.regions.len(),
            footprint.text.len(),
            footprint.component_bodies.len()
        );
        output.push_str(&canvas.framed());
        output.push_str(
            "Legend: # = pad, - = track, o = arc, O = via, = = fill, % = region, T = text, \
             . = 3D body, + = origin\n",
        );
        output
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
            return ToolCallResult::error(super::component_not_found(
                component_name,
                &library.names(),
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

    /// The extent of every primitive kind a footprint holds, so nothing it
    /// holds renders as "empty" and nothing falls outside the canvas.
    fn footprint_bounds(footprint: &crate::altium::pcblib::Footprint) -> Bounds {
        let mut bounds = Bounds::default();
        for pad in &footprint.pads {
            bounds.add_rect(
                pad.x - pad.width / 2.0,
                pad.y - pad.height / 2.0,
                pad.x + pad.width / 2.0,
                pad.y + pad.height / 2.0,
            );
        }
        for via in &footprint.vias {
            bounds.add_circle(via.x, via.y, via.diameter / 2.0);
        }
        for track in &footprint.tracks {
            let half = track.width / 2.0;
            bounds.add_rect(
                track.x1.min(track.x2) - half,
                track.y1.min(track.y2) - half,
                track.x1.max(track.x2) + half,
                track.y1.max(track.y2) + half,
            );
        }
        for arc in &footprint.arcs {
            bounds.add_circle(arc.x, arc.y, arc.radius);
        }
        for text in &footprint.text {
            bounds.add_rect(text.x, text.y, text.x + text.height, text.y + text.height);
        }
        for fill in &footprint.fills {
            bounds.add_rect(fill.x1, fill.y1, fill.x2, fill.y2);
        }
        for region in &footprint.regions {
            for v in &region.vertices {
                bounds.add(v.x, v.y);
            }
        }
        for body in &footprint.component_bodies {
            bounds.add_points(&body.outline);
        }
        bounds
    }

    /// Draws every primitive kind back to front: bodies and regions as
    /// outlines, fills, tracks, arcs, vias, text marks, then pads with their
    /// designators on top, and the origin where nothing else is.
    fn draw_footprint(canvas: &mut Canvas, footprint: &crate::altium::pcblib::Footprint) {
        for body in &footprint.component_bodies {
            canvas.polyline(&body.outline, true, '.');
        }
        for region in &footprint.regions {
            let points: Vec<(f64, f64)> = region.vertices.iter().map(|v| (v.x, v.y)).collect();
            canvas.polyline(&points, true, '%');
        }
        for fill in &footprint.fills {
            canvas.rect_fill(fill.x1, fill.y1, fill.x2, fill.y2, '=');
        }
        for track in &footprint.tracks {
            canvas.line(track.x1, track.y1, track.x2, track.y2, '-');
        }
        for arc in &footprint.arcs {
            canvas.arc(
                (arc.x, arc.y),
                (arc.radius, arc.radius),
                (arc.start_angle, arc.end_angle),
                'o',
            );
        }
        for via in &footprint.vias {
            canvas.plot(via.x, via.y, 'O');
        }
        for text in &footprint.text {
            canvas.plot(text.x, text.y, 'T');
        }
        for pad in &footprint.pads {
            canvas.rect_fill(
                pad.x - pad.width / 2.0,
                pad.y - pad.height / 2.0,
                pad.x + pad.width / 2.0,
                pad.y + pad.height / 2.0,
                '#',
            );
            canvas.label_centred(pad.x, pad.y, &pad.designator);
        }
        canvas.origin();
    }

    /// Renders a schematic symbol as ASCII art: every record kind of the
    /// requested part (every part when `part_id` is 0), each with its own
    /// marker.
    #[allow(clippy::too_many_lines, clippy::cast_precision_loss)]
    pub(crate) fn render_symbol_ascii(
        symbol: &crate::altium::schlib::Symbol,
        scale: f64,
        max_width: usize,
        max_height: usize,
        part_id: i32,
    ) -> String {
        use crate::altium::schlib::PinOrientation;
        use std::fmt::Write;

        // Whether a primitive of `owner_part_id` belongs to the requested part.
        let shown = |owner_part_id: i32| -> bool {
            part_id == 0 || owner_part_id == part_id || owner_part_id == 0
        };
        let pin_end = |pin: &crate::altium::schlib::Pin| -> (f64, f64) {
            let (px, py) = (f64::from(pin.x), f64::from(pin.y));
            let len = f64::from(pin.length);
            match pin.orientation {
                PinOrientation::Right => (px + len, py),
                PinOrientation::Left => (px - len, py),
                PinOrientation::Up => (px, py + len),
                PinOrientation::Down => (px, py - len),
            }
        };
        let bezier_points = |bezier: &crate::altium::schlib::Bezier| -> Vec<(f64, f64)> {
            (0..=16)
                .map(|step| {
                    let t = f64::from(step) / 16.0;
                    let u = 1.0 - t;
                    // The cubic Bernstein weights of the four control points.
                    let weights = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
                    let xs = [bezier.x1, bezier.x2, bezier.x3, bezier.x4];
                    let ys = [bezier.y1, bezier.y2, bezier.y3, bezier.y4];
                    let blend = |values: [f64; 4]| {
                        weights
                            .iter()
                            .zip(values)
                            .fold(0.0_f64, |acc, (w, v)| w.mul_add(v, acc))
                    };
                    (blend(xs), blend(ys))
                })
                .collect()
        };

        // Bounds over every kind of the part, so nothing renders as "empty"
        // and nothing falls outside the canvas.
        let mut bounds = Bounds::default();
        for pin in symbol.pins.iter().filter(|p| shown(p.owner_part_id)) {
            bounds.add(f64::from(pin.x), f64::from(pin.y));
            let (ex, ey) = pin_end(pin);
            bounds.add(ex, ey);
        }
        for r in symbol.rectangles.iter().filter(|r| shown(r.owner_part_id)) {
            bounds.add_rect(r.x1, r.y1, r.x2, r.y2);
        }
        for r in symbol.round_rects.iter().filter(|r| shown(r.owner_part_id)) {
            bounds.add_rect(r.x1, r.y1, r.x2, r.y2);
        }
        for l in symbol.lines.iter().filter(|l| shown(l.owner_part_id)) {
            bounds.add_rect(l.x1, l.y1, l.x2, l.y2);
        }
        for pl in symbol.polylines.iter().filter(|p| shown(p.owner_part_id)) {
            bounds.add_points(&pl.points);
        }
        for pg in symbol.polygons.iter().filter(|p| shown(p.owner_part_id)) {
            bounds.add_points(&pg.points);
        }
        for a in symbol.arcs.iter().filter(|a| shown(a.owner_part_id)) {
            bounds.add_circle(a.x, a.y, a.radius);
        }
        for pie in symbol.pies.iter().filter(|p| shown(p.owner_part_id)) {
            bounds.add_circle(pie.x, pie.y, pie.radius);
        }
        for e in symbol.ellipses.iter().filter(|e| shown(e.owner_part_id)) {
            bounds.add_rect(
                e.x - e.radius_x,
                e.y - e.radius_y,
                e.x + e.radius_x,
                e.y + e.radius_y,
            );
        }
        for ea in symbol
            .elliptical_arcs
            .iter()
            .filter(|a| shown(a.owner_part_id))
        {
            bounds.add_rect(
                ea.x - ea.radius,
                ea.y - ea.secondary_radius,
                ea.x + ea.radius,
                ea.y + ea.secondary_radius,
            );
        }
        for b in symbol.beziers.iter().filter(|b| shown(b.owner_part_id)) {
            bounds.add_points(&bezier_points(b));
        }
        for img in symbol.images.iter().filter(|i| shown(i.owner_part_id)) {
            bounds.add_rect(img.x1, img.y1, img.x2, img.y2);
        }
        for f in symbol.text_frames.iter().filter(|f| shown(f.owner_part_id)) {
            bounds.add_rect(f.x1, f.y1, f.x2, f.y2);
        }
        for l in symbol.labels.iter().filter(|l| shown(l.owner_part_id)) {
            bounds.add_rect(
                l.x,
                l.y,
                10.0_f64.mul_add(l.text.chars().count() as f64, l.x),
                l.y + 10.0,
            );
        }
        for g in symbol
            .ieee_symbols
            .iter()
            .filter(|g| shown(g.owner_part_id))
        {
            bounds.add_circle(g.x, g.y, g.scale_factor);
        }
        if bounds.is_empty() {
            return "Empty symbol (no primitives)".to_string();
        }

        // scale is cells per grid square (10 schematic units); one grid of margin.
        let mut canvas = Canvas::new(&bounds, 10.0, scale / 10.0, max_width, max_height);

        // Back to front: images and frames, bodies, curves, text, pins on top.
        for img in symbol.images.iter().filter(|i| shown(i.owner_part_id)) {
            canvas.rect_outline((img.x1, img.y1), (img.x2, img.y2), (':', ':', ':'));
        }
        for f in symbol.text_frames.iter().filter(|f| shown(f.owner_part_id)) {
            canvas.rect_outline((f.x1, f.y1), (f.x2, f.y2), ('=', '"', '+'));
            canvas.label_at(f.x1.min(f.x2) + 10.0, f.y1.max(f.y2) - 10.0, &f.text);
        }
        for r in symbol.rectangles.iter().filter(|r| shown(r.owner_part_id)) {
            canvas.rect_outline((r.x1, r.y1), (r.x2, r.y2), ('-', '|', '+'));
        }
        for r in symbol.round_rects.iter().filter(|r| shown(r.owner_part_id)) {
            canvas.rect_outline((r.x1, r.y1), (r.x2, r.y2), ('-', '|', '('));
        }
        for pg in symbol.polygons.iter().filter(|p| shown(p.owner_part_id)) {
            canvas.polyline(&pg.points, true, '-');
        }
        for l in symbol.lines.iter().filter(|l| shown(l.owner_part_id)) {
            canvas.line(l.x1, l.y1, l.x2, l.y2, '-');
        }
        for pl in symbol.polylines.iter().filter(|p| shown(p.owner_part_id)) {
            canvas.polyline(&pl.points, false, '-');
        }
        for a in symbol.arcs.iter().filter(|a| shown(a.owner_part_id)) {
            canvas.arc(
                (a.x, a.y),
                (a.radius, a.radius),
                (a.start_angle, a.end_angle),
                'o',
            );
        }
        for ea in symbol
            .elliptical_arcs
            .iter()
            .filter(|a| shown(a.owner_part_id))
        {
            canvas.arc(
                (ea.x, ea.y),
                (ea.radius, ea.secondary_radius),
                (ea.start_angle, ea.end_angle),
                'o',
            );
        }
        for pie in symbol.pies.iter().filter(|p| shown(p.owner_part_id)) {
            canvas.arc(
                (pie.x, pie.y),
                (pie.radius, pie.radius),
                (pie.start_angle, pie.end_angle),
                'o',
            );
            for angle in [pie.start_angle, pie.end_angle] {
                let (sin, cos) = angle.to_radians().sin_cos();
                canvas.line(
                    pie.x,
                    pie.y,
                    pie.radius.mul_add(cos, pie.x),
                    pie.radius.mul_add(sin, pie.y),
                    '-',
                );
            }
        }
        for e in symbol.ellipses.iter().filter(|e| shown(e.owner_part_id)) {
            canvas.arc((e.x, e.y), (e.radius_x, e.radius_y), (0.0, 360.0), 'O');
        }
        for b in symbol.beziers.iter().filter(|b| shown(b.owner_part_id)) {
            canvas.polyline(&bezier_points(b), false, '~');
        }
        for g in symbol
            .ieee_symbols
            .iter()
            .filter(|g| shown(g.owner_part_id))
        {
            canvas.plot(g.x, g.y, '@');
        }
        for l in symbol.labels.iter().filter(|l| shown(l.owner_part_id)) {
            canvas.label_at(l.x, l.y, &l.text);
        }
        for pin in symbol.pins.iter().filter(|p| shown(p.owner_part_id)) {
            let (px, py) = (f64::from(pin.x), f64::from(pin.y));
            let (ex, ey) = pin_end(pin);
            canvas.line(px, py, ex, ey, '~');
            // The designator sits at the connection point, inside the body.
            match pin.orientation {
                PinOrientation::Right => canvas.label_ending_at(px, py, &pin.designator),
                PinOrientation::Left => canvas.label_at(px, py, &pin.designator),
                PinOrientation::Up | PinOrientation::Down => {
                    canvas.label_centred(px, py, &pin.designator);
                }
            }
        }
        canvas.origin();

        let count = |n: usize| n;
        let pin_count = symbol
            .pins
            .iter()
            .filter(|p| shown(p.owner_part_id))
            .count();
        let rect_count = symbol
            .rectangles
            .iter()
            .filter(|r| shown(r.owner_part_id))
            .count();
        let line_count = symbol
            .lines
            .iter()
            .filter(|l| shown(l.owner_part_id))
            .count();
        let other: Vec<String> = [
            (
                "Polylines",
                symbol
                    .polylines
                    .iter()
                    .filter(|p| shown(p.owner_part_id))
                    .count(),
            ),
            (
                "Polygons",
                symbol
                    .polygons
                    .iter()
                    .filter(|p| shown(p.owner_part_id))
                    .count(),
            ),
            (
                "Arcs",
                symbol
                    .arcs
                    .iter()
                    .filter(|a| shown(a.owner_part_id))
                    .count(),
            ),
            (
                "Pies",
                symbol
                    .pies
                    .iter()
                    .filter(|p| shown(p.owner_part_id))
                    .count(),
            ),
            (
                "Ellipses",
                symbol
                    .ellipses
                    .iter()
                    .filter(|e| shown(e.owner_part_id))
                    .count(),
            ),
            (
                "Round rects",
                symbol
                    .round_rects
                    .iter()
                    .filter(|r| shown(r.owner_part_id))
                    .count(),
            ),
            (
                "Elliptical arcs",
                symbol
                    .elliptical_arcs
                    .iter()
                    .filter(|a| shown(a.owner_part_id))
                    .count(),
            ),
            (
                "Beziers",
                symbol
                    .beziers
                    .iter()
                    .filter(|b| shown(b.owner_part_id))
                    .count(),
            ),
            (
                "Images",
                symbol
                    .images
                    .iter()
                    .filter(|i| shown(i.owner_part_id))
                    .count(),
            ),
            (
                "Text frames",
                symbol
                    .text_frames
                    .iter()
                    .filter(|f| shown(f.owner_part_id))
                    .count(),
            ),
            (
                "Labels",
                symbol
                    .labels
                    .iter()
                    .filter(|l| shown(l.owner_part_id))
                    .count(),
            ),
            (
                "IEEE symbols",
                symbol
                    .ieee_symbols
                    .iter()
                    .filter(|g| shown(g.owner_part_id))
                    .count(),
            ),
        ]
        .iter()
        .filter(|(_, n)| count(*n) > 0)
        .map(|(name, n)| format!(", {name}: {n}"))
        .collect();

        let mut output = String::new();
        // Clamp the displayed part index to 1..=part_count so an out-of-range
        // part_id can't render a nonsensical header like "part 5/2".
        let part_count_i32 = i32::try_from(symbol.part_count).unwrap_or(i32::MAX).max(1);
        let shown_part = part_id.clamp(1, part_count_i32);
        let _ = writeln!(
            output,
            "Symbol: {} (part {}/{})",
            symbol.name, shown_part, symbol.part_count
        );
        let _ = writeln!(
            output,
            "Pins: {pin_count}, Rectangles: {rect_count}, Lines: {line_count}{}",
            other.concat()
        );
        output.push_str(&canvas.framed());
        output.push_str(
            "Legend: |-+ = rectangle, ( = rounded rectangle, ~ = pin line or bezier, o = arc or \
             pie, O = ellipse, : = image, =\" = text frame, @ = IEEE symbol, + = origin\n",
        );
        output
    }
}

/// The extent of everything drawn, in world units.
#[derive(Debug, Default, Clone, Copy)]
struct Bounds {
    min_x: f64,
    max_x: f64,
    min_y: f64,
    max_y: f64,
    any: bool,
}

impl Bounds {
    const fn add(&mut self, x: f64, y: f64) {
        if self.any {
            self.min_x = self.min_x.min(x);
            self.max_x = self.max_x.max(x);
            self.min_y = self.min_y.min(y);
            self.max_y = self.max_y.max(y);
        } else {
            (self.min_x, self.max_x, self.min_y, self.max_y) = (x, x, y, y);
            self.any = true;
        }
    }

    const fn add_rect(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        self.add(x1, y1);
        self.add(x2, y2);
    }

    fn add_circle(&mut self, x: f64, y: f64, r: f64) {
        self.add_rect(x - r, y - r, x + r, y + r);
    }

    fn add_points(&mut self, points: &[(f64, f64)]) {
        for &(x, y) in points {
            self.add(x, y);
        }
    }

    const fn is_empty(self) -> bool {
        !self.any
    }

    fn width(self) -> f64 {
        self.max_x - self.min_x
    }

    fn height(self) -> f64 {
        self.max_y - self.min_y
    }
}

/// A character canvas over a world-coordinate window: `y` grows upwards in
/// the world and downwards on the canvas.
struct Canvas {
    cells: Vec<Vec<char>>,
    width: usize,
    height: usize,
    min_x: f64,
    min_y: f64,
    scale_x: f64,
    scale_y: f64,
}

impl Canvas {
    /// A canvas over `bounds` plus `margin` all round, at `scale` cells per
    /// world unit, clamped to `max_width` x `max_height` (at least 10 x 5).
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    fn new(bounds: &Bounds, margin: f64, scale: f64, max_width: usize, max_height: usize) -> Self {
        let (min_x, min_y) = (bounds.min_x - margin, bounds.min_y - margin);
        let (width_units, height_units) = (
            2.0_f64.mul_add(margin, bounds.width()),
            2.0_f64.mul_add(margin, bounds.height()),
        );
        let width = ((width_units * scale).ceil() as usize).clamp(10, max_width.max(10));
        let height = ((height_units * scale).ceil() as usize).clamp(5, max_height.max(5));
        Self {
            cells: vec![vec![' '; width]; height],
            width,
            height,
            min_x,
            min_y,
            scale_x: width as f64 / width_units,
            scale_y: height as f64 / height_units,
        }
    }

    /// The cell for a world point, clamped to the canvas.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn cell(&self, x: f64, y: f64) -> (usize, usize) {
        let cx = ((x - self.min_x) * self.scale_x).round().max(0.0) as usize;
        let dy = (((y - self.min_y) * self.scale_y).round().max(0.0) as usize).min(self.height - 1);
        (cx.min(self.width - 1), self.height - 1 - dy)
    }

    fn plot(&mut self, x: f64, y: f64, ch: char) {
        let (cx, cy) = self.cell(x, y);
        self.cells[cy][cx] = ch;
    }

    /// Bresenham between two world points.
    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, ch: char) {
        let (x0, y0) = self.cell(x1, y1);
        let (xn, yn) = self.cell(x2, y2);
        let (dx, dy) = (
            (xn as isize - x0 as isize).abs(),
            (yn as isize - y0 as isize).abs(),
        );
        let sx: isize = if x0 < xn { 1 } else { -1 };
        let sy: isize = if y0 < yn { 1 } else { -1 };
        let (mut err, mut x, mut y) = (dx - dy, x0 as isize, y0 as isize);
        loop {
            self.cells[y as usize][x as usize] = ch;
            if x == xn as isize && y == yn as isize {
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

    fn polyline(&mut self, points: &[(f64, f64)], closed: bool, ch: char) {
        for pair in points.windows(2) {
            self.line(pair[0].0, pair[0].1, pair[1].0, pair[1].1, ch);
        }
        if closed && points.len() > 2 {
            let (first, last) = (points[0], points[points.len() - 1]);
            self.line(last.0, last.1, first.0, first.1, ch);
        }
        if points.len() == 1 {
            self.plot(points[0].0, points[0].1, ch);
        }
    }

    /// A box between two world corners, drawn with the `(horizontal,
    /// vertical, corner)` characters of `style`.
    fn rect_outline(&mut self, a: (f64, f64), b: (f64, f64), style: (char, char, char)) {
        let (horizontal, vertical, corner) = style;
        let (ax, ay) = self.cell(a.0, a.1);
        let (bx, by) = self.cell(b.0, b.1);
        let (left, right, top, bottom) = (ax.min(bx), ax.max(bx), ay.min(by), ay.max(by));
        for cx in left..=right {
            self.cells[top][cx] = horizontal;
            self.cells[bottom][cx] = horizontal;
        }
        for cy in top..=bottom {
            self.cells[cy][left] = vertical;
            self.cells[cy][right] = vertical;
        }
        for (cy, cx) in [(top, left), (top, right), (bottom, left), (bottom, right)] {
            self.cells[cy][cx] = corner;
        }
    }

    fn rect_fill(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, ch: char) {
        let (ax, ay) = self.cell(x1, y1);
        let (bx, by) = self.cell(x2, y2);
        for cy in ay.min(by)..=ay.max(by) {
            for cx in ax.min(bx)..=ax.max(bx) {
                self.cells[cy][cx] = ch;
            }
        }
    }

    /// An elliptical arc sampled point by point: `centre`, `radii` (x, y)
    /// and `angles` (start, end) in degrees counter-clockwise from the +X
    /// axis; a full circle when the angles coincide.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn arc(&mut self, centre: (f64, f64), radii: (f64, f64), angles: (f64, f64), ch: char) {
        let (cx, cy) = centre;
        let (rx, ry) = radii;
        let (start_deg, end_deg) = angles;
        let mut sweep = (end_deg - start_deg).rem_euclid(360.0);
        if sweep == 0.0 {
            sweep = 360.0;
        }
        let perimeter_cells = (rx.max(ry) * self.scale_x.max(self.scale_y) * 6.3).ceil();
        let steps = (perimeter_cells * sweep / 360.0).ceil().max(8.0) as u32;
        for i in 0..=steps {
            let fraction = f64::from(i) / f64::from(steps);
            let angle = sweep.mul_add(fraction, start_deg).to_radians();
            let (sin, cos) = angle.sin_cos();
            self.plot(rx.mul_add(cos, cx), ry.mul_add(sin, cy), ch);
        }
    }

    /// Text starting at a world point and running right.
    fn label_at(&mut self, x: f64, y: f64, text: &str) {
        let (cx, cy) = self.cell(x, y);
        self.put(cx, cy, text);
    }

    /// Text ending at a world point (for a pin pointing right).
    fn label_ending_at(&mut self, x: f64, y: f64, text: &str) {
        let (cx, cy) = self.cell(x, y);
        let start = cx.saturating_sub(text.chars().count().saturating_sub(1));
        self.put(start, cy, text);
    }

    /// Text centred on a world point.
    fn label_centred(&mut self, x: f64, y: f64, text: &str) {
        let (cx, cy) = self.cell(x, y);
        self.put(cx.saturating_sub(text.chars().count() / 2), cy, text);
    }

    fn put(&mut self, start: usize, cy: usize, text: &str) {
        for (i, ch) in text.chars().enumerate() {
            if let Some(cell) = self.cells[cy].get_mut(start + i) {
                *cell = ch;
            }
        }
    }

    /// The origin as `+`, only onto a blank cell so it never hides a primitive.
    fn origin(&mut self) {
        let (cx, cy) = self.cell(0.0, 0.0);
        if self.cells[cy][cx] == ' ' {
            self.cells[cy][cx] = '+';
        }
    }

    /// The canvas as rows between `|` bars, ruled above and below.
    fn framed(&self) -> String {
        let rule = format!("{}\n", "-".repeat(self.width + 2));
        let mut out = rule.clone();
        for row in &self.cells {
            out.push('|');
            out.extend(row.iter());
            out.push_str("|\n");
        }
        out.push_str(&rule);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::altium::pcblib::{Footprint, PcbLib};
    use crate::mcp::tools::test_support::{
        create_test_pcblib, create_test_schlib, create_test_server, get_result_text,
        parse_result_json, test_temp_dir,
    };

    #[test]
    fn render_footprint_success() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Render.PcbLib");
        create_test_pcblib(&path);

        let result = server.call_render_footprint(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "CHIP_0402",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["component_name"], "CHIP_0402");
        let render = parsed["render"].as_str().unwrap();
        assert!(render.starts_with("Footprint: CHIP_0402"));
        assert!(render.contains("Pads: 2, Tracks: 0, Arcs: 0"));
        assert!(render.contains('#'), "pads should be drawn: {render}");
        assert!(render.contains("Legend: # = pad"));
    }

    #[test]
    fn render_footprint_empty_footprint() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let mut lib = PcbLib::new();
        lib.add(Footprint::new("EMPTY"));
        let path = dir.path().join("Empty.PcbLib");
        lib.save(&path).unwrap();

        let result = server.call_render_footprint(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "EMPTY",
        }));
        assert!(!result.is_error);
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["render"], "Empty footprint (no primitives)");
    }

    #[test]
    fn render_footprint_error_paths() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("RenderErr.PcbLib");
        create_test_pcblib(&path);
        let filepath = path.to_string_lossy().to_string();

        let result = server.call_render_footprint(&json!({}));
        assert!(result.is_error);
        assert_eq!(
            get_result_text(&result),
            "Missing required parameter: filepath"
        );

        // Unknown footprint lists available ones as a hint.
        let result = server.call_render_footprint(&json!({
            "filepath": filepath,
            "component_name": "GHOST",
        }));
        assert!(result.is_error);
        let text = get_result_text(&result);
        assert!(text.contains("'GHOST' not found"));
        assert!(text.contains("CHIP_0402"));

        // Non-positive scale is rejected.
        let result = server.call_render_footprint(&json!({
            "filepath": filepath,
            "component_name": "CHIP_0402",
            "scale": 0.0,
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("scale must be greater than 0"));
    }

    #[test]
    fn render_symbol_success() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Render.SchLib");
        create_test_schlib(&path);

        let result = server.call_render_symbol(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "RESISTOR",
        }));
        assert!(!result.is_error, "{}", get_result_text(&result));
        let parsed = parse_result_json(&result);
        assert_eq!(parsed["status"], "success");
        assert_eq!(parsed["part_id"], 1);
        let render = parsed["render"].as_str().unwrap();
        assert!(render.starts_with("Symbol: RESISTOR (part 1/1)"));
        assert!(render.contains("Pins: 2, Rectangles: 1, Lines: 0"));
        assert!(render.contains('~'), "pin lines should be drawn: {render}");
        assert!(render.contains("Legend: |-+ = rectangle"));
    }

    #[test]
    fn render_symbol_error_paths() {
        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("RenderErr.SchLib");
        create_test_schlib(&path);
        let filepath = path.to_string_lossy().to_string();

        let result = server.call_render_symbol(&json!({
            "filepath": filepath,
        }));
        assert!(result.is_error);
        assert_eq!(
            get_result_text(&result),
            "Missing required parameter: component_name"
        );

        let result = server.call_render_symbol(&json!({
            "filepath": filepath,
            "component_name": "GHOST",
        }));
        assert!(result.is_error);
        let text = get_result_text(&result);
        assert!(text.contains("'GHOST' not found"));
        assert!(text.contains("RESISTOR"));

        let result = server.call_render_symbol(&json!({
            "filepath": filepath,
            "component_name": "RESISTOR",
            "scale": -1.0,
        }));
        assert!(result.is_error);
        assert!(get_result_text(&result).contains("scale must be greater than 0"));
    }

    // ==================== deep rendering paths ====================

    mod deep_coverage {
        use super::*;
        use crate::altium::pcblib::{Arc as PcbArc, Layer, Pad, Track};
        use crate::altium::schlib::{
            Arc as SchArc, Ellipse, Line, Pin, PinOrientation, Polyline, Rectangle, SchLib,
            ShapeDisplayFlags, Symbol,
        };

        fn poly(owner_part_id: i32) -> Polyline {
            Polyline {
                raw_params: Vec::new(),
                points: vec![(-5.0, 0.0), (0.0, 8.0), (5.0, 0.0)],
                line_width: 1,
                color: 0,
                line_style: 0,
                start_line_shape: 0,
                end_line_shape: 0,
                line_shape_size: 0,
                transparent: false,
                is_not_accessible: true,
                owner_part_id,
                display_flags: ShapeDisplayFlags::default(),
                unique_id: None,
            }
        }

        fn sch_arc(owner_part_id: i32) -> SchArc {
            SchArc {
                raw_params: Vec::new(),
                x: 0.0,
                y: 0.0,
                radius: 6.0,
                is_not_accessible: true,
                start_angle: 0.0,
                end_angle: 180.0,
                line_width: 1,
                color: 0,
                fill_color: 0,
                owner_part_id,
                display_flags: ShapeDisplayFlags::default(),
                unique_id: None,
            }
        }

        #[test]
        fn render_footprint_with_tracks_arcs_and_clamp() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("RichFp.PcbLib");
            let mut lib = PcbLib::new();
            let mut fp = Footprint::new("RICH_FP");
            fp.add_pad(Pad::smd("10", -8.0, 0.0, 1.0, 1.0)); // 2-char designator
            fp.add_pad(Pad::smd("2", 8.0, 0.0, 1.0, 1.0));
            fp.add_track(Track::new(-8.0, 0.0, 8.0, 0.0, 0.2, Layer::TopOverlay));
            fp.add_arc(PcbArc::circle(0.0, 3.0, 1.0, 0.15, Layer::TopOverlay));
            lib.add(fp);
            lib.save(&path).unwrap();

            let r = server.call_render_footprint(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "RICH_FP",
                "max_width": 10, // force canvas clamping
                "max_height": 5,
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let render = parse_result_json(&r)["render"]
                .as_str()
                .unwrap()
                .to_string();
            assert!(render.contains("Tracks: 1, Arcs: 1"), "{render}");
        }

        #[test]
        fn render_footprint_error_branches() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            // Missing component_name.
            let path = dir.path().join("A.PcbLib");
            create_test_pcblib(&path);
            let r = server.call_render_footprint(&json!({ "filepath": path.to_string_lossy() }));
            assert!(r.is_error);
            assert_eq!(
                get_result_text(&r),
                "Missing required parameter: component_name"
            );

            // Corrupt file -> failed to read.
            let garbage = dir.path().join("Garbage.PcbLib");
            std::fs::write(&garbage, b"not a library").unwrap();
            let r = server.call_render_footprint(&json!({
                "filepath": garbage.to_string_lossy(), "component_name": "X",
            }));
            assert!(r.is_error);

            // Empty library -> "Library is empty" hint.
            let empty = dir.path().join("Empty.PcbLib");
            PcbLib::new().save(&empty).unwrap();
            let r = server.call_render_footprint(&json!({
                "filepath": empty.to_string_lossy(), "component_name": "GHOST",
            }));
            assert!(r.is_error);
            assert!(get_result_text(&r).contains("empty"));

            // Not-found with >5 candidates -> "and N more" hint.
            let many = dir.path().join("Many.PcbLib");
            let mut lib = PcbLib::new();
            for i in 0..12 {
                lib.add(Footprint::new(format!("FP{i}")));
            }
            lib.save(&many).unwrap();
            let r = server.call_render_footprint(&json!({
                "filepath": many.to_string_lossy(), "component_name": "GHOST",
            }));
            assert!(r.is_error);
            let text = get_result_text(&r);
            assert!(
                text.contains("Available: FP0, FP1") && text.contains("... and 2 more"),
                "{text}"
            );
        }

        #[test]
        fn render_symbol_rich_primitives() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());
            let path = dir.path().join("RichSym.SchLib");

            let mut sym = Symbol::new("RICH_SYM");
            sym.part_count = 2;
            sym.add_pin(Pin::new("A", "A", 0, 20, 10, PinOrientation::Up));
            sym.add_pin(Pin::new("B", "B", 0, -20, 10, PinOrientation::Down));
            let mut skip_pin = Pin::new("Z", "Z", 40, 0, 10, PinOrientation::Right);
            skip_pin.owner_part_id = 2; // filtered out at part_id 1
            sym.add_pin(skip_pin);
            sym.add_rectangle(Rectangle::new(-15, -10, 15, 10));
            let mut r2 = Rectangle::new(-1, -1, 1, 1);
            r2.owner_part_id = 2;
            sym.add_rectangle(r2);
            sym.add_line(Line::new(-10, -10, 10, 10));
            let mut l2 = Line::new(0, 0, 1, 1);
            l2.owner_part_id = 2;
            sym.add_line(l2);
            sym.add_polyline(poly(1));
            sym.add_polyline(poly(2));
            sym.add_arc(sch_arc(1));
            sym.add_arc(sch_arc(2));
            sym.add_ellipse(Ellipse::circle(0, 0, 5));
            let mut e2 = Ellipse::circle(0, 0, 1);
            e2.owner_part_id = 2;
            sym.add_ellipse(e2);

            let mut lib = SchLib::new();
            lib.add(sym);
            lib.save(&path).unwrap();

            let r = server.call_render_symbol(&json!({
                "filepath": path.to_string_lossy(),
                "component_name": "RICH_SYM",
                "part_id": 1,
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            let p = parse_result_json(&r);
            assert_eq!(p["part_id"], 1);
            let render = p["render"].as_str().unwrap().to_string();
            assert!(
                render.starts_with("Symbol: RICH_SYM (part 1/2)"),
                "{render}"
            );
        }

        #[test]
        fn render_symbol_empty_and_error_branches() {
            let dir = test_temp_dir();
            let server = create_test_server(dir.path());

            // Empty symbol.
            let path = dir.path().join("EmptySym.SchLib");
            let mut lib = SchLib::new();
            lib.add(Symbol::new("EMPTY_SYM"));
            lib.save(&path).unwrap();
            let r = server.call_render_symbol(&json!({
                "filepath": path.to_string_lossy(), "component_name": "EMPTY_SYM",
            }));
            assert!(!r.is_error, "{}", get_result_text(&r));
            assert_eq!(
                parse_result_json(&r)["render"],
                "Empty symbol (no primitives)"
            );

            // Missing filepath.
            let r = server.call_render_symbol(&json!({}));
            assert!(r.is_error);
            assert_eq!(get_result_text(&r), "Missing required parameter: filepath");

            // Corrupt file.
            let garbage = dir.path().join("Garbage.SchLib");
            std::fs::write(&garbage, b"not a library").unwrap();
            let r = server.call_render_symbol(&json!({
                "filepath": garbage.to_string_lossy(), "component_name": "X",
            }));
            assert!(r.is_error);

            // Not-found with >5 candidates.
            let many = dir.path().join("ManySym.SchLib");
            let mut lib = SchLib::new();
            for i in 0..12 {
                lib.add(Symbol::new(format!("S{i}")));
            }
            lib.save(&many).unwrap();
            let r = server.call_render_symbol(&json!({
                "filepath": many.to_string_lossy(), "component_name": "GHOST",
            }));
            assert!(r.is_error);
            let text = get_result_text(&r);
            assert!(
                text.contains("Available: S0, S1") && text.contains("... and 2 more"),
                "{text}"
            );
        }
    }

    #[test]
    fn rendering_refuses_a_library_outside_the_allowed_directories() {
        // Both renderers read the file before drawing, so the sandbox check
        // has to come first — otherwise a render is an arbitrary file read.
        let dir = test_temp_dir();
        let other = test_temp_dir();
        let server = create_test_server(dir.path());

        let outside_pcb = other.path().join("Outside.PcbLib");
        create_test_pcblib(&outside_pcb);
        let r = server.call_render_footprint(&json!({
            "filepath": outside_pcb.to_string_lossy(),
            "component_name": "CHIP_0402",
        }));
        assert!(r.is_error);
        assert!(
            get_result_text(&r).contains("Access denied"),
            "{}",
            get_result_text(&r)
        );

        let outside_sch = other.path().join("Outside.SchLib");
        create_test_schlib(&outside_sch);
        let r = server.call_render_symbol(&json!({
            "filepath": outside_sch.to_string_lossy(),
            "component_name": "RESISTOR",
        }));
        assert!(r.is_error);
        assert!(
            get_result_text(&r).contains("Access denied"),
            "{}",
            get_result_text(&r)
        );
    }

    #[test]
    fn rendering_from_an_empty_library_says_so_rather_than_listing_nothing() {
        // An empty "Available:" list would read as a bug in the tool; naming
        // the real cause points at the file instead.
        use crate::altium::schlib::SchLib;

        let dir = test_temp_dir();
        let server = create_test_server(dir.path());
        let path = dir.path().join("Empty.SchLib");
        SchLib::new().save(&path).unwrap();

        let r = server.call_render_symbol(&json!({
            "filepath": path.to_string_lossy(),
            "component_name": "ANY",
        }));
        assert!(r.is_error);
        assert!(
            get_result_text(&r).contains("Available: none (the library is empty)"),
            "{}",
            get_result_text(&r)
        );
    }

    /// Every primitive kind is drawn and counted: a footprint holding only
    /// the kinds the old renderer ignored is not "empty", each kind leaves
    /// its marker, and the header counts every kind.
    #[test]
    fn render_footprint_draws_every_kind() {
        use crate::altium::pcblib::{
            Arc, ComponentBody, Fill, Layer, Pad, Region, Text, Track, Via,
        };

        let layer = Layer::TopOverlay;
        let mut fp = Footprint::new("KINDS");
        fp.add_via(Via::new(-3.0, 0.0, 0.6, 0.3));
        fp.add_fill(Fill::new(-2.0, -2.0, -1.0, -1.0, layer));
        fp.add_region(Region::rectangle(1.0, 1.0, 2.5, 2.5, layer));
        fp.add_text(Text::new(0.0, 2.0, "T", 0.5, layer));
        let mut body = ComponentBody::new("", "b.step");
        body.embedded = false;
        body.outline = vec![(-3.0, -3.0), (3.0, -3.0), (3.0, 3.0), (-3.0, 3.0)];
        fp.add_component_body(body);
        let without_pads = McpServer::render_footprint_ascii(&fp, 4.0, 120, 60);
        assert!(!without_pads.starts_with("Empty"), "{without_pads}");
        for marker in ['O', '=', '%', 'T', '.'] {
            assert!(
                without_pads.contains(marker),
                "marker {marker:?} missing:
{without_pads}"
            );
        }
        assert!(without_pads.contains(
            "Pads: 0, Tracks: 0, Arcs: 0, Vias: 1, Fills: 1, Regions: 1, Text: 1, Bodies: 1"
        ));

        fp.add_pad(Pad::smd("1", 0.0, 0.0, 1.0, 1.0));
        fp.add_track(Track::new(-2.0, 2.5, 2.0, 2.5, 0.2, layer));
        fp.add_arc(Arc::circle(0.0, -2.0, 0.5, 0.1, layer));
        let everything = McpServer::render_footprint_ascii(&fp, 4.0, 120, 60);
        for marker in ['#', '-', 'o'] {
            assert!(
                everything.contains(marker),
                "marker {marker:?} missing:
{everything}"
            );
        }
        assert!(everything.contains("Pads: 1, Tracks: 1, Arcs: 1, Vias: 1"));
    }

    /// The same for a symbol: every record kind of the part is drawn and
    /// counted, and a symbol made only of the kinds the old renderer
    /// ignored is not "empty".
    #[test]
    fn render_symbol_draws_every_kind() {
        use crate::altium::schlib::{
            Bezier, EllipticalArc, IeeeSymbol, Image, Label, Pie, Polygon, RoundRect, Symbol,
            TextFrame,
        };

        let mut sym = Symbol::new("KINDS");
        sym.add_polygon(Polygon::new(vec![
            (-40.0, -40.0),
            (-20.0, -40.0),
            (-30.0, -20.0),
        ]));
        sym.add_pie(Pie::new(30, 30, 8, 0.0, 180.0));
        sym.add_round_rect(RoundRect::new(-40, 10, -10, 30, 2, 2));
        sym.add_bezier(Bezier::new(0, -40, 5, -30, 15, -30, 20, -40));
        sym.add_elliptical_arc(EllipticalArc::new(0, 0, 10, 5, 0.0, 270.0));
        sym.add_image(Image::new(10, -20, 40, 0, "logo.bmp"));
        sym.add_text_frame(TextFrame::new(-40, -10, -10, 0, "NOTE"));
        sym.add_label(Label::new(20, 20, "LBL"));
        sym.add_ieee_symbol(IeeeSymbol::new(1, 40.0, 40.0));

        let render = McpServer::render_symbol_ascii(&sym, 2.0, 160, 80, 1);
        assert!(!render.starts_with("Empty"), "{render}");
        for marker in ['-', 'o', '(', '~', ':', '=', 'L', '@'] {
            assert!(
                render.contains(marker),
                "marker {marker:?} missing:
{render}"
            );
        }
        assert!(
            render.contains("NOTE") && render.contains("LBL"),
            "{render}"
        );
        let header = render.lines().nth(1).expect("the count line");
        assert_eq!(
            header,
            concat!(
                "Pins: 0, Rectangles: 0, Lines: 0, Polygons: 1, Pies: 1, Round rects: 1, ",
                "Elliptical arcs: 1, Beziers: 1, Images: 1, Text frames: 1, Labels: 1, ",
                "IEEE symbols: 1"
            )
        );
    }
}
