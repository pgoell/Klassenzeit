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
        body.contains("| lahc_kempe |"),
        "missing lahc_kempe row in objectives section (between table cell delimiters): {body}",
    );
    assert!(
        body.contains(
            "class_gap, teacher_gap, class_day_balance, home_room, prefer_early, avoid_first, avoid_last, prefer_late"
        ),
        "missing lahc-family optimised set in objectives section (item 52: lahc accepts on full canonical): {body}",
    );
    assert!(
        body.contains("Klassenlehrer share (median)"),
        "missing klassenlehrer-share header: {body}"
    );
    assert!(
        body.contains("Quality (pass / 5)"),
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

#[test]
fn supervisor_does_not_emit_unpinned_h2_when_not_appending() {
    let out = unique_outfile("unpinned-no-append");
    // Use --backends lahc and --fixtures grundschule. The LAHC cell may
    // panic on the unpinned path; the assertion below only checks the
    // markdown shape, which is unaffected by per-cell panics.
    let _status = Command::new(env!("CARGO_BIN_EXE_solver-bench"))
        .args([
            "--budget",
            "200ms",
            "--seeds",
            "1",
            "--fixtures",
            "grundschule",
            "--backends",
            "lahc",
            "--teacher-pins",
            "off",
            "--out",
            out.to_str().expect("path utf-8"),
        ])
        .status()
        .expect("spawn supervisor");
    // Do NOT assert status.success(): the cell panics under unpinned grundschule
    // and the supervisor exits FAILURE when zero cells succeed. The artifact
    // we care about is the markdown shape.
    let body = std::fs::read_to_string(&out).expect("read markdown output");
    assert!(
        !body.contains("## Unpinned variant"),
        "non-append unpinned mode must not emit the Unpinned variant H2: {body}"
    );
    let _ = std::fs::remove_file(&out);
}

#[test]
fn supervisor_appends_unpinned_variant_h2_when_teacher_pins_off_and_append() {
    let out = unique_outfile("unpinned-append");
    // First pass: write the canonical pinned table.
    let s1 = Command::new(env!("CARGO_BIN_EXE_solver-bench"))
        .args([
            "--budget",
            "200ms",
            "--seeds",
            "1",
            "--fixtures",
            "grundschule",
            "--backends",
            "lahc",
            "--teacher-pins",
            "on",
            "--out",
            out.to_str().expect("path utf-8"),
        ])
        .status()
        .expect("spawn supervisor pinned");
    assert!(s1.success(), "pinned pass exited non-zero");
    // Second pass: append the unpinned variant. Cell may panic; we only
    // care about the H2 + preamble being written.
    let _s2 = Command::new(env!("CARGO_BIN_EXE_solver-bench"))
        .args([
            "--budget",
            "200ms",
            "--seeds",
            "1",
            "--fixtures",
            "grundschule",
            "--backends",
            "lahc",
            "--teacher-pins",
            "off",
            "--append",
            "--out",
            out.to_str().expect("path utf-8"),
        ])
        .status()
        .expect("spawn supervisor unpinned-append");
    let body = std::fs::read_to_string(&out).expect("read markdown output");
    assert!(
        body.contains("## Unpinned variant"),
        "append-mode unpinned must emit the Unpinned variant H2: {body}"
    );
    assert!(
        body.contains("teacher_pin = None"),
        "append-mode unpinned preamble must mention teacher_pin = None: {body}"
    );
    assert!(
        body.contains("ADR 0036"),
        "append-mode unpinned preamble must reference ADR 0036: {body}"
    );
    let _ = std::fs::remove_file(&out);
}

#[test]
fn supervisor_renders_kempe_chain_column_in_sweep_mode() {
    let out = unique_outfile("kempe-sweep");
    let status = Command::new(env!("CARGO_BIN_EXE_solver-bench"))
        .args([
            "--budget",
            "1s",
            "--seeds",
            "1",
            "--fixtures",
            "grundschule",
            "--backends",
            "lahc_kempe",
            "--kempe-max-chain",
            "4,8",
            "--out",
            out.to_str().expect("path utf-8"),
        ])
        .status()
        .expect("solver-bench runs");
    assert!(status.success(), "solver-bench exit was non-zero");

    let body = std::fs::read_to_string(&out).expect("output file");
    assert!(
        body.contains("| Kempe Chain |"),
        "expected `| Kempe Chain |` column header in output:\n{body}",
    );
    assert!(
        body.contains("| 4 |") && body.contains("| 8 |"),
        "expected both depth cells in output:\n{body}",
    );
    let _ = std::fs::remove_file(&out);
}

#[test]
fn supervisor_runs_cells_concurrently_when_jobs_is_greater_than_one() {
    // RED gate: under serial execution, every "cell done:" line precedes the
    // next "cell start:" line. Under --jobs 4 the first "cell done:" line is
    // preceded by at least 2 "cell start:" lines.
    let out = unique_outfile("jobs-stderr");
    let child = Command::new(env!("CARGO_BIN_EXE_solver-bench"))
        .args([
            "--budget",
            "500ms",
            "--seeds",
            "1",
            "--fixtures",
            "grundschule,zweizuegig",
            "--backends",
            "lahc",
            "--jobs",
            "4",
            "--out",
            out.to_str().expect("path utf-8"),
        ])
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("spawn supervisor");
    let output = child.wait_with_output().expect("wait supervisor");
    assert!(output.status.success(), "supervisor exited non-zero");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let mut starts_before_first_done = 0usize;
    let mut seen_done = false;
    for line in stderr.lines() {
        if line.starts_with("cell start:") && !seen_done {
            starts_before_first_done += 1;
        }
        if line.starts_with("cell done:") {
            seen_done = true;
        }
    }
    assert!(
        starts_before_first_done >= 2,
        "expected >= 2 'cell start:' lines before the first 'cell done:' under --jobs 4; saw {starts_before_first_done}. stderr:\n{stderr}",
    );
    let _ = std::fs::remove_file(&out);
}

#[test]
fn supervisor_parallel_and_serial_agree_on_row_order_and_body() {
    let serial_out = unique_outfile("agree-serial");
    let parallel_out = unique_outfile("agree-parallel");

    let common = [
        "--budget",
        "200ms",
        "--seeds",
        "2",
        "--fixtures",
        "grundschule,zweizuegig",
    ];

    let s_status = Command::new(env!("CARGO_BIN_EXE_solver-bench"))
        .args(common)
        .args([
            "--jobs",
            "1",
            "--out",
            serial_out.to_str().expect("path utf-8"),
        ])
        .status()
        .expect("spawn serial supervisor");
    assert!(s_status.success(), "serial supervisor exited non-zero");

    let p_status = Command::new(env!("CARGO_BIN_EXE_solver-bench"))
        .args(common)
        .args([
            "--jobs",
            "4",
            "--out",
            parallel_out.to_str().expect("path utf-8"),
        ])
        .status()
        .expect("spawn parallel supervisor");
    assert!(p_status.success(), "parallel supervisor exited non-zero");

    let serial_body = std::fs::read_to_string(&serial_out).expect("read serial");
    let parallel_body = std::fs::read_to_string(&parallel_out).expect("read parallel");

    let extract_fixture_order = |body: &str| -> Vec<String> {
        body.lines()
            .filter(|l| l.starts_with("| grundschule") || l.starts_with("| zweizuegig"))
            .map(|l| l.split('|').nth(1).map(str::trim).unwrap_or("").to_string())
            .collect()
    };
    assert_eq!(
        extract_fixture_order(&serial_body),
        extract_fixture_order(&parallel_body),
        "row order differs between serial and parallel runs",
    );

    let strip_timing = |body: &str| -> String {
        body.lines()
            .map(|l| {
                if l.starts_with('|') {
                    l.split('|')
                        .map(|cell| {
                            let trimmed = cell.trim();
                            if trimmed.is_empty() {
                                cell.to_string()
                            } else if trimmed
                                .chars()
                                .all(|c| c.is_ascii_digit() || c == '.' || c == '-' || c == '/')
                            {
                                "<n>".to_string()
                            } else {
                                cell.to_string()
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("|")
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    assert_eq!(
        strip_timing(&serial_body),
        strip_timing(&parallel_body),
        "non-timing body differs between serial and parallel runs",
    );

    let _ = std::fs::remove_file(&serial_out);
    let _ = std::fs::remove_file(&parallel_out);
}

#[test]
fn supervisor_parallel_is_meaningfully_faster_than_serial() {
    // Skip on machines with too few cores; the test needs at least 4 cells
    // running in parallel to be reliable.
    let available = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    if available < 4 {
        eprintln!("skipping: needs >= 4 cores, have {available}");
        return;
    }

    let serial_out = unique_outfile("speed-serial");
    let parallel_out = unique_outfile("speed-parallel");

    let common = [
        "--budget",
        "1s",
        "--seeds",
        "1",
        "--fixtures",
        "grundschule,zweizuegig,dreizuegig,lock_in",
        "--backends",
        "lahc",
    ];

    let t0 = std::time::Instant::now();
    let s_status = Command::new(env!("CARGO_BIN_EXE_solver-bench"))
        .args(common)
        .args([
            "--jobs",
            "1",
            "--out",
            serial_out.to_str().expect("path utf-8"),
        ])
        .status()
        .expect("spawn serial supervisor");
    let serial_elapsed = t0.elapsed();
    assert!(s_status.success());

    // Bail if the serial run was too short for the speedup signal to dominate
    // process-spawn overhead and CI jitter.
    if serial_elapsed < std::time::Duration::from_secs(2) {
        eprintln!("skipping speedup assertion: serial run too short ({serial_elapsed:?})");
        let _ = std::fs::remove_file(&serial_out);
        return;
    }

    let t1 = std::time::Instant::now();
    let p_status = Command::new(env!("CARGO_BIN_EXE_solver-bench"))
        .args(common)
        .args([
            "--jobs",
            "4",
            "--out",
            parallel_out.to_str().expect("path utf-8"),
        ])
        .status()
        .expect("spawn parallel supervisor");
    let parallel_elapsed = t1.elapsed();
    assert!(p_status.success());

    // Conservative: parallel run should be at least 1.5x faster than serial.
    // With 4 cells, 1s budget each, --jobs 4 should run all four roughly
    // concurrently, dropping wall clock from ~4s to ~1s. The 1.5x bar
    // is loose to tolerate process-spawn overhead and CI jitter.
    assert!(
        parallel_elapsed.as_secs_f64() * 1.5 < serial_elapsed.as_secs_f64(),
        "parallel run was not meaningfully faster: serial={serial_elapsed:?} parallel={parallel_elapsed:?}",
    );

    let _ = std::fs::remove_file(&serial_out);
    let _ = std::fs::remove_file(&parallel_out);
}
