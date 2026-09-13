//! Round-trip fidelity against the Altium-authored goldens.
//!
//! Every other test here checks that a *value* survives. This one checks that
//! the *file* does: each committed golden is read with our reader, written back
//! with our writer, and the result diffed against the original at two levels —
//! which OLE streams exist, and what every parameter block inside them says.
//!
//! It exists because the alternative kept failing. `CAVITYHEIGHT` sat on the
//! reader's modelled-keys list, excluded from the unknown-key passthrough, with
//! no field parsing it and the writer emitting a hard-coded `0mil`; the whole
//! `PrimitiveGuids` stream was never read or written at all. Neither is
//! reachable by a self-round-trip, because our reader and writer agreed with
//! each other perfectly in both cases. Both were found by hand, late.
//!
//! Differences correct by design sit in [`BY_DESIGN`]; real losses this test
//! found sit in [`KNOWN_DEFECTS`], which is debt meant to shrink to nothing.
//! Anything in neither list fails, so a newly dropped field surfaces here
//! rather than in a corrupted library weeks later.

use altium_designer_mcp::altium::pcblib::PcbLib;
use altium_designer_mcp::altium::schlib::SchLib;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn sample(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("samples")
        .join(name)
}

/// Keys whose value is expected to differ on every write and carries no
/// fidelity meaning: identities Altium regenerates, the timestamp of the save
/// itself, and the absolute path of the file being written.
///
/// The per-layer GUID caches are *not* here. They are stable now that the
/// library's own parameter block is replayed byte-for-byte, and leaving them
/// checked is what proves the replay happened.
/// Audited against a UI double-save (`identifier.PcbLib` saved twice from one
/// unchanged in-memory state, 2026-08-16): `MODELID`, `MODEL.CHECKSUM`,
/// `ITEMGUID` and `REVISIONGUID` are all STABLE per save — the first two are
/// per-body constants Altium round-trips, the last two are literally empty in
/// every library-authored footprint — so they are compared now, not excused.
/// What remains: the save timestamp, the absolute path of the file being
/// written, and record `UNIQUEID`s our `SchLib` writer still regenerates where
/// no read value exists (header + RECORD=45).
const VOLATILE_KEYS: &[&str] = &["UNIQUEID", "FILENAME", "DATE", "TIME"];

fn is_volatile_key(key: &str) -> bool {
    // A %UTF8% twin's content depends on the ANSI code page of the machine
    // that wrote the file — Altium builds it by decoding the value's UTF-8
    // bytes through the authoring locale (Windows-1250 for the golden), we
    // write the raw bytes — while the plain key beside it carries the
    // authoritative value on every machine. The plain key is still compared,
    // so the value itself cannot silently change.
    VOLATILE_KEYS.contains(&key) || key.starts_with("%UTF8%")
}

/// Differences that are correct by design and will not change.
const BY_DESIGN: &[(&str, &str)] = &[
    (
        "library/padvialibrary/data",
        "an empty pad/via template cache with a fresh library id; no template is \
         modelled, so there is nothing to carry",
    ),
    (
        "library/componentparamstoc/data",
        "regenerated from the footprints, so ordering and spacing follow our \
         writer rather than the original",
    ),
];

/// Known defects: real fidelity losses this test found, each still open.
///
/// This is debt, not permission. Every entry is a field or stream Altium wrote
/// and we do not reproduce, and the list is meant to shrink to nothing — delete
/// an entry as its fix lands. It is spelled out here rather than left implicit
/// so the cost is visible in code review instead of being discovered in a
/// corrupted library.
const KNOWN_DEFECTS: &[(&str, &str)] = &[];

/// The five fixture symbols `DelphiScript` mangled before Altium saw them.
///
/// Each is internally inconsistent IN THE GOLDEN ITSELF: the CFB storage name
/// folds to the correct word (Javanese, Bengali, Cherokee, Inuktitut,
/// beyond-BMP Han) while the record inside stores a different, shifted string —
/// so no self-consistent writer can reproduce both at once. Root cause is AD's
/// own reader (four scripted repair attempts each failed differently; see the
/// `DOCUMENTED NEGATIVE` in `GenerateSamples.pas`). These five scripts get
/// their real, consistent coverage from the hand-authored
/// `scripts/samples/manual/i18n5.SchLib` instead; the excusal here stays
/// because the damaged copies remain in the generated golden.
const FIXTURE_INCONSISTENT: &[&str] = &["_jv", "_bn", "_cr", "_iu", "_sb"];

/// Whether a canonical path belongs to one of the five damaged fixtures.
fn is_fixture_inconsistent(what: &str) -> bool {
    let component = what.split('/').next().unwrap_or(what);
    FIXTURE_INCONSISTENT
        .iter()
        .any(|suffix| component.ends_with(suffix))
}

fn is_known(what: &str) -> bool {
    let lower = what.to_lowercase();
    is_fixture_inconsistent(&lower)
        || BY_DESIGN
            .iter()
            .chain(KNOWN_DEFECTS)
            .any(|(key, _)| lower.contains(&key.to_lowercase()))
}

/// Folds one path segment to a locale-independent form.
///
/// A CFB storage name for a non-Windows-1252 component is the name's UTF-8
/// bytes widened one-per-byte through the ANSI code page of the machine that
/// wrote the file — the golden was authored on a Windows-1250 box, we widen
/// through Windows-1252 — so the same component gets different UTF-16 storage
/// names on different machines while the underlying bytes are identical. This
/// inverts the widening by trying each plausible code page and keeping the
/// first whose bytes decode as UTF-8, which recovers the real name; the lossy
/// decode also absorbs a name the 31-unit cap cut mid-codepoint (the golden's
/// Sinhala symbol), identically on both sides.
fn canonical_segment(seg: &str) -> String {
    if seg.is_ascii() {
        return seg.to_lowercase();
    }
    for enc in [
        encoding_rs::WINDOWS_1252,
        encoding_rs::WINDOWS_1250,
        encoding_rs::WINDOWS_1251,
        encoding_rs::WINDOWS_1253,
        encoding_rs::WINDOWS_1254,
        encoding_rs::WINDOWS_1255,
        encoding_rs::WINDOWS_1256,
        encoding_rs::WINDOWS_1257,
        encoding_rs::WINDOWS_1258,
        encoding_rs::WINDOWS_874,
    ] {
        let (bytes, _, had_errors) = enc.encode(seg);
        if had_errors {
            continue;
        }
        let sound = match std::str::from_utf8(&bytes) {
            Ok(s) => !s.is_ascii(),
            // A tail the 31-unit cap cut mid-codepoint is not an arbitrary
            // decode failure: accept the fold when everything up to the cut is
            // sound UTF-8 and only the end is incomplete.
            Err(e) => e.error_len().is_none() && e.valid_up_to() > 0,
        };
        if sound {
            return String::from_utf8_lossy(&bytes).to_lowercase();
        }
    }
    seg.to_lowercase()
}

/// Folds a whole stream path segment-by-segment.
fn canonical_path(path: &str) -> String {
    path.trim_start_matches('/')
        .split(['/', '\\'])
        .map(canonical_segment)
        .collect::<Vec<_>>()
        .join("/")
}

/// Every stream in an OLE file, keyed by its canonical path (see
/// [`canonical_segment`]) with the actual path as the value, so two files can
/// be compared component-by-component regardless of the authoring locale.
fn stream_map(path: &Path) -> BTreeMap<String, String> {
    let file = std::fs::File::open(path).expect("open library");
    let cfb = cfb::CompoundFile::open(file).expect("parse OLE");
    cfb.walk()
        .filter(cfb::Entry::is_stream)
        .map(|e| {
            let actual = e.path().to_string_lossy().into_owned();
            (canonical_path(&actual), actual)
        })
        .collect()
}

/// Reads one stream's bytes, or `None` when it is absent.
fn stream_bytes(path: &Path, stream: &str) -> Option<Vec<u8>> {
    use std::io::Read as _;
    let file = std::fs::File::open(path).expect("open library");
    let mut cfb = cfb::CompoundFile::open(file).expect("parse OLE");
    let mut s = cfb.open_stream(stream).ok()?;
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// Splits a Data stream into its `|KEY=VALUE|…` parameter blocks and parses
/// each into a key/value map. Blocks are NUL-terminated inside the stream.
fn parameter_blocks(bytes: &[u8]) -> Vec<BTreeMap<String, String>> {
    let text = bytes
        .iter()
        .map(|&b| b as char) // Windows-1252 low range; adequate for key matching
        .collect::<String>();
    text.split('\0')
        .filter(|seg| seg.contains('|') && seg.contains('='))
        .map(|seg| {
            seg.split('|')
                .filter_map(|p| p.split_once('='))
                .map(|(k, v)| (k.to_ascii_uppercase(), v.to_string()))
                .collect()
        })
        .filter(|m: &BTreeMap<String, String>| m.len() >= 3)
        .collect()
}

/// Scores how well two blocks correspond: shared keys, plus a bonus for
/// agreeing on the values that identify a record rather than describe it.
fn similarity(a: &BTreeMap<String, String>, b: &BTreeMap<String, String>) -> usize {
    let shared: Vec<_> = a.keys().filter(|k| b.contains_key(*k)).collect();
    let agreeing = shared.iter().filter(|k| a.get(**k) == b.get(**k)).count();
    // Value agreement dominates: a region and a component body share almost
    // every key, so key overlap alone pairs them with each other.
    shared.len() + agreeing * 4
}

/// Pairs golden blocks with rewritten ones greedily, best match first and each
/// used only once, then reports every key whose value changed or vanished.
///
/// One-to-one matching matters: regions and component bodies share nearly their
/// whole key set, so a naive best-overlap pass happily compares a region
/// against a body and invents divergences for every field that legitimately
/// differs between them.
fn block_divergences(golden: &[u8], ours: &[u8], label: &str) -> Vec<String> {
    let (a, b) = (parameter_blocks(golden), parameter_blocks(ours));

    let mut pairs: Vec<(usize, usize, usize)> = Vec::new();
    for (i, ga) in a.iter().enumerate() {
        for (j, ob) in b.iter().enumerate() {
            pairs.push((similarity(ga, ob), i, j));
        }
    }
    pairs.sort_unstable_by_key(|&(score, _, _)| std::cmp::Reverse(score));

    let mut taken_g = vec![false; a.len()];
    let mut taken_o = vec![false; b.len()];
    let mut matched: Vec<(usize, usize)> = Vec::new();
    for (score, i, j) in pairs {
        if score < 3 || taken_g[i] || taken_o[j] {
            continue;
        }
        taken_g[i] = true;
        taken_o[j] = true;
        matched.push((i, j));
    }

    let mut out = Vec::new();
    for (i, j) in matched {
        for (key, want) in &a[i] {
            if is_volatile_key(key) {
                continue;
            }
            let got = b[j].get(key);
            if got != Some(want) {
                out.push(format!(
                    "{label}: {key} golden={want:?} ours={:?}",
                    got.map(String::as_str)
                ));
            }
        }
        // The other direction: a key we emit that Altium did not store is an
        // invention, not fidelity — an extruded body used to grow a whole
        // MODEL.* group (including a fresh MODELID GUID per save) that this
        // loop never saw, because only golden-side keys were compared.
        for (key, got) in &b[j] {
            if !is_volatile_key(key) && !a[i].contains_key(key) {
                out.push(format!(
                    "{label}: {key} invented; ours={got:?}, golden omits it"
                ));
            }
        }
    }
    for (i, used) in taken_g.iter().enumerate() {
        if !used {
            let kind = a[i]
                .get("RECORD")
                .or_else(|| a[i].get("V7_LAYER"))
                .map_or("?", String::as_str);
            out.push(format!(
                "{label}: a golden block ({} keys, kind {kind}) has no counterpart in our output",
                a[i].len()
            ));
        }
    }
    out
}

#[test]
#[allow(clippy::too_many_lines)] // five comparison passes, one straight-line block each
fn pcblib_golden_survives_a_round_trip() {
    let src = sample("footprints.PcbLib");
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("rewritten.PcbLib");
    let mut lib = PcbLib::open(&src).expect("read golden PcbLib");
    lib.save(&out).expect("write it back");

    let mut failures = Vec::new();

    // 1. Streams the golden has that we did not write back, compared by
    //    canonical path so a locale-widened storage name meets its twin.
    let (before, after) = (stream_map(&src), stream_map(&out));
    for missing in before.keys().filter(|k| !after.contains_key(*k)) {
        if !is_known(missing) {
            failures.push(format!("stream dropped entirely: {missing}"));
        }
    }

    // 2. The library-level layer stack and metadata.
    if let (Some(g), Some(o)) = (
        stream_bytes(&src, "/Library/Data"),
        stream_bytes(&out, "/Library/Data"),
    ) {
        failures.extend(
            block_divergences(&g, &o, "Library")
                .into_iter()
                .filter(|d| !is_known(d)),
        );
    }

    // 3. Parameter values inside each footprint's Data stream — every
    //    footprint the two files share, including the ones whose storage names
    //    differ only by authoring locale.
    for (canonical, g_path) in &before {
        let Some(fp) = canonical
            .strip_suffix("/data")
            .filter(|fp| !fp.contains('/') && *fp != "library")
        else {
            continue;
        };
        let Some(o_path) = after.get(canonical) else {
            continue; // already reported as dropped
        };
        let (Some(g), Some(o)) = (stream_bytes(&src, g_path), stream_bytes(&out, o_path)) else {
            continue;
        };
        failures.extend(
            block_divergences(&g, &o, fp)
                .into_iter()
                .filter(|d| !is_known(d)),
        );
    }

    // 4. The identity streams. Both key a primitive by its ordinal among all
    //    of the footprint's primitives, and a block-level diff cannot see a
    //    reordering. The unique-id records are rebuilt in ordinal order, which
    //    is also the order Altium stores them — compared byte for byte.
    //    PrimitiveGuids records are keyed (kind, ordinal, guid) but Altium
    //    scrambles their order in the stream while our writer emits them
    //    canonically, so they are compared as record SETS: same identities on
    //    the same primitives, which is the property that matters.
    for (canonical, g_path) in &before {
        if is_known(canonical) {
            continue;
        }
        if canonical.ends_with("uniqueidprimitiveinformation/data") {
            let g = stream_bytes(&src, g_path).expect("walked stream exists");
            match after.get(canonical).and_then(|p| stream_bytes(&out, p)) {
                Some(o) if o == g => {}
                Some(o) => failures.push(format!(
                    "{canonical} differs: {} bytes golden, {} ours",
                    g.len(),
                    o.len()
                )),
                None => failures.push(format!("{canonical} was not written back")),
            }
        } else if canonical.ends_with("primitiveguids/data") {
            let records = |bytes: &[u8]| -> std::collections::BTreeSet<Vec<u8>> {
                bytes.chunks_exact(24).map(<[u8]>::to_vec).collect()
            };
            let g = stream_bytes(&src, g_path).expect("walked stream exists");
            match after.get(canonical).and_then(|p| stream_bytes(&out, p)) {
                Some(o) if records(&o) == records(&g) => {}
                Some(o) => failures.push(format!(
                    "{canonical} records differ: {} golden, {} ours",
                    g.len() / 24,
                    o.len() / 24
                )),
                None => failures.push(format!("{canonical} was not written back")),
            }
        }
    }

    // 5. The ultimate bar, reached 2026-08-16: every footprint's Data stream
    //    byte-identical through the round trip. The parameter- and
    //    record-level comparisons above localise a failure; this catches
    //    everything else — template drift, invented identities, padding
    //    rewrites. PcbLib has no per-save volatile bytes inside component
    //    Data (unlike SchLib's regenerated UniqueIDs), so exact equality is
    //    the honest requirement.
    for (canonical, g_path) in &before {
        let Some(name) = canonical
            .strip_suffix("/data")
            .filter(|n| !n.contains('/') && *n != "library")
        else {
            continue;
        };
        if is_known(canonical) {
            continue;
        }
        let (Some(g), Some(o)) = (
            stream_bytes(&src, g_path),
            after.get(canonical).and_then(|p| stream_bytes(&out, p)),
        ) else {
            continue;
        };
        if g != o {
            let first = g
                .iter()
                .zip(o.iter())
                .position(|(a, b)| a != b)
                .unwrap_or_else(|| g.len().min(o.len()));
            failures.push(format!(
                "{name}: Data stream not byte-identical (lens {}/{}, first divergence at {first:#x})",
                g.len(),
                o.len()
            ));
        }
    }

    // 6. Every footprint's WideStrings stream byte-identical: it carries the
    //    text the Windows-1252 Data block cannot (as UTF-16 code units), and
    //    the reader prefers it, so a wrong encoding here corrupts every
    //    non-ASCII text a character further on each save while the Data
    //    stream above stays byte-identical.
    for (canonical, g_path) in &before {
        let Some(name) = canonical
            .strip_suffix("/widestrings")
            .filter(|n| !n.contains('/'))
        else {
            continue;
        };
        if is_known(canonical) {
            continue;
        }
        let g = stream_bytes(&src, g_path).expect("walked stream exists");
        match after.get(canonical).and_then(|p| stream_bytes(&out, p)) {
            Some(o) if o == g => {}
            Some(o) => failures.push(format!(
                "{name}: WideStrings not byte-identical: golden {:?}, ours {:?}",
                String::from_utf8_lossy(&g),
                String::from_utf8_lossy(&o)
            )),
            None => failures.push(format!("{name}: WideStrings not written back")),
        }
    }

    assert!(
        failures.is_empty(),
        "the golden does not survive a read/write cycle intact ({} divergence(s)).\n\
         Each line is a field Altium stored that we changed or lost. Fix the \
         reader/writer, or add a reasoned entry to KNOWN_DEFECTS.\n\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Two saves of the same `PcbLib` are byte-identical, and so is a save of
/// our own output: nothing varies per save and nothing moves.
#[test]
fn pcblib_saves_are_deterministic() {
    let src = sample("footprints.PcbLib");
    let dir = tempfile::tempdir().expect("tempdir");
    let (a, b, c) = (
        dir.path().join("a.PcbLib"),
        dir.path().join("b.PcbLib"),
        dir.path().join("c.PcbLib"),
    );
    PcbLib::open(&src).expect("read").save(&a).expect("save a");
    PcbLib::open(&src).expect("read").save(&b).expect("save b");
    PcbLib::open(&a).expect("reopen").save(&c).expect("save c");
    // /Library/Data names this save — the path written, the date and time
    // — in three keys Altium rewrites on every save too; everything else in
    // it must be the same.
    let masked = |bytes: &[u8]| -> Vec<u8> {
        let text = String::from_utf8_lossy(bytes).into_owned();
        let mut out = Vec::new();
        for (i, segment) in text.split('|').enumerate() {
            if i > 0 {
                out.push(b'|');
            }
            let key = segment.split('=').next().unwrap_or("");
            if ["FILENAME", "DATE", "TIME"].contains(&key) {
                out.extend_from_slice(format!("{key}=*").as_bytes());
            } else {
                out.extend_from_slice(segment.as_bytes());
            }
        }
        out
    };
    let sa = stream_map(&a);
    for (other, label) in [
        (&b, "two saves of the same library"),
        (&c, "our own output re-saved"),
    ] {
        let so = stream_map(other);
        assert_eq!(
            sa.keys().collect::<Vec<_>>(),
            so.keys().collect::<Vec<_>>(),
            "{label}"
        );
        for (canonical, path) in &sa {
            let (mut ours, mut theirs) = (
                stream_bytes(&a, path).unwrap_or_default(),
                stream_bytes(other, &so[canonical]).unwrap_or_default(),
            );
            if canonical == "library/data" {
                (ours, theirs) = (masked(&ours[4..]), masked(&theirs[4..]));
            }
            if ours != theirs {
                let first = ours
                    .iter()
                    .zip(theirs.iter())
                    .position(|(p, q)| p != q)
                    .unwrap_or_else(|| ours.len().min(theirs.len()));
                panic!(
                    "{canonical} differs ({label}; lens {}/{}, at {first:#x}): {:?} vs {:?}",
                    ours.len(),
                    theirs.len(),
                    &ours[first.saturating_sub(24)..(first + 24).min(ours.len())],
                    &theirs[first.saturating_sub(24)..(first + 24).min(theirs.len())]
                );
            }
        }
    }
}

/// Two saves of the same `SchLib` are byte-identical: no record gains an
/// identity the file never gave it (a pie is stored without a `UniqueID`
/// and must stay that way), and nothing else varies per save. A failure
/// here names a record that changes on every write — the kind of drift a
/// version-controlled library shows as a phantom diff.
#[test]
fn schlib_saves_are_deterministic() {
    let src = sample("symbols.SchLib");
    let dir = tempfile::tempdir().expect("tempdir");
    let (a, b) = (dir.path().join("a.SchLib"), dir.path().join("b.SchLib"));
    SchLib::open(&src).expect("read").save(&a).expect("save a");
    SchLib::open(&src).expect("read").save(&b).expect("save b");
    let (sa, sb) = (stream_map(&a), stream_map(&b));
    assert_eq!(sa.keys().collect::<Vec<_>>(), sb.keys().collect::<Vec<_>>());
    for (canonical, path) in &sa {
        let (x, y) = (stream_bytes(&a, path), stream_bytes(&b, &sb[canonical]));
        assert!(
            x == y,
            "{canonical} differs between two saves of the same library"
        );
    }
    // And the second generation: a save of our own output re-saved.
    let c = dir.path().join("c.SchLib");
    SchLib::open(&a).expect("reopen").save(&c).expect("save c");
    let sc = stream_map(&c);
    for (canonical, path) in &sa {
        let (ours, theirs) = (stream_bytes(&a, path), stream_bytes(&c, &sc[canonical]));
        if ours != theirs {
            let (ours, theirs) = (ours.unwrap_or_default(), theirs.unwrap_or_default());
            let first = ours
                .iter()
                .zip(theirs.iter())
                .position(|(p, q)| p != q)
                .unwrap_or_else(|| ours.len().min(theirs.len()));
            let window = |v: &[u8]| {
                String::from_utf8_lossy(&v[first.saturating_sub(40)..(first + 40).min(v.len())])
                    .into_owned()
            };
            panic!(
                "{canonical} differs once our own output is re-saved (lens {}/{}, at {first:#x}):\n  a: {}\n  c: {}",
                ours.len(),
                theirs.len(),
                window(&ours),
                window(&theirs)
            );
        }
    }
}

/// Every binary (pin) record of a `SchLib` `Data` stream, bytes included,
/// in stream order.
fn binary_records(data: &[u8]) -> Vec<Vec<u8>> {
    let mut records = Vec::new();
    let mut offset = 0;
    while offset + 4 <= data.len() {
        let len = usize::from(data[offset])
            | usize::from(data[offset + 1]) << 8
            | usize::from(data[offset + 2]) << 16;
        let flags = data[offset + 3];
        let end = (offset + 4 + len).min(data.len());
        if flags == 1 {
            records.push(data[offset + 4..end].to_vec());
        }
        offset = end;
    }
    records
}

/// The kind of every record in a `SchLib` `Data` stream, in stream order.
///
/// Framing is `[len: 3 bytes LE][flags: 1]` then the payload; `flags == 1`
/// marks the binary pin record, which has no `RECORD=` key of its own.
fn record_kinds(data: &[u8]) -> Vec<String> {
    let mut kinds = Vec::new();
    let mut offset = 0;
    while offset + 4 <= data.len() {
        let len = usize::from(data[offset])
            | usize::from(data[offset + 1]) << 8
            | usize::from(data[offset + 2]) << 16;
        let flags = data[offset + 3];
        offset += 4;
        let Some(payload) = data.get(offset..offset + len) else {
            break;
        };
        offset += len;
        if flags == 1 {
            kinds.push("pin".to_string());
            continue;
        }
        let text: String = payload.iter().map(|&b| b as char).collect();
        let kind = text
            .split('|')
            .find_map(|p| p.strip_prefix("RECORD="))
            .unwrap_or("?");
        kinds.push(kind.to_string());
    }
    kinds
}

#[test]
fn schlib_golden_survives_a_round_trip() {
    let src = sample("symbols.SchLib");
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("rewritten.SchLib");
    let lib = SchLib::open(&src).expect("read golden SchLib");
    lib.save(&out).expect("write it back");

    let mut failures = Vec::new();

    let (before, after) = (stream_map(&src), stream_map(&out));
    for missing in before.keys().filter(|k| !after.contains_key(*k)) {
        if !is_known(missing) {
            failures.push(format!("stream dropped entirely: {missing}"));
        }
    }

    for (canonical, g_path) in &before {
        let Some(name) = canonical
            .strip_suffix("/data")
            .filter(|name| !name.contains('/'))
        else {
            continue;
        };
        let Some(o_path) = after.get(canonical) else {
            continue; // already reported as dropped
        };
        let (Some(g), Some(o)) = (stream_bytes(&src, g_path), stream_bytes(&out, o_path)) else {
            continue;
        };
        // Excused by the divergence's own text or by the component, like the
        // two checks below: a damaged fixture's storage now pairs with ours
        // (its storage name is kept on rewrite), so its inconsistent record
        // shows up here rather than as an unmatched storage.
        failures.extend(
            block_divergences(&g, &o, name)
                .into_iter()
                .filter(|d| !is_known(d) && !is_known(name)),
        );

        // The record sequence itself. `IndexInSheet` is one shared counter over
        // the content records in stream order, so the values only line up if
        // the order does — and a block-level diff pairs records by content, not
        // by position, so it cannot see a reordering on its own.
        let (gk, ok) = (record_kinds(&g), record_kinds(&o));
        if gk != ok && !is_known(name) {
            failures.push(format!(
                "{name}: record order changed\n  golden: {}\n  ours:   {}",
                gk.join(" "),
                ok.join(" ")
            ));
        }
        // The binary pin records are not parameter blocks, so the passes
        // above never see them: every one must come back byte for byte (a
        // non-ASCII name is stored as UTF-8 bytes, not the code page).
        let (gp, op) = (binary_records(&g), binary_records(&o));
        if gp != op && !is_known(name) {
            let first = gp
                .iter()
                .zip(op.iter())
                .position(|(a, b)| a != b)
                .map_or_else(|| "count".to_string(), |i| format!("pin {i}"));
            failures.push(format!(
                "{name}: binary pin records differ ({} golden, {} ours, first at {first})",
                gp.len(),
                op.len()
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "the golden does not survive a read/write cycle intact ({} divergence(s)).\n\
         Each line is a field Altium stored that we changed or lost. Fix the \
         reader/writer, or add a reasoned entry to KNOWN_DEFECTS.\n\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Sweeps every library of a corpus — real, hand-authored Altium libraries
/// the scripted golden cannot stand in for — through a read/write cycle and
/// reports what changed: streams dropped, parameter blocks that diverge,
/// records reordered, and for a `PcbLib` every `Data` and `WideStrings`
/// stream that is not byte-identical. The corpus is the sibling
/// `altium-designer-pcb-libraries` checkout unless `ALTIUM_CORPUS_DIR` points
/// elsewhere at build time (a compile-time constant, not a runtime path);
/// ignored by default and never writes into the corpus.
///
/// A library Altium did not author is normalised rather than reproduced —
/// the sibling corpus's `test_component.*` pair (a third-party writer's
/// explicit defaults, trailing pipes and an `OwnerIndex=0` footprint link)
/// is reported as a divergence by design, so a clean sweep is "every
/// Altium-authored library OK", not an empty report.
///
/// ```text
/// ALTIUM_CORPUS_DIR=../my-libraries cargo test --test golden_fidelity corpus -- --ignored --nocapture
/// ```
#[test]
#[ignore = "sweeps a corpus outside the repository"]
#[allow(clippy::too_many_lines)] // one straight-line sweep, reported per library
fn corpus_survives_a_round_trip() {
    const DEFAULT_CORPUS: &str = "../altium-designer-pcb-libraries";
    let corpus = PathBuf::from(option_env!("ALTIUM_CORPUS_DIR").unwrap_or(DEFAULT_CORPUS));
    if !corpus.is_dir() {
        eprintln!("no corpus at {}; nothing to sweep", corpus.display());
        return;
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let mut libraries: Vec<PathBuf> = std::fs::read_dir(&corpus)
        .expect("read corpus dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                e.eq_ignore_ascii_case("pcblib") || e.eq_ignore_ascii_case("schlib")
            })
        })
        .collect();
    libraries.sort();
    assert!(
        !libraries.is_empty(),
        "no libraries under {}",
        corpus.display()
    );

    let mut report: Vec<String> = Vec::new();
    for src in &libraries {
        let file = src.file_name().unwrap().to_string_lossy().into_owned();
        let is_pcb = file.to_ascii_lowercase().ends_with(".pcblib");
        let out = dir.path().join(&file);
        let opened = if is_pcb {
            PcbLib::open(src).map(|mut lib| lib.save(&out))
        } else {
            SchLib::open(src).map(|lib| lib.save(&out))
        };
        match opened {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                report.push(format!("{file}: save failed: {e}"));
                continue;
            }
            Err(e) => {
                report.push(format!("{file}: open failed: {e}"));
                continue;
            }
        }
        let (before, after) = (stream_map(src), stream_map(&out));
        let mut lines = Vec::new();
        for missing in before.keys().filter(|k| !after.contains_key(*k)) {
            if !is_known(missing) {
                lines.push(format!("stream dropped: {missing}"));
            }
        }
        for (canonical, g_path) in &before {
            let Some(o_path) = after.get(canonical) else {
                continue;
            };
            let (Some(g), Some(o)) = (stream_bytes(src, g_path), stream_bytes(&out, o_path)) else {
                continue;
            };
            let component = canonical.split('/').next().unwrap_or("");
            if component == "library" || is_known(canonical) {
                continue;
            }
            if canonical.ends_with("/data") && !canonical.contains("primitiveguids") {
                if is_pcb && !canonical.contains("uniqueidprimitiveinformation") && g != o {
                    let first = g
                        .iter()
                        .zip(o.iter())
                        .position(|(a, b)| a != b)
                        .unwrap_or_else(|| g.len().min(o.len()));
                    lines.push(format!(
                        "{canonical}: not byte-identical (lens {}/{}, first divergence at {first:#x})",
                        g.len(),
                        o.len()
                    ));
                }
                lines.extend(block_divergences(&g, &o, canonical));
                if !is_pcb {
                    let (gk, ok) = (record_kinds(&g), record_kinds(&o));
                    if gk != ok {
                        lines.push(format!("{canonical}: record order changed"));
                    }
                }
            } else if canonical.ends_with("/widestrings") && g != o {
                lines.push(format!(
                    "{canonical}: WideStrings differ: {:?} vs {:?}",
                    String::from_utf8_lossy(&g),
                    String::from_utf8_lossy(&o)
                ));
            }
        }
        let n = lines.len();
        lines.sort();
        lines.dedup();
        if lines.is_empty() {
            eprintln!("OK   {file}");
        } else {
            eprintln!("DIFF {file}: {n} divergence(s)");
            for line in &lines {
                eprintln!("     {line}");
            }
            report.push(format!("{file}: {n} divergence(s)"));
        }
    }
    assert!(report.is_empty(), "corpus sweep:\n{}", report.join("\n"));
}
