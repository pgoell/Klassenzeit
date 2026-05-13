//! solver-py — thin PyO3 wrapper over solver-core. Only glue lives here.

#![deny(missing_docs)]

use std::sync::Arc;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use solver_core::ProgressBeacon;

/// Solve a timetable problem supplied as a JSON string and return the resulting
/// Solution as a JSON string. Uses the production-default 200 ms LAHC deadline.
/// Releases the GIL during the call so parallel Python threads are not
/// serialised behind the interpreter lock.
#[pyfunction]
#[pyo3(name = "solve_json")]
fn py_solve_json(py: Python<'_>, problem_json: &str) -> PyResult<String> {
    py.detach(|| solver_core::solve_json(problem_json))
        .map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Like [`py_solve_json`] but with an explicit LAHC deadline in milliseconds.
/// `None` skips LAHC entirely (greedy-only); `Some(n)` runs LAHC for `n` ms
/// wall-clock. Releases the GIL during the call. `lahc_rr_period` and
/// `lahc_kempe_period` enable the corresponding LAHC moves; both default to
/// `None` (disabled).
#[pyfunction]
#[pyo3(
    name = "solve_json_with_config",
    signature = (problem_json, deadline_ms, lahc_rr_period=None, lahc_kempe_period=None)
)]
fn py_solve_json_with_config(
    py: Python<'_>,
    problem_json: &str,
    deadline_ms: Option<u64>,
    lahc_rr_period: Option<u32>,
    lahc_kempe_period: Option<u32>,
) -> PyResult<String> {
    py.detach(|| {
        solver_core::solve_json_with_config(
            problem_json,
            deadline_ms,
            lahc_rr_period,
            lahc_kempe_period,
        )
    })
    .map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Score a `Placement[]` against a `Problem` using the production-active
/// `ConstraintWeights`. Returns the same `u32` soft-score that
/// `solver_core::score_solution` produces internally during a
/// `solve_with_config` call. Used by the CP-SAT path in
/// `klassenzeit_solver.cpsat` to populate `Solution.soft_score` post-solve,
/// so all bake-off backends compare on the same Rust scorer (ADR 0030).
#[pyfunction]
#[pyo3(name = "score_solution_json", signature = (problem_json, placements_json))]
fn py_score_solution_json(
    py: Python<'_>,
    problem_json: &str,
    placements_json: &str,
) -> PyResult<u32> {
    py.detach(|| solver_core::score_solution_json(problem_json, placements_json))
        .map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Compute the `QualityReport` for the given `Placement[]` + `Violation[]`
/// against a `Problem` using production-active weights. Returns the
/// breakdown as a JSON object string. Used by the CP-SAT path in
/// `klassenzeit_solver.cpsat` to populate `Solution.quality_report`
/// post-solve, mirroring how `score_solution_json` populates `soft_score`
/// today (ADR 0030 cross-backend scorer parity).
#[pyfunction]
#[pyo3(name = "quality_report_json", signature = (problem_json, placements_json, violations_json))]
fn py_quality_report_json(
    py: Python<'_>,
    problem_json: &str,
    placements_json: &str,
    violations_json: &str,
) -> PyResult<String> {
    py.detach(|| solver_core::quality_report_json(problem_json, placements_json, violations_json))
        .map_err(|e| PyValueError::new_err(e.to_string()))
}

/// PyO3 wrapper around an `Arc<ProgressBeacon>`. Exposes `snapshot()` and
/// `cancel()` to Python; passes the shared beacon to the solver via
/// `solve_json_with_progress`. Construct fresh per solve; the underlying
/// atomics are lock-free.
#[pyclass]
struct ProgressHandle {
    inner: Arc<ProgressBeacon>,
}

#[pymethods]
impl ProgressHandle {
    /// Construct a fresh handle wrapping a new beacon.
    #[new]
    fn py_new_progress_handle() -> Self {
        Self {
            inner: ProgressBeacon::new(),
        }
    }

    /// Return a dict snapshot of the current beacon state with keys
    /// `iter`, `placement_count`, `best_score`, `is_feasible`,
    /// `cancel_requested`.
    fn snapshot<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("iter", self.inner.iter_snapshot())?;
        dict.set_item("placement_count", self.inner.placement_count_snapshot())?;
        dict.set_item("best_score", self.inner.best_score_snapshot())?;
        dict.set_item("is_feasible", self.inner.feasible_snapshot())?;
        dict.set_item("cancel_requested", self.inner.cancel_requested())?;
        Ok(dict)
    }

    /// Request cancellation of the running solve. Idempotent.
    fn cancel(&self) {
        self.inner.request_cancel();
    }
}

/// Like [`py_solve_json_with_config`] but accepts a [`ProgressHandle`] for
/// live progress polling and cooperative cancel. Releases the GIL during
/// solve. The returned Solution JSON carries `was_cancelled: true` when the
/// loop exited because `handle.cancel()` was called.
#[pyfunction]
#[pyo3(
    name = "solve_json_with_progress",
    signature = (problem_json, deadline_ms, progress, lahc_rr_period=None, lahc_kempe_period=None)
)]
fn py_solve_json_with_progress(
    py: Python<'_>,
    problem_json: &str,
    deadline_ms: Option<u64>,
    progress: &ProgressHandle,
    lahc_rr_period: Option<u32>,
    lahc_kempe_period: Option<u32>,
) -> PyResult<String> {
    let beacon_handle = Arc::clone(&progress.inner);
    py.detach(|| {
        solver_core::solve_json_with_progress(
            problem_json,
            deadline_ms,
            &beacon_handle,
            lahc_rr_period,
            lahc_kempe_period,
        )
    })
    .map_err(|e| PyValueError::new_err(e.to_string()))
}

/// Python module exposing solver-core functions.
#[pymodule]
fn _rust(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(py_solve_json, m)?)?;
    m.add_function(wrap_pyfunction!(py_solve_json_with_config, m)?)?;
    m.add_function(wrap_pyfunction!(py_score_solution_json, m)?)?;
    m.add_function(wrap_pyfunction!(py_quality_report_json, m)?)?;
    m.add_class::<ProgressHandle>()?;
    m.add_function(wrap_pyfunction!(py_solve_json_with_progress, m)?)?;
    Ok(())
}
