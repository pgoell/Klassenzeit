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

from klassenzeit_solver._rust import quality_report_json, score_solution_json

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
_W_PREFER_CLASS_TEACHER = 5  # item 67: tentative, mirrors _W_PREFER_HOME_ROOM
# item 57: mirror PRODUCTION_ACTIVE_WEIGHTS per-class worst-case axes
_W_MAX_PER_CLASS_SPREAD = 10
_W_MAX_PER_CLASS_INTERIOR_GAPS = 10
_W_SOFT_PIN_MISS = 5  # ADR 0042: tentative weight, mirrors _W_PREFER_HOME_ROOM


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
        placements_json = json.dumps(placements)
        soft_score = score_solution_json(problem_json, placements_json)
        quality_report = json.loads(
            quality_report_json(problem_json, placements_json, json.dumps([]))
        )
        ttf = callback.first_ms
        tto = solver.WallTime() * 1000.0 if status == cp_model.OPTIMAL else None
        return json.dumps(
            {
                "placements": placements,
                "violations": [],
                "soft_score": int(soft_score),
                "quality_report": quality_report,
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
        quality_report = json.loads(quality_report_json(problem_json, "[]", json.dumps(violations)))
        return json.dumps(
            {
                "placements": [],
                "violations": violations,
                "soft_score": 0,
                "quality_report": quality_report,
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
    t_chosen = _create_teacher_choice_vars(model, problem)
    class_subject_lessons = _emit_class_subject_teacher_uniformity(model, problem, t_chosen)

    y_and_t = _create_y_and_t_vars(model, problem, anchor_vars, anchors_for_lesson, t_chosen)

    _emit_cardinality(model, problem, anchor_vars, anchors_for_lesson)
    _emit_per_candidate_anchor_compatibility(
        model, problem, anchor_vars, anchors_for_lesson, lookups, t_chosen
    )
    _emit_non_overlap(model, anchor_vars, lookups, y_and_t)
    _emit_teacher_max_hours(model, anchor_vars, lookups, t_chosen)
    _emit_same_room(model, problem, anchor_vars, anchors_for_lesson)
    _emit_lesson_group_co_placement(model, problem, anchor_vars, anchors_for_lesson)
    _emit_pinned_placements(model, problem, anchor_vars, lookups)

    _emit_objective(
        model,
        problem,
        anchor_vars,
        anchors_for_lesson,
        lookups,
        t_chosen,
        class_subject_lessons,
        y_and_t,
    )
    meta: dict[str, Any] = {
        "lesson_lookup": lookups["lesson_lookup"],
        "tb_at": lookups["tb_at"],
        "t_chosen": t_chosen,
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
    """Create one BoolVar per (lesson, day, start_pos, room) after pruning.

    With teacher chosen as a decision variable (items 64-68), an anchor
    exists iff AT LEAST ONE candidate teacher is qualified for the subject
    AND not blocked at every window position; the per-candidate
    availability constraints linking the anchor to ``t_chosen[(lesson, t)]``
    are emitted by ``_emit_per_candidate_anchor_compatibility``.
    """
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
        candidates: list[str] = list(lesson["teacher_candidates"])
        subject_id = lesson["subject_id"]
        n = lesson["preferred_block_size"]
        # Keep only candidates qualified for the subject.
        qualified_candidates = [t for t in candidates if (t, subject_id) in teacher_qualifies]
        if not qualified_candidates:
            continue

        for day, positions in positions_per_day.items():
            pos_set = set(positions)
            for start_pos in positions:
                if not all((start_pos + i) in pos_set for i in range(n)):
                    continue
                window_tb_ids = [tb_at[(day, start_pos + i)] for i in range(n)]
                # At least one qualified candidate must be unblocked across
                # the window for the anchor to be feasible at all.
                window_feasible_for_some_candidate = any(
                    all((t, tb_id) not in teacher_blocked for tb_id in window_tb_ids)
                    for t in qualified_candidates
                )
                if not window_feasible_for_some_candidate:
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


def _create_teacher_choice_vars(
    model: cp_model.CpModel,
    problem: dict[str, Any],
) -> dict[tuple[str, str], cp_model.IntVar]:
    """Create one BoolVar per (lesson, teacher) over teacher_candidates.

    Adds an exactly-one constraint per lesson and forces the pin (when set)
    to 1. Item 64-68: teacher assignment becomes a CP-SAT decision.
    """
    t_chosen: dict[tuple[str, str], cp_model.IntVar] = {}
    for lesson in problem["lessons"]:
        l_id = lesson["id"]
        candidates: list[str] = list(lesson["teacher_candidates"])
        if not candidates:
            # validate_structural would have rejected this earlier; safety only.
            continue
        for teacher_id in candidates:
            t_chosen[(l_id, teacher_id)] = model.new_bool_var(f"t_{l_id}_{teacher_id}")
        model.add_exactly_one([t_chosen[(l_id, t)] for t in candidates])
        pin = lesson.get("teacher_pin")
        if pin is not None and pin in candidates:
            model.add(t_chosen[(l_id, pin)] == 1)
    return t_chosen


def _emit_class_subject_teacher_uniformity(  # noqa: PLR0912 (per-pair / per-candidate / per-lesson nested branches; flattening hurts readability)
    model: cp_model.CpModel,
    problem: dict[str, Any],
    t_chosen: dict[tuple[str, str], cp_model.IntVar],
) -> dict[tuple[str, str], list[dict[str, Any]]]:
    """Pairwise per-(class, subject) uniformity over t_chosen (item 66).

    Every pair of lessons sharing a (class, subject) must end up taught by
    the same teacher. A multi-class lesson contributes to multiple
    (class, subject) groups; the lesson is one variable so a lesson
    appearing in multiple groups is consistent across them by transitivity
    of equality constraints.
    """
    class_subject_lessons: dict[tuple[str, str], list[dict[str, Any]]] = {}
    for lesson in problem["lessons"]:
        for class_id in lesson["school_class_ids"]:
            key = (class_id, lesson["subject_id"])
            class_subject_lessons.setdefault(key, []).append(lesson)

    for lessons_in_pair in class_subject_lessons.values():
        if len(lessons_in_pair) <= 1:
            continue
        union_candidates: set[str] = set()
        for lp in lessons_in_pair:
            union_candidates.update(lp["teacher_candidates"])
        anchor = lessons_in_pair[0]
        anchor_candidates = set(anchor["teacher_candidates"])
        for teacher_id in union_candidates:
            if teacher_id in anchor_candidates:
                anchor_var = t_chosen[(anchor["id"], teacher_id)]
                for other in lessons_in_pair[1:]:
                    if teacher_id in other["teacher_candidates"]:
                        model.add(t_chosen[(other["id"], teacher_id)] == anchor_var)
                    else:
                        # Other lesson cannot pick this teacher -> anchor cannot either.
                        model.add(anchor_var == 0)
            else:
                # Anchor cannot pick this teacher -> force every other lesson off.
                for other in lessons_in_pair[1:]:
                    if teacher_id in other["teacher_candidates"]:
                        model.add(t_chosen[(other["id"], teacher_id)] == 0)
    return class_subject_lessons


def _emit_per_candidate_anchor_compatibility(
    model: cp_model.CpModel,
    problem: dict[str, Any],
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    anchors_for_lesson: dict[str, list[AnchorKey]],
    lookups: dict[str, Any],
    t_chosen: dict[tuple[str, str], cp_model.IntVar],
) -> None:
    """Forbid the (anchor, teacher) pair when the teacher is blocked at any window position.

    Anchor existence (``_create_anchor_vars``) admits a window if AT LEAST
    ONE candidate is feasible there; this helper closes the loop by saying:
    if anchor `y` is taken AND teacher `t` is chosen, `t` cannot be blocked
    at any of the window's time blocks. Encoded as
    ``anchor_var + t_chosen[(l, t)] <= 1`` per blocked candidate at the
    window.
    """
    tb_at = lookups["tb_at"]
    teacher_blocked = lookups["teacher_blocked"]
    teacher_qualifies = lookups["teacher_qualifies"]
    for lesson in problem["lessons"]:
        l_id = lesson["id"]
        subject_id = lesson["subject_id"]
        n = lesson["preferred_block_size"]
        candidates = lesson["teacher_candidates"]
        # Unqualified candidates are forbidden globally for the lesson.
        for teacher_id in candidates:
            if (teacher_id, subject_id) not in teacher_qualifies:
                model.add(t_chosen[(l_id, teacher_id)] == 0)
        for key in anchors_for_lesson.get(l_id, []):
            (_l, day, start_pos, _r) = key
            anchor_var = anchor_vars[key]
            window_tb_ids = [tb_at[(day, start_pos + i)] for i in range(n)]
            for teacher_id in candidates:
                if (teacher_id, subject_id) not in teacher_qualifies:
                    continue
                if any((teacher_id, tb_id) in teacher_blocked for tb_id in window_tb_ids):
                    model.add(anchor_var + t_chosen[(l_id, teacher_id)] <= 1)


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


def _create_y_and_t_vars(
    model: cp_model.CpModel,
    problem: dict[str, Any],
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    anchors_for_lesson: dict[str, list[AnchorKey]],
    t_chosen: dict[tuple[str, str], cp_model.IntVar],
) -> dict[tuple[str, str, AnchorKey], cp_model.IntVar]:
    """Build ``y_and_t[(lesson, teacher, anchor)] = anchor_var AND t_chosen[(lesson, teacher)]``.

    Singleton-candidate lessons reuse ``anchor_var`` directly (t_chosen is
    identically 1 by ``add_exactly_one``). Multi-candidate lessons get an
    explicit AND-var with three channeling inequalities. Used by both
    teacher non-overlap (sum<=1 per (teacher, day, pos)) and the teacher
    gap presence channeling in ``_emit_objective``.
    """
    y_and_t: dict[tuple[str, str, AnchorKey], cp_model.IntVar] = {}
    for lesson in problem["lessons"]:
        l_id = lesson["id"]
        candidates: list[str] = list(lesson["teacher_candidates"])
        for key in anchors_for_lesson.get(l_id, []):
            anchor_var = anchor_vars[key]
            for teacher_id in candidates:
                if len(candidates) == 1:
                    y_and_t[(l_id, teacher_id, key)] = anchor_var
                    continue
                t_var = t_chosen[(l_id, teacher_id)]
                and_var = model.new_bool_var(f"yt_{l_id}_{teacher_id}_{key[1]}_{key[2]}_{key[3]}")
                model.add(and_var <= anchor_var)
                model.add(and_var <= t_var)
                model.add(and_var >= anchor_var + t_var - 1)
                y_and_t[(l_id, teacher_id, key)] = and_var
    return y_and_t


def _emit_non_overlap(  # noqa: PLR0912 (lesson-group dedup plus teacher-product plus room non-overlap branches; splitting hurts readability)
    model: cp_model.CpModel,
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    lookups: dict[str, Any],
    y_and_t: dict[tuple[str, str, AnchorKey], cp_model.IntVar],
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

    Teacher non-overlap is per-(teacher, day, pos): for each candidate
    teacher, sum over ``(lesson, anchor)`` covering the slot of
    ``y_and_t[(lesson, teacher, anchor)]`` must be at most 1. Room
    non-overlap stays anchor-only (rooms are not a decision variable).
    """
    lesson_lookup = lookups["lesson_lookup"]
    positions_per_day = lookups["positions_per_day"]
    for day, positions in positions_per_day.items():
        for pos in positions:
            class_group_first: dict[tuple[str, str], tuple[str, list[cp_model.IntVar]]] = {}
            teacher_terms: dict[str, list[cp_model.IntVar]] = defaultdict(list)
            room_terms: dict[str, list[cp_model.IntVar]] = defaultdict(list)
            for key, var in anchor_vars.items():
                (l_id, d, start_pos, r_id) = key
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
                for teacher_id in lesson["teacher_candidates"]:
                    teacher_terms[teacher_id].append(y_and_t[(l_id, teacher_id, key)])
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
    t_chosen: dict[tuple[str, str], cp_model.IntVar],
) -> None:
    """Per-teacher max-hours ceiling via ``t_chosen``-weighted lesson hours.

    A teacher's per-week hours equal the sum over lessons that picked them
    (``t_chosen[(l, t)] == 1``) of ``hours_per_week``. ``hours_per_week ==
    preferred_block_size * K`` where K is the number of blocks; using
    ``t_chosen`` instead of ``anchor_vars * N`` collapses the K anchors
    into one term per (lesson, teacher) pair.
    """
    lesson_lookup = lookups["lesson_lookup"]
    teacher_max_hours = lookups["teacher_max_hours"]
    _ = anchor_vars  # not used; t_chosen carries the lesson-pick information
    teacher_lesson_terms: dict[str, list[tuple[cp_model.IntVar, int]]] = defaultdict(list)
    for (l_id, t_id), tvar in t_chosen.items():
        lesson = lesson_lookup[l_id]
        teacher_lesson_terms[t_id].append((tvar, lesson["hours_per_week"]))
    for t_id, terms in teacher_lesson_terms.items():
        if t_id in teacher_max_hours:
            model.add(sum(var * h for var, h in terms) <= teacher_max_hours[t_id])


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
    """Force y[lesson, day, anchor_pos, room] = 1 per HARD pin; multi-block pins collapse to anchor.

    Soft pins (`kind == "soft"`, ADR 0042) are aspirational and ride along
    `_objective_soft_pin_term`; they must NOT be forced here or the model
    would either pin them verbatim (zero misses by construction) or panic
    via `_force_infeasible` when the soft pin is incompatible with other
    constraints. Filter to hard pins (default `kind`) before forcing.
    """
    tb_pos_lookup = lookups["tb_pos_lookup"]
    pins_by_lesson_day_room: dict[tuple[str, int, str], list[int]] = defaultdict(list)
    for pin in problem.get("pinned_placements", []):
        if pin.get("kind", "hard") == "soft":
            continue
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
    y_and_t: dict[tuple[str, str, AnchorKey], cp_model.IntVar] | None = None,
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

    Teacher scope (item 68): the contributing variable is
    ``y_and_t[(lesson, teacher, anchor)] = anchor_var AND t_chosen[(lesson, teacher)]``,
    not the bare ``anchor_var``; a teacher only counts as present when an
    anchor of one of its candidate lessons is taken AND the teacher is the
    chosen one for that lesson.
    """
    lesson_lookup = lookups["lesson_lookup"]
    positions_per_day = lookups["positions_per_day"]
    if scope_kind == "class":
        entity_ids: set[str] = {c["id"] for c in problem["school_classes"]}
    elif scope_kind == "teacher":
        if y_and_t is None:  # pragma: no cover - guarded by callers
            raise ValueError("scope_kind='teacher' requires y_and_t")
        entity_ids = {t["id"] for t in problem["teachers"]}
    else:  # pragma: no cover - guarded by callers
        raise ValueError(f"unknown scope_kind: {scope_kind}")

    coverage: dict[tuple[str, int, int], list[cp_model.IntVar]] = {}
    for key, var in anchor_vars.items():
        (l_id, day, start_pos, _r_id) = key
        lesson = lesson_lookup[l_id]
        n = lesson["preferred_block_size"]
        if scope_kind == "class":
            for offset in range(n):
                p = start_pos + offset
                for owner in lesson["school_class_ids"]:
                    coverage.setdefault((owner, day, p), []).append(var)
        else:
            # y_and_t presence guarded at function entry for scope_kind="teacher".
            assert y_and_t is not None  # noqa: S101 (postcondition guard)
            for offset in range(n):
                p = start_pos + offset
                for teacher_id in lesson["teacher_candidates"]:
                    coverage.setdefault((teacher_id, day, p), []).append(
                        y_and_t[(l_id, teacher_id, key)]
                    )

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


def _build_gap_vars_by_entity_day(
    model: cp_model.CpModel,
    problem: dict[str, Any],
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    lookups: dict[str, Any],
    scope_kind: str,
    y_and_t: dict[tuple[str, str, AnchorKey], cp_model.IntVar] | None = None,
) -> dict[tuple[str, int], list[cp_model.IntVar]]:
    """Build per-(entity, day, position) gap BoolVars, indexed by (entity, day).

    Channels has_left/has_right/gap exactly once per (entity, day, position)
    so the class_gap summand and the per-class worst-case interior-gaps
    summand (item 57) share the same gap variables.
    """
    presence = _build_per_slot_presence(model, problem, anchor_vars, lookups, scope_kind, y_and_t)
    positions_per_day = lookups["positions_per_day"]
    if scope_kind == "class":
        entity_ids: set[str] = {c["id"] for c in problem["school_classes"]}
    else:
        entity_ids = {t["id"] for t in problem["teachers"]}

    gaps_by_entity_day: dict[tuple[str, int], list[cp_model.IntVar]] = defaultdict(list)
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
                gaps_by_entity_day[(entity_id, day)].append(gap)
    return gaps_by_entity_day


def _objective_gap_term(
    gaps_by_entity_day: dict[tuple[str, int], list[cp_model.IntVar]],
    weight: int,
) -> cp_model.LinearExpr | int:
    """Weighted sum of all per-(entity, day, position) gap BoolVars.

    Consumes the gap vars built by ``_build_gap_vars_by_entity_day``.
    """
    gap_vars: list[cp_model.IntVar] = []
    for vars_list in gaps_by_entity_day.values():
        gap_vars.extend(vars_list)
    return weight * cp_model.LinearExpr.sum(gap_vars) if gap_vars else 0


def _build_class_count_per_day(
    model: cp_model.CpModel,
    problem: dict[str, Any],
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    lookups: dict[str, Any],
) -> dict[str, dict[int, cp_model.IntVar]]:
    """Per-class per-day placement count IntVar; channels N(l) * y[anchor].

    Shared by ``_objective_class_day_balance_term`` and the per-class
    worst-case spread summand (item 57). For each class with at least one
    contributing lesson, builds one IntVar per day in ``positions_per_day``;
    the var equals ``sum over anchors (l, day, *, *) where c_id in
    lesson.school_class_ids of N(l) * y[anchor]``. Classes with zero total
    hours are omitted from the returned map (they would contribute 0 to
    either consumer).
    """
    lesson_lookup = lookups["lesson_lookup"]
    positions_per_day = lookups["positions_per_day"]
    days_set: set[int] = set(positions_per_day.keys())
    classes = problem["school_classes"]

    class_total: dict[str, int] = {}
    for cls in classes:
        c_id = cls["id"]
        total = 0
        for lesson in problem["lessons"]:
            if c_id in lesson["school_class_ids"]:
                total += lesson["hours_per_week"]
        class_total[c_id] = total

    per_class: dict[str, dict[int, cp_model.IntVar]] = {}
    for cls in classes:
        c_id = cls["id"]
        total = class_total[c_id]
        if total == 0:
            continue
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
        per_class[c_id] = c_count_vars
    return per_class


def _objective_class_day_balance_term(
    model: cp_model.CpModel,
    problem: dict[str, Any],
    lookups: dict[str, Any],
    class_count_per_day: dict[str, dict[int, cp_model.IntVar]],
) -> cp_model.LinearExpr | int:
    """Per-class scaled L1 day-balance cost mirroring score::class_day_balance_cost.

    Consumes the per-class per-day count IntVars from
    ``_build_class_count_per_day``. For each class with placements:
      class_total = sum lesson.hours_per_week for lessons where class is in school_class_ids
      dev[class, day] = abs(c_count[class, day] * D - class_total)
      scaled[class] = sum_day dev[class, day]
      quotient[class] = scaled[class] // D (CP-SAT add_division_equality)
    Returns _W_CLASS_DAY_BALANCE * sum_class quotient[class].
    """
    positions_per_day = lookups["positions_per_day"]
    days_set: set[int] = set(positions_per_day.keys())
    if not days_set:
        return 0
    d = len(days_set)

    class_total: dict[str, int] = {}
    for cls in problem["school_classes"]:
        c_id = cls["id"]
        total = 0
        for lesson in problem["lessons"]:
            if c_id in lesson["school_class_ids"]:
                total += lesson["hours_per_week"]
        class_total[c_id] = total

    quotients: list[cp_model.IntVar] = []
    for c_id, c_count_vars in class_count_per_day.items():
        total = class_total[c_id]
        if total == 0:
            continue
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


def _objective_max_per_class_spread_term(
    model: cp_model.CpModel,
    problem: dict[str, Any],
    lookups: dict[str, Any],
    class_count_per_day: dict[str, dict[int, cp_model.IntVar]],
) -> cp_model.LinearExpr | int:
    """Per-class worst-case daily-load spread, maxed across classes.

    Mirrors ``score::worst_class_spread`` (item 57): the Rust helper uses a
    fixed-width ``[0; 5]`` per-class day array (day axis 0..5), so a class
    with placements only on Monday has ``min(daily_count) == 0`` from the
    untouched Tuesday-Friday entries and ``spread == max - 0``. We mirror
    by feeding the per-day count vars for every ``day_of_week`` in
    ``positions_per_day`` PLUS one literal ``0`` for each ``day_of_week``
    in 0..5 absent from ``positions_per_day``, so the ``min`` over an
    under-populated day axis still resolves to ``0`` the way the Rust
    helper does on its fixed-width array.

    Classes with zero total hours are absent from ``class_count_per_day``
    and contribute 0 by construction (the Rust helper omits them from its
    ``counts`` map for the same reason).

    Returns ``_W_MAX_PER_CLASS_SPREAD * max over classes of spread_class``,
    or the literal ``0`` when no class has any placement.
    """
    if _W_MAX_PER_CLASS_SPREAD == 0 or not class_count_per_day:
        return 0
    positions_per_day = lookups["positions_per_day"]
    if not positions_per_day:
        return 0

    # Per-class upper bound = total hours of lessons that include the class.
    # Matches ``_build_class_count_per_day``'s domain on each c_count var.
    class_total: dict[str, int] = {}
    for cls in problem["school_classes"]:
        c_id = cls["id"]
        if c_id not in class_count_per_day:
            continue
        total = 0
        for lesson in problem["lessons"]:
            if c_id in lesson["school_class_ids"]:
                total += lesson["hours_per_week"]
        class_total[c_id] = total

    # The Rust helper's fixed-width day axis runs 0..5. Days without anchors
    # contribute a literal 0 to min/max so under-populated problems still
    # report spread = max - 0. Materialising as a constant IntVar keeps the
    # min/max-equality channeling untouched.
    rust_day_axis_size = 5
    days_in_problem: set[int] = set(positions_per_day.keys())
    missing_days: list[int] = [d for d in range(rust_day_axis_size) if d not in days_in_problem]
    zero_const: cp_model.IntVar | None = None
    if missing_days:
        zero_const = model.new_int_var(0, 0, "spread_zero_day")

    spread_vars: list[cp_model.IntVar] = []
    for c_id, c_count_vars in class_count_per_day.items():
        per_day_vars: list[cp_model.IntVar] = list(c_count_vars.values())
        if zero_const is not None:
            per_day_vars.extend([zero_const] * len(missing_days))
        upper = class_total[c_id]
        max_var = model.new_int_var(0, upper, f"cmax_{c_id}")
        min_var = model.new_int_var(0, upper, f"cmin_{c_id}")
        model.add_max_equality(max_var, per_day_vars)
        model.add_min_equality(min_var, per_day_vars)
        spread_var = model.new_int_var(0, upper, f"cspread_{c_id}")
        model.add(spread_var == max_var - min_var)
        spread_vars.append(spread_var)

    upper_bound = max(class_total[c_id] for c_id in class_count_per_day)
    worst_var = model.new_int_var(0, upper_bound, "worst_per_class_spread")
    model.add_max_equality(worst_var, spread_vars)
    return _W_MAX_PER_CLASS_SPREAD * worst_var


def _objective_max_per_class_interior_gaps_term(
    model: cp_model.CpModel,
    problem: dict[str, Any],
    gaps_by_class_day: dict[tuple[str, int], list[cp_model.IntVar]],
    lookups: dict[str, Any],
) -> cp_model.LinearExpr | int:
    """Per-class summed-over-days interior gaps, maxed across classes.

    Mirrors ``score::worst_class_interior_gaps`` (item 57). Reuses the
    per-(class, day, position) gap BoolVars channelled for the class_gap
    summand: sums gap vars over days per class, then maxes across classes.
    Classes with no contributing gap var (i.e. classes with no interior
    position on any day) contribute 0.
    """
    if _W_MAX_PER_CLASS_INTERIOR_GAPS == 0 or not gaps_by_class_day:
        return 0
    positions_per_day = lookups["positions_per_day"]
    # Upper bound: total interior positions across all days. Safe loose bound.
    interior_positions_per_day = sum(max(len(ps) - 2, 0) for ps in positions_per_day.values())
    if interior_positions_per_day == 0:
        return 0

    # Sum gap vars over days per class.
    per_class_gaps: dict[str, list[cp_model.IntVar]] = defaultdict(list)
    for (entity_id, _day), gap_vars in gaps_by_class_day.items():
        per_class_gaps[entity_id].extend(gap_vars)

    if not per_class_gaps:
        return 0

    per_class_sums: list[cp_model.IntVar] = []
    for c_id, gap_vars in per_class_gaps.items():
        class_sum = model.new_int_var(0, interior_positions_per_day, f"cgaps_{c_id}")
        model.add(class_sum == cp_model.LinearExpr.sum(gap_vars))
        per_class_sums.append(class_sum)

    # Include classes that have NO gap vars: they contribute 0. Adding them
    # to the max via per_class_sums is unnecessary because zeros do not raise
    # the max; the existing per_class_sums list is sufficient.
    _ = problem  # signature parity with other axis helpers; problem walked above

    worst_var = model.new_int_var(0, interior_positions_per_day, "worst_per_class_interior_gaps")
    model.add_max_equality(worst_var, per_class_sums)
    return _W_MAX_PER_CLASS_INTERIOR_GAPS * worst_var


def _objective_soft_pin_term(
    model: cp_model.CpModel,
    problem: dict[str, Any],
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    anchors_for_lesson: dict[str, list[AnchorKey]],
    lookups: dict[str, Any],
) -> cp_model.LinearExpr | int:
    """Soft-pin miss term: one per soft pin not covered by any selected anchor.

    Each miss contributes +1 to the minimised objective.

    Mirrors `solver_core::score::score_solution`'s `soft_pin_miss` axis
    (ADR 0042). Soft pin `(lesson, tb)` is HONORED when at least one selected
    anchor `(lesson, day_of_tb, start_pos, room)` covers `pos_of_tb` in its
    N-block window. Anchor coverage is taken from `anchors_for_lesson` and
    the lesson's `preferred_block_size`; the per-pin presence indicator is
    the disjunction of those anchor BoolVars.

    A pin whose lesson has no candidate anchor (e.g. orphaned `lesson_id`,
    or the anchor was pruned by `_create_anchor_vars`) contributes a literal
    miss of 1 since the pin cannot be honored under any feasible assignment.
    """
    tb_pos_lookup = lookups["tb_pos_lookup"]
    lesson_lookup = lookups["lesson_lookup"]
    miss_terms: list[cp_model.LinearExpr | int] = []
    for pin in problem.get("pinned_placements", []):
        if pin.get("kind", "hard") != "soft":
            continue
        if pin["time_block_id"] not in tb_pos_lookup:
            miss_terms.append(1)
            continue
        l_id = pin["lesson_id"]
        lesson = lesson_lookup.get(l_id)
        if lesson is None:
            miss_terms.append(1)
            continue
        n = lesson["preferred_block_size"]
        pin_day, pin_pos = tb_pos_lookup[pin["time_block_id"]]
        covering_anchors: list[cp_model.IntVar] = []
        for key in anchors_for_lesson.get(l_id, []):
            (_l, day, start_pos, _r) = key
            if day != pin_day:
                continue
            if start_pos <= pin_pos < start_pos + n:
                covering_anchors.append(anchor_vars[key])
        if not covering_anchors:
            miss_terms.append(1)
            continue
        # `present` is 1 iff at least one covering anchor is selected.
        # Because class non-overlap forbids two anchors of the same lesson
        # covering the same (day, position), at most one var in
        # `covering_anchors` is 1 in any feasible solution, so the sum is a
        # valid 0-1 presence indicator (no `add_max_equality` needed).
        miss_var = model.new_bool_var(f"soft_pin_miss[{l_id}_{pin['time_block_id']}]")
        model.add(miss_var == 1 - cp_model.LinearExpr.sum(covering_anchors))
        miss_terms.append(miss_var)
    if not miss_terms:
        return 0
    return _W_SOFT_PIN_MISS * cp_model.LinearExpr.sum(miss_terms)


def _objective_prefer_class_teacher_term(
    problem: dict[str, Any],
    t_chosen: dict[tuple[str, str], cp_model.IntVar],
    class_subject_lessons: dict[tuple[str, str], list[dict[str, Any]]],
) -> cp_model.LinearExpr | int:
    """Penalise (class, subject) pairs that pick a non-klassenlehrer when the klt is qualified.

    Mirrors ``solver_core::score::score_solution``'s ``prefer_class_teacher``
    summand: count one miss per (class, subject) pair whose
    ``class.class_teacher_id`` is qualified for the subject but the picked
    teacher is not the klt. Pairwise uniformity (item 66) ensures every
    lesson in the pair shares one teacher; we read the anchor lesson's
    ``t_chosen`` for the klt as the indicator. Item 67.
    """
    school_classes_by_id: dict[str, dict[str, Any]] = {
        c["id"]: c for c in problem["school_classes"]
    }
    qualified_by_subject: dict[str, set[str]] = defaultdict(set)
    for q in problem["teacher_qualifications"]:
        qualified_by_subject[q["subject_id"]].add(q["teacher_id"])

    terms: list[cp_model.LinearExpr] = []
    fixed_cost = 0
    for (cid, sid), lessons_in_pair in class_subject_lessons.items():
        cls = school_classes_by_id.get(cid)
        if cls is None:
            continue
        klt = cls.get("class_teacher_id")
        if klt is None:
            continue
        if klt not in qualified_by_subject.get(sid, set()):
            continue
        anchor = lessons_in_pair[0]
        if klt in anchor["teacher_candidates"]:
            # Cost is _W times (1 - t_chosen[(anchor, klt)]): zero when klt picked, _W otherwise.
            terms.append(_W_PREFER_CLASS_TEACHER * (1 - t_chosen[(anchor["id"], klt)]))
        else:
            # klt qualified but not in candidates: pair cannot satisfy the
            # preference, fixed full-weight cost. Mirrors the score-solution
            # behavior where a (class, subject) miss is one weight unit.
            fixed_cost += _W_PREFER_CLASS_TEACHER
    if not terms:
        return fixed_cost
    return cp_model.LinearExpr.sum(terms) + fixed_cost


def _emit_objective(
    model: cp_model.CpModel,
    problem: dict[str, Any],
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    anchors_for_lesson: dict[str, list[AnchorKey]],
    lookups: dict[str, Any],
    t_chosen: dict[tuple[str, str], cp_model.IntVar],
    class_subject_lessons: dict[tuple[str, str], list[dict[str, Any]]],
    y_and_t: dict[tuple[str, str, AnchorKey], cp_model.IntVar],
) -> None:
    """Build CP-SAT model objective mirroring solver_core::score_solution.

    Nine summands: subject_preference (per-anchor constant coefficient),
    home_room (per-anchor constant coefficient), class_gap (per-(class,
    day, position) channeling), teacher_gap (per-(teacher, day, position)
    channeling on ``y_and_t`` so unchosen candidates do not count as
    present), class_day_balance (per-class abs-equality plus
    division-equality), prefer_class_teacher (per-(class, subject) pair
    soft cost via ``t_chosen``), max_per_class_spread (per-class
    max(daily_count) - min(daily_count), maxed across classes, item 57),
    max_per_class_interior_gaps (per-class summed gap BoolVars, maxed
    across classes, item 57), soft_pin_miss (per-soft-pin presence
    indicator over covering anchors, ADR 0042). The per-(class, day)
    placement-count IntVars are shared between class_day_balance and
    max_per_class_spread via ``_build_class_count_per_day``; the
    per-(class, day, position) gap BoolVars are shared between class_gap
    and max_per_class_interior_gaps via ``_build_gap_vars_by_entity_day``.
    """
    summand_subject_pref = _objective_subject_preference_terms(problem, anchor_vars, lookups)
    summand_home_room = _objective_home_room_term(problem, anchor_vars, lookups)
    class_gaps_by_day = _build_gap_vars_by_entity_day(
        model, problem, anchor_vars, lookups, scope_kind="class"
    )
    summand_class_gap = _objective_gap_term(class_gaps_by_day, _W_CLASS_GAP)
    teacher_gaps_by_day = _build_gap_vars_by_entity_day(
        model,
        problem,
        anchor_vars,
        lookups,
        scope_kind="teacher",
        y_and_t=y_and_t,
    )
    summand_teacher_gap = _objective_gap_term(teacher_gaps_by_day, _W_TEACHER_GAP)
    class_count_per_day = _build_class_count_per_day(model, problem, anchor_vars, lookups)
    summand_class_day_balance = _objective_class_day_balance_term(
        model, problem, lookups, class_count_per_day
    )
    summand_prefer_class_teacher = _objective_prefer_class_teacher_term(
        problem, t_chosen, class_subject_lessons
    )
    summand_max_per_class_spread = _objective_max_per_class_spread_term(
        model, problem, lookups, class_count_per_day
    )
    summand_max_per_class_interior_gaps = _objective_max_per_class_interior_gaps_term(
        model, problem, class_gaps_by_day, lookups
    )
    summand_soft_pin_miss = _objective_soft_pin_term(
        model, problem, anchor_vars, anchors_for_lesson, lookups
    )
    model.minimize(
        summand_subject_pref
        + summand_home_room
        + summand_class_gap
        + summand_teacher_gap
        + summand_class_day_balance
        + summand_prefer_class_teacher
        + summand_max_per_class_spread
        + summand_max_per_class_interior_gaps
        + summand_soft_pin_miss
    )


def _extract_placements(
    solver: cp_model.CpSolver,
    anchor_vars: dict[AnchorKey, cp_model.IntVar],
    meta: dict[str, Any],
) -> list[dict[str, str]]:
    """Walk solved anchor variables; expand each block to N per-hour Placement entries."""
    t_chosen: dict[tuple[str, str], cp_model.IntVar] = meta["t_chosen"]
    lesson_to_teacher: dict[str, str] = {}
    for (lesson_id, teacher_id), tvar in t_chosen.items():
        if solver.value(tvar) == 1:
            lesson_to_teacher[lesson_id] = teacher_id
    out: list[dict[str, str]] = []
    for (lesson_id, day, start_pos, room_id), var in anchor_vars.items():
        if solver.value(var) != 1:
            continue
        lesson = meta["lesson_lookup"][lesson_id]
        n = lesson["preferred_block_size"]
        teacher_id = lesson_to_teacher.get(lesson_id)
        if teacher_id is None:
            raise RuntimeError(
                f"CP-SAT did not pick a teacher for lesson {lesson_id} "
                f"despite candidates {lesson['teacher_candidates']}"
            )
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
