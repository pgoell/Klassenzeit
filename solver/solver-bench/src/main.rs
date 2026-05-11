//! Solver feasibility bake-off bench harness.
//!
//! Two-mode binary:
//! - Supervisor (default): parses CLI, spawns one `solver-bench --cell ...`
//!   child per `(fixture, backend)` pair, collects each cell's CellResult JSON,
//!   writes a markdown table to BENCH_RESULTS.md.
//! - Cell-child (`--cell ...`): runs the seed loop for one (fixture, backend)
//!   pair, reads its own peak RSS via `getrusage(RUSAGE_SELF)`, prints one
//!   CellResult JSON object on stdout, exits.
//!
//! ADR: docs/adr/0034-bench-cell-subprocess-and-observability.md

mod quality;

use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use solver_core::solve_with_config_stats;
use solver_core::test_fixtures::{
    dreizuegig_fixture, ffd_lock_in_grundschule, grundschule_fixture, zweizuegig_fixture,
};
use solver_core::types::{Problem, SolveConfig};
use solver_core::PRODUCTION_ACTIVE_WEIGHTS;

fn placements_expected_for_problem(problem: &Problem) -> u64 {
    problem
        .lessons
        .iter()
        .map(|l| l.hours_per_week as u64)
        .sum()
}

/// Clear `teacher_pin` on every lesson and widen `teacher_candidates` to the
/// deterministic-sorted, deduplicated set of teachers qualified for each
/// lesson's subject. Bench-only; production fixtures retain their original
/// pins until item 73's no-pinned data lands in BENCH_RESULTS.md.
pub(crate) fn unpin_teachers_in_problem(problem: &mut Problem) {
    use std::collections::HashMap;
    let mut quals_by_subject: HashMap<solver_core::SubjectId, Vec<solver_core::TeacherId>> =
        HashMap::new();
    for q in &problem.teacher_qualifications {
        quals_by_subject
            .entry(q.subject_id)
            .or_default()
            .push(q.teacher_id);
    }
    for v in quals_by_subject.values_mut() {
        v.sort_by_key(|t| t.0);
        v.dedup();
    }
    for lesson in &mut problem.lessons {
        lesson.teacher_pin = None;
        lesson.teacher_candidates = quals_by_subject
            .get(&lesson.subject_id)
            .cloned()
            .unwrap_or_default();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BenchBackend {
    Lahc,
    LahcRr,
    LahcKempe,
    LahcRrKempe,
    CpSat,
}

impl BenchBackend {
    fn label(self) -> &'static str {
        match self {
            BenchBackend::Lahc => "lahc",
            BenchBackend::LahcRr => "lahc_rr",
            BenchBackend::LahcKempe => "lahc_kempe",
            BenchBackend::LahcRrKempe => "lahc_rr_kempe",
            BenchBackend::CpSat => "cpsat",
        }
    }
    fn parse(s: &str) -> Result<Self, String> {
        match s {
            "lahc" => Ok(Self::Lahc),
            "lahc_rr" => Ok(Self::LahcRr),
            "lahc_kempe" => Ok(Self::LahcKempe),
            "lahc_rr_kempe" => Ok(Self::LahcRrKempe),
            "cpsat" => Ok(Self::CpSat),
            other => Err(format!("unknown backend '{other}'")),
        }
    }
    const ALL: [Self; 5] = [
        Self::Lahc,
        Self::LahcRr,
        Self::LahcKempe,
        Self::LahcRrKempe,
        Self::CpSat,
    ];
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum TeacherPinsMode {
    On,
    Off,
}

impl TeacherPinsMode {
    /// Render the mode as the CLI flag value (`on` / `off`); the supervisor
    /// propagates `--teacher-pins <label>` to the per-cell child process.
    fn teacher_pins_label(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
        }
    }

    fn parse_teacher_pins_mode(s: &str) -> Result<Self, String> {
        match s {
            "on" => Ok(Self::On),
            "off" => Ok(Self::Off),
            other => Err(format!(
                "--teacher-pins expects 'on' or 'off', got '{other}'"
            )),
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

fn fixture_by_name(name: &str) -> Option<fn() -> Problem> {
    FIXTURES.iter().find(|(n, _)| *n == name).map(|(_, f)| *f)
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct CellResult {
    seeds: u64,
    feasibility_count: u64,
    hard_violations_median: u32,
    placements_total_median: u64,
    placements_expected: u64,
    soft_score_median: Option<u32>,
    ffd_ms_median: f64,
    total_ms_median: f64,
    peak_kb: u64,
    time_to_first_feasible_ms_median: Option<f64>,
    time_to_optimal_ms_median: Option<f64>,
    worst_spread_median: Option<u32>,
    worst_home_room_ratio_median: Option<f64>,
    total_interior_gaps_median: Option<u32>,
    late_period_ratio_median: Option<f64>,
    quality_pass_count_median: Option<u32>,
    unplaced_hours_median: Option<u32>,
    class_gap_hours_median: Option<u32>,
    teacher_gap_hours_median: Option<u32>,
    class_day_balance_cost_median: Option<u32>,
    home_room_misses_median: Option<u32>,
    prefer_early_units_median: Option<u32>,
    avoid_first_units_median: Option<u32>,
    avoid_last_units_median: Option<u32>,
    prefer_late_units_median: Option<u32>,
    #[serde(default)]
    rr_k: Option<u32>,
    #[serde(default)]
    rr_period: Option<u32>,
    #[serde(default)]
    kempe_max_chain: Option<u32>,
}

struct SupervisorArgs {
    budget: Duration,
    seeds: u64,
    fixtures: Vec<String>,
    out: PathBuf,
    backends: Option<Vec<BenchBackend>>,
    rr_k_values: Option<Vec<u32>>,
    rr_period_values: Option<Vec<u32>>,
    kempe_max_chain_values: Option<Vec<u32>>,
    append: bool,
    teacher_pins: TeacherPinsMode,
}

fn default_supervisor_args() -> SupervisorArgs {
    SupervisorArgs {
        budget: Duration::from_secs(60),
        seeds: 20,
        fixtures: FIXTURES.iter().map(|(n, _)| (*n).to_string()).collect(),
        out: PathBuf::from("solver/solver-core/benches/BENCH_RESULTS.md"),
        backends: None,
        rr_k_values: None,
        rr_period_values: None,
        kempe_max_chain_values: None,
        append: false,
        teacher_pins: TeacherPinsMode::On,
    }
}

fn parse_u32_csv(label: &str, value: &str) -> Result<Vec<u32>, String> {
    if value.is_empty() {
        return Err(format!("{label} must not be empty"));
    }
    value
        .split(',')
        .map(|s| {
            s.parse::<u32>()
                .map_err(|e| format!("{label} entry '{s}': {e}"))
        })
        .collect()
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

fn parse_supervisor_args(raw: Vec<String>) -> Result<SupervisorArgs, String> {
    let mut args = default_supervisor_args();
    let mut iter = raw.into_iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--budget" => {
                let v = iter.next().ok_or("--budget needs a value")?;
                args.budget = parse_duration(&v)?;
            }
            "--seeds" => {
                let v = iter.next().ok_or("--seeds needs a value")?;
                args.seeds = v
                    .parse::<u64>()
                    .map_err(|e| format!("--seeds must be a positive integer: {e}"))?;
            }
            "--fixtures" => {
                let v = iter.next().ok_or("--fixtures needs a value")?;
                args.fixtures = v.split(',').map(str::to_string).collect();
            }
            "--out" => {
                let v = iter.next().ok_or("--out needs a value")?;
                args.out = PathBuf::from(v);
            }
            "--backends" => {
                let v = iter.next().ok_or("--backends needs a value")?;
                if v.is_empty() {
                    return Err("--backends must not be empty".to_string());
                }
                let parsed: Result<Vec<BenchBackend>, String> =
                    v.split(',').map(BenchBackend::parse).collect();
                args.backends = Some(parsed?);
            }
            "--rr-k" => {
                let v = iter.next().ok_or("--rr-k needs a value")?;
                args.rr_k_values = Some(parse_u32_csv("--rr-k", &v)?);
            }
            "--rr-period" => {
                let v = iter.next().ok_or("--rr-period needs a value")?;
                args.rr_period_values = Some(parse_u32_csv("--rr-period", &v)?);
            }
            "--kempe-max-chain" => {
                let v = iter.next().ok_or("--kempe-max-chain needs a value")?;
                args.kempe_max_chain_values = Some(parse_u32_csv("--kempe-max-chain", &v)?);
            }
            "--teacher-pins" => {
                let v = iter.next().ok_or("--teacher-pins needs a value")?;
                args.teacher_pins = TeacherPinsMode::parse_teacher_pins_mode(&v)?;
            }
            "--append" => {
                args.append = true;
            }
            other => return Err(format!("unknown flag '{other}'")),
        }
    }
    Ok(args)
}

struct CellArgs {
    fixture: String,
    backend: BenchBackend,
    budget: Duration,
    seeds: u64,
    rr_k: Option<u32>,
    rr_period: Option<u32>,
    kempe_max_chain: Option<u32>,
    teacher_pins: TeacherPinsMode,
}

fn parse_cell_args(raw: Vec<String>) -> Result<CellArgs, String> {
    let mut fixture: Option<String> = None;
    let mut backend: Option<BenchBackend> = None;
    let mut budget: Option<Duration> = None;
    let mut seeds: Option<u64> = None;
    let mut rr_k: Option<u32> = None;
    let mut rr_period: Option<u32> = None;
    let mut kempe_max_chain: Option<u32> = None;
    let mut teacher_pins: Option<TeacherPinsMode> = None;
    let mut iter = raw.into_iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--cell" => {
                fixture = Some(iter.next().ok_or("--cell needs a fixture name")?);
            }
            "--backend" => {
                backend = Some(BenchBackend::parse(
                    &iter.next().ok_or("--backend needs a value")?,
                )?);
            }
            "--budget" => {
                budget = Some(parse_duration(
                    &iter.next().ok_or("--budget needs a value")?,
                )?);
            }
            "--seeds" => {
                seeds = Some(
                    iter.next()
                        .ok_or("--seeds needs a value")?
                        .parse::<u64>()
                        .map_err(|e| format!("--seeds must be a positive integer: {e}"))?,
                );
            }
            "--rr-k" => {
                rr_k = Some(
                    iter.next()
                        .ok_or("--rr-k needs a value")?
                        .parse::<u32>()
                        .map_err(|e| format!("--rr-k must be a positive integer: {e}"))?,
                );
            }
            "--rr-period" => {
                rr_period = Some(
                    iter.next()
                        .ok_or("--rr-period needs a value")?
                        .parse::<u32>()
                        .map_err(|e| format!("--rr-period must be a positive integer: {e}"))?,
                );
            }
            "--kempe-max-chain" => {
                kempe_max_chain = Some(
                    iter.next()
                        .ok_or("--kempe-max-chain needs a value")?
                        .parse::<u32>()
                        .map_err(|e| {
                            format!("--kempe-max-chain must be a positive integer: {e}")
                        })?,
                );
            }
            "--teacher-pins" => {
                teacher_pins = Some(TeacherPinsMode::parse_teacher_pins_mode(
                    &iter.next().ok_or("--teacher-pins needs a value")?,
                )?);
            }
            other => return Err(format!("unknown cell flag '{other}'")),
        }
    }
    Ok(CellArgs {
        fixture: fixture.ok_or("--cell <fixture> required")?,
        backend: backend.ok_or("--backend <name> required")?,
        budget: budget.ok_or("--budget <dur> required")?,
        seeds: seeds.ok_or("--seeds <n> required")?,
        rr_k,
        rr_period,
        kempe_max_chain,
        teacher_pins: teacher_pins.unwrap_or(TeacherPinsMode::On),
    })
}

fn main() -> ExitCode {
    let raw: Vec<String> = env::args().skip(1).collect();
    if matches!(raw.first().map(|s| s.as_str()), Some("--cell")) {
        return run_cell_child(raw);
    }
    run_supervisor(raw)
}

/// One unit of work scheduled by the supervisor: a (fixture, backend) pair plus
/// optional `(rr_k, rr_period)` overrides and an optional Kempe chain-depth
/// override. RR backends with a sweep grid expand to one CellSpec per
/// (k, period) pair; non-RR backends always produce one CellSpec with
/// `rr_k = rr_period = None`. Kempe-using backends with a chain-depth sweep
/// expand by an outer loop over depths, leaving non-Kempe backends with
/// `kempe_max_chain = None` (renders `-`).
#[derive(Debug, Clone)]
struct CellSpec {
    fixture: &'static str,
    backend: BenchBackend,
    rr_k: Option<u32>,
    rr_period: Option<u32>,
    kempe_max_chain: Option<u32>,
}

fn build_plan(
    fixtures: &[String],
    backends: &[BenchBackend],
    rr_k_values: Option<&[u32]>,
    rr_period_values: Option<&[u32]>,
    kempe_max_chain_values: Option<&[u32]>,
) -> Vec<CellSpec> {
    let mut plan: Vec<CellSpec> = Vec::new();
    for (name, _) in FIXTURES.iter() {
        if !fixtures.iter().any(|f| f == name) {
            continue;
        }
        for backend in backends {
            // Outer loop: Kempe chain-depth sweep applies only to Kempe-using
            // backends. For non-Kempe backends or when the flag is absent, we
            // emit a single None so the existing single-cell shape is preserved.
            let chain_values: Vec<Option<u32>> = match kempe_max_chain_values {
                Some(vs)
                    if matches!(backend, BenchBackend::LahcKempe | BenchBackend::LahcRrKempe) =>
                {
                    vs.iter().map(|v| Some(*v)).collect()
                }
                _ => vec![None],
            };
            for k_chain in &chain_values {
                match backend {
                    BenchBackend::LahcRr | BenchBackend::LahcRrKempe => {
                        let ks: Vec<u32> =
                            rr_k_values.map(|v| v.to_vec()).unwrap_or_else(|| vec![5]);
                        let ps: Vec<u32> = rr_period_values
                            .map(|v| v.to_vec())
                            .unwrap_or_else(|| vec![25]);
                        for k in &ks {
                            for p in &ps {
                                plan.push(CellSpec {
                                    fixture: name,
                                    backend: *backend,
                                    rr_k: Some(*k),
                                    rr_period: Some(*p),
                                    kempe_max_chain: *k_chain,
                                });
                            }
                        }
                    }
                    _ => {
                        plan.push(CellSpec {
                            fixture: name,
                            backend: *backend,
                            rr_k: None,
                            rr_period: None,
                            kempe_max_chain: *k_chain,
                        });
                    }
                }
            }
        }
    }
    plan
}

fn run_supervisor(raw: Vec<String>) -> ExitCode {
    let args = match parse_supervisor_args(raw) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("solver-bench: {e}");
            return ExitCode::from(2);
        }
    };

    let exe = match env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("solver-bench: cannot resolve current exe: {e}");
            return ExitCode::FAILURE;
        }
    };

    let backends_owned: Vec<BenchBackend> = args
        .backends
        .clone()
        .unwrap_or_else(|| BenchBackend::ALL.to_vec());
    let sweep_mode = (args.rr_k_values.is_some() && args.rr_period_values.is_some())
        || args
            .kempe_max_chain_values
            .as_ref()
            .is_some_and(|v| v.len() > 1);

    let plan = build_plan(
        &args.fixtures,
        &backends_owned,
        args.rr_k_values.as_deref(),
        args.rr_period_values.as_deref(),
        args.kempe_max_chain_values.as_deref(),
    );
    let cells_attempted = plan.len();
    let render_kempe_chain_col = plan.iter().any(|c| c.kempe_max_chain.is_some());

    let mut markdown = String::new();
    if !args.append {
        write_title_and_intro(&mut markdown);
        write_backend_objectives_section(&mut markdown, &BenchBackend::ALL);
    } else if args.teacher_pins == TeacherPinsMode::Off {
        markdown.push_str(
            "\n## Unpinned variant (solver-driven teacher assignment, ADR 0036)\n\n\
             Lessons in this section have `teacher_pin = None` and `teacher_candidates` \
             widened to every teacher qualified for the lesson's subject \
             (`Problem.teacher_qualifications`). Captures the cost of widening \
             teacher decision variables relative to the canonical all-pinned table above.\n\n",
        );
    } else {
        markdown.push_str(&format!("\n## RR sweep {}\n\n", today_yyyy_mm_dd()));
    }
    write_table_header(&mut markdown, render_kempe_chain_col);

    let mut all_results: Vec<(CellSpec, CellResult)> = Vec::new();
    let mut runner = |spec: &CellSpec| -> Result<CellResult, String> {
        spawn_cell(&exe, spec, args.budget, args.seeds, args.teacher_pins)
    };
    let successes = render_cells_with_specs(
        &plan,
        &mut runner,
        &mut markdown,
        &mut all_results,
        render_kempe_chain_col,
        args.teacher_pins.teacher_pins_label(),
    );

    if sweep_mode {
        write_pareto_and_recommendation(&mut markdown, &args.fixtures, &all_results);
    }

    if !args.append {
        write_footer(&mut markdown);
    }

    let write_result = if args.append && args.out.exists() {
        fs::read_to_string(&args.out).and_then(|prior| fs::write(&args.out, prior + &markdown))
    } else {
        fs::write(&args.out, &markdown)
    };
    if let Err(e) = write_result {
        eprintln!("solver-bench: failed to write {:?}: {e}", args.out);
        return ExitCode::FAILURE;
    }
    eprintln!("wrote {:?}", args.out);

    if cells_attempted == 0 || successes >= 1 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn spawn_cell(
    exe: &Path,
    spec: &CellSpec,
    budget: Duration,
    seeds: u64,
    teacher_pins: TeacherPinsMode,
) -> Result<CellResult, String> {
    let budget_str = if budget < Duration::from_secs(1) {
        format!("{}ms", budget.as_millis())
    } else {
        format!("{}s", budget.as_secs())
    };
    let mut cmd = Command::new(exe);
    cmd.arg("--cell")
        .arg(spec.fixture)
        .arg("--backend")
        .arg(spec.backend.label())
        .arg("--budget")
        .arg(&budget_str)
        .arg("--seeds")
        .arg(seeds.to_string());
    if let Some(k) = spec.rr_k {
        cmd.arg("--rr-k").arg(k.to_string());
    }
    if let Some(p) = spec.rr_period {
        cmd.arg("--rr-period").arg(p.to_string());
    }
    if let Some(c) = spec.kempe_max_chain {
        cmd.arg("--kempe-max-chain").arg(c.to_string());
    }
    cmd.arg("--teacher-pins")
        .arg(teacher_pins.teacher_pins_label());
    cmd.stdout(Stdio::piped()).stderr(Stdio::inherit());
    let child = cmd.spawn().map_err(|e| format!("spawn cell: {e}"))?;
    let output = child
        .wait_with_output()
        .map_err(|e| format!("wait cell: {e}"))?;
    if !output.status.success() {
        return Err(format!("cell exited with {:?}", output.status));
    }
    let stdout =
        std::str::from_utf8(&output.stdout).map_err(|e| format!("cell stdout utf-8: {e}"))?;
    serde_json::from_str(stdout.trim()).map_err(|e| format!("cell JSON: {e}; raw: {stdout}"))
}

fn render_cells_with_specs<F>(
    plan: &[CellSpec],
    runner: &mut F,
    markdown: &mut String,
    all_results: &mut Vec<(CellSpec, CellResult)>,
    render_kempe_chain_col: bool,
    teacher_pins_label: &str,
) -> usize
where
    F: FnMut(&CellSpec) -> Result<CellResult, String>,
{
    let mut successes = 0usize;
    for spec in plan {
        eprintln!(
            "cell start: {} / {} teacher_pins={}",
            spec.fixture,
            spec.backend.label(),
            teacher_pins_label,
        );
        match runner(spec) {
            Ok(cell) => {
                eprintln!(
                    "cell done: {} / {} teacher_pins={} feasibility {}/{} hard_med={} \
                     placements_med={}/{} soft_med={} total_ms_med={:.0} peak_kb={} ttf_med={} \
                     tto_med={} worst_spread_med={} worst_home_med={} gaps_med={} late_med={} \
                     quality_med={}",
                    spec.fixture,
                    spec.backend.label(),
                    teacher_pins_label,
                    cell.feasibility_count,
                    cell.seeds,
                    cell.hard_violations_median,
                    cell.placements_total_median,
                    cell.placements_expected,
                    cell.soft_score_median
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    cell.total_ms_median,
                    cell.peak_kb,
                    cell.time_to_first_feasible_ms_median
                        .map(|v| format!("{:.0}", v))
                        .unwrap_or_else(|| "-".to_string()),
                    cell.time_to_optimal_ms_median
                        .map(|v| format!("{:.0}", v))
                        .unwrap_or_else(|| "-".to_string()),
                    cell.worst_spread_median
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    cell.worst_home_room_ratio_median
                        .map(|v| format!("{v:.2}"))
                        .unwrap_or_else(|| "-".to_string()),
                    cell.total_interior_gaps_median
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    cell.late_period_ratio_median
                        .map(|v| format!("{v:.2}"))
                        .unwrap_or_else(|| "-".to_string()),
                    cell.quality_pass_count_median
                        .map(|v| format!("{v}/4"))
                        .unwrap_or_else(|| "-".to_string()),
                );
                write_row(
                    markdown,
                    spec.fixture,
                    spec.backend,
                    &cell,
                    render_kempe_chain_col,
                );
                all_results.push((spec.clone(), cell));
                successes += 1;
            }
            Err(reason) => {
                eprintln!(
                    "cell error: {} / {}: {reason}",
                    spec.fixture,
                    spec.backend.label()
                );
                write_error_row(
                    markdown,
                    spec.fixture,
                    spec.backend,
                    &reason,
                    render_kempe_chain_col,
                );
            }
        }
    }
    successes
}

fn run_cell_child(raw: Vec<String>) -> ExitCode {
    let args = match parse_cell_args(raw) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("solver-bench --cell: {e}");
            return ExitCode::from(2);
        }
    };
    let build = match fixture_by_name(&args.fixture) {
        Some(b) => b,
        None => {
            eprintln!("solver-bench --cell: unknown fixture '{}'", args.fixture);
            return ExitCode::from(2);
        }
    };
    let mut problem = build();
    if args.teacher_pins == TeacherPinsMode::Off {
        unpin_teachers_in_problem(&mut problem);
    }
    let expected = placements_expected_for_problem(&problem);
    let cell = match args.backend {
        BenchBackend::CpSat => run_cpsat_cell(&problem, expected, args.budget, args.seeds),
        _ => run_lahc_cell(
            args.backend,
            &problem,
            expected,
            args.budget,
            args.seeds,
            LahcCellOverrides {
                rr_k: args.rr_k,
                rr_period: args.rr_period,
                kempe_max_chain: args.kempe_max_chain,
            },
        ),
    };
    let json = serde_json::to_string(&cell).expect("serialise CellResult");
    let mut stdout = std::io::stdout().lock();
    if let Err(e) = stdout.write_all(json.as_bytes()) {
        eprintln!("solver-bench --cell: write stdout: {e}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn read_self_peak_kb() -> u64 {
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
    if rc != 0 {
        return 0;
    }
    let raw = usage.ru_maxrss as u64;
    if cfg!(target_os = "macos") {
        raw / 1024
    } else {
        raw
    }
}

/// Per-cell overrides forwarded from the supervisor's CLI flags into the
/// LAHC dispatch. `None` means "use `SolveConfig::default()` for this knob".
struct LahcCellOverrides {
    rr_k: Option<u32>,
    rr_period: Option<u32>,
    kempe_max_chain: Option<u32>,
}

fn run_lahc_cell(
    backend: BenchBackend,
    problem: &Problem,
    expected: u64,
    budget: Duration,
    seeds: u64,
    overrides: LahcCellOverrides,
) -> CellResult {
    let LahcCellOverrides {
        rr_k: rr_k_override,
        rr_period: rr_period_override,
        kempe_max_chain: kempe_max_chain_override,
    } = overrides;
    let weights = PRODUCTION_ACTIVE_WEIGHTS.clone();
    let greedy_cfg = SolveConfig {
        weights: weights.clone(),
        deadline: None,
        ..SolveConfig::default()
    };

    let ffd_start = Instant::now();
    let _greedy = solve_with_config_stats(problem, &greedy_cfg).expect("greedy solve");
    let ffd_ms = ffd_start.elapsed().as_secs_f64() * 1_000.0;

    let mut total_ms_samples: Vec<f64> = Vec::with_capacity(seeds as usize);
    let mut hard_violations_samples: Vec<u32> = Vec::with_capacity(seeds as usize);
    let mut placements_total_samples: Vec<u64> = Vec::with_capacity(seeds as usize);
    let mut soft_score_feasible: Vec<u32> = Vec::with_capacity(seeds as usize);
    let mut ttf_feasible: Vec<f64> = Vec::with_capacity(seeds as usize);
    let mut tto_feasible: Vec<f64> = Vec::with_capacity(seeds as usize);
    let mut quality_reports: Vec<quality::QualityPredicates> = Vec::with_capacity(seeds as usize);
    let mut component_reports: Vec<quality::QualityReport> = Vec::with_capacity(seeds as usize);
    let mut feasibility_count: u64 = 0;

    let (dispatch_rr_period, lahc_kempe_period) = match backend {
        BenchBackend::Lahc => (None, None),
        BenchBackend::LahcRr => (Some(50u32), None),
        BenchBackend::LahcKempe => (None, Some(23u32)),
        BenchBackend::LahcRrKempe => (Some(50u32), Some(23u32)),
        BenchBackend::CpSat => unreachable!("cpsat dispatched above"),
    };
    // RR-period override only applies on RR-enabled backends (where the dispatch
    // returns Some(_)); on non-RR backends the override is ignored.
    let lahc_rr_period = match dispatch_rr_period {
        Some(_) => rr_period_override.or(dispatch_rr_period),
        None => None,
    };
    let lahc_rr_k = rr_k_override.unwrap_or(SolveConfig::default().lahc_rr_k);
    // Kempe-chain override applies to Kempe-using backends only; on non-Kempe
    // backends the dispatch returns `lahc_kempe_period = None` and the chain
    // depth is unused inside the solver, but we still write the field so the
    // SolveConfig literal stays uniform.
    let lahc_kempe_max_chain =
        kempe_max_chain_override.unwrap_or(SolveConfig::default().lahc_kempe_max_chain);

    for seed in 1..=seeds {
        let cfg = SolveConfig {
            weights: weights.clone(),
            deadline: Some(budget),
            seed,
            lahc_rr_period,
            lahc_kempe_period,
            lahc_rr_k,
            lahc_kempe_max_chain,
            ..SolveConfig::default()
        };
        let start = Instant::now();
        let (solution, stats) = solve_with_config_stats(problem, &cfg).expect("solve");
        let total_ms = start.elapsed().as_secs_f64() * 1_000.0;
        let hard = solution.violations.len() as u32;
        let placements_total = solution.placements.len() as u64;
        debug_assert!(placements_total <= expected);
        let feasible = hard == 0 && placements_total == expected;
        if feasible {
            feasibility_count += 1;
            soft_score_feasible.push(solution.soft_score);
            if let Some(t) = stats.time_to_first_feasible_ms {
                ttf_feasible.push(t);
            }
            if let Some(t) = stats.time_to_optimal_ms {
                tto_feasible.push(t);
            }
            quality_reports.push(quality::evaluate_quality_predicates(problem, &solution));
            component_reports.push(solver_core::quality_report(
                problem,
                &solution.placements,
                &solution.violations,
                &PRODUCTION_ACTIVE_WEIGHTS,
            ));
        }
        hard_violations_samples.push(hard);
        total_ms_samples.push(total_ms);
        placements_total_samples.push(placements_total);
    }

    let (
        worst_spread_median,
        worst_home_room_ratio_median,
        total_interior_gaps_median,
        late_period_ratio_median,
        quality_pass_count_median,
    ) = aggregate_quality_medians(&quality_reports);
    let (
        unplaced_hours_median,
        class_gap_hours_median,
        teacher_gap_hours_median,
        class_day_balance_cost_median,
        home_room_misses_median,
        prefer_early_units_median,
        avoid_first_units_median,
        avoid_last_units_median,
        prefer_late_units_median,
    ) = aggregate_component_medians(&component_reports);

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
        peak_kb: read_self_peak_kb(),
        time_to_first_feasible_ms_median: if ttf_feasible.is_empty() {
            None
        } else {
            Some(median_f64(&mut ttf_feasible))
        },
        time_to_optimal_ms_median: if tto_feasible.is_empty() {
            None
        } else {
            Some(median_f64(&mut tto_feasible))
        },
        worst_spread_median,
        worst_home_room_ratio_median,
        total_interior_gaps_median,
        late_period_ratio_median,
        quality_pass_count_median,
        unplaced_hours_median,
        class_gap_hours_median,
        teacher_gap_hours_median,
        class_day_balance_cost_median,
        home_room_misses_median,
        prefer_early_units_median,
        avoid_first_units_median,
        avoid_last_units_median,
        prefer_late_units_median,
        rr_k: if dispatch_rr_period.is_some() {
            Some(lahc_rr_k)
        } else {
            None
        },
        rr_period: lahc_rr_period,
        kempe_max_chain: if lahc_kempe_period.is_some() {
            kempe_max_chain_override
        } else {
            None
        },
    }
}

fn build_cpsat_command(problem_path: &Path, budget: Duration, seed: u64) -> Command {
    let mut cmd = Command::new("python3");
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

fn tempfile_path(prefix: &str, suffix: &str) -> PathBuf {
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
    let mut ttf_feasible: Vec<f64> = Vec::with_capacity(seeds as usize);
    let mut tto_feasible: Vec<f64> = Vec::with_capacity(seeds as usize);
    let mut quality_reports: Vec<quality::QualityPredicates> = Vec::with_capacity(seeds as usize);
    let mut component_reports: Vec<quality::QualityReport> = Vec::with_capacity(seeds as usize);
    let mut feasibility_count: u64 = 0;
    let mut peak_kb_max: u64 = 0;

    #[derive(Deserialize)]
    struct CpSatJson {
        placements: serde_json::Value,
        violations: Vec<serde_json::Value>,
        soft_score: u32,
        peak_rss_kb: Option<u64>,
        time_to_first_feasible_ms: Option<f64>,
        time_to_optimal_ms: Option<f64>,
    }

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
        let parsed: CpSatJson = match serde_json::from_str(&solution_json) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("cpsat parse error (seed={seed}): {e}");
                hard_violations_samples.push(u32::MAX);
                total_ms_samples.push(total_ms);
                placements_total_samples.push(0);
                continue;
            }
        };
        let placements_total = parsed
            .placements
            .as_array()
            .map(|a| a.len() as u64)
            .unwrap_or(0);
        let hard = parsed.violations.len() as u32;
        debug_assert!(placements_total <= expected);
        let feasible = hard == 0 && placements_total == expected;
        if feasible {
            feasibility_count += 1;
            soft_score_feasible.push(parsed.soft_score);
            if let Some(t) = parsed.time_to_first_feasible_ms {
                ttf_feasible.push(t);
            }
            if let Some(t) = parsed.time_to_optimal_ms {
                tto_feasible.push(t);
            }
            let placements: Vec<solver_core::Placement> =
                serde_json::from_value(parsed.placements.clone())
                    .expect("cpsat placements deserialise into Vec<Placement>");
            let solution = solver_core::Solution {
                placements,
                violations: vec![],
                soft_score: parsed.soft_score,
            };
            quality_reports.push(quality::evaluate_quality_predicates(problem, &solution));
            component_reports.push(solver_core::quality_report(
                problem,
                &solution.placements,
                &solution.violations,
                &PRODUCTION_ACTIVE_WEIGHTS,
            ));
        }
        if let Some(p) = parsed.peak_rss_kb {
            peak_kb_max = peak_kb_max.max(p);
        }
        hard_violations_samples.push(hard);
        total_ms_samples.push(total_ms);
        placements_total_samples.push(placements_total);
    }

    let _ = std::fs::remove_file(&tmpfile);

    let (
        worst_spread_median,
        worst_home_room_ratio_median,
        total_interior_gaps_median,
        late_period_ratio_median,
        quality_pass_count_median,
    ) = aggregate_quality_medians(&quality_reports);
    let (
        unplaced_hours_median,
        class_gap_hours_median,
        teacher_gap_hours_median,
        class_day_balance_cost_median,
        home_room_misses_median,
        prefer_early_units_median,
        avoid_first_units_median,
        avoid_last_units_median,
        prefer_late_units_median,
    ) = aggregate_component_medians(&component_reports);

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
        peak_kb: peak_kb_max,
        time_to_first_feasible_ms_median: if ttf_feasible.is_empty() {
            None
        } else {
            Some(median_f64(&mut ttf_feasible))
        },
        time_to_optimal_ms_median: if tto_feasible.is_empty() {
            None
        } else {
            Some(median_f64(&mut tto_feasible))
        },
        worst_spread_median,
        worst_home_room_ratio_median,
        total_interior_gaps_median,
        late_period_ratio_median,
        quality_pass_count_median,
        unplaced_hours_median,
        class_gap_hours_median,
        teacher_gap_hours_median,
        class_day_balance_cost_median,
        home_room_misses_median,
        prefer_early_units_median,
        avoid_first_units_median,
        avoid_last_units_median,
        prefer_late_units_median,
        rr_k: None,
        rr_period: None,
        kempe_max_chain: None,
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

/// Five-tuple returned by [`aggregate_quality_medians`]: worst spread, worst
/// home-room ratio, total interior gaps, late-period ratio, quality pass count.
type QualityMedians = (
    Option<u32>,
    Option<f64>,
    Option<u32>,
    Option<f64>,
    Option<u32>,
);

fn aggregate_quality_medians(reports: &[quality::QualityPredicates]) -> QualityMedians {
    if reports.is_empty() {
        return (None, None, None, None, None);
    }
    let mut spreads: Vec<u32> = reports.iter().map(|r| r.worst_spread).collect();
    let worst_spread = Some(median_u32(&mut spreads));

    let mut home_room_ratios: Vec<f64> = reports
        .iter()
        .filter_map(|r| r.worst_home_room_ratio)
        .collect();
    let worst_home_room_ratio = if home_room_ratios.is_empty() {
        None
    } else {
        Some(median_f64(&mut home_room_ratios))
    };

    let mut gaps: Vec<u32> = reports.iter().map(|r| r.total_interior_gaps).collect();
    let total_interior_gaps = Some(median_u32(&mut gaps));

    let mut late: Vec<f64> = reports.iter().filter_map(|r| r.late_period_ratio).collect();
    let late_period_ratio = if late.is_empty() {
        None
    } else {
        Some(median_f64(&mut late))
    };

    let mut pass_counts: Vec<u32> = reports.iter().map(quality::quality_pass_count).collect();
    let quality_pass_count = Some(median_u32(&mut pass_counts));

    (
        worst_spread,
        worst_home_room_ratio,
        total_interior_gaps,
        late_period_ratio,
        quality_pass_count,
    )
}

/// Nine-tuple returned by [`aggregate_component_medians`]: one entry per
/// `QualityReport` field that does not already have a CellResult median
/// (`hard_violations` and `weighted_score` are mirrored by the existing
/// `hard_violations_median` and `soft_score_median`).
type ComponentMedians = (
    Option<u32>, // unplaced_hours
    Option<u32>, // class_gap_hours
    Option<u32>, // teacher_gap_hours
    Option<u32>, // class_day_balance_cost
    Option<u32>, // home_room_misses
    Option<u32>, // prefer_early_units
    Option<u32>, // avoid_first_units
    Option<u32>, // avoid_last_units
    Option<u32>, // prefer_late_units
);

fn aggregate_component_medians(reports: &[quality::QualityReport]) -> ComponentMedians {
    if reports.is_empty() {
        return (None, None, None, None, None, None, None, None, None);
    }
    let median = |samples: Vec<u32>| -> Option<u32> {
        if samples.is_empty() {
            None
        } else {
            let mut s = samples;
            Some(median_u32(&mut s))
        }
    };
    (
        median(reports.iter().map(|r| r.unplaced_hours).collect()),
        median(reports.iter().map(|r| r.class_gap_hours).collect()),
        median(reports.iter().map(|r| r.teacher_gap_hours).collect()),
        median(reports.iter().map(|r| r.class_day_balance_cost).collect()),
        median(reports.iter().map(|r| r.home_room_misses).collect()),
        median(reports.iter().map(|r| r.prefer_early_units).collect()),
        median(reports.iter().map(|r| r.avoid_first_units).collect()),
        median(reports.iter().map(|r| r.avoid_last_units).collect()),
        median(reports.iter().map(|r| r.prefer_late_units).collect()),
    )
}

fn median_f64(values: &mut [f64]) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    values[mid]
}

/// Render the static "Backend objectives" section that lives above the
/// bake-off table. Sourced from `solver_core::backend_objective(name)`;
/// renders one row per known backend showing optimised / declared-skipped
/// canonical components plus a one-sentence rationale. Item 51.
fn write_backend_objectives_section(out: &mut String, backends: &[BenchBackend]) {
    out.push_str("## Backend objectives\n\n");
    out.push_str(
        "Each backend's *internal* acceptance criterion or model objective optimises \
         the listed canonical components. Components in `declared_skipped` are not \
         part of the backend's own search loop today; they are still recomputed \
         post-solve by `quality_report(...)` and contribute to the `Soft score` \
         column, so a backend can score badly on a skipped axis without that being \
         a bug. Items 48, 52, 54 move skipped components into `optimised`.\n\n",
    );
    out.push_str("| Backend | Optimised | Declared skipped | Notes |\n");
    out.push_str("| --- | --- | --- | --- |\n");
    for backend in backends {
        let label = backend.label();
        let bo = solver_core::backend_objective(label)
            .unwrap_or_else(|| panic!("backend_objective({label:?}) must be registered"));
        let render_set = |s: &std::collections::BTreeSet<solver_core::QualityComponent>| {
            if s.is_empty() {
                "(none)".to_string()
            } else {
                s.iter()
                    .map(|c| c.component_label())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            label,
            render_set(&bo.optimised),
            render_set(&bo.declared_skipped),
            bo.notes,
        ));
    }
    out.push('\n');
}

fn write_title_and_intro(out: &mut String) {
    out.push_str("# Solver bake-off feasibility bench\n\n");
    out.push_str("<!-- Regenerated by `mise run bench:bakeoff`. Do not hand-edit. -->\n\n");
}

fn write_table_header(out: &mut String, render_kempe_chain_col: bool) {
    if render_kempe_chain_col {
        out.push_str(
            "| Fixture | Backend | RR_K | Period | Kempe Chain | Seeds | Feasibility | Hard violations (median) | Placements (median / expected) | Soft score (median, feasible) | Class gap h (median) | Teacher gap h (median) | Home room miss (median) | Day balance (median) | FFD wall-clock (ms, median) | Total wall-clock (ms, median) | Peak RSS (kB) | Time to first feasible (ms, median) | Time to optimal (ms, median) | Worst spread (median) | Worst home-room ratio (median) | Total interior gaps (median) | Late-period ratio (median) | Quality (pass / 4) |\n",
        );
        out.push_str(
            "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n",
        );
    } else {
        out.push_str(
            "| Fixture | Backend | RR_K | Period | Seeds | Feasibility | Hard violations (median) | Placements (median / expected) | Soft score (median, feasible) | Class gap h (median) | Teacher gap h (median) | Home room miss (median) | Day balance (median) | FFD wall-clock (ms, median) | Total wall-clock (ms, median) | Peak RSS (kB) | Time to first feasible (ms, median) | Time to optimal (ms, median) | Worst spread (median) | Worst home-room ratio (median) | Total interior gaps (median) | Late-period ratio (median) | Quality (pass / 4) |\n",
        );
        out.push_str(
            "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n",
        );
    }
}

fn write_row(
    out: &mut String,
    fixture: &str,
    backend: BenchBackend,
    cell: &CellResult,
    render_kempe_chain_col: bool,
) {
    let soft = match cell.soft_score_median {
        Some(s) => s.to_string(),
        None => "-".to_string(),
    };
    let class_gap_h = match cell.class_gap_hours_median {
        Some(v) => v.to_string(),
        None => "-".to_string(),
    };
    let teacher_gap_h = match cell.teacher_gap_hours_median {
        Some(v) => v.to_string(),
        None => "-".to_string(),
    };
    let home_room_miss = match cell.home_room_misses_median {
        Some(v) => v.to_string(),
        None => "-".to_string(),
    };
    let day_balance = match cell.class_day_balance_cost_median {
        Some(v) => v.to_string(),
        None => "-".to_string(),
    };
    let ttf = match cell.time_to_first_feasible_ms_median {
        Some(v) => format!("{v:.0}"),
        None => "-".to_string(),
    };
    let tto = match cell.time_to_optimal_ms_median {
        Some(v) => format!("{v:.0}"),
        None => "-".to_string(),
    };
    let worst_spread = match cell.worst_spread_median {
        Some(v) => v.to_string(),
        None => "-".to_string(),
    };
    let worst_home = match cell.worst_home_room_ratio_median {
        Some(v) => format!("{v:.2}"),
        None => "-".to_string(),
    };
    let gaps = match cell.total_interior_gaps_median {
        Some(v) => v.to_string(),
        None => "-".to_string(),
    };
    let late = match cell.late_period_ratio_median {
        Some(v) => format!("{v:.2}"),
        None => "-".to_string(),
    };
    let quality = match cell.quality_pass_count_median {
        Some(v) => format!("{v}/4"),
        None => "-".to_string(),
    };
    let rr_k_col = match cell.rr_k {
        Some(v) => v.to_string(),
        None => "-".to_string(),
    };
    let rr_period_col = match cell.rr_period {
        Some(v) => v.to_string(),
        None => "-".to_string(),
    };
    if render_kempe_chain_col {
        let kempe_chain_col = match cell.kempe_max_chain {
            Some(v) => v.to_string(),
            None => "-".to_string(),
        };
        out.push_str(&format!(
            "| {fixture} | {backend} | {rr_k_col} | {rr_period_col} | {kempe_chain_col} | {seeds} | {n}/{seeds} | {hard} | {placed}/{expected} | {soft} | {class_gap_h} | {teacher_gap_h} | {home_room_miss} | {day_balance} | {ffd:.2} | {total:.0} | {peak} | {ttf} | {tto} | {worst_spread} | {worst_home} | {gaps} | {late} | {quality} |\n",
            backend = backend.label(),
            seeds = cell.seeds,
            n = cell.feasibility_count,
            hard = cell.hard_violations_median,
            placed = cell.placements_total_median,
            expected = cell.placements_expected,
            ffd = cell.ffd_ms_median,
            total = cell.total_ms_median,
            peak = cell.peak_kb,
        ));
    } else {
        out.push_str(&format!(
            "| {fixture} | {backend} | {rr_k_col} | {rr_period_col} | {seeds} | {n}/{seeds} | {hard} | {placed}/{expected} | {soft} | {class_gap_h} | {teacher_gap_h} | {home_room_miss} | {day_balance} | {ffd:.2} | {total:.0} | {peak} | {ttf} | {tto} | {worst_spread} | {worst_home} | {gaps} | {late} | {quality} |\n",
            backend = backend.label(),
            seeds = cell.seeds,
            n = cell.feasibility_count,
            hard = cell.hard_violations_median,
            placed = cell.placements_total_median,
            expected = cell.placements_expected,
            ffd = cell.ffd_ms_median,
            total = cell.total_ms_median,
            peak = cell.peak_kb,
        ));
    }
}

fn write_error_row(
    out: &mut String,
    fixture: &str,
    backend: BenchBackend,
    _reason: &str,
    render_kempe_chain_col: bool,
) {
    if render_kempe_chain_col {
        out.push_str(&format!(
            "| {fixture} | {backend} | - | - | - | - | panic | - | - | - | - | - | - | - | - | - | - | - | - | - | - | - | - | - |\n",
            backend = backend.label(),
        ));
    } else {
        out.push_str(&format!(
            "| {fixture} | {backend} | - | - | - | panic | - | - | - | - | - | - | - | - | - | - | - | - | - | - | - | - | - |\n",
            backend = backend.label(),
        ));
    }
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
        "bench is host-sensitive on wall-clock and Peak RSS columns and host-stable on feasibility / hard-violation\n",
    );
    out.push_str(
        "columns. Each cell runs in its own subprocess so Peak RSS reflects only that cell. Linux\n",
    );
    out.push_str(
        "`ru_maxrss` is kilobytes; macOS is bytes (the bench normalises to kilobytes). Time to first\n",
    );
    out.push_str(
        "feasible and Time to optimal are medians over feasible seeds; '-' marks no feasible seed.\n\n",
    );
    out.push_str(
        "Quality columns (rightmost five): per-cell median across feasible seeds. Predicates pass at\n",
    );
    out.push_str(
        "worst spread <= 2, worst home-room ratio >= 0.6, total interior gaps <= 2, late-period ratio >= 0.5.\n",
    );
    out.push_str(
        "Late-period ratio is the median normalised position (`position / max_position_per_day`) of all\n",
    );
    out.push_str(
        "placements of subjects with `Subject.prefer_late_period > 0`; rendered as `-` when no fixture\n",
    );
    out.push_str(
        "subject has the axis enabled, and that case counts as pass for the composite Quality column.\n",
    );
    out.push_str(
        "Cells whose subprocess fails (panic, non-zero exit, JSON parse error) render `panic` in the\n",
    );
    out.push_str(
        "Feasibility column with `-` in every other numeric column. The supervisor logs the underlying\n",
    );
    out.push_str("reason to stderr and continues to the next cell.\n\n");
    out.push_str(
        "Home-room ratio exempts subjects whose `room_subject_suitabilities` exclude the class's\n",
    );
    out.push_str(
        "`home_room_id` (e.g. gym / Werkraum / Musikraum on Grundschule). Mirrors `quality_checks.py`\n",
    );
    out.push_str(
        "predicates by intent; implementations are intentionally separate (Python operates on persisted\n",
    );
    out.push_str("ORM rows, Rust on the in-memory `Solution`).\n\n");
    out.push_str("See `docs/adr/0029-solver-feasibility-bake-off.md` for methodology and `docs/adr/0034-bench-cell-subprocess-and-observability.md` for the cell-subprocess architecture.\n");
}

/// One point on the (period, K) sweep grid; ordered by `(period, k)` so the
/// helpers can break ties deterministically by smallest period, then smallest K.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SweepTuple {
    period: u32,
    k: u32,
}

/// Return non-dominated `(k, period)` tuples for one fixture's RR cells.
/// Domination on a single fixture: A dominates B iff
/// `(A.feasibility > B.feasibility AND A.soft <= B.soft)`
/// OR `(A.feasibility >= B.feasibility AND A.soft < B.soft)`.
/// Cells without an `rr_k` / `rr_period` are skipped. Cells with no
/// `soft_score_median` (zero feasible seeds) get treated as "infinitely soft":
/// any feasible cell with feasibility >= dominates it.
fn sweep_pareto_non_dominated(cells: &[&CellResult]) -> Vec<SweepTuple> {
    let candidates: Vec<(SweepTuple, u64, Option<u32>)> = cells
        .iter()
        .filter_map(|c| match (c.rr_k, c.rr_period) {
            (Some(k), Some(period)) => Some((
                SweepTuple { period, k },
                c.feasibility_count,
                c.soft_score_median,
            )),
            _ => None,
        })
        .collect();
    let dominates =
        |(a_feas, a_soft): (u64, Option<u32>), (b_feas, b_soft): (u64, Option<u32>)| -> bool {
            // Treat None as +inf for soft score.
            let cmp_soft_le = match (a_soft, b_soft) {
                (Some(a), Some(b)) => a <= b,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => true,
            };
            let cmp_soft_lt = match (a_soft, b_soft) {
                (Some(a), Some(b)) => a < b,
                (Some(_), None) => true,
                (None, _) => false,
            };
            (a_feas > b_feas && cmp_soft_le) || (a_feas >= b_feas && cmp_soft_lt)
        };
    let mut frontier: Vec<SweepTuple> = Vec::new();
    for (i, (tup_a, feas_a, soft_a)) in candidates.iter().enumerate() {
        let dominated = candidates
            .iter()
            .enumerate()
            .any(|(j, (_, feas_b, soft_b))| {
                i != j && dominates((*feas_b, *soft_b), (*feas_a, *soft_a))
            });
        if !dominated && !frontier.contains(tup_a) {
            frontier.push(*tup_a);
        }
    }
    frontier.sort();
    frontier
}

/// Recommend a `(k, period)` across all fixtures' RR cells.
/// Algorithm: per (k, period), mean of `soft_score_median` across feasible cells
/// (only cells where the cell has feasibility_count > 0 and a soft_score_median).
/// A (k, period) is eligible only if it appears feasible on every fixture.
/// Pick min mean; ties broken by smallest period, then smallest K.
///
/// Chain-depth axis is not part of the recommendation tie-break: a sweep that
/// mixes multiple `--kempe-max-chain` values on the same fixture+(K, period)
/// collapses all depths into one mean. Extending `SweepTuple` to include chain
/// depth would either widen the tuple for non-Kempe `LahcRr` cells (which never
/// carry chain depth) or split into parallel groupings; land that if a future
/// Kempe-tuning sweep proves the simplification harmful.
fn sweep_recommend(rr_cells: &[(&str, &CellResult)], fixture_count: usize) -> Option<SweepTuple> {
    use std::collections::BTreeMap;
    // (period, k) -> Vec<(fixture, soft)>
    let mut grouped: BTreeMap<SweepTuple, Vec<(String, u32)>> = BTreeMap::new();
    for (fixture, cell) in rr_cells {
        let (k, period) = match (cell.rr_k, cell.rr_period) {
            (Some(k), Some(p)) => (k, p),
            _ => continue,
        };
        if cell.feasibility_count == 0 {
            continue;
        }
        let soft = match cell.soft_score_median {
            Some(s) => s,
            None => continue,
        };
        grouped
            .entry(SweepTuple { period, k })
            .or_default()
            .push((fixture.to_string(), soft));
    }
    let mut best: Option<(f64, SweepTuple)> = None;
    for (tup, samples) in grouped {
        // Distinct fixtures with feasibility for this tuple.
        let mut distinct: Vec<&String> = samples.iter().map(|(f, _)| f).collect();
        distinct.sort();
        distinct.dedup();
        if distinct.len() < fixture_count {
            continue;
        }
        let sum: u64 = samples.iter().map(|(_, s)| u64::from(*s)).sum();
        let mean = sum as f64 / samples.len() as f64;
        let candidate = (mean, tup);
        match best {
            None => best = Some(candidate),
            Some((m, t)) => {
                if mean < m || (mean == m && tup < t) {
                    best = Some(candidate);
                }
            }
        }
    }
    best.map(|(_, t)| t)
}

fn write_pareto_and_recommendation(
    out: &mut String,
    fixtures: &[String],
    all_results: &[(CellSpec, CellResult)],
) {
    out.push_str("\n## Pareto frontier\n");
    for fixture in fixtures {
        let cells: Vec<&CellResult> = all_results
            .iter()
            .filter(|(spec, _)| spec.fixture == fixture)
            .filter(|(spec, _)| {
                matches!(
                    spec.backend,
                    BenchBackend::LahcRr | BenchBackend::LahcRrKempe
                )
            })
            .map(|(_, c)| c)
            .collect();
        let frontier = sweep_pareto_non_dominated(&cells);
        out.push_str(&format!("\n### {fixture}\n"));
        if frontier.is_empty() {
            out.push_str("- (no feasible cells)\n");
        } else {
            for t in &frontier {
                out.push_str(&format!("- (K={}, period={})\n", t.k, t.period));
            }
        }
    }
    out.push_str("\n## Recommendation\n\n");
    let rr_cells: Vec<(&str, &CellResult)> = all_results
        .iter()
        .filter(|(spec, _)| {
            matches!(
                spec.backend,
                BenchBackend::LahcRr | BenchBackend::LahcRrKempe
            )
        })
        .map(|(spec, c)| (spec.fixture, c))
        .collect();
    match sweep_recommend(&rr_cells, fixtures.len()) {
        Some(t) => out.push_str(&format!(
            "Default `lahc_rr_k = {}, lahc_rr_period = Some({})` (minimises mean soft_score_median across feasible fixtures; tie-break: smallest period, then smallest K).\n",
            t.k, t.period,
        )),
        None => out.push_str("No clear winner; defaults unchanged.\n"),
    }
}

/// Today's date as `YYYY-MM-DD`. Reuses `chrono_today` (calls `date -Idate`),
/// falls back to a hardcoded string when `date` is unavailable.
fn today_yyyy_mm_dd() -> String {
    chrono_today()
}

fn read_cpu() -> Option<String> {
    fs::read_to_string("/proc/cpuinfo").ok().and_then(|c| {
        c.lines()
            .find_map(|l| l.strip_prefix("model name").and_then(|s| s.split_once(':')))
            .map(|(_, v)| v.trim().to_string())
    })
}

fn read_kernel() -> Option<String> {
    Command::new("uname").arg("-r").output().ok().and_then(|o| {
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
    Command::new("rustc")
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
    Command::new("date")
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
    fn parse_supervisor_args_reads_all_flags() {
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
        let args = parse_supervisor_args(raw).unwrap();
        assert_eq!(args.budget, Duration::from_secs(5));
        assert_eq!(args.seeds, 4);
        assert_eq!(
            args.fixtures,
            vec!["grundschule".to_string(), "lock_in".to_string()]
        );
        assert_eq!(args.out, PathBuf::from("/tmp/out.md"));
    }

    #[test]
    fn parse_supervisor_args_rejects_unknown_flag() {
        let raw = vec!["--unknown".to_string()];
        assert!(parse_supervisor_args(raw).is_err());
    }

    #[test]
    fn parse_cell_args_reads_fixture_backend_budget_seeds() {
        let raw = vec![
            "--cell".to_string(),
            "grundschule".to_string(),
            "--backend".to_string(),
            "lahc_rr".to_string(),
            "--budget".to_string(),
            "200ms".to_string(),
            "--seeds".to_string(),
            "3".to_string(),
        ];
        let args = parse_cell_args(raw).unwrap();
        assert_eq!(args.fixture, "grundschule");
        assert_eq!(args.backend, BenchBackend::LahcRr);
        assert_eq!(args.budget, Duration::from_millis(200));
        assert_eq!(args.seeds, 3);
        assert_eq!(args.teacher_pins, TeacherPinsMode::On);
    }

    #[test]
    fn parse_cell_args_accepts_teacher_pins_off() {
        let raw = vec![
            "--cell".to_string(),
            "grundschule".to_string(),
            "--backend".to_string(),
            "lahc".to_string(),
            "--budget".to_string(),
            "100ms".to_string(),
            "--seeds".to_string(),
            "1".to_string(),
            "--teacher-pins".to_string(),
            "off".to_string(),
        ];
        let args = parse_cell_args(raw).unwrap();
        assert_eq!(args.teacher_pins, TeacherPinsMode::Off);
    }

    #[test]
    fn median_u32_returns_middle_value() {
        let mut v = vec![5, 1, 3];
        assert_eq!(median_u32(&mut v), 3);
    }

    #[test]
    fn write_header_includes_three_new_columns() {
        let mut out = String::new();
        write_table_header(&mut out, false);
        assert!(out.contains("Peak RSS (kB)"), "missing peak header: {out}");
        assert!(
            out.contains("Time to first feasible"),
            "missing ttf header: {out}"
        );
        assert!(out.contains("Time to optimal"), "missing tto header: {out}");
    }

    #[test]
    fn write_row_renders_observability_columns() {
        let cell = CellResult {
            seeds: 20,
            feasibility_count: 20,
            hard_violations_median: 0,
            placements_total_median: 45,
            placements_expected: 45,
            soft_score_median: Some(0),
            ffd_ms_median: 0.13,
            total_ms_median: 60000.0,
            peak_kb: 49152,
            time_to_first_feasible_ms_median: Some(0.4),
            time_to_optimal_ms_median: Some(1500.0),
            worst_spread_median: None,
            worst_home_room_ratio_median: None,
            total_interior_gaps_median: None,
            late_period_ratio_median: None,
            quality_pass_count_median: None,
            unplaced_hours_median: None,
            class_gap_hours_median: None,
            teacher_gap_hours_median: None,
            class_day_balance_cost_median: None,
            home_room_misses_median: None,
            prefer_early_units_median: None,
            avoid_first_units_median: None,
            avoid_last_units_median: None,
            prefer_late_units_median: None,
            rr_k: None,
            rr_period: None,
            kempe_max_chain: None,
        };
        let mut out = String::new();
        write_row(
            &mut out,
            "grundschule",
            BenchBackend::LahcRrKempe,
            &cell,
            false,
        );
        assert!(out.contains("| 49152 |"), "missing peak: {out}");
        assert!(out.contains("| 0 |"), "missing ttf rounded to 0 ms: {out}");
        assert!(out.contains("| 1500 |"), "missing tto: {out}");
    }

    #[test]
    fn write_row_renders_dash_when_no_feasible_seed() {
        let cell = CellResult {
            seeds: 20,
            feasibility_count: 0,
            hard_violations_median: 1,
            placements_total_median: 0,
            placements_expected: 0,
            soft_score_median: None,
            ffd_ms_median: 0.05,
            total_ms_median: 60050.0,
            peak_kb: 49152,
            time_to_first_feasible_ms_median: None,
            time_to_optimal_ms_median: None,
            worst_spread_median: None,
            worst_home_room_ratio_median: None,
            total_interior_gaps_median: None,
            late_period_ratio_median: None,
            quality_pass_count_median: None,
            unplaced_hours_median: None,
            class_gap_hours_median: None,
            teacher_gap_hours_median: None,
            class_day_balance_cost_median: None,
            home_room_misses_median: None,
            prefer_early_units_median: None,
            avoid_first_units_median: None,
            avoid_last_units_median: None,
            prefer_late_units_median: None,
            rr_k: None,
            rr_period: None,
            kempe_max_chain: None,
        };
        let mut out = String::new();
        write_row(&mut out, "grundschule", BenchBackend::Lahc, &cell, false);
        assert!(out.contains("| 0/20 |"));
        assert!(out.contains("| 49152 |"));
        // Three dash-cells in a row: soft_score, ttf, tto.
        assert!(out.contains("| - |"));
    }

    #[test]
    fn cell_result_round_trips_through_json() {
        let cell = CellResult {
            seeds: 4,
            feasibility_count: 4,
            hard_violations_median: 0,
            placements_total_median: 45,
            placements_expected: 45,
            soft_score_median: Some(15),
            ffd_ms_median: 0.5,
            total_ms_median: 60000.0,
            peak_kb: 12345,
            time_to_first_feasible_ms_median: Some(2.5),
            time_to_optimal_ms_median: Some(40.0),
            worst_spread_median: Some(2),
            worst_home_room_ratio_median: Some(0.75),
            total_interior_gaps_median: Some(1),
            late_period_ratio_median: Some(0.6),
            quality_pass_count_median: Some(4),
            unplaced_hours_median: None,
            class_gap_hours_median: None,
            teacher_gap_hours_median: None,
            class_day_balance_cost_median: None,
            home_room_misses_median: None,
            prefer_early_units_median: None,
            avoid_first_units_median: None,
            avoid_last_units_median: None,
            prefer_late_units_median: None,
            rr_k: None,
            rr_period: None,
            kempe_max_chain: None,
        };
        let s = serde_json::to_string(&cell).unwrap();
        let back: CellResult = serde_json::from_str(&s).unwrap();
        assert_eq!(back, cell);
    }

    #[test]
    fn write_header_includes_five_quality_columns() {
        let mut out = String::new();
        write_table_header(&mut out, false);
        assert!(
            out.contains("Worst spread (median)"),
            "missing worst-spread header: {out}"
        );
        assert!(
            out.contains("Worst home-room ratio (median)"),
            "missing home-room header: {out}"
        );
        assert!(
            out.contains("Total interior gaps (median)"),
            "missing gaps header: {out}"
        );
        assert!(
            out.contains("Late-period ratio (median)"),
            "missing late-period header: {out}"
        );
        assert!(
            out.contains("Quality (pass / 4)"),
            "missing quality column header: {out}"
        );
    }

    #[test]
    fn write_row_renders_quality_columns() {
        let cell = CellResult {
            seeds: 20,
            feasibility_count: 20,
            hard_violations_median: 0,
            placements_total_median: 45,
            placements_expected: 45,
            soft_score_median: Some(0),
            ffd_ms_median: 0.13,
            total_ms_median: 60000.0,
            peak_kb: 49152,
            time_to_first_feasible_ms_median: Some(0.4),
            time_to_optimal_ms_median: Some(1500.0),
            worst_spread_median: Some(2),
            worst_home_room_ratio_median: Some(0.75),
            total_interior_gaps_median: Some(1),
            late_period_ratio_median: Some(0.6),
            quality_pass_count_median: Some(4),
            unplaced_hours_median: None,
            class_gap_hours_median: None,
            teacher_gap_hours_median: None,
            class_day_balance_cost_median: None,
            home_room_misses_median: None,
            prefer_early_units_median: None,
            avoid_first_units_median: None,
            avoid_last_units_median: None,
            prefer_late_units_median: None,
            rr_k: None,
            rr_period: None,
            kempe_max_chain: None,
        };
        let mut out = String::new();
        write_row(
            &mut out,
            "grundschule",
            BenchBackend::LahcRrKempe,
            &cell,
            false,
        );
        assert!(out.contains("| 2 |"), "missing worst spread: {out}");
        assert!(out.contains("| 0.75 |"), "missing home-room ratio: {out}");
        assert!(out.contains("| 1 |"), "missing interior gaps: {out}");
        assert!(
            out.contains("| 0.60 |") || out.contains("| 0.6 |"),
            "missing late-period: {out}"
        );
        assert!(out.contains("| 4/4 |"), "missing quality pass count: {out}");
    }

    #[test]
    fn write_row_renders_dash_when_quality_fields_are_none() {
        let cell = CellResult {
            seeds: 20,
            feasibility_count: 0,
            hard_violations_median: 1,
            placements_total_median: 0,
            placements_expected: 0,
            soft_score_median: None,
            ffd_ms_median: 0.05,
            total_ms_median: 60050.0,
            peak_kb: 49152,
            time_to_first_feasible_ms_median: None,
            time_to_optimal_ms_median: None,
            worst_spread_median: None,
            worst_home_room_ratio_median: None,
            total_interior_gaps_median: None,
            late_period_ratio_median: None,
            quality_pass_count_median: None,
            unplaced_hours_median: None,
            class_gap_hours_median: None,
            teacher_gap_hours_median: None,
            class_day_balance_cost_median: None,
            home_room_misses_median: None,
            prefer_early_units_median: None,
            avoid_first_units_median: None,
            avoid_last_units_median: None,
            prefer_late_units_median: None,
            rr_k: None,
            rr_period: None,
            kempe_max_chain: None,
        };
        let mut out = String::new();
        write_row(&mut out, "grundschule", BenchBackend::Lahc, &cell, false);
        // Five dash-cells appended at the right end (worst spread, home-room
        // ratio, interior gaps, late-period, quality).
        let dash_count = out.matches("| - |").count();
        assert!(
            dash_count >= 5,
            "expected at least 5 dashes for quality cells: {out}"
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
            Duration::from_secs(60),
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

    fn synthetic_cell_for_resilience_tests(seeds: u64) -> CellResult {
        CellResult {
            seeds,
            feasibility_count: seeds,
            hard_violations_median: 0,
            placements_total_median: 45,
            placements_expected: 45,
            soft_score_median: Some(0),
            ffd_ms_median: 0.13,
            total_ms_median: 60_000.0,
            peak_kb: 49_152,
            time_to_first_feasible_ms_median: Some(1.0),
            time_to_optimal_ms_median: Some(2.0),
            worst_spread_median: None,
            worst_home_room_ratio_median: None,
            total_interior_gaps_median: None,
            late_period_ratio_median: None,
            quality_pass_count_median: None,
            unplaced_hours_median: None,
            class_gap_hours_median: None,
            teacher_gap_hours_median: None,
            class_day_balance_cost_median: None,
            home_room_misses_median: None,
            prefer_early_units_median: None,
            avoid_first_units_median: None,
            avoid_last_units_median: None,
            prefer_late_units_median: None,
            rr_k: None,
            rr_period: None,
            kempe_max_chain: None,
        }
    }

    fn resilience_spec(fixture: &'static str, backend: BenchBackend) -> CellSpec {
        let (rr_k, rr_period) = match backend {
            BenchBackend::LahcRr | BenchBackend::LahcRrKempe => (Some(5u32), Some(25u32)),
            _ => (None, None),
        };
        CellSpec {
            fixture,
            backend,
            rr_k,
            rr_period,
            kempe_max_chain: None,
        }
    }

    #[test]
    fn supervisor_renders_panic_row_and_continues_on_cell_error() {
        let plan = vec![
            resilience_spec("grundschule", BenchBackend::Lahc),
            resilience_spec("grundschule", BenchBackend::LahcRr),
            resilience_spec("grundschule", BenchBackend::LahcRrKempe),
        ];
        let mut runner = |spec: &CellSpec| -> Result<CellResult, String> {
            if matches!(spec.backend, BenchBackend::LahcRr) {
                Err("synthetic-panic: cell exited with non-zero".to_string())
            } else {
                Ok(synthetic_cell_for_resilience_tests(20))
            }
        };
        let mut markdown = String::new();
        let mut all_results: Vec<(CellSpec, CellResult)> = Vec::new();
        let successes = render_cells_with_specs(
            &plan,
            &mut runner,
            &mut markdown,
            &mut all_results,
            false,
            "on",
        );
        assert_eq!(successes, 2, "two cells should have succeeded");
        assert!(
            markdown.contains("| grundschule | lahc | - | - | 20 | 20/20 |"),
            "missing surviving lahc row: {markdown}"
        );
        assert!(
            markdown.contains("| grundschule | lahc_rr_kempe | - | - | 20 | 20/20 |"),
            "missing surviving lahc_rr_kempe row: {markdown}"
        );
        assert!(
            markdown.contains("| grundschule | lahc_rr | - | - | - | panic |"),
            "missing panic placeholder for failed cell: {markdown}"
        );
    }

    #[test]
    fn supervisor_returns_zero_successes_when_every_cell_panics() {
        let plan = vec![
            resilience_spec("grundschule", BenchBackend::Lahc),
            resilience_spec("grundschule", BenchBackend::LahcRr),
        ];
        let mut runner = |_spec: &CellSpec| -> Result<CellResult, String> {
            Err("everything is on fire".to_string())
        };
        let mut markdown = String::new();
        let mut all_results: Vec<(CellSpec, CellResult)> = Vec::new();
        let successes = render_cells_with_specs(
            &plan,
            &mut runner,
            &mut markdown,
            &mut all_results,
            false,
            "on",
        );
        assert_eq!(successes, 0);
        let panic_row_count = markdown.matches("| panic |").count();
        assert_eq!(
            panic_row_count, 2,
            "every plan entry should render a panic row: {markdown}"
        );
    }

    #[test]
    fn write_footer_documents_panic_token() {
        let mut out = String::new();
        write_footer(&mut out);
        assert!(
            out.contains("panic"),
            "footer must document the panic token: {out}"
        );
        assert!(
            out.contains("Feasibility"),
            "footer must reference the Feasibility column: {out}"
        );
    }

    #[test]
    fn cell_result_serialises_nine_new_quality_report_medians() {
        let cell = CellResult {
            seeds: 4,
            feasibility_count: 4,
            hard_violations_median: 0,
            placements_total_median: 45,
            placements_expected: 45,
            soft_score_median: Some(90),
            ffd_ms_median: 1.0,
            total_ms_median: 2.0,
            peak_kb: 1024,
            time_to_first_feasible_ms_median: Some(1.0),
            time_to_optimal_ms_median: Some(2.0),
            worst_spread_median: Some(2),
            worst_home_room_ratio_median: Some(0.7),
            total_interior_gaps_median: Some(0),
            late_period_ratio_median: None,
            quality_pass_count_median: Some(4),
            unplaced_hours_median: Some(0),
            class_gap_hours_median: Some(1),
            teacher_gap_hours_median: Some(2),
            class_day_balance_cost_median: Some(3),
            home_room_misses_median: Some(4),
            prefer_early_units_median: Some(5),
            avoid_first_units_median: Some(6),
            avoid_last_units_median: Some(7),
            prefer_late_units_median: Some(8),
            rr_k: None,
            rr_period: None,
            kempe_max_chain: None,
        };
        let json = serde_json::to_string(&cell).expect("serialise");
        let parsed: CellResult = serde_json::from_str(&json).expect("parse");
        assert_eq!(cell, parsed);
    }

    #[test]
    fn aggregate_component_medians_returns_per_field_medians() {
        let reports = vec![
            solver_core::QualityReport {
                unplaced_hours: 0,
                class_gap_hours: 1,
                teacher_gap_hours: 2,
                class_day_balance_cost: 3,
                home_room_misses: 4,
                prefer_early_units: 5,
                avoid_first_units: 6,
                avoid_last_units: 7,
                prefer_late_units: 8,
                ..solver_core::QualityReport::default()
            },
            solver_core::QualityReport {
                unplaced_hours: 0,
                class_gap_hours: 3,
                teacher_gap_hours: 4,
                class_day_balance_cost: 5,
                home_room_misses: 6,
                prefer_early_units: 7,
                avoid_first_units: 8,
                avoid_last_units: 9,
                prefer_late_units: 10,
                ..solver_core::QualityReport::default()
            },
            solver_core::QualityReport {
                unplaced_hours: 0,
                class_gap_hours: 2,
                teacher_gap_hours: 3,
                class_day_balance_cost: 4,
                home_room_misses: 5,
                prefer_early_units: 6,
                avoid_first_units: 7,
                avoid_last_units: 8,
                prefer_late_units: 9,
                ..solver_core::QualityReport::default()
            },
        ];
        let medians = aggregate_component_medians(&reports);
        // Three samples sorted; median is the middle one.
        assert_eq!(medians.0, Some(0)); // unplaced_hours
        assert_eq!(medians.1, Some(2)); // class_gap_hours
        assert_eq!(medians.2, Some(3)); // teacher_gap_hours
        assert_eq!(medians.3, Some(4)); // class_day_balance_cost
        assert_eq!(medians.4, Some(5)); // home_room_misses
        assert_eq!(medians.5, Some(6)); // prefer_early_units
        assert_eq!(medians.6, Some(7)); // avoid_first_units
        assert_eq!(medians.7, Some(8)); // avoid_last_units
        assert_eq!(medians.8, Some(9)); // prefer_late_units
    }

    #[test]
    fn aggregate_component_medians_returns_none_on_empty_input() {
        let medians = aggregate_component_medians(&[]);
        assert_eq!(
            medians,
            (None, None, None, None, None, None, None, None, None)
        );
    }

    fn sweep_test_cell(k: u32, period: u32, feasibility_count: u64, soft: u32) -> CellResult {
        CellResult {
            seeds: 4,
            feasibility_count,
            hard_violations_median: 0,
            placements_total_median: 45,
            placements_expected: 45,
            soft_score_median: if feasibility_count == 0 {
                None
            } else {
                Some(soft)
            },
            ffd_ms_median: 0.0,
            total_ms_median: 0.0,
            peak_kb: 0,
            time_to_first_feasible_ms_median: None,
            time_to_optimal_ms_median: None,
            worst_spread_median: None,
            worst_home_room_ratio_median: None,
            total_interior_gaps_median: None,
            late_period_ratio_median: None,
            quality_pass_count_median: None,
            unplaced_hours_median: None,
            class_gap_hours_median: None,
            teacher_gap_hours_median: None,
            class_day_balance_cost_median: None,
            home_room_misses_median: None,
            prefer_early_units_median: None,
            avoid_first_units_median: None,
            avoid_last_units_median: None,
            prefer_late_units_median: None,
            rr_k: Some(k),
            rr_period: Some(period),
            kempe_max_chain: None,
        }
    }

    #[test]
    fn sweep_pareto_filters_strictly_worse_tuples() {
        let cells = [
            sweep_test_cell(3, 10, 4, 100),
            sweep_test_cell(5, 25, 4, 80),
            sweep_test_cell(8, 50, 3, 60),
        ];
        let refs: Vec<&CellResult> = cells.iter().collect();
        let frontier = sweep_pareto_non_dominated(&refs);
        assert!(frontier.contains(&SweepTuple { period: 25, k: 5 }));
        assert!(frontier.contains(&SweepTuple { period: 50, k: 8 }));
        assert!(!frontier.contains(&SweepTuple { period: 10, k: 3 }));
    }

    #[test]
    fn sweep_recommend_picks_lowest_mean_then_smallest_period_then_smallest_k() {
        let cells = [
            sweep_test_cell(5, 25, 4, 100),
            sweep_test_cell(5, 25, 4, 80),
            sweep_test_cell(3, 10, 4, 100),
            sweep_test_cell(3, 10, 4, 80),
        ];
        let pairs: [(&str, &CellResult); 4] = [
            ("a", &cells[0]),
            ("b", &cells[1]),
            ("a", &cells[2]),
            ("b", &cells[3]),
        ];
        let pick = sweep_recommend(&pairs, 2).unwrap();
        assert_eq!(pick, SweepTuple { period: 10, k: 3 });
    }

    #[test]
    fn sweep_recommend_returns_none_when_no_tuple_feasible_on_every_fixture() {
        let cells = [sweep_test_cell(5, 25, 4, 100), sweep_test_cell(8, 50, 0, 0)];
        let pairs: [(&str, &CellResult); 2] = [("a", &cells[0]), ("b", &cells[1])];
        assert!(sweep_recommend(&pairs, 2).is_none());
    }

    #[test]
    fn write_backend_objectives_section_renders_every_registered_backend() {
        let mut out = String::new();
        write_backend_objectives_section(&mut out, &BenchBackend::ALL);
        assert!(
            out.contains("## Backend objectives"),
            "section header missing: {out}",
        );
        for backend in BenchBackend::ALL {
            assert!(
                out.contains(backend.label()),
                "backend {} missing from rendered section: {out}",
                backend.label(),
            );
        }
        assert!(
            out.contains("Optimised") && out.contains("Declared skipped"),
            "objectives table missing column headers: {out}",
        );
        assert!(
            out.contains(
                "class_gap, teacher_gap, class_day_balance, home_room, prefer_early, avoid_first, avoid_last, prefer_late"
            ),
            "lahc family optimised set rendered incorrectly (item 52: lahc accepts on full canonical): {out}",
        );
        assert!(
            out.contains("(none)"),
            "cpsat optimised set should render as (none) today: {out}",
        );
    }

    #[test]
    fn teacher_pins_mode_label_and_parse_round_trip() {
        assert_eq!(TeacherPinsMode::On.teacher_pins_label(), "on");
        assert_eq!(TeacherPinsMode::Off.teacher_pins_label(), "off");
        assert_eq!(
            TeacherPinsMode::parse_teacher_pins_mode("on").unwrap(),
            TeacherPinsMode::On
        );
        assert_eq!(
            TeacherPinsMode::parse_teacher_pins_mode("off").unwrap(),
            TeacherPinsMode::Off
        );
        assert!(TeacherPinsMode::parse_teacher_pins_mode("maybe").is_err());
    }

    #[test]
    fn unpin_teachers_in_problem_clears_pins_and_widens_candidates() {
        use solver_core::test_fixtures::grundschule_fixture;
        use solver_core::{SubjectId, TeacherId};
        use std::collections::HashMap;
        let mut p = grundschule_fixture();
        for l in &p.lessons {
            assert!(
                l.teacher_pin.is_some(),
                "fixture precondition: every lesson is pinned"
            );
        }
        crate::unpin_teachers_in_problem(&mut p);
        let mut quals: HashMap<SubjectId, Vec<TeacherId>> = HashMap::new();
        for q in &p.teacher_qualifications {
            quals.entry(q.subject_id).or_default().push(q.teacher_id);
        }
        for v in quals.values_mut() {
            v.sort_by_key(|t| t.0);
            v.dedup();
        }
        for l in &p.lessons {
            assert_eq!(l.teacher_pin, None, "pin must be cleared");
            let expected = quals.get(&l.subject_id).cloned().unwrap_or_default();
            assert_eq!(
                l.teacher_candidates, expected,
                "candidates must equal sorted-deduped qualified teachers for lesson {:?}",
                l.id
            );
        }
    }

    #[test]
    fn unpin_teachers_in_problem_is_deterministic() {
        use solver_core::test_fixtures::grundschule_fixture;
        let mut a = grundschule_fixture();
        let mut b = grundschule_fixture();
        crate::unpin_teachers_in_problem(&mut a);
        crate::unpin_teachers_in_problem(&mut b);
        for (la, lb) in a.lessons.iter().zip(b.lessons.iter()) {
            assert_eq!(
                la.teacher_candidates, lb.teacher_candidates,
                "candidate ordering must be deterministic across calls"
            );
        }
    }

    #[test]
    fn unpin_teachers_in_problem_handles_subject_with_no_quals() {
        use solver_core::test_fixtures::grundschule_fixture;
        use solver_core::{Lesson, LessonId, SubjectId, TeacherId};
        use uuid::Uuid;
        let mut p = grundschule_fixture();
        // Inject a synthetic lesson whose subject has zero qualifications.
        let phantom_subject = SubjectId(Uuid::from_u128(0xDEADBEEF));
        let phantom_class = p.school_classes[0].id;
        p.lessons.push(Lesson {
            id: LessonId(Uuid::from_u128(0xC0FFEE)),
            school_class_ids: vec![phantom_class],
            subject_id: phantom_subject,
            teacher_candidates: vec![TeacherId(Uuid::from_u128(0x1))],
            teacher_pin: Some(TeacherId(Uuid::from_u128(0x1))),
            hours_per_week: 1,
            preferred_block_size: 1,
            lesson_group_id: None,
        });
        // If the Lesson struct grows new fields after 2026-05-10, `cargo build`
        // surfaces them: add them with sensible defaults to keep this test focused
        // on the no-qualifications path. Do NOT add `room_lock` (not a field as of
        // the plan-write date).
        crate::unpin_teachers_in_problem(&mut p);
        let phantom = p.lessons.last().unwrap();
        assert_eq!(phantom.teacher_pin, None);
        assert!(
            phantom.teacher_candidates.is_empty(),
            "unqualified subject must yield empty candidates"
        );
    }
}
