//! Criterion benches for the FFD greedy solver across a fixture matrix.
//!
//! Three fixture builders live in this file: `grundschule_fixture`
//! (45 placements, einzügige Grundschule), `zweizuegig_fixture`
//! (196 placements, two-Zug Grundschule), and `dreizuegig_fixture`
//! (294 placements, three-Zug Grundschule with cross-class Religion
//! trios). Each is a hand-coded mirror of a Python seed in
//! `backend/src/klassenzeit_backend/seed/demo_*.py`; drift is caught by
//! `assert_eq!(lessons.len(), N)` against literals shared with the
//! matching Python solvability test. A `gesamtschule_fixture` is tracked
//! under `docs/OPEN_THINGS.md` "Acknowledged deferrals".
//!
//! All three fixtures iterate subjects in the natural authoring order; FFD
//! ordering inside `solve_with_config` sorts lessons by eligibility before
//! placement so the global solve succeeds regardless of input permutation.
//!
//! Output contract: after `group.finish()` we print a tab-separated block
//! fenced by `---SOLVER-BENCH-BASELINE---` / `---END---` to stderr.
//! `scripts/record_solver_bench.sh` depends on those markers, not on
//! criterion's default output format.
//!
//! The percentile helper lives in `percentile.rs` alongside its unit tests;
//! `tests/bench_percentile.rs` pulls it in via `#[path]` so libtest can
//! discover the tests (a `harness = false` bench binary cannot).

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use criterion::{criterion_group, criterion_main, Criterion, SamplingMode};
use solver_core::{
    ids::{RoomId, TimeBlockId},
    solve_with_config,
    test_fixtures::{dreizuegig_fixture, grundschule_fixture, zweizuegig_fixture},
    types::{Problem, SolveConfig, PRODUCTION_ACTIVE_WEIGHTS},
};

#[path = "percentile.rs"]
mod percentile;
use percentile::compute_percentiles;

const GREEDY_SAMPLE_COUNT: usize = 200;
/// LAHC samples are wall-clock-bound by `LAHC_DEADLINE`, so each sample
/// costs ~200 ms. Drop sample count to keep `mise run bench` runtime sane
/// while still computing meaningful percentile bands.
const LAHC_SAMPLE_COUNT: usize = 30;
const LAHC_DEADLINE: Duration = Duration::from_millis(200);
const LAHC_SEED: u64 = 42;

fn bench_greedy_cfg() -> SolveConfig {
    SolveConfig {
        weights: PRODUCTION_ACTIVE_WEIGHTS,
        ..SolveConfig::default()
    }
}

fn bench_lahc_cfg() -> SolveConfig {
    SolveConfig {
        weights: PRODUCTION_ACTIVE_WEIGHTS,
        deadline: Some(LAHC_DEADLINE),
        seed: LAHC_SEED,
        ..SolveConfig::default()
    }
}

/// (mode_name, sample_count, config_builder) tuple alias. The function-pointer
/// shape is unique to the bench and not worth factoring out beyond the alias.
type Mode = (&'static str, usize, fn() -> SolveConfig);

fn bench_fixtures(c: &mut Criterion) {
    let fixtures: [(&str, Problem); 3] = [
        ("grundschule", grundschule_fixture()),
        ("zweizuegig", zweizuegig_fixture()),
        ("dreizuegig", dreizuegig_fixture()),
    ];

    // LAHC config is built per sample so the benchmark cannot accidentally
    // share an RNG sequence across the timed iterations.
    let modes: [Mode; 2] = [
        ("greedy", GREEDY_SAMPLE_COUNT, bench_greedy_cfg),
        ("lahc", LAHC_SAMPLE_COUNT, bench_lahc_cfg),
    ];

    // Two-key map: (fixture_name, mode_name) -> samples / totals.
    type Key = (&'static str, &'static str);
    let samples_by_key: Mutex<HashMap<Key, Vec<Duration>>> = Mutex::new(HashMap::new());
    let totals_by_fixture: Mutex<HashMap<&'static str, u32>> = Mutex::new(HashMap::new());

    for (fixture_name, problem) in &fixtures {
        let expected_hours: u32 = problem
            .lessons
            .iter()
            .map(|l| u32::from(l.hours_per_week))
            .sum();
        totals_by_fixture
            .lock()
            .expect("totals mutex poisoned")
            .insert(*fixture_name, expected_hours);

        for (mode_name, sample_count, build_cfg) in &modes {
            let mut group = c.benchmark_group(format!("solver_{mode_name}"));
            group.sample_size(*sample_count);
            group.sampling_mode(SamplingMode::Flat);
            // LAHC's 200 ms deadline pushes per-sample wall-clock past
            // criterion's default 5 s warm-up + measurement target; setting
            // measurement_time well above sample_count * deadline lets it
            // fit without warning.
            if *mode_name == "lahc" {
                group.measurement_time(Duration::from_secs(20));
                group.warm_up_time(Duration::from_millis(200));
            }
            let cfg = build_cfg();
            group.bench_function(*fixture_name, |b| {
                b.iter_custom(|iters| {
                    let mut total = Duration::ZERO;
                    let mut local: Vec<Duration> = Vec::with_capacity(iters as usize);
                    for _ in 0..iters {
                        let start = Instant::now();
                        let solution = solve_with_config(problem, &cfg)
                            .expect("solve must succeed on the bench fixture");
                        let elapsed = start.elapsed();
                        total += elapsed;
                        local.push(elapsed);

                        assert!(solution.violations.is_empty());
                        assert_eq!(solution.placements.len() as u32, expected_hours);
                        let mut seen: HashSet<(RoomId, TimeBlockId)> = HashSet::new();
                        for pl in &solution.placements {
                            assert!(seen.insert((pl.room_id, pl.time_block_id)));
                        }
                    }
                    samples_by_key
                        .lock()
                        .expect("samples mutex poisoned")
                        .entry((*fixture_name, *mode_name))
                        .or_default()
                        .extend(local);
                    total
                });
            });
            group.finish();
        }
    }

    eprintln!("---SOLVER-BENCH-BASELINE---");
    eprint_bench_header();
    for (fixture_name, problem) in &fixtures {
        for (mode_name, sample_count, build_cfg) in &modes {
            let mut collected = samples_by_key
                .lock()
                .expect("samples mutex poisoned")
                .get(&(*fixture_name, *mode_name))
                .cloned()
                .expect("samples missing for fixture/mode");
            let total_samples = collected.len();
            assert!(
                total_samples >= *sample_count,
                "criterion produced fewer samples than requested for {fixture_name}/{mode_name}"
            );
            let (p1, p50, p99) = compute_percentiles(&mut collected);
            let mean = collected.iter().copied().sum::<Duration>() / total_samples as u32;
            let expected_hours = *totals_by_fixture
                .lock()
                .expect("totals mutex poisoned")
                .get(fixture_name)
                .expect("totals missing for fixture");
            let placements_per_sec = if mean.is_zero() {
                0
            } else {
                (f64::from(expected_hours) / mean.as_secs_f64()) as u64
            };
            // One extra solve outside the timing loop captures the soft_score
            // for the BASELINE row. Both modes are deterministic under their
            // configured (seed, max_iterations) pair so this matches what the
            // timed iterations produced; LAHC determinism additionally
            // depends on wall-clock for the deadline-only case, but with the
            // fixed LAHC_DEADLINE the iterations-per-sample variance stays
            // small enough that the soft score floors at the same value.
            let cfg = build_cfg();
            let solution =
                solve_with_config(problem, &cfg).expect("solve must succeed on the bench fixture");
            eprint_bench_row(
                fixture_name,
                mode_name,
                total_samples,
                p1,
                p50,
                p99,
                placements_per_sec,
                expected_hours,
                0,
                solution.soft_score,
            );
        }
    }
    eprintln!("---END---");
}

fn eprint_bench_header() {
    eprintln!(
        "fixture\tmode\tsamples\tp1_us\tp50_us\tp99_us\tplacements_per_sec\ttotal_placements\ttotal_hard_violations\tsoft_score"
    );
}

// Reason: every column is a named scalar; a wrapper struct would not improve clarity.
#[allow(clippy::too_many_arguments)]
fn eprint_bench_row(
    fixture: &str,
    mode: &str,
    samples: usize,
    p1: std::time::Duration,
    p50: std::time::Duration,
    p99: std::time::Duration,
    placements_per_sec: u64,
    total_placements: u32,
    hard_violations: u32,
    soft_score: u32,
) {
    eprintln!(
        "{fixture}\t{mode}\t{samples}\t{p1}\t{p50}\t{p99}\t{placements_per_sec}\t{total_placements}\t{hard_violations}\t{soft_score}",
        p1 = p1.as_micros(),
        p50 = p50.as_micros(),
        p99 = p99.as_micros(),
    );
}

criterion_group!(benches, bench_fixtures);
criterion_main!(benches);
