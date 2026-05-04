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
use solver_core::types::{ConstraintWeights, Problem, SolveConfig};

#[derive(Clone, Copy)]
enum BenchBackend {
    Lahc,
}

impl BenchBackend {
    fn label(self) -> &'static str {
        match self {
            BenchBackend::Lahc => "lahc",
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

fn production_active_weights() -> ConstraintWeights {
    ConstraintWeights {
        class_gap: 10,
        teacher_gap: 10,
        prefer_early_period: 1,
        avoid_first_period: 1,
        prefer_home_room: 5,
        avoid_last_period: 1,
        prefer_late_period: 1,
        class_day_balance: 5,
    }
}

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

    let backends = [BenchBackend::Lahc];
    let mut markdown = String::new();
    write_header(&mut markdown);

    for (name, build) in FIXTURES {
        if !args.fixtures.iter().any(|f| f == name) {
            continue;
        }
        let problem = build();
        for backend in &backends {
            let cell = run_cell(*backend, &problem, args.budget, args.seeds);
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
    soft_score_median: Option<u32>,
    ffd_ms_median: f64,
    total_ms_median: f64,
}

fn run_cell(backend: BenchBackend, problem: &Problem, budget: Duration, seeds: u64) -> CellResult {
    let weights = production_active_weights();
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
    let mut soft_score_feasible: Vec<u32> = Vec::with_capacity(seeds as usize);
    let mut feasibility_count: u64 = 0;

    for seed in 1..=seeds {
        let cfg = SolveConfig {
            weights: weights.clone(),
            deadline: Some(budget),
            seed,
            ..SolveConfig::default()
        };
        let start = Instant::now();
        let solution = match backend {
            BenchBackend::Lahc => solve_with_config(problem, &cfg).expect("lahc solve"),
        };
        let total_ms = start.elapsed().as_secs_f64() * 1_000.0;
        let hard = solution.violations.len() as u32;
        let feasible = hard == 0;
        if feasible {
            feasibility_count += 1;
            soft_score_feasible.push(solution.soft_score);
        }
        hard_violations_samples.push(hard);
        total_ms_samples.push(total_ms);
    }

    CellResult {
        seeds,
        feasibility_count,
        hard_violations_median: median_u32(&mut hard_violations_samples),
        soft_score_median: if soft_score_feasible.is_empty() {
            None
        } else {
            Some(median_u32(&mut soft_score_feasible))
        },
        ffd_ms_median: ffd_ms,
        total_ms_median: median_f64(&mut total_ms_samples),
    }
}

fn median_u32(values: &mut [u32]) -> u32 {
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
    out.push_str("| Fixture | Backend | Seeds | Feasibility | Hard violations (median) | Soft score (median, feasible) | FFD wall-clock (ms, median) | Total wall-clock (ms, median) |\n");
    out.push_str("| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |\n");
}

fn write_row(out: &mut String, fixture: &str, backend: BenchBackend, cell: &CellResult) {
    let soft = match cell.soft_score_median {
        Some(s) => s.to_string(),
        None => "-".to_string(),
    };
    out.push_str(&format!(
        "| {fixture} | {backend} | {seeds} | {n}/{seeds} | {hard} | {soft} | {ffd:.2} | {total:.0} |\n",
        backend = backend.label(),
        seeds = cell.seeds,
        n = cell.feasibility_count,
        hard = cell.hard_violations_median,
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
}
