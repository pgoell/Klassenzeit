"""Tests for UserSchoolMembership model (M:N user<->school)."""

import uuid

import pytest
from sqlalchemy import select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.school import School
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.db.models.user_school_membership import UserSchoolMembership


async def _make_school_for_membership(db: AsyncSession, name: str) -> School:
    school = School(name=name)
    db.add(school)
    await db.flush()
    return school


async def _make_user_for_membership(
    db: AsyncSession, email: str, home_school_id: uuid.UUID
) -> User:
    user = User(
        email=email,
        password_hash="x",  # noqa: S106 (bogus hash for model test)
        role="user",
        school_id=home_school_id,
    )
    db.add(user)
    await db.flush()
    return user


@pytest.mark.asyncio
async def test_unique_user_school_pair_rejects_duplicates(
    db_session: AsyncSession,
) -> None:
    home = await _make_school_for_membership(db_session, "Home A")
    other = await _make_school_for_membership(db_session, "Other A")
    user = await _make_user_for_membership(db_session, "coach1@example.com", home.id)

    db_session.add(UserSchoolMembership(user_id=user.id, school_id=other.id))
    await db_session.flush()

    with pytest.raises(IntegrityError):
        async with db_session.begin_nested():
            db_session.add(UserSchoolMembership(user_id=user.id, school_id=other.id))
            await db_session.flush()


@pytest.mark.asyncio
async def test_cascade_on_user_delete(db_session: AsyncSession) -> None:
    home = await _make_school_for_membership(db_session, "Home B")
    other = await _make_school_for_membership(db_session, "Other B")
    user = await _make_user_for_membership(db_session, "coach2@example.com", home.id)
    db_session.add(UserSchoolMembership(user_id=user.id, school_id=other.id))
    await db_session.flush()

    await db_session.delete(user)
    await db_session.flush()

    rows = (
        (
            await db_session.execute(
                select(UserSchoolMembership).where(UserSchoolMembership.user_id == user.id)
            )
        )
        .scalars()
        .all()
    )
    assert rows == []


@pytest.mark.asyncio
async def test_cascade_on_school_delete(db_session: AsyncSession) -> None:
    home = await _make_school_for_membership(db_session, "Home C")
    other = await _make_school_for_membership(db_session, "Other C")
    user = await _make_user_for_membership(db_session, "coach3@example.com", home.id)
    db_session.add(UserSchoolMembership(user_id=user.id, school_id=other.id))
    await db_session.flush()

    await db_session.delete(other)
    await db_session.flush()

    rows = (
        (
            await db_session.execute(
                select(UserSchoolMembership).where(UserSchoolMembership.user_id == user.id)
            )
        )
        .scalars()
        .all()
    )
    assert rows == []
