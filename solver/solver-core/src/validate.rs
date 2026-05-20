//! Structural validation and the pre-solve cross-entity check.
//!
//! `validate_structural` returns `Err(Error::Input)` on malformed input (unknown
//! references, duplicate IDs, `hours_per_week == 0`, empty `time_blocks` or
//! `rooms`). `pre_solve_violations` takes a structurally-valid `Problem` and
//! emits `NoQualifiedTeacher` violations for every lesson whose teacher lacks
//! the subject qualification.

use std::collections::{HashMap, HashSet};

use crate::error::Error;
use crate::ids::{LessonId, RoomId, SchoolClassId, SubjectId, TeacherId, TimeBlockId};
use crate::types::{Lesson, Placement, Problem, TimeBlockKind, Violation, ViolationKind};

/// Validate a `Problem` against purely structural rules: non-empty core
/// collections, unique IDs, known references, `hours_per_week > 0`.
pub fn validate_structural(problem: &Problem) -> Result<(), Error> {
    if problem.time_blocks.is_empty() {
        return Err(Error::Input("problem has no time_blocks".into()));
    }
    if problem.rooms.is_empty() {
        return Err(Error::Input("problem has no rooms".into()));
    }

    let time_block_ids: HashSet<TimeBlockId> =
        collect_unique(problem.time_blocks.iter().map(|tb| tb.id), "time_blocks")?;
    let teacher_ids: HashSet<TeacherId> =
        collect_unique(problem.teachers.iter().map(|t| t.id), "teachers")?;
    let room_ids: HashSet<RoomId> = collect_unique(problem.rooms.iter().map(|r| r.id), "rooms")?;
    let subject_ids: HashSet<SubjectId> =
        collect_unique(problem.subjects.iter().map(|s| s.id), "subjects")?;
    let class_ids: HashSet<SchoolClassId> = collect_unique(
        problem.school_classes.iter().map(|c| c.id),
        "school_classes",
    )?;
    let _lesson_ids: HashSet<LessonId> =
        collect_unique(problem.lessons.iter().map(|l| l.id), "lessons")?;

    for lesson in &problem.lessons {
        if lesson.hours_per_week == 0 {
            return Err(Error::Input(format!(
                "lesson {} has hours_per_week = 0",
                lesson.id.0
            )));
        }
        if lesson.preferred_block_size == 0 {
            return Err(Error::Input(format!(
                "lesson {} has preferred_block_size = 0",
                lesson.id.0
            )));
        }
        if lesson.hours_per_week % lesson.preferred_block_size != 0 {
            return Err(Error::Input(format!(
                "lesson {}: hours_per_week ({}) is not divisible by preferred_block_size ({})",
                lesson.id.0, lesson.hours_per_week, lesson.preferred_block_size
            )));
        }
        if lesson.teacher_pin.is_none() && lesson.teacher_candidates.is_empty() {
            return Err(Error::Input(format!(
                "lesson {} has neither teacher_pin nor teacher_candidates",
                lesson.id.0
            )));
        }
        let assigned_teacher = lesson.assigned_teacher_id();
        if !teacher_ids.contains(&assigned_teacher) {
            return Err(Error::Input(format!(
                "lesson {} references unknown teacher {}",
                lesson.id.0, assigned_teacher.0
            )));
        }
        if !subject_ids.contains(&lesson.subject_id) {
            return Err(Error::Input(format!(
                "lesson {} references unknown subject {}",
                lesson.id.0, lesson.subject_id.0
            )));
        }
        if lesson.school_class_ids.is_empty() {
            return Err(Error::Input(format!(
                "lesson {} has empty school_class_ids",
                lesson.id.0
            )));
        }
        let mut seen_classes: HashSet<SchoolClassId> = HashSet::new();
        for class_id in &lesson.school_class_ids {
            if !seen_classes.insert(*class_id) {
                return Err(Error::Input(format!(
                    "lesson {} has duplicate school_class {} in school_class_ids",
                    lesson.id.0, class_id.0
                )));
            }
            if !class_ids.contains(class_id) {
                return Err(Error::Input(format!(
                    "lesson {} references unknown school_class {}",
                    lesson.id.0, class_id.0
                )));
            }
        }
    }
    for q in &problem.teacher_qualifications {
        if !teacher_ids.contains(&q.teacher_id) {
            return Err(Error::Input(format!(
                "teacher_qualification references unknown teacher {}",
                q.teacher_id.0
            )));
        }
        if !subject_ids.contains(&q.subject_id) {
            return Err(Error::Input(format!(
                "teacher_qualification references unknown subject {}",
                q.subject_id.0
            )));
        }
    }
    for b in &problem.teacher_blocked_times {
        if !teacher_ids.contains(&b.teacher_id) {
            return Err(Error::Input(format!(
                "teacher_blocked_time references unknown teacher {}",
                b.teacher_id.0
            )));
        }
        if !time_block_ids.contains(&b.time_block_id) {
            return Err(Error::Input(format!(
                "teacher_blocked_time references unknown time_block {}",
                b.time_block_id.0
            )));
        }
    }
    for b in &problem.room_blocked_times {
        if !room_ids.contains(&b.room_id) {
            return Err(Error::Input(format!(
                "room_blocked_time references unknown room {}",
                b.room_id.0
            )));
        }
        if !time_block_ids.contains(&b.time_block_id) {
            return Err(Error::Input(format!(
                "room_blocked_time references unknown time_block {}",
                b.time_block_id.0
            )));
        }
    }
    for s in &problem.room_subject_suitabilities {
        if !room_ids.contains(&s.room_id) {
            return Err(Error::Input(format!(
                "room_subject_suitability references unknown room {}",
                s.room_id.0
            )));
        }
        if !subject_ids.contains(&s.subject_id) {
            return Err(Error::Input(format!(
                "room_subject_suitability references unknown subject {}",
                s.subject_id.0
            )));
        }
    }

    use crate::ids::LessonGroupId;
    let mut groups: std::collections::HashMap<LessonGroupId, Vec<&crate::types::Lesson>> =
        std::collections::HashMap::new();
    for lesson in &problem.lessons {
        if let Some(group_id) = lesson.lesson_group_id {
            groups.entry(group_id).or_default().push(lesson);
        }
    }
    for (group_id, members) in &groups {
        if members.len() < 2 {
            continue;
        }
        let first = &members[0];
        for member in &members[1..] {
            if member.hours_per_week != first.hours_per_week {
                return Err(Error::Input(format!(
                    "lesson group {} members disagree on hours_per_week: {} vs {}",
                    group_id.0, first.hours_per_week, member.hours_per_week
                )));
            }
            if member.preferred_block_size != first.preferred_block_size {
                return Err(Error::Input(format!(
                    "lesson group {} members disagree on preferred_block_size: {} vs {}",
                    group_id.0, first.preferred_block_size, member.preferred_block_size
                )));
            }
        }
        let mut seen_teachers: HashSet<TeacherId> = HashSet::new();
        for member in members {
            let teacher = member.assigned_teacher_id();
            if !seen_teachers.insert(teacher) {
                return Err(Error::Input(format!(
                    "lesson group {} has duplicate teacher {}",
                    group_id.0, teacher.0
                )));
            }
        }
    }
    Ok(())
}

fn collect_unique<Id, I>(iter: I, kind: &'static str) -> Result<HashSet<Id>, Error>
where
    Id: std::hash::Hash + Eq + Copy + std::fmt::Display,
    I: IntoIterator<Item = Id>,
{
    let mut set = HashSet::new();
    for id in iter {
        if !set.insert(id) {
            return Err(Error::Input(format!("duplicate id {id} in {kind}")));
        }
    }
    Ok(set)
}

/// Hard-constraint sanity check: no `(class, day_of_week, subject)` triple may
/// span more than one room. Returns `Err(Error::Input)` listing the
/// violating triple and the conflicting rooms when triggered. A failure
/// here indicates a solver bug rather than malformed input; production
/// callers surface it as a runtime error.
pub fn validate_no_room_hopping(problem: &Problem, placements: &[Placement]) -> Result<(), Error> {
    use std::collections::hash_map::Entry;
    use std::collections::HashMap;
    let mut groups: HashMap<(SchoolClassId, u8, SubjectId), RoomId> = HashMap::new();
    for placement in placements {
        let lesson = problem
            .lessons
            .iter()
            .find(|l| l.id == placement.lesson_id)
            .ok_or_else(|| Error::Input(format!("unknown lesson {:?}", placement.lesson_id)))?;
        let tb = problem
            .time_blocks
            .iter()
            .find(|t| t.id == placement.time_block_id)
            .ok_or_else(|| {
                Error::Input(format!("unknown time block {:?}", placement.time_block_id))
            })?;
        for class_id in &lesson.school_class_ids {
            let key = (*class_id, tb.day_of_week, lesson.subject_id);
            match groups.entry(key) {
                Entry::Vacant(v) => {
                    v.insert(placement.room_id);
                }
                Entry::Occupied(o) => {
                    if *o.get() != placement.room_id {
                        return Err(Error::Input(format!(
                            "room hopping for class {:?} day {} subject {:?}: rooms {:?} and {:?}",
                            class_id,
                            tb.day_of_week,
                            lesson.subject_id,
                            o.get(),
                            placement.room_id
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Hard-constraint sanity check: per-day caps are never exceeded by the final
/// placements. `Subject.max_hours_per_day` counts hours (one per placement
/// row); `SchoolClass.max_lessons_per_day` counts blocks (a maximal run of
/// contiguous same-day same-lesson positions). A failure here indicates a
/// solver bug because cap-violating candidates are pruned at placement time
/// (ADR 0033). Cross-class lessons attribute every placement to every member
/// class so the same hour can trip the cap on more than one class.
pub fn validate_daily_caps(problem: &Problem, placements: &[Placement]) -> Result<(), Error> {
    use std::collections::HashMap;

    let mut rows_by_lesson_day: HashMap<(LessonId, u8), Vec<u8>> = HashMap::new();
    for placement in placements {
        let lesson = problem
            .lessons
            .iter()
            .find(|l| l.id == placement.lesson_id)
            .ok_or_else(|| Error::Input(format!("unknown lesson {:?}", placement.lesson_id)))?;
        let tb = problem
            .time_blocks
            .iter()
            .find(|t| t.id == placement.time_block_id)
            .ok_or_else(|| {
                Error::Input(format!("unknown time block {:?}", placement.time_block_id))
            })?;
        rows_by_lesson_day
            .entry((lesson.id, tb.day_of_week))
            .or_default()
            .push(tb.position);
    }

    let mut subject_hours: HashMap<(SchoolClassId, u8, SubjectId), u32> = HashMap::new();
    let mut class_blocks: HashMap<(SchoolClassId, u8), u32> = HashMap::new();
    for ((lesson_id, day), positions) in &rows_by_lesson_day {
        let lesson = problem
            .lessons
            .iter()
            .find(|l| l.id == *lesson_id)
            .ok_or_else(|| Error::Input(format!("unknown lesson {:?}", lesson_id)))?;
        let hours = u32::try_from(positions.len()).unwrap_or(u32::MAX);
        let mut sorted = positions.clone();
        sorted.sort_unstable();
        let mut blocks: u32 = 0;
        let mut prev: Option<u8> = None;
        for p in &sorted {
            if prev.is_none_or(|q| *p != q + 1) {
                blocks += 1;
            }
            prev = Some(*p);
        }
        for class_id in &lesson.school_class_ids {
            *subject_hours
                .entry((*class_id, *day, lesson.subject_id))
                .or_default() += hours;
            *class_blocks.entry((*class_id, *day)).or_default() += blocks;
        }
    }

    for ((class_id, day, subject_id), hours) in &subject_hours {
        let subject = problem
            .subjects
            .iter()
            .find(|s| s.id == *subject_id)
            .ok_or_else(|| Error::Input(format!("unknown subject {:?}", subject_id)))?;
        if *hours > u32::from(subject.max_hours_per_day) {
            return Err(Error::Input(format!(
                "subject {:?} exceeds max_hours_per_day on (class {:?}, day {}): {} > {}",
                subject_id, class_id, day, hours, subject.max_hours_per_day
            )));
        }
    }

    for ((class_id, day), blocks) in &class_blocks {
        let class = problem
            .school_classes
            .iter()
            .find(|c| c.id == *class_id)
            .ok_or_else(|| Error::Input(format!("unknown school_class {:?}", class_id)))?;
        if let Some(cap) = class.max_lessons_per_day {
            if *blocks > u32::from(cap) {
                return Err(Error::Input(format!(
                    "class {:?} exceeds max_lessons_per_day on day {}: {} > {}",
                    class_id, day, blocks, cap
                )));
            }
        }
    }

    Ok(())
}

/// Hard-constraint sanity check: the final placements vector contains no
/// class / teacher / room double-booking, every lesson appears at most
/// `hours_per_week` times, and every block is `preferred_block_size`
/// contiguous positions on one day in one room. A failure here indicates
/// a solver bug (a move applied without contains-checks) rather than
/// malformed input; production callers surface it as a runtime error.
/// Failure messages are prefixed with `double-booking:`,
/// `lesson cardinality:`, or `block shape:` so debug-mode panic messages
/// discriminate which check fired without parsing.
///
/// Under-placement (`rows.len() < hours_per_week`) is legal output:
/// `try_place_block` may fail and emit `Violation::NoFreeTimeBlock` /
/// `NoSuitableRoom` / `TeacherOverCapacity` per missing hour. The
/// validator catches over-placement (Kempe / R&R move bugs that insert
/// duplicate rows) and malformed blocks; under-placement is detected
/// upstream via the violations vec and the bake-off bench's per-cell
/// placement-count gate (item 28).
///
/// Lesson-group co-placement is exempted from the class double-booking
/// check: two lessons sharing a `(class, time_block)` pair are allowed
/// when both have the same `Some(lesson_group_id)`. The per-Jahrgang
/// religion trio (RK / RE / ETH) co-places all three lessons at one
/// `(day, position)` window in the same class set because students pick
/// exactly one of the three; `try_place_group` enforces this. Teacher
/// and room collisions remain hard violations regardless of group: a
/// teacher cannot teach two lessons at once, and `try_place_group`'s
/// `taken: HashSet<RoomId>` already forbids two members from sharing a
/// room.
pub fn validate_no_double_booking(
    problem: &Problem,
    placements: &[Placement],
) -> Result<(), Error> {
    use std::collections::hash_map::Entry;
    use std::collections::HashMap;

    let lesson_by_id: HashMap<LessonId, &crate::types::Lesson> =
        problem.lessons.iter().map(|l| (l.id, l)).collect();
    let tb_by_id: HashMap<TimeBlockId, &crate::types::TimeBlock> =
        problem.time_blocks.iter().map(|t| (t.id, t)).collect();

    let mut class_used: HashMap<(SchoolClassId, TimeBlockId), LessonId> = HashMap::new();
    let mut teacher_used: HashMap<(TeacherId, TimeBlockId), LessonId> = HashMap::new();
    let mut room_used: HashMap<(RoomId, TimeBlockId), LessonId> = HashMap::new();
    let mut rows_by_lesson: HashMap<LessonId, Vec<(u8, u8, RoomId)>> = HashMap::new();

    for p in placements {
        let lesson = lesson_by_id
            .get(&p.lesson_id)
            .ok_or_else(|| Error::Input(format!("unknown lesson {:?}", p.lesson_id)))?;
        let tb = tb_by_id
            .get(&p.time_block_id)
            .ok_or_else(|| Error::Input(format!("unknown time block {:?}", p.time_block_id)))?;

        for class_id in &lesson.school_class_ids {
            match class_used.entry((*class_id, p.time_block_id)) {
                Entry::Vacant(v) => {
                    v.insert(p.lesson_id);
                }
                Entry::Occupied(o) if *o.get() == p.lesson_id => {
                    // Same lesson, same row: caught by the cardinality check below.
                }
                Entry::Occupied(o) => {
                    let other = lesson_by_id[o.get()];
                    let same_group = matches!(
                        (lesson.lesson_group_id, other.lesson_group_id),
                        (Some(a), Some(b)) if a == b
                    );
                    if same_group {
                        // Lesson-group co-placement: members share `(class, tb)`
                        // by design (e.g. RK / RE / ETH religion trio; students
                        // pick one). `try_place_group` enforces the same
                        // (day, position) window for every member.
                        continue;
                    }
                    return Err(Error::Input(format!(
                        "double-booking: class {:?} at time-block {:?}: lessons {:?} and {:?}",
                        class_id,
                        p.time_block_id,
                        o.get(),
                        p.lesson_id
                    )));
                }
            }
        }
        let assigned_teacher = p.teacher_id;
        match teacher_used.entry((assigned_teacher, p.time_block_id)) {
            Entry::Vacant(v) => {
                v.insert(p.lesson_id);
            }
            Entry::Occupied(o) if *o.get() == p.lesson_id => {}
            Entry::Occupied(o) => {
                return Err(Error::Input(format!(
                    "double-booking: teacher {:?} at time-block {:?}: lessons {:?} and {:?}",
                    assigned_teacher,
                    p.time_block_id,
                    o.get(),
                    p.lesson_id
                )));
            }
        }
        match room_used.entry((p.room_id, p.time_block_id)) {
            Entry::Vacant(v) => {
                v.insert(p.lesson_id);
            }
            Entry::Occupied(o) if *o.get() == p.lesson_id => {}
            Entry::Occupied(o) => {
                return Err(Error::Input(format!(
                    "double-booking: room {:?} at time-block {:?}: lessons {:?} and {:?}",
                    p.room_id,
                    p.time_block_id,
                    o.get(),
                    p.lesson_id
                )));
            }
        }
        rows_by_lesson.entry(p.lesson_id).or_default().push((
            tb.day_of_week,
            tb.position,
            p.room_id,
        ));
    }

    for (lesson_id, mut rows) in rows_by_lesson {
        let lesson = lesson_by_id[&lesson_id];
        if rows.len() > lesson.hours_per_week as usize {
            return Err(Error::Input(format!(
                "lesson cardinality: lesson {:?} has {} placements, expected at most {}",
                lesson_id,
                rows.len(),
                lesson.hours_per_week
            )));
        }
        rows.sort_unstable_by_key(|(day, pos, _)| (*day, *pos));
        let n = lesson.preferred_block_size as usize;
        let mut day_groups: HashMap<u8, Vec<(u8, RoomId)>> = HashMap::new();
        for (day, pos, room) in rows {
            day_groups.entry(day).or_default().push((pos, room));
        }
        for (day, day_rows) in day_groups {
            if day_rows.len() % n != 0 {
                return Err(Error::Input(format!(
                    "block shape: lesson {:?} on day {} has {} rows, expected multiple of {}",
                    lesson_id,
                    day,
                    day_rows.len(),
                    n
                )));
            }
            for chunk in day_rows.chunks(n) {
                let first_pos = chunk[0].0;
                let first_room = chunk[0].1;
                for (i, (pos, _)) in chunk.iter().enumerate() {
                    if *pos != first_pos + i as u8 {
                        return Err(Error::Input(format!(
                            "block shape: lesson {:?} on day {} has positions {:?}, expected contiguous run of length {}",
                            lesson_id,
                            day,
                            chunk.iter().map(|(p, _)| *p).collect::<Vec<_>>(),
                            n
                        )));
                    }
                }
                for (_, room) in chunk.iter() {
                    if *room != first_room {
                        return Err(Error::Input(format!(
                            "block shape: lesson {:?} on day {} has rooms {:?}, expected one room per block",
                            lesson_id,
                            day,
                            chunk.iter().map(|(_, r)| *r).collect::<Vec<_>>()
                        )));
                    }
                }
            }
        }
    }

    Ok(())
}

/// Post-condition validator: every Placement.teacher_id is in the corresponding
/// Lesson's teacher_candidates, and matches teacher_pin when the pin is set.
///
/// A failure here indicates a solver bug, not malformed input. Pattern matches
/// `validate_no_double_booking` / `validate_no_room_hopping` /
/// `validate_daily_caps`: returns `Err(Error::Input)` so the caller can `?`-bail.
pub fn validate_placement_teacher_in_candidates(
    problem: &Problem,
    placements: &[Placement],
) -> Result<(), Error> {
    let lesson_by_id: HashMap<LessonId, &Lesson> =
        problem.lessons.iter().map(|l| (l.id, l)).collect();
    for placement in placements {
        let lesson = lesson_by_id.get(&placement.lesson_id).ok_or_else(|| {
            Error::Input(format!(
                "placement references unknown lesson {:?}",
                placement.lesson_id
            ))
        })?;
        if let Some(pin) = lesson.teacher_pin {
            if placement.teacher_id != pin {
                return Err(Error::Input(format!(
                    "placement teacher {:?} does not match lesson pin {:?}",
                    placement.teacher_id, pin
                )));
            }
        }
        if !lesson.teacher_candidates.contains(&placement.teacher_id) {
            return Err(Error::Input(format!(
                "placement teacher {:?} not in lesson candidates {:?}",
                placement.teacher_id, lesson.teacher_candidates
            )));
        }
    }
    Ok(())
}

/// Post-condition validator: every `(SchoolClass, Subject)` pair appearing in
/// the placement set has at most one distinct teacher.
///
/// Belt-and-braces guard for `ViolationKind::ClassSubjectTeacherSplit` (item
/// 66). The placement-time uniformity lock in `try_place_block` prevents
/// the search from reaching split-teacher states under normal operation;
/// this validator catches a future move that mutates teachers without
/// consulting the lock map. Multi-class lessons attribute each placement to
/// every member class, so the same lesson contributes one `(class, subject)`
/// pair per class in `lesson.school_class_ids`. A failure here indicates a
/// solver bug, not malformed input. Pattern matches `validate_no_room_hopping`
/// and the rest of the validator quartet: returns `Err(Error::Input)` so the
/// caller can `?`-bail.
pub fn validate_class_subject_teacher_uniformity(
    problem: &Problem,
    placements: &[Placement],
) -> Result<(), Error> {
    use std::collections::hash_map::Entry;
    let mut teacher_by_pair: HashMap<(SchoolClassId, SubjectId), TeacherId> = HashMap::new();
    let lesson_by_id: HashMap<LessonId, &Lesson> =
        problem.lessons.iter().map(|l| (l.id, l)).collect();
    for placement in placements {
        let lesson = lesson_by_id.get(&placement.lesson_id).ok_or_else(|| {
            Error::Input(format!(
                "placement references unknown lesson {:?}",
                placement.lesson_id
            ))
        })?;
        for class_id in &lesson.school_class_ids {
            let key = (*class_id, lesson.subject_id);
            match teacher_by_pair.entry(key) {
                Entry::Vacant(v) => {
                    v.insert(placement.teacher_id);
                }
                Entry::Occupied(o) => {
                    if *o.get() != placement.teacher_id {
                        return Err(Error::Input(format!(
                            "class-subject teacher split: class {:?} subject {:?} teachers {:?} and {:?}",
                            class_id,
                            lesson.subject_id,
                            o.get(),
                            placement.teacher_id
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Post-condition validator: every placement of a lesson with non-zero
/// `pre_buffer_minutes` or `post_buffer_minutes` has either a `Break`-kind
/// adjacent slot or no placement for the same class and the same teacher
/// at the adjacent slot. Lessons with `pre_buffer_minutes > 0` cannot be
/// placed at day-position 0; lessons with `post_buffer_minutes > 0`
/// cannot be placed when no slot follows the lesson's block on the same
/// day. Self-placements of a Doppelstunde (the second slot of a
/// `preferred_block_size > 1` block) are not foreign placements and are
/// skipped by lesson-id equality.
///
/// Returns `Err(Error::Input)` on the first violation found. The reason
/// string carries `TravelBufferConflict: lesson=<id>
/// [class=<id>|teacher=<id>] day=<d> position=<p>
/// [conflicting_lesson=<id>|reason=first_slot|reason=last_slot]
/// side=pre|post` so consumers can parse the diagnostic. ADR 0044.
pub fn validate_travel_buffer(problem: &Problem, placements: &[Placement]) -> Result<(), Error> {
    let lesson_by_id: HashMap<LessonId, &Lesson> =
        problem.lessons.iter().map(|l| (l.id, l)).collect();
    let tb_by_id: HashMap<TimeBlockId, &crate::types::TimeBlock> =
        problem.time_blocks.iter().map(|t| (t.id, t)).collect();

    // (day, position) -> &TimeBlock, used to look up the kind of the
    // slot adjacent to a buffered lesson.
    let mut tb_by_day_pos: HashMap<(u8, u8), &crate::types::TimeBlock> = HashMap::new();
    for tb in &problem.time_blocks {
        tb_by_day_pos.insert((tb.day_of_week, tb.position), tb);
    }

    // (class, day, position) -> lesson_id of the placement holding the
    // slot. (teacher, day, position) -> lesson_id, same. Multi-class
    // lessons populate one entry per member class.
    let mut class_occ: HashMap<(SchoolClassId, u8, u8), LessonId> = HashMap::new();
    let mut teacher_occ: HashMap<(TeacherId, u8, u8), LessonId> = HashMap::new();
    for p in placements {
        let tb = tb_by_id.get(&p.time_block_id).ok_or_else(|| {
            Error::Input(format!(
                "placement references unknown time block {:?}",
                p.time_block_id
            ))
        })?;
        let lesson = lesson_by_id.get(&p.lesson_id).ok_or_else(|| {
            Error::Input(format!(
                "placement references unknown lesson {:?}",
                p.lesson_id
            ))
        })?;
        for class_id in &lesson.school_class_ids {
            class_occ.insert((*class_id, tb.day_of_week, tb.position), lesson.id);
        }
        teacher_occ.insert((p.teacher_id, tb.day_of_week, tb.position), lesson.id);
    }

    // For multi-row blocks (Doppelstunden), only the FIRST row of the
    // block carries the pre/post adjacency check. Iterating every row and
    // computing `next_pos = pos + preferred_block_size` would, for the
    // second row of a Doppelstunde at (block_start, block_start+1), look at
    // `block_start + 1 + 2 = block_start + 3`, two slots past the block's
    // actual post-slot. Determine the per-(lesson, day) anchor up front so
    // the loop below evaluates each buffered placement at its correct
    // adjacent slot.
    let mut anchor_by_lesson_day: HashMap<(LessonId, u8), u8> = HashMap::new();
    for p in placements {
        let tb = tb_by_id[&p.time_block_id];
        let key = (p.lesson_id, tb.day_of_week);
        anchor_by_lesson_day
            .entry(key)
            .and_modify(|cur| {
                if tb.position < *cur {
                    *cur = tb.position;
                }
            })
            .or_insert(tb.position);
    }

    for p in placements {
        let lesson = lesson_by_id[&p.lesson_id];
        if lesson.pre_buffer_minutes == 0 && lesson.post_buffer_minutes == 0 {
            continue;
        }
        let tb = tb_by_id[&p.time_block_id];
        let day = tb.day_of_week;
        let pos = tb.position;
        // Only the block-anchor row drives the buffer check; non-anchor
        // rows of a Doppelstunde re-use the anchor's pre/post slot
        // computation and would otherwise report bogus self-violations.
        if let Some(anchor) = anchor_by_lesson_day.get(&(lesson.id, day)) {
            if pos != *anchor {
                continue;
            }
        }

        if lesson.pre_buffer_minutes > 0 {
            if pos == 0 {
                return Err(Error::Input(format!(
                    "TravelBufferConflict: lesson={} day={} position={} reason=first_slot side=pre",
                    lesson.id.0, day, pos
                )));
            }
            let prev_pos = pos - 1;
            let prev_is_break = tb_by_day_pos
                .get(&(day, prev_pos))
                .is_some_and(|t| t.kind == TimeBlockKind::Break);
            if !prev_is_break {
                for class_id in &lesson.school_class_ids {
                    if let Some(&conflict) = class_occ.get(&(*class_id, day, prev_pos)) {
                        if conflict != lesson.id {
                            return Err(Error::Input(format!(
                                "TravelBufferConflict: lesson={} class={} day={} position={} conflicting_lesson={} side=pre",
                                lesson.id.0, class_id.0, day, pos, conflict.0
                            )));
                        }
                    }
                }
                if let Some(&conflict) = teacher_occ.get(&(p.teacher_id, day, prev_pos)) {
                    if conflict != lesson.id {
                        return Err(Error::Input(format!(
                            "TravelBufferConflict: lesson={} teacher={} day={} position={} conflicting_lesson={} side=pre",
                            lesson.id.0, p.teacher_id.0, day, pos, conflict.0
                        )));
                    }
                }
            }
        }

        if lesson.post_buffer_minutes > 0 {
            // The Doppelstunde occupies positions `pos .. pos+block_size`.
            // The first slot AFTER the block is at `pos + block_size`.
            let next_pos = pos.saturating_add(lesson.preferred_block_size);
            let next_tb = tb_by_day_pos.get(&(day, next_pos)).copied();
            if next_tb.is_none() {
                return Err(Error::Input(format!(
                    "TravelBufferConflict: lesson={} day={} position={} reason=last_slot side=post",
                    lesson.id.0, day, pos
                )));
            }
            let next_is_break = next_tb.is_some_and(|t| t.kind == TimeBlockKind::Break);
            if !next_is_break {
                for class_id in &lesson.school_class_ids {
                    if let Some(&conflict) = class_occ.get(&(*class_id, day, next_pos)) {
                        if conflict != lesson.id {
                            return Err(Error::Input(format!(
                                "TravelBufferConflict: lesson={} class={} day={} position={} conflicting_lesson={} side=post",
                                lesson.id.0, class_id.0, day, pos, conflict.0
                            )));
                        }
                    }
                }
                if let Some(&conflict) = teacher_occ.get(&(p.teacher_id, day, next_pos)) {
                    if conflict != lesson.id {
                        return Err(Error::Input(format!(
                            "TravelBufferConflict: lesson={} teacher={} day={} position={} conflicting_lesson={} side=post",
                            lesson.id.0, p.teacher_id.0, day, pos, conflict.0
                        )));
                    }
                }
            }
        }
    }

    Ok(())
}

/// Predicate sibling of [`validate_travel_buffer`]: would placing `lesson`
/// at `tb_id` violate the pre or post travel-buffer constraint for either
/// the lesson's class or its currently-assigned teacher (read via
/// [`crate::solve::GreedyState`])? Used by FFD and LAHC move sites as a
/// legality filter. The validator is the canary; this helper is the
/// hot-path predicate.
///
/// `teacher` is the teacher to score the placement against (caller threads
/// `lesson_teacher_in_state` for LAHC, or `lesson.assigned_teacher_id()` for
/// FFD, depending on the lock-map state).
///
/// `ignore_self` overlays a "the lesson's own placement at this `(day,
/// position..position+block_size)` window is being removed by the move and
/// MUST NOT count as a conflict". FFD callers pass `None` because the
/// lesson is not yet in `state`; LAHC Change/Swap/Block-move callers pass
/// `Some((old_day, old_start_pos))` so the adjacent-slot check does not
/// fire against the lesson's own pre-move position.
///
/// `placements` is the current placement vector (or an empty slice when the
/// caller is FFD-with-no-prior-placements). The helper uses it to detect the
/// symmetric case: even when the candidate `lesson` is itself unbuffered,
/// the chosen `tb_id` may be adjacent to an existing buffered placement
/// whose pre/post side faces this slot. Without this check, FFD/LAHC moves
/// of non-buffered lessons can land next to a buffered placement and the
/// `validate_travel_buffer` post-condition rejects the entire run.
pub(crate) fn would_violate_travel_buffer(
    problem: &Problem,
    state: &crate::solve::GreedyState,
    placements: &[Placement],
    lesson: &Lesson,
    tb_id: TimeBlockId,
    teacher: TeacherId,
    ignore_self: Option<(u8, u8)>,
) -> bool {
    let Some(tb) = problem.time_blocks.iter().find(|t| t.id == tb_id) else {
        return false;
    };
    let day = tb.day_of_week;
    let pos = tb.position;

    // Symmetric pruning: even when `lesson` itself is unbuffered, the
    // candidate slot range may collide with an existing buffered
    // placement's pre/post-side. We must reject the move if any buffered
    // placement on the same day, sharing a class or teacher with the
    // current candidate, has its buffer side pointing at one of the
    // candidate's positions. Cheap by construction: only buffered lessons
    // are scanned, and there is typically just one per fixture. The
    // current lesson's own pre-move placement (via `ignore_self`) is
    // skipped so a same-lesson re-anchoring does not self-collide.
    let candidate_n = lesson.preferred_block_size;
    for p in placements {
        let Some(p_lesson) = problem.lessons.iter().find(|l| l.id == p.lesson_id) else {
            continue;
        };
        if p_lesson.id == lesson.id {
            continue;
        }
        if p_lesson.pre_buffer_minutes == 0 && p_lesson.post_buffer_minutes == 0 {
            continue;
        }
        let Some(p_tb) = problem.time_blocks.iter().find(|t| t.id == p.time_block_id) else {
            continue;
        };
        if p_tb.day_of_week != day {
            continue;
        }
        // Treat `ignore_self`'s window as "the candidate's pre-move
        // footprint"; placements that the move is REMOVING from those
        // slots must not count for the symmetric check either. The
        // helper's `placements` slice still contains the old placement
        // (LAHC mutates in place after the legality filter), so without
        // this guard a Change move that pulls the buffered lesson away
        // would self-collide on its own old block.
        if let Some((ignore_day, ignore_start)) = ignore_self {
            if p_tb.day_of_week == ignore_day {
                let ignore_n = lesson.preferred_block_size;
                if p_tb.position >= ignore_start
                    && p_tb.position < ignore_start.saturating_add(ignore_n)
                {
                    continue;
                }
            }
        }
        let p_pos = p_tb.position;
        let p_n = p_lesson.preferred_block_size;
        let shares_class = lesson
            .school_class_ids
            .iter()
            .any(|c| p_lesson.school_class_ids.contains(c));
        let shares_teacher = teacher == p.teacher_id;
        if !shares_class && !shares_teacher {
            continue;
        }
        // The candidate occupies positions [pos .. pos + candidate_n).
        // `p`'s pre side is the slot at `p_pos - 1`; violated iff the
        // candidate's range covers it (`pos <= p_pos - 1 < pos +
        // candidate_n`). `p`'s post side is the slot at `p_pos + p_n`;
        // violated iff the candidate's range covers it
        // (`pos <= p_pos + p_n < pos + candidate_n`).
        if p_lesson.pre_buffer_minutes > 0 && p_pos > 0 {
            let pre_slot = p_pos - 1;
            let pre_tb = problem
                .time_blocks
                .iter()
                .find(|t| t.day_of_week == day && t.position == pre_slot);
            let pre_is_break = pre_tb.is_some_and(|t| t.kind == TimeBlockKind::Break);
            if !pre_is_break && pos <= pre_slot && pre_slot < pos + candidate_n {
                return true;
            }
        }
        if p_lesson.post_buffer_minutes > 0 {
            let post_slot = p_pos.saturating_add(p_n);
            let post_tb = problem
                .time_blocks
                .iter()
                .find(|t| t.day_of_week == day && t.position == post_slot);
            let post_is_break = post_tb.is_some_and(|t| t.kind == TimeBlockKind::Break);
            if !post_is_break && pos <= post_slot && post_slot < pos + candidate_n {
                return true;
            }
        }
    }

    if lesson.pre_buffer_minutes == 0 && lesson.post_buffer_minutes == 0 {
        return false;
    }

    // The lesson's own pre-move block spans `[ignore_start, ignore_end]` on
    // `ignore_day`. A class/teacher position in this range is the lesson
    // being moved, not a foreign placement.
    let is_self = |check_day: u8, check_pos: u8| -> bool {
        let Some((ignore_day, ignore_start)) = ignore_self else {
            return false;
        };
        if check_day != ignore_day {
            return false;
        }
        let n = lesson.preferred_block_size;
        check_pos >= ignore_start && check_pos < ignore_start.saturating_add(n)
    };

    if lesson.pre_buffer_minutes > 0 {
        if pos == 0 {
            return true;
        }
        let prev_pos = pos - 1;
        let prev_tb = problem
            .time_blocks
            .iter()
            .find(|t| t.day_of_week == day && t.position == prev_pos);
        let prev_is_break = prev_tb.is_some_and(|t| t.kind == TimeBlockKind::Break);
        if !prev_is_break && !is_self(day, prev_pos) {
            for class_id in &lesson.school_class_ids {
                if let Some(positions) = state.class_positions.get(&(*class_id, day)) {
                    if positions.contains(&prev_pos) {
                        return true;
                    }
                }
            }
            if let Some(positions) = state.teacher_positions.get(&(teacher, day)) {
                if positions.contains(&prev_pos) {
                    return true;
                }
            }
        }
    }

    if lesson.post_buffer_minutes > 0 {
        let next_pos = pos.saturating_add(lesson.preferred_block_size);
        let next_tb = problem
            .time_blocks
            .iter()
            .find(|t| t.day_of_week == day && t.position == next_pos);
        if next_tb.is_none() {
            return true;
        }
        let next_is_break = next_tb.is_some_and(|t| t.kind == TimeBlockKind::Break);
        if !next_is_break && !is_self(day, next_pos) {
            for class_id in &lesson.school_class_ids {
                if let Some(positions) = state.class_positions.get(&(*class_id, day)) {
                    if positions.contains(&next_pos) {
                        return true;
                    }
                }
            }
            if let Some(positions) = state.teacher_positions.get(&(teacher, day)) {
                if positions.contains(&next_pos) {
                    return true;
                }
            }
        }
    }

    false
}

/// Post-condition validator: no placement targets a break-kind
/// [`crate::types::TimeBlock`]. Lessons must only land on `TimeBlockKind::Lesson`
/// slots; Hofpause supervision is handled separately by
/// [`crate::supervision::compute_supervision_full`]. A failure here indicates a
/// solver bug (FFD / LAHC enumeration site missed the kind filter) rather than
/// malformed input; the canonical fix is to filter the enumeration site, not
/// to relax the validator. Pattern matches the rest of the validator quintet:
/// returns `Err(Error::Input)` so the caller can `?`-bail.
pub fn validate_no_lesson_on_break_slot(
    problem: &Problem,
    placements: &[Placement],
) -> Result<(), Error> {
    let tb_by_id: HashMap<TimeBlockId, &crate::types::TimeBlock> =
        problem.time_blocks.iter().map(|t| (t.id, t)).collect();
    for placement in placements {
        let tb = tb_by_id.get(&placement.time_block_id).ok_or_else(|| {
            Error::Input(format!(
                "placement references unknown time block {:?}",
                placement.time_block_id
            ))
        })?;
        if tb.kind != TimeBlockKind::Lesson {
            return Err(Error::Input(format!(
                "lesson placed on break-kind time block: lesson {:?} on {:?} (day={} position={})",
                placement.lesson_id, tb.id, tb.day_of_week, tb.position,
            )));
        }
    }
    Ok(())
}

/// Scan lessons for teacher / subject pairs that are not in
/// `teacher_qualifications` and record one `NoQualifiedTeacher` violation per
/// hour on the affected lesson.
pub fn pre_solve_violations(problem: &Problem) -> Vec<Violation> {
    let mut qualified: HashSet<(TeacherId, SubjectId)> = HashSet::new();
    for q in &problem.teacher_qualifications {
        qualified.insert((q.teacher_id, q.subject_id));
    }

    let mut out = Vec::new();
    for lesson in &problem.lessons {
        if qualified.contains(&(lesson.assigned_teacher_id(), lesson.subject_id)) {
            continue;
        }
        let n = lesson.preferred_block_size;
        let block_count = lesson.hours_per_week / n;
        for block_index in 0..block_count {
            out.push(Violation {
                kind: ViolationKind::NoQualifiedTeacher,
                lesson_id: lesson.id,
                hour_index: block_index * n,
                reason: None,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        Lesson, Problem, Room, RoomSubjectSuitability, SchoolClass, Subject, Teacher,
        TeacherQualification, TimeBlock, TimeBlockKind,
    };
    use uuid::Uuid;

    fn uuid(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    fn minimal_problem() -> Problem {
        let tb = TimeBlock {
            id: TimeBlockId(uuid(1)),
            day_of_week: 0,
            position: 0,
            kind: TimeBlockKind::Lesson,
        };
        let teacher = Teacher {
            id: TeacherId(uuid(2)),
            max_hours_per_week: 10,
            reserve_hours_per_week: 0,
        };
        let room = Room {
            id: RoomId(uuid(3)),
        };
        let subject = Subject {
            id: SubjectId(uuid(4)),
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        };
        let class = SchoolClass {
            id: SchoolClassId(uuid(5)),
            home_room_id: None,
            max_lessons_per_day: None,
            class_teacher_id: None,
        };
        let lesson = Lesson {
            id: LessonId(uuid(6)),
            school_class_ids: vec![class.id],
            subject_id: subject.id,
            teacher_candidates: vec![teacher.id],
            teacher_pin: Some(teacher.id),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        };
        Problem {
            time_blocks: vec![tb],
            teachers: vec![teacher],
            rooms: vec![room],
            subjects: vec![subject],
            school_classes: vec![class],
            lessons: vec![lesson],
            teacher_qualifications: vec![TeacherQualification {
                teacher_id: TeacherId(uuid(2)),
                subject_id: SubjectId(uuid(4)),
            }],
            teacher_blocked_times: vec![],
            room_blocked_times: vec![],
            room_subject_suitabilities: vec![],
            pinned_placements: vec![],
            pre_first_slot_grace_minutes: 0,
        }
    }

    #[test]
    fn minimal_problem_is_structurally_valid() {
        validate_structural(&minimal_problem()).unwrap();
    }

    #[test]
    fn empty_time_blocks_is_input_error() {
        let mut p = minimal_problem();
        p.time_blocks.clear();
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("time_blocks")));
    }

    #[test]
    fn empty_rooms_is_input_error() {
        let mut p = minimal_problem();
        p.rooms.clear();
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("rooms")));
    }

    #[test]
    fn duplicate_teacher_id_is_input_error() {
        let mut p = minimal_problem();
        p.teachers.push(p.teachers[0].clone());
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("duplicate id")));
    }

    #[test]
    fn lesson_with_zero_hours_is_input_error() {
        let mut p = minimal_problem();
        p.lessons[0].hours_per_week = 0;
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("hours_per_week")));
    }

    #[test]
    fn lesson_with_zero_block_size_is_input_error() {
        let mut p = minimal_problem();
        p.lessons[0].preferred_block_size = 0;
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("preferred_block_size")));
    }

    #[test]
    fn lesson_with_non_divisible_hours_is_input_error() {
        let mut p = minimal_problem();
        p.lessons[0].hours_per_week = 3;
        p.lessons[0].preferred_block_size = 2;
        let err = validate_structural(&p).unwrap_err();
        assert!(
            matches!(err, Error::Input(msg) if msg.contains("divisible by preferred_block_size"))
        );
    }

    #[test]
    fn block_size_one_with_any_hours_is_valid() {
        let mut p = minimal_problem();
        p.lessons[0].hours_per_week = 7;
        p.lessons[0].preferred_block_size = 1;
        validate_structural(&p).unwrap();
    }

    #[test]
    fn block_size_two_with_even_hours_is_valid() {
        let mut p = minimal_problem();
        p.lessons[0].hours_per_week = 4;
        p.lessons[0].preferred_block_size = 2;
        validate_structural(&p).unwrap();
    }

    #[test]
    fn unknown_teacher_ref_is_input_error() {
        let mut p = minimal_problem();
        let bogus = TeacherId(uuid(99));
        p.lessons[0].teacher_candidates = vec![bogus];
        p.lessons[0].teacher_pin = Some(bogus);
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("unknown teacher")));
    }

    #[test]
    fn validate_structural_rejects_empty_school_class_ids() {
        let mut p = minimal_problem();
        p.lessons[0].school_class_ids.clear();
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("empty school_class_ids")));
    }

    #[test]
    fn validate_structural_rejects_duplicate_school_class_ids() {
        let mut p = minimal_problem();
        let class_id = p.lessons[0].school_class_ids[0];
        p.lessons[0].school_class_ids.push(class_id);
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("duplicate school_class")));
    }

    #[test]
    fn validate_structural_rejects_unknown_school_class_id_in_set() {
        let mut p = minimal_problem();
        p.lessons[0].school_class_ids.push(SchoolClassId(uuid(99)));
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("unknown school_class")));
    }

    #[test]
    fn unknown_room_suitability_ref_is_input_error() {
        let mut p = minimal_problem();
        p.room_subject_suitabilities.push(RoomSubjectSuitability {
            room_id: RoomId(uuid(99)),
            subject_id: SubjectId(uuid(4)),
        });
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("unknown room")));
    }

    #[test]
    fn pre_solve_emits_no_violations_when_all_teachers_qualified() {
        let violations = pre_solve_violations(&minimal_problem());
        assert!(violations.is_empty());
    }

    #[test]
    fn pre_solve_emits_violations_per_hour_for_unqualified_teacher() {
        let mut p = minimal_problem();
        p.teacher_qualifications.clear();
        p.lessons[0].hours_per_week = 3;
        let violations = pre_solve_violations(&p);
        assert_eq!(violations.len(), 3);
        assert!(violations
            .iter()
            .all(|v| v.kind == ViolationKind::NoQualifiedTeacher));
        assert_eq!(
            violations.iter().map(|v| v.hour_index).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }

    fn two_member_group_problem() -> Problem {
        use crate::ids::LessonGroupId;
        let group_id = LessonGroupId(uuid(99));
        let mut p = minimal_problem();
        p.school_classes.push(SchoolClass {
            id: SchoolClassId(uuid(7)),
            home_room_id: None,
            max_lessons_per_day: None,
            class_teacher_id: None,
        });
        p.subjects.push(Subject {
            id: SubjectId(uuid(8)),
            prefer_early_period: 0,
            avoid_first_period: 0,
            avoid_last_period: 0,
            prefer_late_period: 0,
            max_hours_per_day: 8,
        });
        p.teachers.push(Teacher {
            id: TeacherId(uuid(9)),
            max_hours_per_week: 10,
            reserve_hours_per_week: 0,
        });
        p.teacher_qualifications.push(TeacherQualification {
            teacher_id: TeacherId(uuid(9)),
            subject_id: SubjectId(uuid(8)),
        });
        p.lessons[0].lesson_group_id = Some(group_id);
        p.lessons.push(Lesson {
            id: LessonId(uuid(10)),
            school_class_ids: vec![SchoolClassId(uuid(7))],
            subject_id: SubjectId(uuid(8)),
            teacher_candidates: vec![TeacherId(uuid(9))],
            teacher_pin: Some(TeacherId(uuid(9))),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: Some(group_id),
        });
        p
    }

    #[test]
    fn validate_structural_accepts_group_with_consistent_invariants() {
        validate_structural(&two_member_group_problem()).unwrap();
    }

    #[test]
    fn validate_structural_accepts_single_member_group() {
        use crate::ids::LessonGroupId;
        let mut p = minimal_problem();
        p.lessons[0].lesson_group_id = Some(LessonGroupId(uuid(99)));
        validate_structural(&p).unwrap();
    }

    #[test]
    fn validate_structural_rejects_group_members_with_different_hours_per_week() {
        let mut p = two_member_group_problem();
        p.lessons[1].hours_per_week = 2;
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("hours_per_week")));
    }

    #[test]
    fn validate_structural_rejects_group_members_with_different_block_size() {
        let mut p = two_member_group_problem();
        p.lessons[0].hours_per_week = 2;
        p.lessons[0].preferred_block_size = 2;
        p.lessons[1].hours_per_week = 2;
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("preferred_block_size")));
    }

    #[test]
    fn validate_structural_rejects_group_with_duplicate_teacher() {
        let mut p = two_member_group_problem();
        let teacher0 = p.lessons[0].assigned_teacher_id();
        p.lessons[1].teacher_candidates = vec![teacher0];
        p.lessons[1].teacher_pin = Some(teacher0);
        let err = validate_structural(&p).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("duplicate teacher")));
    }

    fn caps_problem_two_periods_one_day() -> Problem {
        let mut p = minimal_problem();
        p.time_blocks.push(TimeBlock {
            id: TimeBlockId(uuid(11)),
            day_of_week: 0,
            position: 1,
            kind: TimeBlockKind::Lesson,
        });
        p.lessons[0].hours_per_week = 2;
        p
    }

    #[test]
    fn validate_daily_caps_accepts_within_caps() {
        let p = caps_problem_two_periods_one_day();
        let placements = vec![
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[1].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
        ];
        validate_daily_caps(&p, &placements).unwrap();
    }

    #[test]
    fn validate_daily_caps_rejects_subject_hours_per_day_exceeded() {
        let mut p = caps_problem_two_periods_one_day();
        p.subjects[0].max_hours_per_day = 1;
        let placements = vec![
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[1].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
        ];
        let err = validate_daily_caps(&p, &placements).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("max_hours_per_day")));
    }

    #[test]
    fn validate_daily_caps_rejects_class_lessons_per_day_exceeded() {
        let mut p = caps_problem_two_periods_one_day();
        p.school_classes[0].max_lessons_per_day = Some(1);
        p.time_blocks.push(TimeBlock {
            id: TimeBlockId(uuid(12)),
            day_of_week: 0,
            position: 3,
            kind: TimeBlockKind::Lesson,
        });
        let placements = vec![
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[2].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
        ];
        let err = validate_daily_caps(&p, &placements).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("max_lessons_per_day")));
    }

    #[test]
    fn validate_daily_caps_counts_two_period_block_as_one_lesson() {
        let mut p = caps_problem_two_periods_one_day();
        p.school_classes[0].max_lessons_per_day = Some(1);
        let placements = vec![
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[1].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
        ];
        validate_daily_caps(&p, &placements).unwrap();
    }

    #[test]
    fn validate_no_double_booking_accepts_well_formed_schedule() {
        let mut p = minimal_problem();
        p.time_blocks.push(TimeBlock {
            id: TimeBlockId(uuid(11)),
            day_of_week: 0,
            position: 1,
            kind: TimeBlockKind::Lesson,
        });
        p.lessons[0].hours_per_week = 2;
        p.lessons[0].preferred_block_size = 2;
        let placements = vec![
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[1].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
        ];
        validate_no_double_booking(&p, &placements).unwrap();
    }

    #[test]
    fn validate_no_double_booking_rejects_class_double_booking() {
        let mut p = minimal_problem();
        let class_id = p.school_classes[0].id;
        p.lessons.push(Lesson {
            id: LessonId(uuid(20)),
            school_class_ids: vec![class_id],
            subject_id: p.subjects[0].id,
            teacher_candidates: vec![p.teachers[0].id],
            teacher_pin: Some(p.teachers[0].id),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        });
        p.lessons[0].hours_per_week = 1;
        let placements = vec![
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: p.lessons[1].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
        ];
        let err = validate_no_double_booking(&p, &placements).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("double-booking: class")));
    }

    #[test]
    fn validate_no_double_booking_rejects_teacher_double_booking() {
        let mut p = minimal_problem();
        let class2 = SchoolClass {
            id: SchoolClassId(uuid(30)),
            home_room_id: None,
            max_lessons_per_day: None,
            class_teacher_id: None,
        };
        p.school_classes.push(class2.clone());
        p.lessons.push(Lesson {
            id: LessonId(uuid(31)),
            school_class_ids: vec![class2.id],
            subject_id: p.subjects[0].id,
            teacher_candidates: vec![p.teachers[0].id],
            teacher_pin: Some(p.teachers[0].id),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        });
        p.lessons[0].hours_per_week = 1;
        let placements = vec![
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: p.lessons[1].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
        ];
        let err = validate_no_double_booking(&p, &placements).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("double-booking: teacher")));
    }

    #[test]
    fn validate_no_double_booking_rejects_room_double_booking() {
        let mut p = minimal_problem();
        let class2 = SchoolClass {
            id: SchoolClassId(uuid(40)),
            home_room_id: None,
            max_lessons_per_day: None,
            class_teacher_id: None,
        };
        p.school_classes.push(class2.clone());
        p.teachers.push(Teacher {
            id: TeacherId(uuid(41)),
            max_hours_per_week: 10,
            reserve_hours_per_week: 0,
        });
        p.teacher_qualifications.push(TeacherQualification {
            teacher_id: TeacherId(uuid(41)),
            subject_id: p.subjects[0].id,
        });
        p.lessons.push(Lesson {
            id: LessonId(uuid(42)),
            school_class_ids: vec![class2.id],
            subject_id: p.subjects[0].id,
            teacher_candidates: vec![TeacherId(uuid(41))],
            teacher_pin: Some(TeacherId(uuid(41))),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        });
        p.lessons[0].hours_per_week = 1;
        let placements = vec![
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(uuid(2)),
            },
            Placement {
                lesson_id: p.lessons[1].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(uuid(41)),
            },
        ];
        let err = validate_no_double_booking(&p, &placements).unwrap_err();
        assert!(matches!(err, Error::Input(msg) if msg.contains("double-booking: room")));
    }

    #[test]
    fn validate_no_double_booking_rejects_class_double_booking_via_cross_class_lesson() {
        let mut p = minimal_problem();
        let class1 = p.school_classes[0].id;
        let class2 = SchoolClass {
            id: SchoolClassId(uuid(50)),
            home_room_id: None,
            max_lessons_per_day: None,
            class_teacher_id: None,
        };
        p.school_classes.push(class2.clone());
        p.lessons.push(Lesson {
            id: LessonId(uuid(51)),
            school_class_ids: vec![class1, class2.id],
            subject_id: p.subjects[0].id,
            teacher_candidates: vec![p.teachers[0].id],
            teacher_pin: Some(p.teachers[0].id),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        });
        p.lessons[0].hours_per_week = 1;
        let placements = vec![
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: p.lessons[1].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
        ];
        let err = validate_no_double_booking(&p, &placements).unwrap_err();
        let Error::Input(msg) = err;
        assert!(msg.contains("double-booking: class"), "msg: {msg}");
        assert!(msg.contains(&format!("{:?}", class1)), "msg: {msg}");
    }

    #[test]
    fn validate_no_double_booking_accepts_under_placed_lesson() {
        let mut p = minimal_problem();
        p.lessons[0].hours_per_week = 2;
        p.time_blocks.push(TimeBlock {
            id: TimeBlockId(uuid(60)),
            day_of_week: 0,
            position: 1,
            kind: TimeBlockKind::Lesson,
        });
        let placements = vec![Placement {
            lesson_id: p.lessons[0].id,
            time_block_id: p.time_blocks[0].id,
            room_id: p.rooms[0].id,
            teacher_id: TeacherId(Uuid::nil()),
        }];
        validate_no_double_booking(&p, &placements).unwrap();
    }

    #[test]
    fn validate_no_double_booking_rejects_lesson_cardinality_too_many() {
        let mut p = minimal_problem();
        p.lessons[0].hours_per_week = 2;
        p.time_blocks.push(TimeBlock {
            id: TimeBlockId(uuid(70)),
            day_of_week: 0,
            position: 1,
            kind: TimeBlockKind::Lesson,
        });
        p.time_blocks.push(TimeBlock {
            id: TimeBlockId(uuid(71)),
            day_of_week: 1,
            position: 0,
            kind: TimeBlockKind::Lesson,
        });
        let placements = vec![
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[1].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[2].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
        ];
        let err = validate_no_double_booking(&p, &placements).unwrap_err();
        let Error::Input(msg) = err;
        assert!(msg.contains("lesson cardinality"), "msg: {msg}");
        assert!(msg.contains("expected at most 2"), "msg: {msg}");
    }

    #[test]
    fn validate_no_double_booking_rejects_block_shape_non_contiguous() {
        let mut p = minimal_problem();
        p.lessons[0].hours_per_week = 2;
        p.lessons[0].preferred_block_size = 2;
        p.time_blocks.push(TimeBlock {
            id: TimeBlockId(uuid(80)),
            day_of_week: 0,
            position: 2,
            kind: TimeBlockKind::Lesson,
        });
        let placements = vec![
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[1].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
        ];
        let err = validate_no_double_booking(&p, &placements).unwrap_err();
        let Error::Input(msg) = err;
        assert!(msg.contains("block shape"), "msg: {msg}");
        assert!(msg.contains("contiguous run of length 2"), "msg: {msg}");
    }

    #[test]
    fn validate_no_double_booking_rejects_block_shape_split_across_rooms() {
        let mut p = minimal_problem();
        p.lessons[0].hours_per_week = 2;
        p.lessons[0].preferred_block_size = 2;
        p.time_blocks.push(TimeBlock {
            id: TimeBlockId(uuid(90)),
            day_of_week: 0,
            position: 1,
            kind: TimeBlockKind::Lesson,
        });
        p.rooms.push(Room {
            id: RoomId(uuid(91)),
        });
        let placements = vec![
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[1].id,
                room_id: p.rooms[1].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
        ];
        let err = validate_no_double_booking(&p, &placements).unwrap_err();
        let Error::Input(msg) = err;
        assert!(msg.contains("block shape"), "msg: {msg}");
        assert!(msg.contains("one room per block"), "msg: {msg}");
    }

    #[test]
    fn validate_no_double_booking_rejects_block_shape_orphan_row() {
        let mut p = minimal_problem();
        p.lessons[0].hours_per_week = 2;
        p.lessons[0].preferred_block_size = 2;
        p.time_blocks.push(TimeBlock {
            id: TimeBlockId(uuid(100)),
            day_of_week: 1,
            position: 0,
            kind: TimeBlockKind::Lesson,
        });
        let placements = vec![
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[1].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
        ];
        let err = validate_no_double_booking(&p, &placements).unwrap_err();
        let Error::Input(msg) = err;
        assert!(msg.contains("block shape"), "msg: {msg}");
        assert!(msg.contains("multiple of 2"), "msg: {msg}");
    }

    #[test]
    fn validate_no_double_booking_accepts_lesson_group_class_share() {
        use crate::ids::LessonGroupId;
        let mut p = minimal_problem();
        let group_id = LessonGroupId(uuid(110));
        let class_id = p.school_classes[0].id;
        p.lessons[0].lesson_group_id = Some(group_id);
        p.lessons[0].hours_per_week = 1;
        p.teachers.push(Teacher {
            id: TeacherId(uuid(111)),
            max_hours_per_week: 10,
            reserve_hours_per_week: 0,
        });
        p.teacher_qualifications.push(TeacherQualification {
            teacher_id: TeacherId(uuid(111)),
            subject_id: p.subjects[0].id,
        });
        p.rooms.push(Room {
            id: RoomId(uuid(112)),
        });
        p.lessons.push(Lesson {
            id: LessonId(uuid(113)),
            school_class_ids: vec![class_id],
            subject_id: p.subjects[0].id,
            teacher_candidates: vec![TeacherId(uuid(111))],
            teacher_pin: Some(TeacherId(uuid(111))),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: Some(group_id),
        });
        let placements = vec![
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(uuid(2)),
            },
            Placement {
                lesson_id: p.lessons[1].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[1].id,
                teacher_id: TeacherId(uuid(111)),
            },
        ];
        validate_no_double_booking(&p, &placements).unwrap();
    }

    #[test]
    fn validate_no_double_booking_rejects_class_share_when_group_ids_differ() {
        use crate::ids::LessonGroupId;
        let mut p = minimal_problem();
        let class_id = p.school_classes[0].id;
        p.lessons[0].lesson_group_id = Some(LessonGroupId(uuid(120)));
        p.lessons[0].hours_per_week = 1;
        p.teachers.push(Teacher {
            id: TeacherId(uuid(121)),
            max_hours_per_week: 10,
            reserve_hours_per_week: 0,
        });
        p.teacher_qualifications.push(TeacherQualification {
            teacher_id: TeacherId(uuid(121)),
            subject_id: p.subjects[0].id,
        });
        p.rooms.push(Room {
            id: RoomId(uuid(122)),
        });
        p.lessons.push(Lesson {
            id: LessonId(uuid(123)),
            school_class_ids: vec![class_id],
            subject_id: p.subjects[0].id,
            teacher_candidates: vec![TeacherId(uuid(121))],
            teacher_pin: Some(TeacherId(uuid(121))),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: Some(LessonGroupId(uuid(124))),
        });
        let placements = vec![
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: p.lessons[1].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[1].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
        ];
        let err = validate_no_double_booking(&p, &placements).unwrap_err();
        let Error::Input(msg) = err;
        assert!(msg.contains("double-booking: class"), "msg: {msg}");
    }

    #[test]
    fn validate_no_double_booking_rejects_lesson_group_teacher_share() {
        use crate::ids::LessonGroupId;
        let mut p = minimal_problem();
        let group_id = LessonGroupId(uuid(130));
        let class_id = p.school_classes[0].id;
        p.lessons[0].lesson_group_id = Some(group_id);
        p.lessons[0].hours_per_week = 1;
        p.rooms.push(Room {
            id: RoomId(uuid(132)),
        });
        p.lessons.push(Lesson {
            id: LessonId(uuid(133)),
            school_class_ids: vec![class_id],
            subject_id: p.subjects[0].id,
            teacher_candidates: vec![p.teachers[0].id],
            teacher_pin: Some(p.teachers[0].id),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: Some(group_id),
        });
        let placements = vec![
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
            Placement {
                lesson_id: p.lessons[1].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[1].id,
                teacher_id: TeacherId(Uuid::nil()),
            },
        ];
        let err = validate_no_double_booking(&p, &placements).unwrap_err();
        let Error::Input(msg) = err;
        assert!(msg.contains("double-booking: teacher"), "msg: {msg}");
    }

    fn class_subject_uniformity_two_lesson_problem() -> (Problem, TeacherId, TeacherId) {
        let mut p = minimal_problem();
        let teacher1 = p.teachers[0].id;
        let teacher2 = TeacherId(uuid(200));
        p.teachers.push(Teacher {
            id: teacher2,
            max_hours_per_week: 10,
            reserve_hours_per_week: 0,
        });
        p.teacher_qualifications.push(TeacherQualification {
            teacher_id: teacher2,
            subject_id: p.subjects[0].id,
        });
        let class_id = p.school_classes[0].id;
        p.lessons.push(Lesson {
            id: LessonId(uuid(201)),
            school_class_ids: vec![class_id],
            subject_id: p.subjects[0].id,
            teacher_candidates: vec![teacher1, teacher2],
            teacher_pin: None,
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        });
        (p, teacher1, teacher2)
    }

    #[test]
    fn validate_class_subject_teacher_uniformity_rejects_split_teacher_pair() {
        let (p, teacher1, teacher2) = class_subject_uniformity_two_lesson_problem();
        let placements = vec![
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: teacher1,
            },
            Placement {
                lesson_id: p.lessons[1].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: teacher2,
            },
        ];
        let err = validate_class_subject_teacher_uniformity(&p, &placements).unwrap_err();
        let Error::Input(msg) = err;
        assert!(msg.contains("class-subject teacher split"), "msg: {msg}");
    }

    #[test]
    fn validate_class_subject_teacher_uniformity_accepts_uniform_assignment() {
        let (p, teacher1, _teacher2) = class_subject_uniformity_two_lesson_problem();
        let placements = vec![
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: teacher1,
            },
            Placement {
                lesson_id: p.lessons[1].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: teacher1,
            },
        ];
        validate_class_subject_teacher_uniformity(&p, &placements).unwrap();
    }

    #[test]
    fn validate_class_subject_teacher_uniformity_accepts_multi_class_lessons_uniformly_assigned() {
        let mut p = minimal_problem();
        let teacher = p.teachers[0].id;
        let class1 = p.school_classes[0].id;
        let class2 = SchoolClassId(uuid(210));
        p.school_classes.push(SchoolClass {
            id: class2,
            home_room_id: None,
            max_lessons_per_day: None,
            class_teacher_id: None,
        });
        // First lesson covers both classes; second lesson also covers both.
        // Both lessons must use the same teacher to satisfy uniformity for
        // every (class, subject) pair contributed.
        p.lessons[0].school_class_ids = vec![class1, class2];
        p.lessons.push(Lesson {
            id: LessonId(uuid(211)),
            school_class_ids: vec![class1, class2],
            subject_id: p.subjects[0].id,
            teacher_candidates: vec![teacher],
            teacher_pin: Some(teacher),
            hours_per_week: 1,
            preferred_block_size: 1,
            pre_buffer_minutes: 0,
            post_buffer_minutes: 0,
            lesson_group_id: None,
        });
        let placements = vec![
            Placement {
                lesson_id: p.lessons[0].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: teacher,
            },
            Placement {
                lesson_id: p.lessons[1].id,
                time_block_id: p.time_blocks[0].id,
                room_id: p.rooms[0].id,
                teacher_id: teacher,
            },
        ];
        validate_class_subject_teacher_uniformity(&p, &placements).unwrap();
    }
}
