//! End-to-end smoke for the solver-bench supervisor + cell-child split.
//! Spawns the supervisor binary at a tiny budget/seeds count and asserts the
//! markdown output includes the three observability columns.

use std::path::PathBuf;
use std::process::Command;

fn unique_outfile(label: &str) -> PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("kz-bench-end-to-end-{label}-{nanos}.md"))
}

#[test]
fn supervisor_emits_observability_and_quality_columns() {
    let out = unique_outfile("columns");
    let status = Command::new(env!("CARGO_BIN_EXE_solver-bench"))
        .args([
            "--budget",
            "200ms",
            "--seeds",
            "1",
            "--fixtures",
            "grundschule",
            "--out",
            out.to_str().expect("path utf-8"),
        ])
        .status()
        .expect("spawn supervisor");
    assert!(status.success(), "supervisor exited non-zero");
    let body = std::fs::read_to_string(&out).expect("read markdown output");
    assert!(
        body.contains("Peak RSS (kB)"),
        "missing peak column header: {body}"
    );
    assert!(
        body.contains("Time to first feasible"),
        "missing ttf column header: {body}"
    );
    assert!(
        body.contains("Time to optimal"),
        "missing tto column header: {body}"
    );
    assert!(
        body.contains("Worst spread (median)"),
        "missing worst-spread header: {body}"
    );
    assert!(
        body.contains("Worst home-room ratio (median)"),
        "missing home-room header: {body}"
    );
    assert!(
        body.contains("Total interior gaps (median)"),
        "missing gaps header: {body}"
    );
    assert!(
        body.contains("Late-period ratio (median)"),
        "missing late-period header: {body}"
    );
    assert!(
        body.contains("## Backend objectives"),
        "missing Backend objectives section: {body}",
    );
    assert!(
        body.contains("lahc_rr_kempe"),
        "missing lahc_rr_kempe row in objectives section: {body}",
    );
    assert!(
        body.contains("class_gap, teacher_gap, prefer_early, avoid_first, avoid_last, prefer_late"),
        "missing lahc-family optimised set in objectives section: {body}",
    );
    assert!(
        body.contains("Quality (pass / 4)"),
        "missing quality column header: {body}"
    );
    assert!(
        body.contains("Class gap h (median)"),
        "missing class-gap-h header: {body}"
    );
    assert!(
        body.contains("Teacher gap h (median)"),
        "missing teacher-gap-h header: {body}"
    );
    assert!(
        body.contains("Home room miss (median)"),
        "missing home-room-miss header: {body}"
    );
    assert!(
        body.contains("Day balance (median)"),
        "missing day-balance header: {body}"
    );
    let _ = std::fs::remove_file(&out);
}
