"""Integration tests for the Lesson CRUD routes and generate-lessons endpoint."""

import uuid
from collections.abc import Awaitable, Callable

import pytest
from httpx import AsyncClient

from klassenzeit_backend.db.models.user import User

pytestmark = pytest.mark.anyio

# Type aliases matching the factory fixtures defined in conftest.py
type CreateUserFn = Callable[..., Awaitable[tuple[User, str]]]
type LoginFn = Callable[[str, str], Awaitable[None]]


async def _create_subject(client: AsyncClient, name: str, short_name: str) -> str:
    """Create a Subject via the API and return its ID.

    Args:
        client: The async test HTTP client (must already be authenticated).
        name: Unique name for the Subject.
        short_name: Short abbreviation for the Subject.

    Returns:
        The UUID string of the created Subject.
    """
    resp = await client.post(
        "/api/subjects", json={"name": name, "short_name": short_name, "color": "chart-1"}
    )
    assert resp.status_code == 201, resp.text
    return resp.json()["id"]


async def _setup_week_scheme_for_lessons(client: AsyncClient, name: str) -> str:
    """Create a WeekScheme via the API and return its ID.

    Args:
        client: The async test HTTP client (must already be authenticated).
        name: Unique name for the WeekScheme.

    Returns:
        The UUID string of the created WeekScheme.
    """
    resp = await client.post("/api/week-schemes", json={"name": name})
    assert resp.status_code == 201, resp.text
    return resp.json()["id"]


async def _setup_stundentafel_for_lessons(
    client: AsyncClient, name: str, grade_level: int = 5
) -> str:
    """Create a Stundentafel via the API and return its ID.

    Args:
        client: The async test HTTP client (must already be authenticated).
        name: Unique name for the Stundentafel.
        grade_level: Grade level to assign (default 5).

    Returns:
        The UUID string of the created Stundentafel.
    """
    resp = await client.post("/api/stundentafeln", json={"name": name, "grade_level": grade_level})
    assert resp.status_code == 201, resp.text
    return resp.json()["id"]


async def _create_school_class(
    client: AsyncClient,
    name: str,
    grade_level: int,
    stundentafel_id: str,
    week_scheme_id: str,
) -> str:
    """Create a SchoolClass via the API and return its ID.

    Args:
        client: The async test HTTP client (must already be authenticated).
        name: Unique name for the class.
        grade_level: Grade level for the class.
        stundentafel_id: UUID string of the associated Stundentafel.
        week_scheme_id: UUID string of the associated WeekScheme.

    Returns:
        The UUID string of the created SchoolClass.
    """
    resp = await client.post(
        "/api/classes",
        json={
            "name": name,
            "grade_level": grade_level,
            "stundentafel_id": stundentafel_id,
            "week_scheme_id": week_scheme_id,
        },
    )
    assert resp.status_code == 201, resp.text
    return resp.json()["id"]


async def _create_teacher(
    client: AsyncClient, first_name: str, last_name: str, short_code: str
) -> str:
    """Create a Teacher via the API and return its ID.

    Args:
        client: The async test HTTP client (must already be authenticated).
        first_name: Teacher's given name.
        last_name: Teacher's family name.
        short_code: Unique short abbreviation.

    Returns:
        The UUID string of the created Teacher.
    """
    resp = await client.post(
        "/api/teachers",
        json={
            "first_name": first_name,
            "last_name": last_name,
            "short_code": short_code,
            "max_hours_per_week": 24,
        },
    )
    assert resp.status_code == 201, resp.text
    return resp.json()["id"]


async def test_create_lesson(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /lessons creates a new lesson and returns 201 with the full body.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@les1.com", role="admin")
    await login_as("admin@les1.com", "testpassword123")

    subject_id = await _create_subject(client, "Mathematik", "Ma")
    scheme_id = await _setup_week_scheme_for_lessons(client, "Scheme L1")
    tafel_id = await _setup_stundentafel_for_lessons(client, "Tafel L1", 5)
    class_id = await _create_school_class(client, "5a-L1", 5, tafel_id, scheme_id)
    teacher_id = await _create_teacher(client, "Hans", "Müller", "HMU1")

    resp = await client.post(
        "/api/lessons",
        json={
            "school_class_ids": [class_id],
            "subject_id": subject_id,
            "teacher_id": teacher_id,
            "hours_per_week": 4,
            "preferred_block_size": 2,
        },
    )
    assert resp.status_code == 201, resp.text
    body = resp.json()
    assert "id" in body
    assert body["hours_per_week"] == 4
    assert body["preferred_block_size"] == 2
    assert body["school_classes"][0]["id"] == class_id
    assert body["subject"]["id"] == subject_id
    assert body["teacher"]["id"] == teacher_id
    assert "created_at" in body
    assert "updated_at" in body


async def test_create_lesson_without_teacher(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /lessons with no teacher_id returns 201 and teacher is null in response.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@les2.com", role="admin")
    await login_as("admin@les2.com", "testpassword123")

    subject_id = await _create_subject(client, "Deutsch", "De")
    scheme_id = await _setup_week_scheme_for_lessons(client, "Scheme L2")
    tafel_id = await _setup_stundentafel_for_lessons(client, "Tafel L2", 5)
    class_id = await _create_school_class(client, "5b-L2", 5, tafel_id, scheme_id)

    resp = await client.post(
        "/api/lessons",
        json={
            "school_class_ids": [class_id],
            "subject_id": subject_id,
            "hours_per_week": 3,
        },
    )
    assert resp.status_code == 201, resp.text
    body = resp.json()
    assert body["teacher"] is None
    assert body["hours_per_week"] == 3


async def test_create_lesson_duplicate_class_subject(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /lessons with a duplicate class+subject pair returns 409 Conflict.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@les3.com", role="admin")
    await login_as("admin@les3.com", "testpassword123")

    subject_id = await _create_subject(client, "Englisch", "En")
    scheme_id = await _setup_week_scheme_for_lessons(client, "Scheme L3")
    tafel_id = await _setup_stundentafel_for_lessons(client, "Tafel L3", 6)
    class_id = await _create_school_class(client, "6a-L3", 6, tafel_id, scheme_id)

    payload = {
        "school_class_ids": [class_id],
        "subject_id": subject_id,
        "hours_per_week": 3,
    }
    await client.post("/api/lessons", json=payload)
    resp = await client.post("/api/lessons", json=payload)
    assert resp.status_code == 409


async def test_list_lessons(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """GET /lessons returns all lessons as a list with 200.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@les4.com", role="admin")
    await login_as("admin@les4.com", "testpassword123")

    subject_id = await _create_subject(client, "Physik", "Ph")
    scheme_id = await _setup_week_scheme_for_lessons(client, "Scheme L4")
    tafel_id = await _setup_stundentafel_for_lessons(client, "Tafel L4", 7)
    class_id = await _create_school_class(client, "7a-L4", 7, tafel_id, scheme_id)

    await client.post(
        "/api/lessons",
        json={"school_class_ids": [class_id], "subject_id": subject_id, "hours_per_week": 2},
    )

    resp = await client.get("/api/lessons")
    assert resp.status_code == 200
    body = resp.json()
    assert isinstance(body, list)
    assert len(body) >= 1


async def test_list_lessons_filter_by_class(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """GET /lessons?class_id=... returns only lessons for the given class.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@les5.com", role="admin")
    await login_as("admin@les5.com", "testpassword123")

    subj1_id = await _create_subject(client, "Chemie", "Ch")
    subj2_id = await _create_subject(client, "Biologie", "Bio")
    scheme_id = await _setup_week_scheme_for_lessons(client, "Scheme L5")
    tafel_id = await _setup_stundentafel_for_lessons(client, "Tafel L5", 8)
    class1_id = await _create_school_class(client, "8a-L5", 8, tafel_id, scheme_id)
    class2_id = await _create_school_class(client, "8b-L5", 8, tafel_id, scheme_id)

    await client.post(
        "/api/lessons",
        json={"school_class_ids": [class1_id], "subject_id": subj1_id, "hours_per_week": 2},
    )
    await client.post(
        "/api/lessons",
        json={"school_class_ids": [class2_id], "subject_id": subj2_id, "hours_per_week": 2},
    )

    resp = await client.get(f"/api/lessons?class_id={class1_id}")
    assert resp.status_code == 200
    body = resp.json()
    assert all(lesson["school_classes"][0]["id"] == class1_id for lesson in body)
    assert len(body) == 1


async def test_list_lessons_filter_by_teacher(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """GET /lessons?teacher_id=... returns only lessons assigned to that teacher.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@les6.com", role="admin")
    await login_as("admin@les6.com", "testpassword123")

    subj1_id = await _create_subject(client, "Sport", "Sp")
    subj2_id = await _create_subject(client, "Musik", "Mu")
    scheme_id = await _setup_week_scheme_for_lessons(client, "Scheme L6")
    tafel_id = await _setup_stundentafel_for_lessons(client, "Tafel L6", 9)
    class_id = await _create_school_class(client, "9a-L6", 9, tafel_id, scheme_id)
    teacher1_id = await _create_teacher(client, "Anna", "Schmidt", "ASC6")
    teacher2_id = await _create_teacher(client, "Bob", "Meier", "BME6")

    await client.post(
        "/api/lessons",
        json={
            "school_class_ids": [class_id],
            "subject_id": subj1_id,
            "teacher_id": teacher1_id,
            "hours_per_week": 2,
        },
    )
    await client.post(
        "/api/lessons",
        json={
            "school_class_ids": [class_id],
            "subject_id": subj2_id,
            "teacher_id": teacher2_id,
            "hours_per_week": 2,
        },
    )

    resp = await client.get(f"/api/lessons?teacher_id={teacher1_id}")
    assert resp.status_code == 200
    body = resp.json()
    assert all(lesson["teacher"]["id"] == teacher1_id for lesson in body)
    assert len(body) == 1


async def test_get_lesson(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """GET /lessons/{id} returns the lesson with nested class, subject and teacher data.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@les7.com", role="admin")
    await login_as("admin@les7.com", "testpassword123")

    subject_id = await _create_subject(client, "Geschichte", "Ge")
    scheme_id = await _setup_week_scheme_for_lessons(client, "Scheme L7")
    tafel_id = await _setup_stundentafel_for_lessons(client, "Tafel L7", 10)
    class_id = await _create_school_class(client, "10a-L7", 10, tafel_id, scheme_id)
    teacher_id = await _create_teacher(client, "Clara", "Weber", "CWE7")

    create_resp = await client.post(
        "/api/lessons",
        json={
            "school_class_ids": [class_id],
            "subject_id": subject_id,
            "teacher_id": teacher_id,
            "hours_per_week": 2,
        },
    )
    lesson_id = create_resp.json()["id"]

    resp = await client.get(f"/api/lessons/{lesson_id}")
    assert resp.status_code == 200
    body = resp.json()
    assert body["id"] == lesson_id
    assert body["school_classes"][0]["id"] == class_id
    assert body["school_classes"][0]["name"] == "10a-L7"
    assert body["subject"]["id"] == subject_id
    assert body["subject"]["short_name"] == "Ge"
    assert body["teacher"]["id"] == teacher_id
    assert body["teacher"]["short_code"] == "CWE7"


async def test_update_lesson_assign_teacher(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """PATCH /lessons/{id} can assign a teacher to a lesson that had none.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@les8.com", role="admin")
    await login_as("admin@les8.com", "testpassword123")

    subject_id = await _create_subject(client, "Latein", "La")
    scheme_id = await _setup_week_scheme_for_lessons(client, "Scheme L8")
    tafel_id = await _setup_stundentafel_for_lessons(client, "Tafel L8", 5)
    class_id = await _create_school_class(client, "5c-L8", 5, tafel_id, scheme_id)
    teacher_id = await _create_teacher(client, "Dirk", "Fischer", "DFI8")

    create_resp = await client.post(
        "/api/lessons",
        json={"school_class_ids": [class_id], "subject_id": subject_id, "hours_per_week": 3},
    )
    lesson_id = create_resp.json()["id"]
    assert create_resp.json()["teacher"] is None

    patch_resp = await client.patch(f"/api/lessons/{lesson_id}", json={"teacher_id": teacher_id})
    assert patch_resp.status_code == 200
    body = patch_resp.json()
    assert body["teacher"]["id"] == teacher_id
    assert body["hours_per_week"] == 3


async def test_delete_lesson(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """DELETE /lessons/{id} removes the lesson; subsequent GET returns 404.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@les9.com", role="admin")
    await login_as("admin@les9.com", "testpassword123")

    subject_id = await _create_subject(client, "Informatik", "In")
    scheme_id = await _setup_week_scheme_for_lessons(client, "Scheme L9")
    tafel_id = await _setup_stundentafel_for_lessons(client, "Tafel L9", 6)
    class_id = await _create_school_class(client, "6c-L9", 6, tafel_id, scheme_id)

    create_resp = await client.post(
        "/api/lessons",
        json={"school_class_ids": [class_id], "subject_id": subject_id, "hours_per_week": 2},
    )
    lesson_id = create_resp.json()["id"]

    delete_resp = await client.delete(f"/api/lessons/{lesson_id}")
    assert delete_resp.status_code == 204

    get_resp = await client.get(f"/api/lessons/{lesson_id}")
    assert get_resp.status_code == 404


async def test_generate_lessons_from_stundentafel(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /classes/{id}/generate-lessons creates lessons from the class's Stundentafel.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@les10.com", role="admin")
    await login_as("admin@les10.com", "testpassword123")

    subj1_id = await _create_subject(client, "Erdkunde", "Ek")
    subj2_id = await _create_subject(client, "Wirtschaft", "Wi")
    scheme_id = await _setup_week_scheme_for_lessons(client, "Scheme L10")
    tafel_id = await _setup_stundentafel_for_lessons(client, "Tafel L10", 7)

    await client.post(
        f"/api/stundentafeln/{tafel_id}/entries",
        json={"subject_id": subj1_id, "hours_per_week": 2, "preferred_block_size": 1},
    )
    await client.post(
        f"/api/stundentafeln/{tafel_id}/entries",
        json={"subject_id": subj2_id, "hours_per_week": 4, "preferred_block_size": 2},
    )

    teacher1_id = await _create_teacher(client, "Erna", "Erdfreund", "EER10")
    teacher2_id = await _create_teacher(client, "Wilma", "Wirtschaft", "WWI10")
    await client.put(
        f"/api/teachers/{teacher1_id}/qualifications",
        json={"subject_ids": [subj1_id]},
    )
    await client.put(
        f"/api/teachers/{teacher2_id}/qualifications",
        json={"subject_ids": [subj2_id]},
    )

    class_id = await _create_school_class(client, "7a-L10", 7, tafel_id, scheme_id)

    resp = await client.post(f"/api/classes/{class_id}/generate-lessons")
    assert resp.status_code == 201, resp.text
    body = resp.json()
    assert len(body) == 2
    subject_ids = {lesson["subject"]["id"] for lesson in body}
    assert subj1_id in subject_ids
    assert subj2_id in subject_ids

    hours_by_subject = {lesson["subject"]["id"]: lesson["hours_per_week"] for lesson in body}
    assert hours_by_subject[subj1_id] == 2
    assert hours_by_subject[subj2_id] == 4


async def test_generate_lessons_skips_existing(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /classes/{id}/generate-lessons skips subjects that already have a lesson.

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@les11.com", role="admin")
    await login_as("admin@les11.com", "testpassword123")

    subj1_id = await _create_subject(client, "Philosophie", "Phi")
    subj2_id = await _create_subject(client, "Religion", "Re")
    scheme_id = await _setup_week_scheme_for_lessons(client, "Scheme L11")
    tafel_id = await _setup_stundentafel_for_lessons(client, "Tafel L11", 8)

    await client.post(
        f"/api/stundentafeln/{tafel_id}/entries",
        json={"subject_id": subj1_id, "hours_per_week": 2},
    )
    await client.post(
        f"/api/stundentafeln/{tafel_id}/entries",
        json={"subject_id": subj2_id, "hours_per_week": 2},
    )

    teacher1_id = await _create_teacher(client, "Phil", "Philsoph", "PPI11")
    teacher2_id = await _create_teacher(client, "Reni", "Religion", "RRE11")
    await client.put(
        f"/api/teachers/{teacher1_id}/qualifications",
        json={"subject_ids": [subj1_id]},
    )
    await client.put(
        f"/api/teachers/{teacher2_id}/qualifications",
        json={"subject_ids": [subj2_id]},
    )

    class_id = await _create_school_class(client, "8a-L11", 8, tafel_id, scheme_id)

    # Pre-create a lesson for subj1
    await client.post(
        "/api/lessons",
        json={"school_class_ids": [class_id], "subject_id": subj1_id, "hours_per_week": 2},
    )

    resp = await client.post(f"/api/classes/{class_id}/generate-lessons")
    assert resp.status_code == 201, resp.text
    body = resp.json()
    assert len(body) == 1
    assert body[0]["subject"]["id"] == subj2_id


async def test_generate_lessons_leaves_teacher_id_null(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /classes/{id}/generate-lessons leaves teacher_id null on every Lesson.

    Per OPEN_THINGS item 63, the route no longer auto-assigns a teacher; the
    solver picks among qualified candidates at solve time. A qualified active
    Teacher is seeded to prove null is the default regardless of teacher
    availability (not a fallback for "no qualified teacher").

    Args:
        client: The async test HTTP client.
        create_test_user: Factory fixture for inserting a User into the DB.
        login_as: Factory fixture for authenticating via /auth/login.
    """
    await create_test_user(email="admin@les-pin1.com", role="admin")
    await login_as("admin@les-pin1.com", "testpassword123")

    subject_id = await _create_subject(client, "Mathematik-PIN", "MaPIN")
    scheme_id = await _setup_week_scheme_for_lessons(client, "Scheme LPIN1")
    tafel_id = await _setup_stundentafel_for_lessons(client, "Tafel LPIN1", 5)
    await client.post(
        f"/api/stundentafeln/{tafel_id}/entries",
        json={"subject_id": subject_id, "hours_per_week": 4, "preferred_block_size": 1},
    )
    class_id = await _create_school_class(client, "5a-LPIN1", 5, tafel_id, scheme_id)
    teacher_id = await _create_teacher(client, "Pia", "Pinner", "PIN1")
    qual_resp = await client.put(
        f"/api/teachers/{teacher_id}/qualifications",
        json={"subject_ids": [subject_id]},
    )
    assert qual_resp.status_code == 200, qual_resp.text

    resp = await client.post(f"/api/classes/{class_id}/generate-lessons")
    assert resp.status_code == 201, resp.text
    body = resp.json()
    assert len(body) == 1
    assert body[0]["teacher"] is None


async def test_generate_lessons_raises_422_when_subject_has_no_qualified_teacher(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /classes/{id}/generate-lessons raises 422 when a subject has no qualified teacher.

    Per OPEN_THINGS item 69, the route validates that every Stundentafel subject
    in the class's curriculum has at least one qualified Teacher in the database
    before creating any Lesson. The 422 detail is a structured dict carrying a
    stable ``code`` and the offending ``subject_ids`` / ``subject_short_names``
    so the frontend can surface an actionable error.
    """
    await create_test_user(email="admin@les-422-1.com", role="admin")
    await login_as("admin@les-422-1.com", "testpassword123")

    subject_id = await _create_subject(client, "Mathematik-422-1", "Ma4221")
    scheme_id = await _setup_week_scheme_for_lessons(client, "Scheme L422-1")
    tafel_id = await _setup_stundentafel_for_lessons(client, "Tafel L422-1", 5)
    await client.post(
        f"/api/stundentafeln/{tafel_id}/entries",
        json={"subject_id": subject_id, "hours_per_week": 4, "preferred_block_size": 1},
    )
    class_id = await _create_school_class(client, "5a-L422-1", 5, tafel_id, scheme_id)

    resp = await client.post(f"/api/classes/{class_id}/generate-lessons")
    assert resp.status_code == 422, resp.text
    detail = resp.json()["detail"]
    assert detail["code"] == "missing_qualified_teacher"
    assert detail["subject_ids"] == [subject_id]
    assert detail["subject_short_names"] == ["Ma4221"]


async def test_generate_lessons_aggregates_multiple_missing_subjects(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /classes/{id}/generate-lessons surfaces every missing-qualified subject in one 422.

    The admin should fix all data gaps in a single batch instead of click-fix-retry
    per subject. The 422 ``detail.subject_ids`` lists all offenders.
    """
    await create_test_user(email="admin@les-422-2.com", role="admin")
    await login_as("admin@les-422-2.com", "testpassword123")

    subj1_id = await _create_subject(client, "Mathematik-422-2", "Ma4222")
    subj2_id = await _create_subject(client, "Deutsch-422-2", "De4222")
    scheme_id = await _setup_week_scheme_for_lessons(client, "Scheme L422-2")
    tafel_id = await _setup_stundentafel_for_lessons(client, "Tafel L422-2", 5)
    await client.post(
        f"/api/stundentafeln/{tafel_id}/entries",
        json={"subject_id": subj1_id, "hours_per_week": 4, "preferred_block_size": 1},
    )
    await client.post(
        f"/api/stundentafeln/{tafel_id}/entries",
        json={"subject_id": subj2_id, "hours_per_week": 4, "preferred_block_size": 1},
    )
    class_id = await _create_school_class(client, "5a-L422-2", 5, tafel_id, scheme_id)

    resp = await client.post(f"/api/classes/{class_id}/generate-lessons")
    assert resp.status_code == 422, resp.text
    detail = resp.json()["detail"]
    assert detail["code"] == "missing_qualified_teacher"
    assert set(detail["subject_ids"]) == {subj1_id, subj2_id}
    assert set(detail["subject_short_names"]) == {"Ma4222", "De4222"}


async def test_lesson_requires_admin(client: AsyncClient) -> None:
    """GET /lessons without authentication returns 401 Unauthorized.

    Args:
        client: The async test HTTP client (no session cookie set).
    """
    resp = await client.get("/api/lessons")
    assert resp.status_code == 401

    resp = await client.post(f"/api/classes/{uuid.uuid4()}/generate-lessons")
    assert resp.status_code == 401


async def test_create_lesson_rejects_odd_hours_with_block_size_two(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /lessons returns 422 when hours_per_week is not divisible by preferred_block_size."""
    await create_test_user(email="admin@les-dop1.com", role="admin")
    await login_as("admin@les-dop1.com", "testpassword123")

    subject_id = await _create_subject(client, "Sport", "Sp")
    scheme_id = await _setup_week_scheme_for_lessons(client, "Scheme Dop1")
    tafel_id = await _setup_stundentafel_for_lessons(client, "Tafel Dop1", 5)
    class_id = await _create_school_class(client, "5a-Dop1", 5, tafel_id, scheme_id)

    resp = await client.post(
        "/api/lessons",
        json={
            "school_class_ids": [class_id],
            "subject_id": subject_id,
            "hours_per_week": 3,
            "preferred_block_size": 2,
        },
    )
    assert resp.status_code == 422, resp.text
    assert "preferred_block_size" in resp.text


async def test_update_lesson_rejects_block_size_change_breaking_divisibility(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """PATCH /lessons/{id} returns 422 when the merged row would have h % n != 0."""
    await create_test_user(email="admin@les-dop2.com", role="admin")
    await login_as("admin@les-dop2.com", "testpassword123")

    subject_id = await _create_subject(client, "Sport", "Sp")
    scheme_id = await _setup_week_scheme_for_lessons(client, "Scheme Dop2")
    tafel_id = await _setup_stundentafel_for_lessons(client, "Tafel Dop2", 5)
    class_id = await _create_school_class(client, "5b-Dop2", 5, tafel_id, scheme_id)

    create = await client.post(
        "/api/lessons",
        json={
            "school_class_ids": [class_id],
            "subject_id": subject_id,
            "hours_per_week": 3,
            "preferred_block_size": 1,
        },
    )
    assert create.status_code == 201, create.text
    lesson_id = create.json()["id"]

    update = await client.patch(
        f"/api/lessons/{lesson_id}",
        json={"preferred_block_size": 2},
    )
    assert update.status_code == 422, update.text
    assert "preferred_block_size" in update.text


async def test_lesson_create_requires_non_empty_school_class_ids(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /lessons with school_class_ids=[] returns 422."""
    await create_test_user(email="admin@les-mc1.com", role="admin")
    await login_as("admin@les-mc1.com", "testpassword123")
    subject_id = await _create_subject(client, "Multi", "MUL")
    resp = await client.post(
        "/api/lessons",
        json={
            "school_class_ids": [],
            "subject_id": subject_id,
            "teacher_id": None,
            "hours_per_week": 1,
            "preferred_block_size": 1,
        },
    )
    assert resp.status_code == 422


async def test_lesson_create_rejects_duplicate_school_class_ids(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """POST /lessons with the same class id twice returns 422."""
    await create_test_user(email="admin@les-mc2.com", role="admin")
    await login_as("admin@les-mc2.com", "testpassword123")
    week_scheme_id = await _setup_week_scheme_for_lessons(client, "WS-dup")
    stundentafel_id = await _setup_stundentafel_for_lessons(client, "ST-dup")
    class_id = await _create_school_class(client, "1a-dup", 1, stundentafel_id, week_scheme_id)
    subject_id = await _create_subject(client, "Subject-dup", "SDP")
    resp = await client.post(
        "/api/lessons",
        json={
            "school_class_ids": [class_id, class_id],
            "subject_id": subject_id,
            "teacher_id": None,
            "hours_per_week": 1,
            "preferred_block_size": 1,
        },
    )
    assert resp.status_code == 422


async def test_lesson_create_409_when_subject_overlaps_existing_membership(
    client: AsyncClient,
    create_test_user: CreateUserFn,
    login_as: LoginFn,
) -> None:
    """A second lesson whose membership overlaps an existing class+subject pair returns 409."""
    await create_test_user(email="admin@les-mc3.com", role="admin")
    await login_as("admin@les-mc3.com", "testpassword123")
    week_scheme_id = await _setup_week_scheme_for_lessons(client, "WS-collide")
    stundentafel_id = await _setup_stundentafel_for_lessons(client, "ST-collide")
    class_a = await _create_school_class(client, "1a-collide", 1, stundentafel_id, week_scheme_id)
    class_b = await _create_school_class(client, "1b-collide", 1, stundentafel_id, week_scheme_id)
    subject_id = await _create_subject(client, "Subject-collide", "SCO")
    first = await client.post(
        "/api/lessons",
        json={
            "school_class_ids": [class_a],
            "subject_id": subject_id,
            "teacher_id": None,
            "hours_per_week": 1,
            "preferred_block_size": 1,
        },
    )
    assert first.status_code == 201
    second = await client.post(
        "/api/lessons",
        json={
            "school_class_ids": [class_a, class_b],
            "subject_id": subject_id,
            "teacher_id": None,
            "hours_per_week": 1,
            "preferred_block_size": 1,
        },
    )
    assert second.status_code == 409
