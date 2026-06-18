//! Performance baseline tests.
//!
//! These establish generous timing baselines for the write/read and
//! compression paths so a future change that makes them dramatically slower
//! (e.g. an accidental quadratic in the OLE writer or STEP compression) is
//! caught. They use simple timing rather than statistical benchmarking to
//! avoid heavy dependencies; thresholds are intentionally loose to stay
//! non-flaky on shared CI runners.
//!
//! Run with: `cargo test --test perf_tests -- --nocapture`

#![allow(clippy::cast_possible_truncation)] // Test file, iterations fit in u32
#![allow(clippy::cast_precision_loss)] // Acceptable for timing display

use std::time::{Duration, Instant};

use altium_designer_mcp::altium::pcblib::{Footprint, Pad, PcbLib};

/// Runs a closure `iterations` times and returns the average duration.
fn measure_avg<F: FnMut()>(iterations: usize, mut f: F) -> Duration {
    let start = Instant::now();
    for _ in 0..iterations {
        f();
    }
    start.elapsed() / iterations as u32
}

/// Formats a duration in a human-readable way.
fn format_duration(d: Duration) -> String {
    if d.as_nanos() < 1000 {
        format!("{}ns", d.as_nanos())
    } else if d.as_micros() < 1000 {
        format!("{:.2}µs", d.as_nanos() as f64 / 1000.0)
    } else if d.as_millis() < 1000 {
        format!("{:.2}ms", d.as_micros() as f64 / 1000.0)
    } else {
        format!("{:.2}s", d.as_millis() as f64 / 1000.0)
    }
}

/// Builds a `PcbLib` with `n` simple two-pad footprints.
fn build_library(n: usize) -> PcbLib {
    let mut lib = PcbLib::new();
    for i in 0..n {
        let mut fp = Footprint::new(format!("FP_{i}"));
        fp.add_pad(Pad::smd("1", -0.5, 0.0, 0.6, 0.5));
        fp.add_pad(Pad::smd("2", 0.5, 0.0, 0.6, 0.5));
        lib.add(fp);
    }
    lib
}

#[test]
fn perf_pcblib_save_100_footprints() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("perf.PcbLib");
    let mut lib = build_library(100);

    let avg = measure_avg(10, || {
        lib.save(&path).expect("save");
    });
    println!(
        "PcbLib save (100 footprints): {} per op",
        format_duration(avg)
    );
    assert!(avg < Duration::from_secs(2), "save regressed: {avg:?}");
}

#[test]
fn perf_pcblib_open_100_footprints() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("perf.PcbLib");
    build_library(100).save(&path).expect("save");

    let avg = measure_avg(10, || {
        let _ = PcbLib::open(&path).expect("open");
    });
    println!(
        "PcbLib open (100 footprints): {} per op",
        format_duration(avg)
    );
    assert!(avg < Duration::from_secs(2), "open regressed: {avg:?}");
}

#[test]
fn perf_flate2_roundtrip_1mb() {
    use flate2::read::ZlibDecoder;
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::{Read, Write};

    // STEP models are stored zlib-compressed inside .PcbLib files; this mirrors
    // that compress/decompress path on ~1MB of data.
    let data = vec![0x5Au8; 1024 * 1024];

    let avg = measure_avg(10, || {
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(&data).expect("compress");
        let compressed = enc.finish().expect("finish");

        let mut dec = ZlibDecoder::new(&compressed[..]);
        let mut out = Vec::new();
        dec.read_to_end(&mut out).expect("decompress");
        assert_eq!(out.len(), data.len());
    });
    println!("flate2 1MB round-trip: {} per op", format_duration(avg));
    assert!(
        avg < Duration::from_secs(5),
        "compression regressed: {avg:?}"
    );
}
