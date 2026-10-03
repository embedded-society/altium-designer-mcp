//! Mutation fidelity: a tool that touches one component leaves every other
//! component byte-identical.
//!
//! Every mutating tool opens the whole library, changes what it was asked to
//! and saves the whole library back, so a stray edit — an identity reset, a
//! default filled in, a stream re-encoded — lands on components the caller
//! never named. Each case here runs one tool on a copy of the golden library
//! and compares every component it did not name, stream for stream, with the
//! library-level save the byte-fidelity suite holds to the golden.

use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use crate::altium::{PcbLib, SchLib};
use crate::mcp::server::McpServer;
use crate::mcp::tools::test_support::{
    assert_same_stream, component_streams, get_result_text, mask_generated_ids, parse_result_json,
    test_temp_dir, unique_ids,
};

type Streams = BTreeMap<String, BTreeMap<String, Vec<u8>>>;

/// The golden library of `kind` (`PcbLib` / `SchLib`).
fn golden(kind: &str) -> PathBuf {
    let name = match kind {
        "PcbLib" => "footprints.PcbLib",
        _ => "symbols.SchLib",
    };
    Path::new("scripts/samples")
        .canonicalize()
        .unwrap()
        .join(name)
}

/// A scratch copy of the golden under `dir`, a server allowed there and in
/// the samples, and the library-level save every comparison is made against.
struct Bench {
    dir: tempfile::TempDir,
    server: McpServer,
    src: PathBuf,
    work: PathBuf,
    baseline: Streams,
    golden_ids: HashSet<Vec<u8>>,
}

impl Bench {
    fn new(kind: &str) -> Self {
        let dir = test_temp_dir();
        let src = golden(kind);
        let samples = src.parent().unwrap().to_path_buf();
        let server = McpServer::new(vec![dir.path().to_path_buf(), samples]);
        let work = dir.path().join(format!("Work.{kind}"));
        std::fs::copy(&src, &work).unwrap();
        let baseline_path = dir.path().join(format!("Baseline.{kind}"));
        if kind == "PcbLib" {
            PcbLib::open(&src).unwrap().save(&baseline_path).unwrap();
        } else {
            SchLib::open(&src).unwrap().save(&baseline_path).unwrap();
        }
        let golden_ids = component_streams(&src)
            .values()
            .flat_map(|streams| streams.values())
            .flat_map(|bytes| unique_ids(bytes))
            .collect();
        Self {
            dir,
            server,
            src,
            work,
            baseline: component_streams(&baseline_path),
            golden_ids,
        }
    }

    fn path(&self, name: &str) -> String {
        self.dir.path().join(name).to_string_lossy().into_owned()
    }

    fn work(&self) -> String {
        self.work.to_string_lossy().into_owned()
    }

    /// Asserts that every component the tool was not asked to touch is
    /// byte-identical to the baseline in `actual` (the work file unless
    /// given), and that the touched ones are exactly `touched`. A file the
    /// tool left entirely as the golden — it had nothing to write — passes
    /// outright: that is the strongest form of "untouched".
    fn assert_untouched_except(&self, touched: &[&str], actual: Option<&Path>) {
        let path = actual.unwrap_or(&self.work);
        if touched.is_empty() && std::fs::read(path).unwrap() == std::fs::read(&self.src).unwrap() {
            return;
        }
        let actual = component_streams(path);
        let touched: BTreeSet<&str> = touched.iter().copied().collect();
        for (name, expected) in &self.baseline {
            if touched.contains(name.as_str()) {
                continue;
            }
            let Some(streams) = actual.get(name) else {
                panic!("{name} vanished although the tool was not asked to touch it");
            };
            self.assert_same_component(name, expected, streams);
        }
        for name in actual.keys() {
            assert!(
                self.baseline.contains_key(name) || touched.contains(name.as_str()),
                "{name} appeared although the tool was not asked to create it"
            );
        }
    }

    /// Asserts two components' streams are byte-identical; `SchLib` IDs the
    /// golden lacks are masked, `PrimitiveGuids` is compared as a set.
    fn assert_same_component(
        &self,
        name: &str,
        expected: &BTreeMap<String, Vec<u8>>,
        actual: &BTreeMap<String, Vec<u8>>,
    ) {
        assert_eq!(
            expected.keys().collect::<Vec<_>>(),
            actual.keys().collect::<Vec<_>>(),
            "{name}: streams differ"
        );
        for (stream, a) in expected {
            let b = &actual[stream];
            if stream == "PrimitiveGuids/Data" {
                let records = |bytes: &[u8]| -> BTreeSet<Vec<u8>> {
                    bytes.chunks_exact(24).map(<[u8]>::to_vec).collect()
                };
                assert_eq!(records(a), records(b), "{name}/{stream}");
            } else {
                let a = mask_generated_ids(a, &self.golden_ids);
                let b = mask_generated_ids(b, &self.golden_ids);
                assert_same_stream(&format!("{name}/{stream}"), Some(&a), Some(&b));
            }
        }
    }

    /// Asserts the work file lists its components in the golden's order
    /// with `renames` applied in place — a renamed component keeps its
    /// place rather than dropping to the end of the library.
    fn assert_order_kept(&self, renames: &[(&str, &str)]) {
        let names = |path: &Path| -> Vec<String> {
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("pcblib"))
            {
                PcbLib::open(path).unwrap().names()
            } else {
                SchLib::open(path).unwrap().names()
            }
        };
        let expected: Vec<String> = names(&self.src)
            .into_iter()
            .map(|name| {
                renames
                    .iter()
                    .find(|(old, _)| *old == name)
                    .map_or(name, |(_, new)| (*new).to_string())
            })
            .collect();
        assert_eq!(names(&self.work), expected, "the library order changed");
    }

    /// Runs `tool` and asserts it succeeded.
    fn run(&self, tool: &str, arguments: &Value) -> Value {
        let result = match tool {
            "delete_component" => self.server.call_delete_component(arguments),
            "rename_component" => self.server.call_rename_component(arguments),
            "copy_component" => self.server.call_copy_component(arguments),
            "reorder_components" => self.server.call_reorder_components(arguments),
            "bulk_rename" => self.server.call_bulk_rename(arguments),
            "update_pad" => self.server.call_update_pad(arguments),
            "update_primitive" => self.server.call_update_primitive(arguments),
            "batch_update" => self.server.call_batch_update(arguments),
            "repair_library" => self.server.call_repair_library(arguments),
            "export_library" => self.server.call_export_library(arguments),
            "import_library" => self.server.call_import_library(arguments),
            "merge_libraries" => self.server.call_merge_libraries(arguments),
            "copy_component_cross_library" => {
                self.server.call_copy_component_cross_library(arguments)
            }
            "manage_schlib_parameters" => self.server.call_manage_schlib_parameters(arguments),
            "manage_schlib_footprints" => self.server.call_manage_schlib_footprints(arguments),
            other => panic!("no such tool in this bench: {other}"),
        };
        assert!(!result.is_error, "{tool}: {}", get_result_text(&result));
        parse_result_json(&result)
    }
}

// ==================== PcbLib ====================

#[test]
fn pcblib_delete_leaves_the_rest_intact() {
    let b = Bench::new("PcbLib");
    b.run(
        "delete_component",
        &json!({ "filepath": b.work(), "component_names": ["ARCS", "VIAS"] }),
    );
    b.assert_untouched_except(&["ARCS", "VIAS"], None);
    assert!(!component_streams(&b.work).contains_key("ARCS"));
}

#[test]
fn pcblib_rename_moves_the_bytes_and_leaves_the_rest_intact() {
    let b = Bench::new("PcbLib");
    b.run(
        "rename_component",
        &json!({ "filepath": b.work(), "old_name": "TRACKS", "new_name": "TRACKS_RENAMED" }),
    );
    b.assert_untouched_except(&["TRACKS", "TRACKS_RENAMED"], None);
    b.assert_order_kept(&[("TRACKS", "TRACKS_RENAMED")]);
    let after = component_streams(&b.work);
    assert!(!after.contains_key("TRACKS"));
    // A rename changes the name block that opens the Data stream and nothing
    // else: the primitive records after it move as they are.
    assert_eq!(
        after_name_block(&b.baseline["TRACKS"]["Data"]),
        after_name_block(&after["TRACKS_RENAMED"]["Data"]),
        "the primitive records changed under a rename"
    );
    for stream in ["WideStrings", "UniqueIDPrimitiveInformation/Data"] {
        assert_eq!(
            b.baseline["TRACKS"].get(stream),
            after["TRACKS_RENAMED"].get(stream),
            "{stream}"
        );
    }
}

/// A `PcbLib` Data stream without its leading `[len:4][name]` block.
fn after_name_block(data: &[u8]) -> &[u8] {
    let len = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
    &data[4 + len..]
}

#[test]
fn pcblib_copy_within_a_library_leaves_the_source_and_the_rest_intact() {
    let b = Bench::new("PcbLib");
    b.run(
        "copy_component",
        &json!({ "filepath": b.work(), "source_name": "REGIONS", "target_name": "REGIONS_COPY" }),
    );
    b.assert_untouched_except(&["REGIONS_COPY"], None);
}

#[test]
fn pcblib_reorder_changes_nothing_but_the_order() {
    let b = Bench::new("PcbLib");
    // Addressed by real name (the non-ASCII one is not its storage name).
    let mut order = PcbLib::open(&b.work).unwrap().names();
    order.reverse();
    b.run(
        "reorder_components",
        &json!({ "filepath": b.work(), "component_order": order }),
    );
    b.assert_untouched_except(&[], None);
}

#[test]
fn pcblib_bulk_rename_touches_only_the_matches() {
    let b = Bench::new("PcbLib");
    let report = b.run(
        "bulk_rename",
        &json!({ "filepath": b.work(), "pattern": "^TEXT_(.*)$", "replacement": "TXT_$1", "dry_run": false }),
    );
    let renamed: Vec<(String, String)> = report["renames"]
        .as_array()
        .map(|r| {
            r.iter()
                .map(|e| {
                    (
                        e["from"].as_str().unwrap().to_string(),
                        e["to"].as_str().unwrap().to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    assert!(!renamed.is_empty(), "{report}");
    let touched: Vec<&str> = renamed
        .iter()
        .flat_map(|(o, n)| [o.as_str(), n.as_str()])
        .collect();
    b.assert_untouched_except(&touched, None);
    let pairs: Vec<(&str, &str)> = renamed
        .iter()
        .map(|(o, n)| (o.as_str(), n.as_str()))
        .collect();
    b.assert_order_kept(&pairs);
    let after = component_streams(&b.work);
    for (old, new) in &renamed {
        assert_eq!(
            after_name_block(&b.baseline[old]["Data"]),
            after_name_block(&after[new]["Data"]),
            "{old} -> {new}: the primitive records changed under a rename"
        );
    }
}

#[test]
fn pcblib_pad_and_primitive_edits_stay_on_their_footprint() {
    let b = Bench::new("PcbLib");
    b.run(
        "update_pad",
        &json!({ "filepath": b.work(), "component_name": "PAD_SHAPES", "designator": "1", "updates": { "width": 1.3 } }),
    );
    b.run(
        "update_primitive",
        &json!({ "filepath": b.work(), "component_name": "TRACKS", "primitive_type": "track", "index": 0, "updates": { "width": 0.3 } }),
    );
    b.assert_untouched_except(&["PAD_SHAPES", "TRACKS"], None);
}

#[test]
fn pcblib_batch_update_that_matches_nothing_changes_nothing() {
    let b = Bench::new("PcbLib");
    let report = b.run(
        "batch_update",
        &json!({ "filepath": b.work(), "operation": "update_track_width", "parameters": { "from_width": 9.87, "to_width": 9.88 }, "dry_run": false }),
    );
    assert_eq!(
        report["tracks_updated"].as_u64().unwrap_or(0),
        0,
        "{report}"
    );
    b.assert_untouched_except(&[], None);
}

#[test]
fn pcblib_repair_of_a_healthy_library_changes_nothing() {
    let b = Bench::new("PcbLib");
    b.run(
        "repair_library",
        &json!({ "filepath": b.work(), "dry_run": false }),
    );
    b.assert_untouched_except(&[], None);
}

#[test]
fn pcblib_export_then_import_is_byte_identical() {
    let b = Bench::new("PcbLib");
    let exported = b.run(
        "export_library",
        &json!({ "filepath": b.work(), "format": "json", "compact": false }),
    );
    let out = b.path("Imported.PcbLib");
    b.run(
        "import_library",
        &json!({
            "output_path": out,
            "json_data": {
                "file_type": "PcbLib",
                "footprints": exported["footprints"],
                "embedded_models": exported["embedded_models"],
            },
        }),
    );
    b.assert_untouched_except(&[], Some(Path::new(&out)));
}

#[test]
fn pcblib_merge_into_an_empty_library_is_byte_identical() {
    let b = Bench::new("PcbLib");
    let target = b.path("Merged.PcbLib");
    b.run(
        "merge_libraries",
        &json!({ "source_filepaths": [b.work()], "target_filepath": target, "on_duplicate": "skip" }),
    );
    b.assert_untouched_except(&[], Some(Path::new(&target)));
}

#[test]
fn pcblib_cross_library_copy_carries_the_record_bytes() {
    let b = Bench::new("PcbLib");
    let target = b.path("Target.PcbLib");
    // A footprint without pads: the identities a copy resets live in the
    // pad record, so its Data stream must come across as it is.
    b.run(
        "copy_component_cross_library",
        &json!({ "source_filepath": b.work(), "target_filepath": target, "component_name": "ARCS" }),
    );
    let copied = component_streams(Path::new(&target));
    assert_same_stream(
        "ARCS/Data",
        b.baseline["ARCS"].get("Data"),
        copied["ARCS"].get("Data"),
    );
    b.assert_untouched_except(&[], None);
}

// ==================== SchLib ====================

#[test]
fn schlib_delete_leaves_the_rest_intact() {
    let b = Bench::new("SchLib");
    b.run(
        "delete_component",
        &json!({ "filepath": b.work(), "component_names": ["PIESYM"] }),
    );
    b.assert_untouched_except(&["PIESYM"], None);
}

#[test]
fn schlib_rename_moves_the_bytes_and_leaves_the_rest_intact() {
    let b = Bench::new("SchLib");
    b.run(
        "rename_component",
        &json!({ "filepath": b.work(), "old_name": "POLYGONS", "new_name": "POLYGONS_RENAMED" }),
    );
    b.assert_untouched_except(&["POLYGONS", "POLYGONS_RENAMED"], None);
    b.assert_order_kept(&[("POLYGONS", "POLYGONS_RENAMED")]);
}

#[test]
fn schlib_reorder_changes_nothing_but_the_order() {
    let b = Bench::new("SchLib");
    let mut order = SchLib::open(&b.work).unwrap().names();
    order.reverse();
    b.run(
        "reorder_components",
        &json!({ "filepath": b.work(), "component_order": order }),
    );
    // The root SectionKeys map is indexed by position, so it legitimately
    // follows the new order; every symbol's own streams must not.
    b.assert_untouched_except(&["SectionKeys"], None);
}

#[test]
fn schlib_parameter_and_footprint_edits_stay_on_their_symbol() {
    let b = Bench::new("SchLib");
    b.run(
        "manage_schlib_parameters",
        &json!({ "filepath": b.work(), "component_name": "POLYGONS", "operation": "add", "parameter_name": "Tolerance", "value": "1%" }),
    );
    b.run(
        "manage_schlib_footprints",
        &json!({ "filepath": b.work(), "component_name": "POLYLINES", "operation": "add", "footprint_name": "RESC1608X55N" }),
    );
    b.assert_untouched_except(&["POLYGONS", "POLYLINES"], None);
}

#[test]
fn schlib_export_then_import_is_byte_identical() {
    let b = Bench::new("SchLib");
    let exported = b.run(
        "export_library",
        &json!({ "filepath": b.work(), "format": "json", "compact": false }),
    );
    let out = b.path("Imported.SchLib");
    b.run(
        "import_library",
        &json!({
            "output_path": out,
            "json_data": { "file_type": "SchLib", "symbols": exported["symbols"] },
        }),
    );
    // A library built anew writes its own root SectionKeys: a rewrite of the
    // source replays Altium's, whose entry order follows no rule this crate
    // can reproduce for a library it builds. Every symbol's own streams must
    // still come through untouched.
    b.assert_untouched_except(&["SectionKeys"], Some(Path::new(&out)));
}

#[test]
fn schlib_merge_into_an_empty_library_is_byte_identical() {
    let b = Bench::new("SchLib");
    let target = b.path("Merged.SchLib");
    b.run(
        "merge_libraries",
        &json!({ "source_filepaths": [b.work()], "target_filepath": target, "on_duplicate": "skip" }),
    );
    // As for export and import: the new library's SectionKeys is its own.
    b.assert_untouched_except(&["SectionKeys"], Some(Path::new(&target)));
}
