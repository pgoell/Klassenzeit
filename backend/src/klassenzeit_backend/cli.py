"""CLI entry point for the Klassenzeit backend.

Provides the ``create-admin`` command for bootstrapping the first admin user,
and the ``cleanup-sessions`` command for removing expired sessions.
"""

import asyncio

import typer
from sqlalchemy import select
from sqlalchemy.exc import IntegrityError
from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker, create_async_engine

from klassenzeit_backend.auth.passwords import (
    PasswordValidationError,
    hash_password,
    validate_password,
)
from klassenzeit_backend.auth.sessions import cleanup_expired_sessions
from klassenzeit_backend.core.settings import get_settings
from klassenzeit_backend.db.models.school import DEFAULT_SCHOOL_ID
from klassenzeit_backend.db.models.user import User
from klassenzeit_backend.seed.demo_grundschule import seed_demo_grundschule
from klassenzeit_backend.seed.demo_grundschule_dreizuegig import (
    seed_demo_grundschule_dreizuegig,
)
from klassenzeit_backend.seed.demo_grundschule_zweizuegig import (
    seed_demo_grundschule_zweizuegig,
)

cli = typer.Typer(no_args_is_help=True)

E2E_ADMIN_EMAIL = "admin@example.com"
E2E_ADMIN_PASSWORD = "test-password-12345"  # noqa: S105


class DuplicateEmailError(ValueError):
    """Raised when a user with the given email already exists."""


async def create_admin_in_db(
    db: AsyncSession,
    email: str,
    password: str,
    *,
    min_password_length: int = 12,
) -> User:
    """Create an admin user in the database.

    Validates the password and checks for duplicate emails.
    Does NOT commit — caller must commit or the test fixture rolls back.

    Raises ``DuplicateEmailError`` on duplicate email, ``ValueError`` on other
    validation failures.
    """
    try:
        validate_password(password, min_length=min_password_length)
    except PasswordValidationError as exc:
        raise ValueError(str(exc)) from exc

    email = email.lower()
    result = await db.execute(select(User).where(User.email == email))
    if result.scalar_one_or_none() is not None:
        msg = f"User with email {email} already exists"
        raise DuplicateEmailError(msg)

    user = User(
        email=email,
        password_hash=hash_password(password),
        role="admin",
        school_id=DEFAULT_SCHOOL_ID,
    )
    db.add(user)
    await db.flush()
    return user


async def _run_create_admin(email: str, password: str) -> None:
    settings = get_settings()
    engine = create_async_engine(str(settings.database_url))
    factory = async_sessionmaker(engine, expire_on_commit=False)

    try:
        async with factory() as session:
            await create_admin_in_db(
                session,
                email,
                password,
                min_password_length=settings.password_min_length,
            )
            await session.commit()
    finally:
        await engine.dispose()


@cli.command()
def create_admin(
    email: str = typer.Option(..., "--email", help="Admin email address"),
) -> None:
    """Create an admin user. Prompts for password on stdin."""
    password = typer.prompt("Password", hide_input=True, confirmation_prompt=True)

    try:
        asyncio.run(_run_create_admin(email, password))
    except ValueError as exc:
        typer.echo(f"Error: {exc}", err=True)
        raise typer.Exit(code=1) from exc

    typer.echo(f"Admin user created: {email}")


async def _run_cleanup_sessions() -> int:
    settings = get_settings()
    engine = create_async_engine(str(settings.database_url))
    factory = async_sessionmaker(engine, expire_on_commit=False)
    try:
        async with factory() as session:
            count = await cleanup_expired_sessions(session)
            await session.commit()
            return count
    finally:
        await engine.dispose()


@cli.command()
def cleanup_sessions() -> None:
    """Delete expired sessions from the database."""
    count = asyncio.run(_run_cleanup_sessions())
    typer.echo(f"Deleted {count} expired session(s)")


@cli.command()
def seed_e2e_admin() -> None:
    """Idempotently seed the fixed e2e admin user.

    Intended to be called by ``mise run e2e`` before Playwright starts.
    No-op if the user already exists (by email).
    Only runs when ``KZ_ENV=test``.
    """
    settings = get_settings()
    if settings.env != "test":
        typer.echo("seed-e2e-admin is only allowed when KZ_ENV=test", err=True)
        raise typer.Exit(code=1)
    try:
        asyncio.run(_run_create_admin(E2E_ADMIN_EMAIL, E2E_ADMIN_PASSWORD))
    except DuplicateEmailError:
        typer.echo(f"Admin user already present: {E2E_ADMIN_EMAIL}")
        return
    except ValueError as exc:
        typer.echo(f"Error: {exc}", err=True)
        raise typer.Exit(code=1) from exc
    typer.echo(f"Admin user created: {E2E_ADMIN_EMAIL}")


async def _run_seed_grundschule() -> None:
    settings = get_settings()
    engine = create_async_engine(str(settings.database_url))
    factory = async_sessionmaker(engine, expire_on_commit=False)
    try:
        async with factory() as session:
            await seed_demo_grundschule(session)
            await session.commit()
    finally:
        await engine.dispose()


@cli.command()
def seed_grundschule() -> None:
    """Seed a demo Hessen Grundschule (4 classes, 6 teachers, 7 rooms).

    Refuses to run when ``KZ_ENV=prod``. On unique-name conflicts the seed
    aborts atomically and prints a reset hint.
    """
    settings = get_settings()
    if settings.env == "prod":
        typer.echo("seed-grundschule is disabled in production", err=True)
        raise typer.Exit(code=1)
    try:
        asyncio.run(_run_seed_grundschule())
    except IntegrityError as exc:
        typer.echo(
            f"Seed aborted (integrity error): {exc.orig}\n"
            "The database already contains conflicting rows. "
            "Reset with `mise run db:reset` (dev) or the /__test__/reset "
            "endpoint (test) and try again.",
            err=True,
        )
        raise typer.Exit(code=1) from exc
    typer.echo("Grundschule demo seeded successfully.")


async def _run_seed_grundschule_zweizuegig() -> None:
    settings = get_settings()
    engine = create_async_engine(str(settings.database_url))
    factory = async_sessionmaker(engine, expire_on_commit=False)
    try:
        async with factory() as session:
            await seed_demo_grundschule_zweizuegig(session)
            await session.commit()
    finally:
        await engine.dispose()


@cli.command()
def seed_grundschule_zweizuegig() -> None:
    """Seed a zweizuegige Hessen Grundschule (8 classes, 12 teachers, 12 rooms).

    Refuses to run when ``KZ_ENV=prod``. On unique-name conflicts the seed
    aborts atomically and prints a reset hint.
    """
    settings = get_settings()
    if settings.env == "prod":
        typer.echo("seed-grundschule-zweizuegig is disabled in production", err=True)
        raise typer.Exit(code=1)
    try:
        asyncio.run(_run_seed_grundschule_zweizuegig())
    except IntegrityError as exc:
        typer.echo(
            f"Seed aborted (integrity error): {exc.orig}\n"
            "The database already contains conflicting rows. "
            "Reset with `mise run db:reset` (dev) or the /__test__/reset "
            "endpoint (test) and try again.",
            err=True,
        )
        raise typer.Exit(code=1) from exc
    typer.echo("Grundschule (zweizuegig) demo seeded successfully.")


async def _run_seed_grundschule_dreizuegig() -> None:
    settings = get_settings()
    engine = create_async_engine(str(settings.database_url))
    factory = async_sessionmaker(engine, expire_on_commit=False)
    try:
        async with factory() as session:
            await seed_demo_grundschule_dreizuegig(session)
            await session.commit()
    finally:
        await engine.dispose()


@cli.command()
def seed_grundschule_dreizuegig() -> None:
    """Seed a dreizuegige Hessen Grundschule (12 classes, 18 teachers, 16 rooms).

    Refuses to run when ``KZ_ENV=prod``. On unique-name conflicts the seed
    aborts atomically and prints a reset hint.
    """
    settings = get_settings()
    if settings.env == "prod":
        typer.echo("seed-grundschule-dreizuegig is disabled in production", err=True)
        raise typer.Exit(code=1)
    try:
        asyncio.run(_run_seed_grundschule_dreizuegig())
    except IntegrityError as exc:
        typer.echo(
            f"Seed aborted (integrity error): {exc.orig}\n"
            "The database already contains conflicting rows. "
            "Reset with `mise run db:reset` (dev) or the /__test__/reset "
            "endpoint (test) and try again.",
            err=True,
        )
        raise typer.Exit(code=1) from exc
    typer.echo("Grundschule (dreizuegig) demo seeded successfully.")


def main() -> None:
    """Entry point for the ``klassenzeit-backend`` CLI."""
    cli()
