"""Tests for the model re-export surface and model metadata."""

import pytest
from sqlalchemy import Boolean, DateTime, String

from klassenzeit_backend.db.base import Base
from klassenzeit_backend.db.models import User, UserSession
from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID
from klassenzeit_backend.db.models.stundentafel import SchoolType, Stundentafel


def test_user_model_is_registered_on_metadata() -> None:
    assert "users" in Base.metadata.tables


def test_user_has_expected_columns() -> None:
    table = Base.metadata.tables["users"]

    id_col = table.c["id"]
    assert id_col.primary_key

    email_col = table.c["email"]
    assert email_col.unique
    assert isinstance(email_col.type, String)
    assert email_col.type.length == 320

    hash_col = table.c["password_hash"]
    assert isinstance(hash_col.type, String)
    assert hash_col.type.length == 256

    role_col = table.c["role"]
    assert isinstance(role_col.type, String)

    active_col = table.c["is_active"]
    assert isinstance(active_col.type, Boolean)

    force_col = table.c["force_password_change"]
    assert isinstance(force_col.type, Boolean)

    login_col = table.c["last_login_at"]
    assert isinstance(login_col.type, DateTime)
    assert login_col.type.timezone is True
    assert login_col.nullable is True

    created_col = table.c["created_at"]
    assert isinstance(created_col.type, DateTime)
    assert created_col.type.timezone is True
    assert created_col.server_default is not None

    updated_col = table.c["updated_at"]
    assert isinstance(updated_col.type, DateTime)
    assert updated_col.type.timezone is True
    assert updated_col.server_default is not None


def test_user_is_importable_from_models_package() -> None:
    assert User.__tablename__ == "users"


def test_user_session_model_is_registered_on_metadata() -> None:
    assert "sessions" in Base.metadata.tables


def test_user_session_has_expected_columns() -> None:
    table = Base.metadata.tables["sessions"]

    id_col = table.c["id"]
    assert id_col.primary_key

    user_id_col = table.c["user_id"]
    assert user_id_col.nullable is False
    fk_names = [fk.target_fullname for fk in user_id_col.foreign_keys]
    assert "users.id" in fk_names

    created_col = table.c["created_at"]
    assert isinstance(created_col.type, DateTime)
    assert created_col.type.timezone is True

    expires_col = table.c["expires_at"]
    assert isinstance(expires_col.type, DateTime)
    assert expires_col.type.timezone is True


def test_user_session_is_importable_from_models_package() -> None:
    assert UserSession.__tablename__ == "sessions"


@pytest.mark.asyncio
async def test_stundentafel_school_type_round_trip(db_session):
    tafel = Stundentafel(
        name="Gymnasium Klasse 5",
        grade_level=5,
        school_type=SchoolType.GYMNASIUM,
        school_id=DEFAULT_SCHOOL_ID,
    )
    db_session.add(tafel)
    await db_session.flush()
    await db_session.refresh(tafel)
    assert tafel.school_type is SchoolType.GYMNASIUM
    assert isinstance(tafel.school_type, SchoolType)
    assert tafel.school_type.value == "Gymnasium"
