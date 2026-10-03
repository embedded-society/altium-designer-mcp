//! Keeps `scripts/samples/golden_expectations.json` — the `-Expect` file
//! `Verify-Libraries.ps1` holds the goldens to on an Altium box — in step with
//! what this reader finds in the goldens. Altium then asserts the same
//! component names and per-component primitive counts from the outside, so a
//! golden whose primitives Altium quietly dropped cannot pass as "opened".

use altium_designer_mcp::altium::pcblib::PcbLib;
use altium_designer_mcp::altium::schlib::SchLib;
use std::fmt::Write as _;

/// Quotes a name as a JSON string.
fn quoted(name: &str) -> String {
    serde_json::to_string(name).expect("a string always serialises")
}

/// Builds the expectations JSON from the two goldens, counts keyed as
/// `AltiumVerify.pas` reports them and listed in this reader's (file) order —
/// the harness matches the two sides by component name, since Altium iterates
/// a library in shortlex order instead.
fn build_expectations(root: &std::path::Path) -> String {
    let mut out = String::from("[\n");

    let pcb = PcbLib::open(root.join("scripts/samples/footprints.PcbLib")).expect("golden PcbLib");
    out.push_str("    {\n        \"file\": \"footprints.PcbLib\",\n        \"components\": [");
    let names: Vec<String> = pcb.iter().map(|f| quoted(&f.name)).collect();
    out.push_str(&names.join(", "));
    out.push_str("],\n        \"primitive_counts\": [\n");
    let counts: Vec<String> = pcb
        .iter()
        .map(|f| {
            let mut line = String::from("            {");
            write!(
                line,
                "\"pads\": {}, \"vias\": {}, \"tracks\": {}, \"arcs\": {}, \"text\": {}, \
                 \"fills\": {}, \"regions\": {}, \"component_bodies\": {}}}",
                f.pads.len(),
                f.vias.len(),
                f.tracks.len(),
                f.arcs.len(),
                f.text.len(),
                f.fills.len(),
                f.regions.len(),
                f.component_bodies.len()
            )
            .expect("writing to a String cannot fail");
            line
        })
        .collect();
    out.push_str(&counts.join(",\n"));
    out.push_str("\n        ]\n    },\n");

    let sch = SchLib::open(root.join("scripts/samples/symbols.SchLib")).expect("golden SchLib");
    out.push_str("    {\n        \"file\": \"symbols.SchLib\",\n        \"components\": [");
    let names: Vec<String> = sch.iter().map(|s| quoted(&s.name)).collect();
    out.push_str(&names.join(", "));
    out.push_str("],\n        \"primitive_counts\": [\n");
    let counts: Vec<String> = sch
        .iter()
        .map(|s| {
            // AD24's component iterator yields hidden user parameters (the
            // JUSTIFY golden's hidden `Tol` comes back) but not the special
            // `Comment` parameter while it is hidden (the PARAMS golden
            // carries 3 parameter records and AD iterates 2; the i18n
            // symbols' visible Comments all come back). Predict that.
            let iterated_parameters = s
                .parameters
                .iter()
                .filter(|p| !(p.hidden && p.name == "Comment"))
                .count();
            let mut line = String::from("            {");
            write!(
                line,
                "\"pins\": {}, \"rectangles\": {}, \"round_rects\": {}, \"lines\": {}, \
                 \"polylines\": {}, \"polygons\": {}, \"arcs\": {}, \"elliptical_arcs\": {}, \
                 \"pies\": {}, \"ellipses\": {}, \"beziers\": {}, \"images\": {}, \
                 \"text_frames\": {}, \"labels\": {}, \"parameters\": {}, \"ieee_symbols\": {}}}",
                s.pins.len(),
                s.rectangles.len(),
                s.round_rects.len(),
                s.lines.len(),
                s.polylines.len(),
                s.polygons.len(),
                s.arcs.len(),
                s.elliptical_arcs.len(),
                s.pies.len(),
                s.ellipses.len(),
                s.beziers.len(),
                s.images.len(),
                s.text_frames.len(),
                s.labels.len(),
                iterated_parameters,
                s.ieee_symbols.len()
            )
            .expect("writing to a String cannot fail");
            line
        })
        .collect();
    out.push_str(&counts.join(",\n"));
    out.push_str("\n        ]\n    }\n]\n");
    out
}

/// The committed expectations match what this reader finds in the goldens.
/// After regenerating the goldens, refresh the file with
/// `UPDATE_GOLDEN_EXPECTATIONS=1 cargo test --test golden_expectations`.
#[test]
fn golden_expectations_file_is_current() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = root.join("scripts/samples/golden_expectations.json");
    let built = build_expectations(&root);

    if std::env::var_os("UPDATE_GOLDEN_EXPECTATIONS").is_some() {
        std::fs::write(&path, &built).expect("write expectations");
        return;
    }

    let committed = std::fs::read_to_string(&path)
        .expect("scripts/samples/golden_expectations.json exists")
        .replace("\r\n", "\n");
    assert_eq!(
        committed, built,
        "scripts/samples/golden_expectations.json is stale; refresh it with \
         `UPDATE_GOLDEN_EXPECTATIONS=1 cargo test --test golden_expectations`"
    );
}

/// The expectations file is valid JSON in the shape `Verify-Libraries.ps1
/// -Expect` consumes: one entry per golden, counts aligned with components.
#[test]
fn golden_expectations_shape_matches_the_verify_bridge() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let expectations: serde_json::Value = serde_json::from_str(&build_expectations(&root))
        .expect("the built expectations are valid JSON");

    let entries = expectations.as_array().expect("an array of files");
    assert_eq!(entries.len(), 2);
    for entry in entries {
        let components = entry["components"].as_array().expect("components");
        let counts = entry["primitive_counts"].as_array().expect("counts");
        assert!(!components.is_empty(), "{}", entry["file"]);
        assert_eq!(components.len(), counts.len(), "{}", entry["file"]);
    }
}
