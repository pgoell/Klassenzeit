//! Solver feasibility bake-off bench harness.
//!
//! Runs `solve_with_config` per `(fixture, backend, seed)` cell against the
//! production active-default `ConstraintWeights` and writes a markdown table
//! to BENCH_RESULTS.md.
//!
//! Spec: `docs/superpowers/specs/2026-05-04-solver-ffd-ordering-and-bench-design.md`.
//! Methodology: `docs/adr/0029-solver-feasibility-bake-off.md`.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use solver_core::solve_with_config;
use solver_core::test_fixtures::{
    dreizuegig_fixture, ffd_lock_in_grundschule, grundschule_fixture, zweizuegig_fixture,
};
use solver_core::types::{Problem, SolveConfig};
use solver_core::PRODUCTION_ACTIVE_WEIGHTS;

/// Total number of placements a fully solved problem must produce: one per (lesson, hour).
/// `placements.len() < placements_expected_for_problem(problem)` is a feasibility failure
/// even when `solution.violations` is empty (a placement drop during local search does
/// not grow the violation list). Bench harness predicate.
fn placements_expected_for_problem(problem: &Problem) -> u64 {
    problem
        .lessons
        .iter()
        .map(|l| l.hours_per_week as u64)
        .sum()
}

#[derive(Clone, Copy)]
enum BenchBackend {
    Lahc,
    LahcRr,
    LahcRrKempe,
    CpSat,
}

impl BenchBackend {
    fn label(self) -> &'static str {
        match self {
            BenchBackend::Lahc => "lahc",
            BenchBackend::LahcRr => "lahc_rr",
            BenchBackend::LahcRrKempe => "lahc_rr_kempe",
            BenchBackend::CpSat => "cpsat",
        }
    }
}

type FixtureEntry = (&'static str, fn() -> Problem);

const FIXTURES: &[FixtureEntry] = &[
    ("grundschule", grundschule_fixture),
    ("zweizuegig", zweizuegig_fixture),
    ("dreizuegig", dreizuegig_fixture),
    ("lock_in", ffd_lock_in_grundschule),
];

struct CliArgs {
    budget: Duration,
    seeds: u64,
    fixtures: Vec<String>,
    out: PathBuf,
}

fn default_args() -> CliArgs {
    CliArgs {
        budget: Duration::from_secs(60),
        seeds: 20,
        fixtures: FIXTURES.iter().map(|(n, _)| (*n).to_string()).collect(),
        out: PathBuf::from("solver/solver-core/benches/BENCH_RESULTS.md"),
    }
}

fn parse_duration(s: &str) -> Result<Duration, String> {
    if let Some(rest) = s.strip_suffix("ms") {
        rest.parse::<u64>()
            .map(Duration::from_millis)
            .map_err(|e| format!("invalid duration '{s}': {e}"))
    } else if let Some(rest) = s.strip_suffix('s') {
        rest.parse::<u64>()
            .map(Duration::from_secs)
            .map_err(|e| format!("invalid duration '{s}': {e}"))
    } else {
        Err(format!("invalid duration '{s}': expect '<n>s' or '<n>ms'"))
    }
}

fn parse_args(raw: Vec<String>) -> Result<CliArgs, String> {
    let mut args = default_args();
    let mut iter = raw.into_iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--budget" => {
                let value = iter.next().ok_or("--budget needs a value")?;
                args.budget = parse_duration(&value)?;
            }
            "--seeds" => {
                let value = iter.next().ok_or("--seeds needs a value")?;
                args.seeds = value
                    .parse::<u64>()
                    .map_err(|e| format!("--seeds must be a positive integer: {e}"))?;
            }
            "--fixtures" => {
                let value = iter.next().ok_or("--fixtures needs a value")?;
                args.fixtures = value.split(',').map(str::to_string).collect();
            }
            "--out" => {
                let value = iter.next().ok_or("--out needs a value")?;
                args.out = PathBuf::from(value);
            }
            other => return Err(format!("unknown flag '{other}'")),
        }
    }
    Ok(args)
}

fn main() -> ExitCode {
    let raw: Vec<String> = env::args().skip(1).collect();
    let args = match parse_args(raw) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("solver-bench: {e}");
            return ExitCode::from(2);
        }
    };

    let backends = [
        BenchBackend::Lahc,
        BenchBackend::LahcRr,
        BenchBackend::LahcRrKempe,
        BenchBackend::CpSat,
    ];
    let mut markdown = String::new();
    write_header(&mut markdown);

    for (name, build) in FIXTURES {
        if !args.fixtures.iter().any(|f| f == name) {
            continue;
        }
        let problem = build();
        let expected = placements_expected_for_problem(&problem);
        for backend in &backends {
            eprintln!("cell start: {} / {}", name, backend.label());
            let cell = run_cell(*backend, &problem, expected, args.budget, args.seeds);
            eprintln!(
                "cell done: {} / {} feasibility {}/{} hard_med={} placements_med={}/{} soft_med={} total_ms_med={:.0}",
                name,
                backend.label(),
                cell.feasibility_count,
                cell.seeds,
                cell.hard_violations_median,
                cell.placements_total_median,
                cell.placements_expected,
                cell.soft_score_median
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                cell.total_ms_median,
            );
            write_row(&mut markdown, name, *backend, &cell);
        }
    }

    write_footer(&mut markdown);

    if let Err(e) = fs::write(&args.out, &markdown) {
        eprintln!("solver-bench: failed to write {:?}: {e}", args.out);
        return ExitCode::FAILURE;
    }
    eprintln!("wrote {:?}", args.out);
    ExitCode::SUCCESS
}

struct CellResult {
    seeds: u64,
    feasibility_count: u64,
    hard_violations_median: u32,
    placements_total_median: u64,
    placements_expected: u64,
    soft_score_median: Option<u32>,
    ffd_ms_median: f64,
    total_ms_median: f64,
}

fn run_cell(
    backend: BenchBackend,
    problem: &Problem,
    expected: u64,
    budget: Duration,
    seeds: u64,
) -> CellResult {
    if let BenchBackend::CpSat = backend {
        return run_cpsat_cell(problem, expected, budget, seeds);
    }

    let weights = PRODUCTION_ACTIVE_WEIGHTS.clone();
    let greedy_cfg = SolveConfig {
        weights: weights.clone(),
        deadline: None,
        ..SolveConfig::default()
    };

    let ffd_start = Instant::now();
    let _greedy = solve_with_config(problem, &greedy_cfg).expect("greedy solve");
    let ffd_ms = ffd_start.elapsed().as_secs_f64() * 1_000.0;

    let mut total_ms_samples: Vec<f64> = Vec::with_capacity(seeds as usize);
    let mut hard_violations_samples: Vec<u32> = Vec::with_capacity(seeds as usize);
    let mut placements_total_samples: Vec<u64> = Vec::with_capacity(seeds as usize);
    let mut soft_score_feasible: Vec<u32> = Vec::with_capacity(seeds as usize);
    let mut feasibility_count: u64 = 0;

    let (lahc_rr_period, lahc_kempe_period) = match backend {
        BenchBackend::Lahc => (None, None),
        BenchBackend::LahcRr => (Some(25u32), None),
        BenchBackend::LahcRrKempe => (Some(25u32), Some(23u32)),
        BenchBackend::CpSat => unreachable!("cpsat dispatched above"),
    };

    for seed in 1..=seeds {
        let cfg = SolveConfig {
            weights: weights.clone(),
            deadline: Some(budget),
            seed,
            lahc_rr_period,
            lahc_kempe_period,
            ..SolveConfig::default()
        };
        let start = Instant::now();
        let solution = solve_with_config(problem, &cfg).expect("solve");
        let total_ms = start.elapsed().as_secs_f64() * 1_000.0;
        let hard = solution.violations.len() as u32;
        let placements_total = solution.placements.len() as u64;
        debug_assert!(
            placements_total <= expected,
            "placements_total ({placements_total}) > expected ({expected}); structural invariant violated",
        );
        let feasible = hard == 0 && placements_total == expected;
        if feasible {
            feasibility_count += 1;
            soft_score_feasible.push(solution.soft_score);
        }
        hard_violations_samples.push(hard);
        total_ms_samples.push(total_ms);
        placements_total_samples.push(placements_total);
    }

    CellResult {
        seeds,
        feasibility_count,
        hard_violations_median: median_u32(&mut hard_violations_samples),
        placements_total_median: median_u64(&mut placements_total_samples),
        placements_expected: expected,
        soft_score_median: if soft_score_feasible.is_empty() {
            None
        } else {
            Some(median_u32(&mut soft_score_feasible))
        },
        ffd_ms_median: ffd_ms,
        total_ms_median: median_f64(&mut total_ms_samples),
    }
}

fn build_cpsat_command(
    problem_path: &std::path::Path,
    budget: std::time::Duration,
    seed: u64,
) -> std::process::Command {
    let mut cmd = std::process::Command::new("python3");
    cmd.arg("-m")
        .arg("klassenzeit_solver.cpsat")
        .arg("--problem-file")
        .arg(problem_path)
        .arg("--deadline-ms")
        .arg(budget.as_millis().to_string())
        .arg("--seed")
        .arg(seed.to_string());
    cmd
}

fn tempfile_path(prefix: &str, suffix: &str) -> std::path::PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{prefix}{nanos}{suffix}"))
}

fn run_cpsat_cell(problem: &Problem, expected: u64, budget: Duration, seeds: u64) -> CellResult {
    let problem_json =
        serde_json::to_string(problem).expect("serialise problem for cpsat subprocess");
    let tmpfile = tempfile_path("kz-bench-problem-", ".json");
    std::fs::write(&tmpfile, problem_json.as_bytes()).expect("write problem tempfile");

    let mut total_ms_samples: Vec<f64> = Vec::with_capacity(seeds as usize);
    let mut hard_violations_samples: Vec<u32> = Vec::with_capacity(seeds as usize);
    let mut placements_total_samples: Vec<u64> = Vec::with_capacity(seeds as usize);
    let mut soft_score_feasible: Vec<u32> = Vec::with_capacity(seeds as usize);
    let mut feasibility_count: u64 = 0;

    for seed in 1..=seeds {
        let start = Instant::now();
        let result = build_cpsat_command(&tmpfile, budget, seed).output();
        let total_ms = start.elapsed().as_secs_f64() * 1_000.0;
        let solution_json = match result {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
            Ok(o) => {
                eprintln!(
                    "cpsat subprocess non-zero exit (seed={seed}): {}",
                    String::from_utf8_lossy(&o.stderr)
                );
                hard_violations_samples.push(u32::MAX);
                total_ms_samples.push(total_ms);
                placements_total_samples.push(0);
                continue;
            }
            Err(e) => {
                eprintln!("cpsat subprocess error (seed={seed}): {e}");
                hard_violations_samples.push(u32::MAX);
                total_ms_samples.push(total_ms);
                placements_total_samples.push(0);
                continue;
            }
        };
        let solution: solver_core::Solution = match serde_json::from_str(&solution_json) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("cpsat parse error (seed={seed}): {e}");
                hard_violations_samples.push(u32::MAX);
                total_ms_samples.push(total_ms);
                placements_total_samples.push(0);
                continue;
            }
        };
        let hard = solution.violations.len() as u32;
        let placements_total = solution.placements.len() as u64;
        debug_assert!(
            placements_total <= expected,
            "cpsat placements_total ({placements_total}) > expected ({expected}); structural invariant violated",
        );
        let feasible = hard == 0 && placements_total == expected;
        if feasible {
            feasibility_count += 1;
            let soft = solver_core::score_solution(
                problem,
                &solution.placements,
                &solver_core::PRODUCTION_ACTIVE_WEIGHTS,
            );
            soft_score_feasible.push(soft);
        }
        hard_violations_samples.push(hard);
        total_ms_samples.push(total_ms);
        placements_total_samples.push(placements_total);
    }

    let _ = std::fs::remove_file(&tmpfile);

    CellResult {
        seeds,
        feasibility_count,
        hard_violations_median: median_u32(&mut hard_violations_samples),
        placements_total_median: median_u64(&mut placements_total_samples),
        placements_expected: expected,
        soft_score_median: if soft_score_feasible.is_empty() {
            None
        } else {
            Some(median_u32(&mut soft_score_feasible))
        },
        ffd_ms_median: 0.0,
        total_ms_median: median_f64(&mut total_ms_samples),
    }
}

fn median_u32(values: &mut [u32]) -> u32 {
    values.sort_unstable();
    let mid = values.len() / 2;
    values[mid]
}

fn median_u64(values: &mut [u64]) -> u64 {
    values.sort_unstable();
    let mid = values.len() / 2;
    values[mid]
}

fn median_f64(values: &mut [f64]) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    values[mid]
}

fn write_header(out: &mut String) {
    out.push_str("# Solver bake-off feasibility bench\n\n");
    out.push_str("<!-- Regenerated by `mise run bench:bakeoff`. Do not hand-edit. -->\n\n");
    out.push_str("| Fixture | Backend | Seeds | Feasibility | Hard violations (median) | Placements (median / expected) | Soft score (median, feasible) | FFD wall-clock (ms, median) | Total wall-clock (ms, median) |\n");
    out.push_str("| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
}

fn write_row(out: &mut String, fixture: &str, backend: BenchBackend, cell: &CellResult) {
    let soft = match cell.soft_score_median {
        Some(s) => s.to_string(),
        None => "-".to_string(),
    };
    out.push_str(&format!(
        "| {fixture} | {backend} | {seeds} | {n}/{seeds} | {hard} | {placed}/{expected} | {soft} | {ffd:.2} | {total:.0} |\n",
        backend = backend.label(),
        seeds = cell.seeds,
        n = cell.feasibility_count,
        hard = cell.hard_violations_median,
        placed = cell.placements_total_median,
        expected = cell.placements_expected,
        ffd = cell.ffd_ms_median,
        total = cell.total_ms_median,
    ));
}

fn write_footer(out: &mut String) {
    let cpu = read_cpu().unwrap_or_else(|| "unknown".to_string());
    let kernel = read_kernel().unwrap_or_else(|| "unknown".to_string());
    let rustc = read_rustc().unwrap_or_else(|| "unknown".to_string());
    let date = chrono_today();
    out.push('\n');
    out.push_str(&format!(
        "Refreshed {date} on {cpu}, Linux {kernel}, {rustc}.\n\n"
    ));
    out.push_str(
        "Refresh with `mise run bench:bakeoff` when a backend changes or a fixture is added. The\n",
    );
    out.push_str(
        "bench is host-sensitive on wall-clock columns and host-stable on feasibility / hard-violation\n",
    );
    out.push_str("columns.\n\n");
    out.push_str("See `docs/adr/0029-solver-feasibility-bake-off.md` for methodology.\n");
}

fn read_cpu() -> Option<String> {
    fs::read_to_string("/proc/cpuinfo").ok().and_then(|c| {
        c.lines()
            .find_map(|l| l.strip_prefix("model name").and_then(|s| s.split_once(':')))
            .map(|(_, v)| v.trim().to_string())
    })
}

fn read_kernel() -> Option<String> {
    std::process::Command::new("uname")
        .arg("-r")
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

fn read_rustc() -> Option<String> {
    std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
}

fn chrono_today() -> String {
    // No chrono dep; shell-out keeps this in step with record_solver_bench.sh.
    std::process::Command::new("date")
        .args(["-Idate"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_duration_accepts_seconds_and_milliseconds() {
        assert_eq!(parse_duration("60s").unwrap(), Duration::from_secs(60));
        assert_eq!(parse_duration("250ms").unwrap(), Duration::from_millis(250));
        assert!(parse_duration("60").is_err());
    }

    #[test]
    fn parse_args_reads_all_flags() {
        let raw = vec![
            "--budget".to_string(),
            "5s".to_string(),
            "--seeds".to_string(),
            "4".to_string(),
            "--fixtures".to_string(),
            "grundschule,lock_in".to_string(),
            "--out".to_string(),
            "/tmp/out.md".to_string(),
        ];
        let args = parse_args(raw).unwrap();
        assert_eq!(args.budget, Duration::from_secs(5));
        assert_eq!(args.seeds, 4);
        assert_eq!(
            args.fixtures,
            vec!["grundschule".to_string(), "lock_in".to_string()]
        );
        assert_eq!(args.out, PathBuf::from("/tmp/out.md"));
    }

    #[test]
    fn parse_args_rejects_unknown_flag() {
        let raw = vec!["--unknown".to_string()];
        assert!(parse_args(raw).is_err());
    }

    #[test]
    fn median_u32_returns_middle_value() {
        let mut v = vec![5, 1, 3];
        assert_eq!(median_u32(&mut v), 3);
    }

    #[test]
    fn write_row_emits_one_line_with_dash_for_no_feasible() {
        let cell = CellResult {
            seeds: 20,
            feasibility_count: 0,
            hard_violations_median: 1,
            placements_total_median: 0,
            placements_expected: 0,
            soft_score_median: None,
            ffd_ms_median: 0.05,
            total_ms_median: 60050.0,
        };
        let mut out = String::new();
        write_row(&mut out, "grundschule", BenchBackend::Lahc, &cell);
        assert!(out.contains("| 0/20 |"));
        assert!(out.contains("| - |"));
        assert!(out.contains("| 0.05 |"));
    }

    #[test]
    fn write_row_renders_lahc_rr_backend_label() {
        let cell = CellResult {
            seeds: 20,
            feasibility_count: 20,
            hard_violations_median: 0,
            placements_total_median: 0,
            placements_expected: 0,
            soft_score_median: Some(10),
            ffd_ms_median: 1.0,
            total_ms_median: 60100.0,
        };
        let mut out = String::new();
        write_row(&mut out, "grundschule", BenchBackend::LahcRr, &cell);
        assert!(out.contains("| lahc_rr |"));
    }

    #[test]
    fn write_row_renders_lahc_rr_kempe_backend_label() {
        let cell = CellResult {
            seeds: 20,
            feasibility_count: 20,
            hard_violations_median: 0,
            placements_total_median: 0,
            placements_expected: 0,
            soft_score_median: Some(10),
            ffd_ms_median: 1.0,
            total_ms_median: 60100.0,
        };
        let mut out = String::new();
        write_row(&mut out, "grundschule", BenchBackend::LahcRrKempe, &cell);
        assert!(out.contains("| lahc_rr_kempe |"));
    }

    #[test]
    fn write_row_renders_cpsat_backend_label() {
        let cell = CellResult {
            seeds: 20,
            feasibility_count: 18,
            hard_violations_median: 0,
            placements_total_median: 0,
            placements_expected: 0,
            soft_score_median: Some(15),
            ffd_ms_median: 0.0,
            total_ms_median: 60050.0,
        };
        let mut out = String::new();
        write_row(&mut out, "grundschule", BenchBackend::CpSat, &cell);
        assert!(out.contains("| cpsat |"));
    }

    #[test]
    fn write_row_renders_placements_column() {
        let cell = CellResult {
            seeds: 20,
            feasibility_count: 20,
            hard_violations_median: 0,
            placements_total_median: 196,
            placements_expected: 196,
            soft_score_median: Some(10),
            ffd_ms_median: 1.0,
            total_ms_median: 60100.0,
        };
        let mut out = String::new();
        write_row(&mut out, "zweizuegig", BenchBackend::LahcRr, &cell);
        assert!(
            out.contains("| 196/196 |"),
            "expected `| 196/196 |` somewhere in row, got: {out}"
        );
    }

    #[test]
    fn write_row_renders_underflow_placement_count() {
        let cell = CellResult {
            seeds: 20,
            feasibility_count: 0,
            hard_violations_median: 0,
            placements_total_median: 60,
            placements_expected: 196,
            soft_score_median: None,
            ffd_ms_median: 0.5,
            total_ms_median: 60000.0,
        };
        let mut out = String::new();
        write_row(&mut out, "zweizuegig", BenchBackend::LahcRr, &cell);
        assert!(
            out.contains("| 60/196 |"),
            "expected `| 60/196 |` somewhere in row, got: {out}"
        );
        assert!(
            out.contains("| 0/20 |"),
            "expected `| 0/20 |` (feasibility) somewhere in row, got: {out}"
        );
    }

    #[test]
    fn placements_expected_for_problem_sums_hours_per_week() {
        let problem = grundschule_fixture();
        let manual_sum: u64 = problem
            .lessons
            .iter()
            .map(|l| l.hours_per_week as u64)
            .sum();
        assert_eq!(placements_expected_for_problem(&problem), manual_sum);
        assert_eq!(placements_expected_for_problem(&problem), 45);
    }

    #[test]
    fn cpsat_subprocess_command_args_match_module_invocation() {
        let cmd = build_cpsat_command(
            std::path::Path::new("/tmp/p.json"),
            std::time::Duration::from_secs(60),
            7,
        );
        let argv: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            argv,
            vec![
                "-m".to_string(),
                "klassenzeit_solver.cpsat".to_string(),
                "--problem-file".to_string(),
                "/tmp/p.json".to_string(),
                "--deadline-ms".to_string(),
                "60000".to_string(),
                "--seed".to_string(),
                "7".to_string(),
            ]
        );
        assert_eq!(cmd.get_program(), "python3");
    }
}
