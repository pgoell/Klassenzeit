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

    model.minimize(0)
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
        teacher_id = lesson["teacher_id"]
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
                teacher_terms[lesson["teacher_id"]].append(var)
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
        teacher_anchor_terms[lesson["teacher_id"]].append((var, lesson["preferred_block_size"]))
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
        n = meta["lesson_lookup"][lesson_id]["preferred_block_size"]
        for i in range(n):
            tb_id = meta["tb_at"][(day, start_pos + i)]
            out.append(
                {
                    "lesson_id": lesson_id,
                    "time_block_id": tb_id,
                    "room_id": room_id,
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
