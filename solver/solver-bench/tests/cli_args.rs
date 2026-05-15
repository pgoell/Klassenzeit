//! End-to-end CLI tests for solver-bench's --rr-k, --rr-period, --backends flags.
//! Exec the supervisor binary; never invoke cpsat (uv venv isn't propagated by
//! `cargo nextest`, so cpsat seeds would `ModuleNotFoundError`).

use std::path::PathBuf;
use std::process::Command;

fn cli_args_outfile(label: &str) -> PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("kz-bench-cli-args-{label}-{nanos}.md"))
}

#[test]
fn cli_args_rr_k_and_rr_period_round_trip_into_table_row() {
    let out = cli_args_outfile("rr");
    let status = Command::new(env!("CARGO_BIN_EXE_solver-bench"))
        .args([
            "--budget",
            "1s",
            "--seeds",
            "1",
            "--fixtures",
            "grundschule",
            "--backends",
            "lahc_rr",
            "--rr-k",
            "8",
            "--rr-period",
            "50",
            "--out",
            out.to_str().expect("path utf-8"),
        ])
        .status()
        .expect("spawn supervisor");
    assert!(status.success(), "supervisor exited non-zero");

    let body = std::fs::read_to_string(&out).expect("read markdown output");
    assert!(
        body.contains("| lahc_rr |"),
        "table missing lahc_rr row delimiter:\n{body}"
    );
    assert!(
        body.contains("| 8 | 50 |"),
        "row missing K=8 period=50 columns:\n{body}"
    );
    let _ = std::fs::remove_file(&out);
}

#[test]
fn cli_args_non_rr_backend_renders_dash_sentinel_in_sweep_mode() {
    let out = cli_args_outfile("dash");
    let status = Command::new(env!("CARGO_BIN_EXE_solver-bench"))
        .args([
            "--budget",
            "1s",
            "--seeds",
            "1",
            "--fixtures",
            "grundschule",
            "--backends",
            "lahc",
            "--rr-k",
            "3,5",
            "--rr-period",
            "10,25",
            "--out",
            out.to_str().expect("path utf-8"),
        ])
        .status()
        .expect("spawn supervisor");
    assert!(status.success(), "supervisor exited non-zero");

    let body = std::fs::read_to_string(&out).expect("read markdown output");
    let lahc_dash_count = body
        .lines()
        .filter(|l| l.contains("| lahc | - | - |"))
        .count();
    assert_eq!(
        lahc_dash_count, 1,
        "lahc renders exactly one row with - sentinels:\n{body}"
    );
    let _ = std::fs::remove_file(&out);
}

#[test]
fn cli_args_sweep_mode_appends_pareto_and_recommendation() {
    let out = cli_args_outfile("sweep");
    let status = Command::new(env!("CARGO_BIN_EXE_solver-bench"))
        .args([
            "--budget",
            "1s",
            "--seeds",
            "1",
            "--fixtures",
            "grundschule",
            "--backends",
            "lahc_rr",
            "--rr-k",
            "5",
            "--rr-period",
            "25",
            "--out",
            out.to_str().expect("path utf-8"),
        ])
        .status()
        .expect("spawn supervisor");
    assert!(status.success(), "supervisor exited non-zero");

    let body = std::fs::read_to_string(&out).expect("read markdown output");
    assert!(
        body.contains("## Pareto frontier"),
        "missing Pareto frontier section in sweep mode:\n{body}"
    );
    assert!(
        body.contains("## Recommendation"),
        "missing Recommendation section in sweep mode:\n{body}"
    );
    let _ = std::fs::remove_file(&out);
}

#[test]
fn jobs_flag_accepts_positive_integer() {
    let out = cli_args_outfile("jobs-accepts");
    let status = Command::new(env!("CARGO_BIN_EXE_solver-bench"))
        .args([
            "--budget",
            "200ms",
            "--seeds",
            "1",
            "--fixtures",
            "grundschule",
            "--backends",
            "lahc",
            "--jobs",
            "4",
            "--out",
            out.to_str().expect("path utf-8"),
        ])
        .status()
        .expect("spawn supervisor");
    assert!(status.success(), "supervisor failed with --jobs 4");
    let _ = std::fs::remove_file(&out);
}

#[test]
fn jobs_flag_rejects_zero() {
    let out = cli_args_outfile("jobs-zero");
    let output = Command::new(env!("CARGO_BIN_EXE_solver-bench"))
        .args([
            "--budget",
            "200ms",
            "--seeds",
            "1",
            "--fixtures",
            "grundschule",
            "--jobs",
            "0",
            "--out",
            out.to_str().expect("path utf-8"),
        ])
        .output()
        .expect("spawn supervisor");
    assert!(
        !output.status.success(),
        "expected nonzero exit on --jobs 0"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--jobs"),
        "stderr should mention --jobs: {stderr}"
    );
    let _ = std::fs::remove_file(&out);
}

#[test]
fn jobs_flag_rejects_non_integer() {
    let out = cli_args_outfile("jobs-non-integer");
    let status = Command::new(env!("CARGO_BIN_EXE_solver-bench"))
        .args([
            "--budget",
            "200ms",
            "--seeds",
            "1",
            "--fixtures",
            "grundschule",
            "--jobs",
            "abc",
            "--out",
            out.to_str().expect("path utf-8"),
        ])
        .status()
        .expect("spawn supervisor");
    assert!(!status.success(), "expected nonzero exit on --jobs abc");
    let _ = std::fs::remove_file(&out);
}

#[test]
fn jobs_flag_rejects_missing_value() {
    let status = Command::new(env!("CARGO_BIN_EXE_solver-bench"))
        .arg("--jobs")
        .status()
        .expect("spawn supervisor");
    assert!(
        !status.success(),
        "expected nonzero exit on --jobs without value"
    );
}
