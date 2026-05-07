"""Item 12 acceptance: FÖ subject in the demo Grundschule seed has prefer_late_period=5.

Pins the seed value that activates the late-period soft-cost axis for FÖ across
all three Python demo seeds (einzügig, zweizügig, dreizügig) via the shared
`_SUBJECTS` tuple in `demo_grundschule.py`. The seed change reverts the no-op
left in PR #171.

The integration-level signal "FÖ runs late under the production solver" is
verified out of band via the bench's `late_period_ratio_median` column
(`mise run bench:bakeoff -- --budget 5s --seeds 4 --fixtures grundschule,zweizuegig`)
because adding any placement-time soft-cost bias on a tight FFD fixture
intermittently flakes the einzügig per-class greedy (documented in
`solver/CLAUDE.md` under "Adding a placement-time hard constraint can flake
FFD greedy without LAHC"). The bench fixtures use the same value via the
mirrored `prefer_late_period` field on the FOE/FÖ subject in
`solver/solver-core/src/test_fixtures.rs::{zweizuegig_fixture, dreizuegig_fixture}`.
"""

from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession

from klassenzeit_backend.db.models.subject import Subject
from klassenzeit_backend.seed.demo_grundschule import seed_demo_grundschule
from klassenzeit_backend.seed.demo_grundschule_dreizuegig import seed_demo_grundschule_dreizuegig
from klassenzeit_backend.seed.demo_grundschule_zweizuegig import seed_demo_grundschule_zweizuegig


async def test_demo_grundschule_einzuegig_foe_has_prefer_late_period_5(
    db_session: AsyncSession,
) -> None:
    await seed_demo_grundschule(db_session)
    await db_session.flush()

    foe = (await db_session.execute(select(Subject).where(Subject.short_name == "FÖ"))).scalar_one()
    assert foe.prefer_late_period == 5, (
        "einzügig demo seed should set FÖ prefer_late_period=5 (item 12); "
        f"got {foe.prefer_late_period}"
    )


async def test_demo_grundschule_zweizuegig_foe_has_prefer_late_period_5(
    db_session: AsyncSession,
) -> None:
    await seed_demo_grundschule_zweizuegig(db_session)
    await db_session.flush()

    foe = (await db_session.execute(select(Subject).where(Subject.short_name == "FÖ"))).scalar_one()
    assert foe.prefer_late_period == 5, (
        f"zweizügig demo seed should inherit FÖ prefer_late_period=5 from _SUBJECTS (item 12); "
        f"got {foe.prefer_late_period}"
    )


async def test_demo_grundschule_dreizuegig_foe_has_prefer_late_period_5(
    db_session: AsyncSession,
) -> None:
    await seed_demo_grundschule_dreizuegig(db_session)
    await db_session.flush()

    foe = (await db_session.execute(select(Subject).where(Subject.short_name == "FÖ"))).scalar_one()
    assert foe.prefer_late_period == 5, (
        f"dreizügig demo seed should inherit FÖ prefer_late_period=5 from _SUBJECTS (item 12); "
        f"got {foe.prefer_late_period}"
    )
