"""CP-SAT seed phase via Google OR-Tools.

Sprint 4 of the solver feasibility bake-off (ADR 0029, 0030). Solves a
Klassenzeit timetable problem via CP-SAT under a wall-clock deadline,
using a per-block-anchor binary encoding. Soft scoring is computed
post-solve via the Rust ``score_solution_json`` PyO3 binding so all four
bake-off backends compare on the same Rust scorer.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import resource
import sys
from collections import defaultdict
from typing import Any

from ortools.sat.python import cp_model

from klassenzeit_solver._rust import score_solution_json

# ``AnchorKey = (lesson_id, day_of_week, start_position, room_id)``.
AnchorKey = tuple[str, int, int, str]


# Mirror of solver_core::types::PRODUCTION_ACTIVE_WEIGHTS so CP-SAT's
# model objective evaluates to the same number as
# `score_solution(..., PRODUCTION_ACTIVE_WEIGHTS)` on any returned
# solution. Item 48 keeps these in lockstep with Rust by referencing both
# in `solver/CLAUDE.md`; the property tests in `test_cpsat.py` flag drift.
_W_CLASS_GAP = 10
_W_TEACHER_GAP = 10
_W_PREFER_EARLY_PERIOD = 1
_W_AVOID_FIRST_PERIOD = 1
_W_PREFER_HOME_ROOM = 5
_W_AVOID_LAST_PERIOD = 1
_W_PREFER_LATE_PERIOD = 1
_W_CLASS_DAY_BALANCE = 5


class _FirstSolutionCallback(cp_model.CpSolverSolutionCallback):
    """Records ``solver.WallTime() * 1000`` on the first feasible solution."""

    def __init__(self) -> None:
        super().__init__()
        self.first_ms: float | None = None

    def on_solution_callback(self) -> None:
        if self.first_ms is None:
            self.first_ms = self.WallTime() * 1000.0


def _read_peak_rss_kb() -> int:
    raw = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return raw // 1024 if sys.platform == "darwin" else raw


def solve_cpsat_json(
    problem_json: str,
    deadline_ms: int | None,
    seed: int = 1,
) -> str:
    """See module docstring. Returns Solution JSON in the wire format used by ``solve_json``."""
    try:
        problem = json.loads(problem_json)
    except json.JSONDecodeError as exc:
        raise ValueError(f"json: {exc}") from exc

    model, anchor_vars, meta = _build_model(problem)
    solver = cp_model.CpSolver()
    solver.parameters.num_search_workers = 1
    solver.parameters.random_seed = seed
    solver.parameters.log_search_progress = False
    if deadline_ms is not None:
        solver.parameters.max_time_in_seconds = deadline_ms / 1000.0
    callback = _FirstSolutionCallback()
    status = solver.solve(model, callback)
    peak_rss_kb = _read_peak_rss_kb()

    if status in (cp_model.OPTIMAL, cp_model.FEASIBLE):
        placements = _extract_placements(solver, anchor_vars, meta)
        soft_score = score_solution_json(problem_json, json.dumps(placements))
        ttf = callback.first_ms
        tto = solver.WallTime() * 1000.0 if status == cp_model.OPTIMAL else None
        return json.dumps(
            {
                "placements": placements,
                "violations": [],
                "soft_score": int(soft_score),
                "model_objective_value": int(solver.objective_value),
                "peak_rss_kb": peak_rss_kb,
                "time_to_first_feasible_ms": ttf,
                "time_to_optimal_ms": tto,
            }
        )
    if status in (cp_model.INFEASIBLE, cp_model.UNKNOWN):
        status_name = solver.status_name(status).lower()
        reason = f"cpsat: {status_name}"
        violations = []
        for lesson in problem["lessons"]:
            for hour in range(lesson["hours_per_week"]):
                violations.append(
                    {
                        "kind": "no_free_time_block",
                        "lesson_id": lesson["id"],
                        "hour_index": hour,
                        "reason": reason,
                    }
                )
        return json.dumps(
            {
                "placements": [],
                "violations": violations,
                "soft_score": 0,
                "model_objective_value": None,
                "peak_rss_kb": peak_rss_kb,
                "time_to_first_feasible_ms": None,
                "time_to_optimal_ms": None,
            }
        )
    if status == cp_model.MODEL_INVALID:
        raise RuntimeError(
            f"cpsat: model invalid - bug in cpsat.py (status={solver.status_name(status)})"
        )
    raise RuntimeError(f"cpsat: unexpected solver status: {solver.status_name(status)}")


# ----------------------------------------------------------------------
# Model builder
# ----------------------------------------------------------------------


def _build_model(
    problem: dict[str, Any],
) -> tuple[cp_model.CpModel, dict[AnchorKey, cp_model.IntVar], dict[str, Any]]:
    """Build the CP-SAT model and return ``(model, anchor_vars, meta)``."""
    model = cp_model.CpModel()
    lookups = _build_lookups(problem)
    anchor_vars, anchors_for_lesson = _create_anchor_vars(model, problem, lookups)

    _emit_cardinality(model, problem, anchor_vars, anchors_for_lesson)
    _emit_non_overlap(model, anchor_vars, lookups)
    _emit_teacher_max_hours(model, anchor_vars, lookups)
    _emit_same_room(model, problem, anchor_vars, anchors_for_lesson)
    _emit_lesson_group_co_placement(model, problem, anchor_vars, anchors_for_lesson)
    _emit_pinned_placements(model, problem, anchor_vars, lookups)

    _emit_objective(model, problem, anchor_vars, lookups)
    meta: dict[str, Any] = {
        "lesson_lookup": lookups["lesson_lookup"],
        "tb_at": lookups["tb_at"],
    }
    return model, anchor_vars, meta


def _build_lookups(problem: dict[str, Any]) -> dict[str, Any]:
    """Pre-compute reverse lookups, pruning sets, and (day, pos) -> tb_id maps."""
    lessons = problem["lessons"]
    time_blocks = problem["time_blocks"]
    teachers = problem["teachers"]

    lesson_lookup: dict[str, dict[str, Any]] = {lesson["id"]: lesson for lesson in lessons}
    teacher_max_hours: dict[str, int] = {t["id"]: t["max_hours_per_week"] for t in teachers}

    tb_at: dict[tuple[int, int], str] = {}
    positions_per_day: dict[int, list[int]] = defaultdict(list)
    for tb in time_blocks:
        tb_at[(tb["day_of_week"], tb["position"])] = tb["id"]
        positions_per_day[tb["day_of_week"]].append(tb["position"])
    for positions in positions_per_day.values():
        positions.sort()

    teacher_qualifies: set[tuple[str, str]] = {
        (q["teacher_id"], q["subject_id"]) for q in problem["teacher_qualifications"]
    }
    teacher_blocked: set[tuple[str, str]] = {
        (b["teacher_id"], b["time_block_id"]) for b in problem["teacher_blocked_times"]
    }
    room_blocked: set[tuple[str, str]] = {
        (b["room_id"], b["time_block_id"]) for b in problem["room_blocked_times"]
    }

    rooms_with_suit: set[str] = set()
    room_subject_suit: set[tuple[str, str]] = set()
    for s in problem["room_subject_suitabilities"]:
        rooms_with_suit.add(s["room_id"])
        room_subject_suit.add((s["room_id"], s["subject_id"]))

    tb_pos_lookup: dict[str, tuple[int, int]] = {
        tb["id"]: (tb["day_of_week"], tb["position"]) for tb in time_blocks
    }

    return {
        "lesson_lookup": lesson_lookup,
        "teacher_max_hours": teacher_max_hours,
        "tb_at": tb_at,
        "positions_per_day": positions_per_day,
        "teacher_qualifies": teacher_qualifies,
        "teacher_blocked": teacher_blocked,
        "room_blocked": room_blocked,
        "rooms_with_suit": rooms_with_suit,
        "room_subject_suit": room_subject_suit,
        "tb_pos_lookup": tb_pos_lookup,
    }


def _room_suits(
    rooms_with_suit: set[str],
    room_subject_suit: set[tuple[str, str]],
    room_id: str,
    subject_id: str,
) -> bool:
    """Room with no entries suits all subjects; otherwise check the explicit pair."""
    if room_id not in rooms_with_suit:
        return True
    return (room_id, subject_id) in room_subject_suit


def _create_anchor_vars(
    model: cp_model.CpModel,
    problem: dict[str, Any],
    lookups: dict[str, Any],
) -> tuple[dict[AnchorKey, cp_model.IntVar], dict[str, list[AnchorKey]]]:
    """Create one BoolVar per (lesson, day, start_pos, room) after pruning."""
    anchor_vars: dict[AnchorKey, cp_model.IntVar] = {}
    anchors_for_lesson: dict[str, list[AnchorKey]] = defaultdict(list)

    positions_per_day = lookups["positions_per_day"]
    tb_at = lookups["tb_at"]
    teacher_qualifies = lookups["teacher_qualifies"]
    teacher_blocked = lookups["teacher_blocked"]
    room_blocked = lookups["room_blocked"]
    rooms_with_suit = lookups["rooms_with_suit"]
    room_subject_suit = lookups["room_subject_suit"]

    for lesson in problem["lessons"]:
        l_id = lesson["id"]
        teacher_id = lesson.get("teacher_pin") or lesson["teacher_candidates"][0]
        subject_id = lesson["subject_id"]
        n = lesson["preferred_block_size"]
        if (teacher_id, subject_id) not in teacher_qualifies:
            continue

        for day, positions in positions_per_day.items():
            pos_set = set(positions)
            for start_pos in positions:
                if not all((start_pos + i) in pos_set for i in range(n)):
                    continue
                window_tb_ids = [tb_at[(day, start_pos + i)] for i in range(n)]
                if any((teacher_id, tb_id) in teacher_blocked for tb_id in window_tb_ids):
                    continue
                for room in problem["rooms"]:
                    r_id = room["id"]
                    if not _room_suits(rooms_with_suit, room_subject_suit, r_id, subject_id):
                        continue
                    if any((r_id, tb_id) in room_blocked for tb_id in window_tb_ids):
                        continue
                    var = model.new_bool_var(f"y_{l_id}_{day}_{start_pos}_{r_id}")
                    key = (l_id, day, start_pos, r_id)
                    anchor_vars[key] = var
                    anchors_for_lesson[l_id].append(key)

    return anchor_vars, anchors_for_lesson


def _force_infeasible(model: cp_model.CpModel) -> None:
    """Add an unsatisfiable constraint so the solver returns INFEASIBLE."""
    model.add_bool_or([])


def _emit_cardinality(
    model: cp_model.CpModel,
    problem: dict[str, Any],
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    anchors_for_lesson: dict[str, list[AnchorKey]],
) -> None:
    """sum_{d,p,r} y[l,d,p,r] = K(l) = H/N for each lesson."""
    for lesson in problem["lessons"]:
        l_id = lesson["id"]
        n = lesson["preferred_block_size"]
        h = lesson["hours_per_week"]
        k = h // n
        keys = anchors_for_lesson.get(l_id, [])
        if not keys and k > 0:
            _force_infeasible(model)
            continue
        model.add(sum(anchor_vars[key] for key in keys) == k)


def _emit_non_overlap(  # noqa: PLR0912 (lesson-group dedup adds bookkeeping branches; splitting hurts readability)
    model: cp_model.CpModel,
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    lookups: dict[str, Any],
) -> None:
    """Class, teacher, and room non-overlap at every (day, position) slot.

    Lesson-group members co-placed at the same (day, pos) (per
    ``_emit_lesson_group_co_placement``) are treated as a single booking for
    each class they share: per ``(class, lesson_group_id)``, only the first
    member's per-room vars at this slot contribute to the class's per-slot
    sum. Without this dedup, multi-class lesson groups (e.g., the dreizuegige
    Religion trio: 3 lessons each spanning the same 3 classes, all forced to
    co-place) make every shared class see ``sum = 3`` against ``sum <= 1``,
    rendering the model infeasible.

    Teacher and room non-overlap do NOT dedup today: the existing fixtures
    have distinct teachers and rooms across group members. If a future fixture
    introduces same-teacher or same-room within a group (e.g., Doppelbesetzung
    or Foerderstunden), extend the dedup to those terms.
    """
    lesson_lookup = lookups["lesson_lookup"]
    positions_per_day = lookups["positions_per_day"]
    for day, positions in positions_per_day.items():
        for pos in positions:
            # Per (class, group_key): list of vars-at-this-slot from the FIRST
            # member of the group seen (sorted by lesson_id for determinism).
            # We sum those vars (== sum-over-rooms-for-that-member at this slot)
            # to get the indicator "is this group's class booked here".
            class_group_first: dict[tuple[str, str], tuple[str, list[cp_model.IntVar]]] = {}
            teacher_terms: dict[str, list[cp_model.IntVar]] = defaultdict(list)
            room_terms: dict[str, list[cp_model.IntVar]] = defaultdict(list)
            for (l_id, d, start_pos, r_id), var in anchor_vars.items():
                if d != day:
                    continue
                lesson = lesson_lookup[l_id]
                n = lesson["preferred_block_size"]
                if not (start_pos <= pos < start_pos + n):
                    continue
                group_key = lesson.get("lesson_group_id") or l_id
                for c_id in lesson["school_class_ids"]:
                    cur = class_group_first.get((c_id, group_key))
                    if cur is None or l_id < cur[0]:
                        class_group_first[(c_id, group_key)] = (l_id, [var])
                    elif l_id == cur[0]:
                        cur[1].append(var)
                    # else: a later (higher) l_id; ignore so only the first member contributes
                lesson_teacher = lesson.get("teacher_pin") or lesson["teacher_candidates"][0]
                teacher_terms[lesson_teacher].append(var)
                room_terms[r_id].append(var)
            class_terms: dict[str, list[cp_model.IntVar]] = defaultdict(list)
            for (c_id, _g), (_l, vars_list) in class_group_first.items():
                class_terms[c_id].extend(vars_list)
            for terms in class_terms.values():
                if len(terms) > 1:
                    model.add(sum(terms) <= 1)
            for terms in teacher_terms.values():
                if len(terms) > 1:
                    model.add(sum(terms) <= 1)
            for terms in room_terms.values():
                if len(terms) > 1:
                    model.add(sum(terms) <= 1)


def _emit_teacher_max_hours(
    model: cp_model.CpModel,
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    lookups: dict[str, Any],
) -> None:
    """sum_{lesson in teacher's} y[..] * N(l) <= teacher.max_hours_per_week."""
    lesson_lookup = lookups["lesson_lookup"]
    teacher_max_hours = lookups["teacher_max_hours"]
    teacher_anchor_terms: dict[str, list[tuple[cp_model.IntVar, int]]] = defaultdict(list)
    for (l_id, _d, _p, _r), var in anchor_vars.items():
        lesson = lesson_lookup[l_id]
        lesson_teacher = lesson.get("teacher_pin") or lesson["teacher_candidates"][0]
        teacher_anchor_terms[lesson_teacher].append((var, lesson["preferred_block_size"]))
    for t_id, terms in teacher_anchor_terms.items():
        if t_id in teacher_max_hours:
            model.add(sum(var * n for var, n in terms) <= teacher_max_hours[t_id])


def _emit_same_room(
    model: cp_model.CpModel,
    problem: dict[str, Any],
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    anchors_for_lesson: dict[str, list[AnchorKey]],
) -> None:
    """For each (class, subject) group of single-class lessons sharing >1 lesson, force one room."""
    single_class_groups: dict[tuple[str, str], list[str]] = defaultdict(list)
    for lesson in problem["lessons"]:
        if len(lesson["school_class_ids"]) == 1:
            key = (lesson["school_class_ids"][0], lesson["subject_id"])
            single_class_groups[key].append(lesson["id"])
    for (c_id, s_id), l_ids in single_class_groups.items():
        if len(l_ids) <= 1:
            continue
        candidate_rooms: set[str] = set()
        for l_id in l_ids:
            for _l, _d, _p, r_id in anchors_for_lesson.get(l_id, []):
                candidate_rooms.add(r_id)
        if not candidate_rooms:
            continue
        z_vars: dict[str, cp_model.IntVar] = {
            r_id: model.new_bool_var(f"z_{c_id}_{s_id}_{r_id}") for r_id in candidate_rooms
        }
        model.add(sum(z_vars.values()) == 1)
        for l_id in l_ids:
            for key in anchors_for_lesson.get(l_id, []):
                (_l, _d, _p, r_id) = key
                model.add(anchor_vars[key] <= z_vars[r_id])


def _emit_lesson_group_co_placement(
    model: cp_model.CpModel,
    problem: dict[str, Any],
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    anchors_for_lesson: dict[str, list[AnchorKey]],
) -> None:
    """Lessons sharing a non-null lesson_group_id are placed at the same (day, start_pos)."""
    lesson_groups: dict[str, list[str]] = defaultdict(list)
    for lesson in problem["lessons"]:
        g_id = lesson.get("lesson_group_id")
        if g_id is not None:
            lesson_groups[g_id].append(lesson["id"])
    for members in lesson_groups.values():
        if len(members) <= 1:
            continue
        anchor_positions: set[tuple[int, int]] = set()
        for l_id in members:
            for _l, d, p, _r in anchors_for_lesson.get(l_id, []):
                anchor_positions.add((d, p))
        for d, p in anchor_positions:
            sums: list[Any] = []
            for l_id in members:
                terms = [
                    anchor_vars[k_]
                    for k_ in anchors_for_lesson.get(l_id, [])
                    if k_[1] == d and k_[2] == p
                ]
                sums.append(sum(terms) if terms else 0)
            for s_expr in sums[1:]:
                model.add(sums[0] == s_expr)


def _emit_pinned_placements(
    model: cp_model.CpModel,
    problem: dict[str, Any],
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    lookups: dict[str, Any],
) -> None:
    """Force y[lesson, day, anchor_pos, room] = 1 per pin; multi-block pins collapse to anchor."""
    tb_pos_lookup = lookups["tb_pos_lookup"]
    pins_by_lesson_day_room: dict[tuple[str, int, str], list[int]] = defaultdict(list)
    for pin in problem.get("pinned_placements", []):
        if pin["time_block_id"] not in tb_pos_lookup:
            continue
        d, p = tb_pos_lookup[pin["time_block_id"]]
        pins_by_lesson_day_room[(pin["lesson_id"], d, pin["room_id"])].append(p)
    for (l_id, d, r_id), positions in pins_by_lesson_day_room.items():
        positions.sort()
        anchor_pos = positions[0]
        key = (l_id, d, anchor_pos, r_id)
        if key in anchor_vars:
            model.add(anchor_vars[key] == 1)
        else:
            _force_infeasible(model)


def _objective_subject_preference_terms(
    problem: dict[str, Any],
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    lookups: dict[str, Any],
) -> cp_model.LinearExpr | int:
    """Per-anchor constant coefficient for subject-preference axes.

    Covers prefer_early, avoid_first, avoid_last, prefer_late. Each axis
    sums over the N positions in the block window; the per-anchor sum
    collapses to a Python int known at build time. Returns
    sum_anchor coeff * y[anchor].
    """
    lesson_lookup = lookups["lesson_lookup"]
    tb_pos_lookup = lookups["tb_pos_lookup"]
    subjects = {s["id"]: s for s in problem["subjects"]}
    max_pos_per_day: dict[int, int] = {}
    for tb in problem["time_blocks"]:
        d = tb["day_of_week"]
        max_pos_per_day[d] = max(max_pos_per_day.get(d, 0), tb["position"])

    terms: list[cp_model.LinearExpr] = []
    for (l_id, day, start_pos, _r_id), var in anchor_vars.items():
        lesson = lesson_lookup[l_id]
        n = lesson["preferred_block_size"]
        subject = subjects[lesson["subject_id"]]
        max_pos = max_pos_per_day[day]
        coeff = 0
        prefer_early = subject.get("prefer_early_period", 0)
        if prefer_early:
            window_pos_sum = n * start_pos + n * (n - 1) // 2
            coeff += _W_PREFER_EARLY_PERIOD * prefer_early * window_pos_sum
        avoid_first = subject.get("avoid_first_period", 0)
        if avoid_first and start_pos == 0:
            coeff += _W_AVOID_FIRST_PERIOD * avoid_first
        avoid_last = subject.get("avoid_last_period", 0)
        if avoid_last and start_pos + n - 1 == max_pos:
            coeff += _W_AVOID_LAST_PERIOD * avoid_last
        prefer_late = subject.get("prefer_late_period", 0)
        if prefer_late:
            window_late_sum = n * max_pos - n * start_pos - n * (n - 1) // 2
            coeff += _W_PREFER_LATE_PERIOD * prefer_late * window_late_sum
        if coeff:
            terms.append(coeff * var)
    # tb_pos_lookup is unused here today; kept on the signature for the
    # gap-axis tasks below so they can reuse the same lookups dict shape.
    _ = tb_pos_lookup
    return cp_model.LinearExpr.sum(terms) if terms else 0


def _objective_home_room_term(
    problem: dict[str, Any],
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    lookups: dict[str, Any],
) -> cp_model.LinearExpr | int:
    """Per-anchor constant coefficient for the home_room axis.

    Sum over class in lesson.school_class_ids of
    (mismatch ? weights.prefer_home_room * N : 0). Mirrors
    score::home_room_penalty per-placement aggregated over the N-block
    window. Multi-class lessons accumulate per-class contributions;
    classes without home_room_id contribute 0.
    """
    lesson_lookup = lookups["lesson_lookup"]
    home_room_by_class: dict[str, str | None] = {
        c["id"]: c.get("home_room_id") for c in problem["school_classes"]
    }

    terms: list[cp_model.LinearExpr] = []
    for (l_id, _day, _start_pos, room_id), var in anchor_vars.items():
        lesson = lesson_lookup[l_id]
        n = lesson["preferred_block_size"]
        coeff = 0
        for class_id in lesson["school_class_ids"]:
            home_room_id = home_room_by_class.get(class_id)
            if home_room_id is not None and home_room_id != room_id:
                coeff += _W_PREFER_HOME_ROOM * n
        if coeff:
            terms.append(coeff * var)
    return cp_model.LinearExpr.sum(terms) if terms else 0


def _build_per_slot_presence(  # noqa: PLR0912 (scope_kind branching plus per-day inner loops; splitting hurts readability)
    model: cp_model.CpModel,
    problem: dict[str, Any],
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    lookups: dict[str, Any],
    scope_kind: str,
) -> dict[tuple[str, int, int], cp_model.IntVar]:
    """Build per-(entity, day, position) 0-1 presence indicators.

    For each (entity, day, position) where entity is a class id (when
    scope_kind == 'class') or a teacher id (when scope_kind == 'teacher'),
    build present[entity, day, p] = OR over anchors covering (day, p) in
    the entity's scope. OR-semantics (via add_max_equality) is required
    for class scope where lesson-group co-placement allows multiple
    anchors to cover the same (class, day, p); score_solution dedups
    positions per (class, day) before counting gaps so the CP-SAT
    presence indicator must mirror that 0-1 coverage view rather than a
    raw sum. Returned mapping is keyed by (entity_id_str, day, position).
    """
    lesson_lookup = lookups["lesson_lookup"]
    positions_per_day = lookups["positions_per_day"]
    if scope_kind == "class":
        entity_ids: set[str] = {c["id"] for c in problem["school_classes"]}
    elif scope_kind == "teacher":
        entity_ids = {t["id"] for t in problem["teachers"]}
    else:  # pragma: no cover - guarded by callers
        raise ValueError(f"unknown scope_kind: {scope_kind}")

    coverage: dict[tuple[str, int, int], list[cp_model.IntVar]] = {}
    for (l_id, day, start_pos, _r_id), var in anchor_vars.items():
        lesson = lesson_lookup[l_id]
        n = lesson["preferred_block_size"]
        if scope_kind == "class":
            owners: list[str] = list(lesson["school_class_ids"])
        else:
            owners = [lesson.get("teacher_pin") or lesson["teacher_candidates"][0]]
        for offset in range(n):
            p = start_pos + offset
            for owner in owners:
                coverage.setdefault((owner, day, p), []).append(var)

    presence: dict[tuple[str, int, int], cp_model.IntVar] = {}
    for entity_id in entity_ids:
        for day, positions in positions_per_day.items():
            for pos in positions:
                key = (entity_id, day, pos)
                covering = coverage.get(key, [])
                pres = model.new_int_var(0, 1, f"present_{scope_kind}_{entity_id}_{day}_{pos}")
                if covering:
                    # OR-semantics, not sum: lesson-group co-placement allows
                    # multiple anchors to cover (class, day, pos) at once,
                    # while score_solution dedups per (class, day) before
                    # counting gaps. add_max_equality on 0-1 vars yields the
                    # 0-1 "is this slot covered" indicator gap counting needs.
                    model.add_max_equality(pres, covering)
                else:
                    model.add(pres == 0)
                presence[key] = pres
    return presence


def _objective_gap_term(
    model: cp_model.CpModel,
    problem: dict[str, Any],
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    lookups: dict[str, Any],
    scope_kind: str,
    weight: int,
) -> cp_model.LinearExpr | int:
    """Build per-(entity, day, position) gap indicators and weight the sum.

    For each (entity, day, position) presence indicator, build
    has_left/has_right/gap channeling and return weight * sum(gap[...]).
    """
    presence = _build_per_slot_presence(model, problem, anchor_vars, lookups, scope_kind)
    positions_per_day = lookups["positions_per_day"]
    if scope_kind == "class":
        entity_ids: set[str] = {c["id"] for c in problem["school_classes"]}
    else:
        entity_ids = {t["id"] for t in problem["teachers"]}

    gap_vars: list[cp_model.IntVar] = []
    for entity_id in entity_ids:
        for day, positions in positions_per_day.items():
            sorted_positions = sorted(positions)
            for idx, pos in enumerate(sorted_positions):
                pres_p = presence[(entity_id, day, pos)]
                left_neighbours = [presence[(entity_id, day, q)] for q in sorted_positions[:idx]]
                right_neighbours = [
                    presence[(entity_id, day, q)] for q in sorted_positions[idx + 1 :]
                ]
                if not left_neighbours or not right_neighbours:
                    # No interior position can have both has_left and has_right.
                    continue
                has_left = model.new_bool_var(f"hl_{scope_kind}_{entity_id}_{day}_{pos}")
                has_right = model.new_bool_var(f"hr_{scope_kind}_{entity_id}_{day}_{pos}")
                model.add_max_equality(has_left, left_neighbours)
                model.add_max_equality(has_right, right_neighbours)
                gap = model.new_bool_var(f"gap_{scope_kind}_{entity_id}_{day}_{pos}")
                model.add(gap >= has_left + has_right + (1 - pres_p) - 2)
                model.add(gap <= has_left)
                model.add(gap <= has_right)
                model.add(gap <= 1 - pres_p)
                gap_vars.append(gap)
    return weight * cp_model.LinearExpr.sum(gap_vars) if gap_vars else 0


def _objective_class_day_balance_term(
    model: cp_model.CpModel,
    problem: dict[str, Any],
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    lookups: dict[str, Any],
) -> cp_model.LinearExpr | int:
    """Per-class scaled L1 day-balance cost mirroring score::class_day_balance_cost.

    For each class:
      class_total = sum lesson.hours_per_week for lessons where class is in school_class_ids
      c_count[class, day] = sum over anchors (l, day, p, r) where day matches
        and class is in scope: N(l) * y[..]
      dev[class, day] = abs(c_count[class, day] * D - class_total)
      scaled[class] = sum_day dev[class, day]
      quotient[class] = scaled[class] // D (CP-SAT add_division_equality)
    Returns _W_CLASS_DAY_BALANCE * sum_class quotient[class].

    Class with class_total == 0 contributes 0 by construction (all c_count
    vars are 0, dev is 0, quotient is 0). Skipped at build time to avoid
    creating unused vars.
    """
    lesson_lookup = lookups["lesson_lookup"]
    positions_per_day = lookups["positions_per_day"]
    days_set: set[int] = set(positions_per_day.keys())
    if not days_set:
        return 0
    d = len(days_set)
    classes = problem["school_classes"]

    class_total: dict[str, int] = {}
    for cls in classes:
        c_id = cls["id"]
        total = 0
        for lesson in problem["lessons"]:
            if c_id in lesson["school_class_ids"]:
                total += lesson["hours_per_week"]
        class_total[c_id] = total

    quotients: list[cp_model.IntVar] = []
    for cls in classes:
        c_id = cls["id"]
        total = class_total[c_id]
        if total == 0:
            continue
        # c_count[day]: sum over anchors covering (c_id, day) of N(l) * y[..]
        c_count_terms: dict[int, list[cp_model.LinearExpr]] = {day: [] for day in days_set}
        for (l_id, day, _start_pos, _r_id), var in anchor_vars.items():
            lesson = lesson_lookup[l_id]
            if c_id not in lesson["school_class_ids"]:
                continue
            n = lesson["preferred_block_size"]
            c_count_terms[day].append(n * var)
        c_count_vars: dict[int, cp_model.IntVar] = {}
        for day in days_set:
            cc = model.new_int_var(0, total, f"ccount_{c_id}_{day}")
            terms = c_count_terms[day]
            if terms:
                model.add(cc == cp_model.LinearExpr.sum(terms))
            else:
                model.add(cc == 0)
            c_count_vars[day] = cc
        dev_vars: list[cp_model.IntVar] = []
        for day in days_set:
            dev = model.new_int_var(0, total * d, f"dev_{c_id}_{day}")
            model.add_abs_equality(dev, c_count_vars[day] * d - total)
            dev_vars.append(dev)
        scaled = model.new_int_var(0, total * d * d, f"scaled_{c_id}")
        model.add(scaled == cp_model.LinearExpr.sum(dev_vars))
        quotient = model.new_int_var(0, total * d, f"quotient_{c_id}")
        model.add_division_equality(quotient, scaled, d)
        quotients.append(quotient)

    return _W_CLASS_DAY_BALANCE * cp_model.LinearExpr.sum(quotients) if quotients else 0


def _emit_objective(
    model: cp_model.CpModel,
    problem: dict[str, Any],
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    lookups: dict[str, Any],
) -> None:
    """Build CP-SAT model objective mirroring solver_core::score_solution.

    Five summands: subject_preference (per-anchor constant coefficient),
    home_room (per-anchor constant coefficient), class_gap (per-(class,
    day, position) channeling), teacher_gap (per-(teacher, day, position)
    channeling), class_day_balance (per-class abs-equality plus
    division-equality).
    """
    summand_subject_pref = _objective_subject_preference_terms(problem, anchor_vars, lookups)
    summand_home_room = _objective_home_room_term(problem, anchor_vars, lookups)
    summand_class_gap = _objective_gap_term(
        model, problem, anchor_vars, lookups, scope_kind="class", weight=_W_CLASS_GAP
    )
    summand_teacher_gap = _objective_gap_term(
        model, problem, anchor_vars, lookups, scope_kind="teacher", weight=_W_TEACHER_GAP
    )
    summand_class_day_balance = _objective_class_day_balance_term(
        model, problem, anchor_vars, lookups
    )
    model.minimize(
        summand_subject_pref
        + summand_home_room
        + summand_class_gap
        + summand_teacher_gap
        + summand_class_day_balance
    )


def _extract_placements(
    solver: cp_model.CpSolver,
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    meta: dict[str, Any],
) -> list[dict[str, str]]:
    """Walk solved anchor variables; expand each block to N per-hour Placement entries."""
    out: list[dict[str, str]] = []
    for (lesson_id, day, start_pos, room_id), var in anchor_vars.items():
        if solver.value(var) != 1:
            continue
        lesson = meta["lesson_lookup"][lesson_id]
        n = lesson["preferred_block_size"]
        teacher_id = lesson.get("teacher_pin") or lesson["teacher_candidates"][0]
        for i in range(n):
            tb_id = meta["tb_at"][(day, start_pos + i)]
            out.append(
                {
                    "lesson_id": lesson_id,
                    "time_block_id": tb_id,
                    "room_id": room_id,
                    "teacher_id": teacher_id,
                }
            )
    return out


# ----------------------------------------------------------------------
# CLI: python3 -m klassenzeit_solver.cpsat
# ----------------------------------------------------------------------


def _main() -> None:
    """``python -m klassenzeit_solver.cpsat --problem-file ... --deadline-ms ... [--seed N]``."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--problem-file", required=True, type=pathlib.Path)
    parser.add_argument("--deadline-ms", type=int, required=True)
    parser.add_argument("--seed", type=int, default=1)
    args = parser.parse_args()
    problem_json = args.problem_file.read_text()
    sys.stdout.write(solve_cpsat_json(problem_json, deadline_ms=args.deadline_ms, seed=args.seed))


if __name__ == "__main__":
    _main()
