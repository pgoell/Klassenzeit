"""Tests for super_admin_audit_log capture (item 10g)."""

import uuid
from dataclasses import dataclass

import pytest

from klassenzeit_backend.auth.audit import should_audit_request


@dataclass
class _Membership:
    school_id: uuid.UUID


@dataclass
class _User:
    """Duck-typed User for predicate unit tests.

    The real ``is_super_admin(user)`` from ``auth.dependencies`` checks
    ``user.role == "super_admin"`` and the predicate reads ``user.school_id``
    / ``user.memberships``; this dataclass matches that surface.
    """

    id: uuid.UUID
    role: str
    school_id: uuid.UUID
    memberships: list[_Membership]


HOME = uuid.UUID("00000000-0000-0000-0000-000000000001")
MEMBER = uuid.UUID("00000000-0000-0000-0000-000000000002")
NON_MEMBER = uuid.UUID("00000000-0000-0000-0000-000000000003")


def _user(role: str) -> _User:
    return _User(
        id=uuid.uuid4(),
        role=role,
        school_id=HOME,
        memberships=[_Membership(school_id=MEMBER)],
    )


@pytest.mark.parametrize(
    ("role", "target", "method", "route", "expected"),
    [
        # super-admin on home school via tenanted route -> NO
        ("super_admin", HOME, "POST", "/api/rooms", False),
        ("super_admin", HOME, "PATCH", "/api/rooms/{room_id}", False),
        # super-admin on member school via tenanted route -> NO
        ("super_admin", MEMBER, "POST", "/api/rooms", False),
        # super-admin on non-member school via tenanted route -> YES
        ("super_admin", NON_MEMBER, "POST", "/api/rooms", True),
        ("super_admin", NON_MEMBER, "PATCH", "/api/rooms/{room_id}", True),
        ("super_admin", NON_MEMBER, "DELETE", "/api/rooms/{room_id}", True),
        (
            "super_admin",
            NON_MEMBER,
            "PUT",
            "/api/teachers/{teacher_id}/qualifications",
            True,
        ),
        # super-admin on /api/schools/* -> always YES (regardless of target_school_id)
        ("super_admin", None, "POST", "/api/schools", True),
        ("super_admin", HOME, "PATCH", "/api/schools/{school_id}", True),
        ("super_admin", NON_MEMBER, "DELETE", "/api/schools/{school_id}", True),
        # super-admin on read methods -> NO
        ("super_admin", NON_MEMBER, "GET", "/api/rooms", False),
        ("super_admin", None, "GET", "/api/schools", False),
        ("super_admin", None, "OPTIONS", "/api/schools", False),
        # regular admin on any school via any method -> NO
        ("admin", HOME, "POST", "/api/rooms", False),
        ("admin", NON_MEMBER, "POST", "/api/rooms", False),
        ("admin", None, "POST", "/api/schools", False),
        # regular user -> NO (defensive; should never reach a write route)
        ("user", NON_MEMBER, "POST", "/api/rooms", False),
    ],
)
def test_should_audit_request(
    role: str,
    target: uuid.UUID | None,
    method: str,
    route: str,
    expected: bool,
) -> None:
    user = _user(role=role)
    assert (
        should_audit_request(user, target, method, route)  # ty: ignore[invalid-argument-type]
        is expected
    )


def test_should_audit_request_returns_false_for_none_user() -> None:
    assert should_audit_request(None, None, "POST", "/api/rooms") is False
