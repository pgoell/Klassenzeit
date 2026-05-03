"""Integration tests for ``POST /api/schedule/all`` and the cross-class
invariant introduced by sibling-pin enforcement.

The whole-school endpoint solves every class in one shot and persists
placements per class atomically. The cross-class invariant says that a
subsequent per-class re-solve must not modify any sibling class's
persisted placements.

The flow mirrors ``test_demo_grundschule_dreizuegig_solvability``:
seed -> per-class ``generate-lessons`` -> override ``Lesson.teacher_id``
from ``_TEACHER_ASSIGNMENTS_DREIZUEGIG`` (Religion was pinned at seed
time and is not in the mapping) -> exercise the new endpoint and the
cross-class invariant.
"""

from collections.abc import Awaitable, Callable
from uuid import UUID

from httpx import AsyncClient
from sqlalchemy import select, update
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.lesson import Lesson
from klassenzeit_backend.db.models.lesson_school_class import LessonSchoolClass
from klassenzeit_backend.db.models.scheduled_lesson import ScheduledLesson
from klassenzeit_backend.db.models.school_class import SchoolClass
from klassenzeit_backend.db.models.subject import Subject
from klassenzeit_backend.db.models.teacher import Teacher
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.seed.demo_grundschule_dreizuegig import (
    _TEACHER_ASSIGNMENTS_DREIZUEGIG,
    seed_demo_grundschule_dreizuegig,
)

CreateUserFnWs = Callable[..., Awaitable[tuple[User, str]]]
LoginFnWs = Callable[[str, str], Awaitable[None]]


async def _seed_generate_and_pin_teachers(
    db_session: AsyncSession,
    client: AsyncClient,
) -> list[SchoolClass]:
    """Seed the dreizuegig demo, generate lessons via the route, and pin teachers.

    Returns the SchoolClass rows ordered by ``(grade_level, name)`` so the
    caller can address them deterministically.
    """
    await seed_demo_grundschule_dreizuegig(db_session)
    await db_session.flush()

    classes = (
        (
            await db_session.execute(
                select(SchoolClass).order_by(SchoolClass.grade_level, SchoolClass.name)
            )
        )
        .scalars()
        .all()
    )
    for school_class in classes:
        gen_resp = await client.post(f"/api/classes/{school_class.id}/generate-lessons")
        assert gen_resp.status_code == 201, gen_resp.text

    teachers_by_short = {
        t.short_code: t for t in (await db_session.execute(select(Teacher))).scalars().all()
    }
    subjects_by_short = {
        s.short_name: s for s in (await db_session.execute(select(Subject))).scalars().all()
    }
    classes_by_name = {c.name: c for c in classes}
    for (class_name, subject_short), teacher_short in _TEACHER_ASSIGNMENTS_DREIZUEGIG.items():
        school_class = classes_by_name[class_name]
        await db_session.execute(
            update(Lesson)
            .where(
                Lesson.id.in_(
                    select(LessonSchoolClass.lesson_id).where(
                        LessonSchoolClass.school_class_id == school_class.id
                    )
                ),
                Lesson.subject_id == subjects_by_short[subject_short].id,
            )
            .values(teacher_id=teachers_by_short[teacher_short].id)
        )
    await db_session.flush()
    return list(classes)


async def test_post_schedule_all_persists_every_class(
    db_session: AsyncSession,
    client: AsyncClient,
    create_test_user: CreateUserFnWs,
    login_as: LoginFnWs,
) -> None:
    """``POST /api/schedule/all`` solves every class in one shot."""
    admin, password = await create_test_user(
        email="admin-ws-all@example.com",
        password="ws-all-password-12345",  # noqa: S106
        role="admin",
    )
    await login_as(admin.email, password)

    classes = await _seed_generate_and_pin_teachers(db_session, client)

    response = await client.post("/api/schedule/all")
    assert response.status_code == 200, response.text
    body = response.json()
    assert body["total_placements"] > 0
    assert body["total_violations"] == 0
    assert len(body["classes"]) == len(classes)


async def test_per_class_resolve_preserves_sibling_persisted_placements(
    db_session: AsyncSession,
    client: AsyncClient,
    create_test_user: CreateUserFnWs,
    login_as: LoginFnWs,
) -> None:
    """A per-class re-solve must not touch sibling persisted placements."""
    admin, password = await create_test_user(
        email="admin-ws-invariant@example.com",
        password="ws-invariant-password-12345",  # noqa: S106
        role="admin",
    )
    await login_as(admin.email, password)

    classes = await _seed_generate_and_pin_teachers(db_session, client)

    all_resp = await client.post("/api/schedule/all")
    assert all_resp.status_code == 200, all_resp.text

    async def snapshot_for(class_id: UUID) -> list[tuple[UUID, UUID, UUID]]:
        rows = (
            (
                await db_session.execute(
                    select(ScheduledLesson)
                    .join(Lesson, Lesson.id == ScheduledLesson.lesson_id)
                    .join(LessonSchoolClass, LessonSchoolClass.lesson_id == Lesson.id)
                    .where(LessonSchoolClass.school_class_id == class_id)
                    .order_by(ScheduledLesson.lesson_id, ScheduledLesson.time_block_id)
                )
            )
            .scalars()
            .all()
        )
        return [(r.lesson_id, r.time_block_id, r.room_id) for r in rows]

    snapshots = {cls.id: await snapshot_for(cls.id) for cls in classes}

    first = classes[0]
    resolve_resp = await client.post(f"/api/classes/{first.id}/schedule")
    assert resolve_resp.status_code == 200, resolve_resp.text

    for cls in classes[1:]:
        assert await snapshot_for(cls.id) == snapshots[cls.id], (
            f"sibling class {cls.name} drifted on per-class re-solve"
        )
