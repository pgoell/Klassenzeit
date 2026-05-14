//! Determinism sweep for `solve_with_config` under randomised soft pins.
//!
//! Soft pins (`PinKind::Soft`) ride along the canonical-score axis but
//! must not break determinism: two solves over the same `Problem` and
//! `SolveConfig` (including the same soft-pin set on the wire) must
//! return byte-equal `Solution`s. ADR 0042 (item 5).
//!
//! Local 5x128 sweep (per `solver/CLAUDE.md` property-test-generator
//! widening rule):
//!
//! ```bash
//! for s in 1 2 3 4 5; do
//!   PROPTEST_CASES=128 PROPTEST_SEED=$s \
//!     cargo nextest run -p solver-core --test proptest_solve || break;
//! done
//! ```

use std::collections::HashSet;
use std::time::Duration;

use proptest::collection::vec;
use proptest::prelude::*;
use solver_core::ids::{LessonId, TimeBlockId};
use solver_core::solve_with_config;
use solver_core::test_fixtures::grundschule_fixture;
use solver_core::types::{ConstraintWeights, PinKind, PinnedPlacement, Problem, SolveConfig};

/// Build a Problem identical to the grundschule fixture but with `n_soft`
/// soft pins drawn from the cross-product of (lesson, time_block, room).
/// Each pin's `(lesson_id, time_block_id, room_id)` tuple is selected by
/// index modulo the cardinality of the respective collection so the same
/// seed produces the same set deterministically.
fn problem_with_soft_pins(base: Problem, seed_offsets: &[(usize, usize)]) -> Problem {
    let mut problem = base;
    let lessons_len = problem.lessons.len();
    let tbs_len = problem.time_blocks.len();
    let rooms_len = problem.rooms.len();
    if lessons_len == 0 || tbs_len == 0 || rooms_len == 0 {
        return problem;
    }
    // Use a set to dedup (lesson, tb) pairs; soft pins on the same
    // (lesson, tb) twice are a no-op and we want the generator to
    // exercise the dedup path inside `validate_pins`.
    let mut seen: HashSet<(LessonId, TimeBlockId)> = HashSet::new();
    let mut pins: Vec<PinnedPlacement> = Vec::with_capacity(seed_offsets.len());
    for (l_off, t_off) in seed_offsets {
        let lesson_id = problem.lessons[l_off % lessons_len].id;
        let tb_id = problem.time_blocks[t_off % tbs_len].id;
        let room_id = problem.rooms[(l_off + t_off) % rooms_len].id;
        if !seen.insert((lesson_id, tb_id)) {
            continue;
        }
        pins.push(PinnedPlacement {
            lesson_id,
            time_block_id: tb_id,
            room_id,
            teacher_id: None,
            kind: PinKind::Soft,
        });
    }
    problem.pinned_placements = pins;
    problem
}

prop_compose! {
    fn soft_pin_offsets()(
        offsets in vec((0usize..256, 0usize..256), 0..=4),
    ) -> Vec<(usize, usize)> {
        offsets
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 32,
        .. ProptestConfig::default()
    })]

    /// Two solves on the same problem (with randomised soft pins) under the
    /// same `SolveConfig` must produce byte-equal `Solution`s. Soft pins
    /// must not perturb the RNG draw count or introduce nondeterministic
    /// HashMap iteration into the search hot path.
    ///
    /// `max_iterations` (not wall-clock `deadline`) bounds the LAHC loop:
    /// determinism is iteration-count invariant, not wall-clock invariant
    /// (sibling pattern in `lahc_property.rs`).
    #[test]
    fn solve_is_deterministic_under_random_soft_pins(
        offsets in soft_pin_offsets(),
        seed in any::<u64>(),
    ) {
        let problem = problem_with_soft_pins(grundschule_fixture(), &offsets);
        let cfg = SolveConfig {
            weights: ConstraintWeights {
                soft_pin_miss: 5,
                ..ConstraintWeights::default()
            },
            seed,
            deadline: Some(Duration::from_secs(60)),
            max_iterations: Some(200),
            ..SolveConfig::default()
        };
        let a = solve_with_config(&problem, &cfg)
            .expect("solve a should succeed");
        let b = solve_with_config(&problem, &cfg)
            .expect("solve b should succeed");
        prop_assert_eq!(a.placements, b.placements);
        prop_assert_eq!(a.violations, b.violations);
        prop_assert_eq!(a.soft_score, b.soft_score);
    }

    /// Solving with no soft pins is byte-equal to solving with the same
    /// problem under `weights.soft_pin_miss == 0` (no axis contribution).
    /// Pins the "byte-equal when empty" determinism contract: the
    /// `state.soft_pinned_blocks` block on `GreedyState` must consume
    /// zero RNG draws and must short-circuit when empty.
    #[test]
    fn solve_byte_equal_no_soft_pins_vs_weight_zero(
        seed in any::<u64>(),
    ) {
        let problem = grundschule_fixture();
        let cfg_with_weight = SolveConfig {
            weights: ConstraintWeights {
                soft_pin_miss: 100,
                ..ConstraintWeights::default()
            },
            seed,
            deadline: Some(Duration::from_secs(60)),
            max_iterations: Some(200),
            ..SolveConfig::default()
        };
        let cfg_without_weight = SolveConfig {
            weights: ConstraintWeights {
                soft_pin_miss: 0,
                ..ConstraintWeights::default()
            },
            seed,
            deadline: Some(Duration::from_secs(60)),
            max_iterations: Some(200),
            ..SolveConfig::default()
        };
        let with = solve_with_config(&problem, &cfg_with_weight)
            .expect("solve with weight");
        let without = solve_with_config(&problem, &cfg_without_weight)
            .expect("solve without weight");
        // With no soft pins in the problem, the soft-pin axis is inert
        // regardless of weight, so the two solves must be byte-equal.
        prop_assert_eq!(with.placements, without.placements);
        prop_assert_eq!(with.soft_score, without.soft_score);
    }
}
