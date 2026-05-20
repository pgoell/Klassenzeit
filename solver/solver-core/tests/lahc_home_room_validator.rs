//! Regression test for OPEN_THINGS item 88: `try_home_room_repair_move`
//! (PR #300, ADR 0050) violates post-condition validators on `lock_in` and
//! `dreizuegig` LAHC cells.
//!
//! The 2026-05-20 production bench refresh exposed 8 panic cells with one
//! of three validator messages: `room hopping for class ...` from
//! `validate_no_room_hopping`, plus `double-booking: class ...` and
//! `double-booking: teacher ...` from `validate_no_double_booking`. This
//! test reproduces the failure on `dreizuegig_fixture` + `LahcKempe`-
//! equivalent config (`lahc_kempe_period = Some(23)`,
//! `lahc_home_room_period = Some(7)`) across a small seed sweep at a
//! reduced budget. The unpinned phase is one of the two phases affected.

use std::collections::HashMap;
use std::time::Duration;

use solver_core::test_fixtures::dreizuegig_fixture;
use solver_core::{
    ids::{SubjectId, TeacherId},
    solve_with_config,
    types::{Problem, SolveConfig},
    PRODUCTION_ACTIVE_WEIGHTS,
};

fn home_room_validator_unpin_teachers(problem: &mut Problem) {
    let mut quals_by_subject: HashMap<SubjectId, Vec<TeacherId>> = HashMap::new();
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

#[test]
fn try_home_room_repair_move_keeps_validators_clean_on_dreizuegig_lahc_kempe() {
    for seed in 1..=5u64 {
        let mut problem = dreizuegig_fixture();
        home_room_validator_unpin_teachers(&mut problem);
        let config = SolveConfig {
            weights: PRODUCTION_ACTIVE_WEIGHTS.clone(),
            deadline: Some(Duration::from_secs(10)),
            seed,
            lahc_rr_period: None,
            lahc_kempe_period: Some(23),
            lahc_home_room_period: Some(7),
            ..SolveConfig::default()
        };
        let result = solve_with_config(&problem, &config);
        assert!(
            result.is_ok(),
            "seed {seed}: expected Ok(Solution) but got Err({:?}) — item 88 regression in try_home_room_repair_move",
            result.err()
        );
    }
}
