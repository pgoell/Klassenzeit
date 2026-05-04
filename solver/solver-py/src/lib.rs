//! solver-py — thin PyO3 wrapper over solver-core. Only glue lives here.

#![deny(missing_docs)]

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

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

/// Python module exposing solver-core functions.
#[pymodule]
fn _rust(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(py_solve_json, m)?)?;
    m.add_function(wrap_pyfunction!(py_solve_json_with_config, m)?)?;
    Ok(())
}
