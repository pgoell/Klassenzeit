# 0025: Per-Subject preference weights

Date: 2026-05-01

## Status

Accepted

## Context

Sprint item 9 (`docs/superpowers/OPEN_THINGS.md`) replaces the three boolean preference flags on `Subject` (`prefer_early_periods`, `avoid_first_period`, `avoid_last_period`) with `u32` weights so a school can express relative strength ("Mathematik more strongly early than Deutsch"), not just on / off. ADR 0017 established the boolean preference axes; ADR 0024 added avoid-last-period as the last boolean axis. Both ADRs anticipated this follow-up.

## Decision

Each per-Subject preference field flips from `bool` to `u32` with `#[serde(default)]` (so callers omitting the field deserialise to `0`). The field `prefer_early_periods` (plural) renames to `prefer_early_period` (singular) to match the matching `ConstraintWeights.prefer_early_period` identifier; `avoid_first_period` and `avoid_last_period` keep their names and only flip type.

Per-placement penalty becomes `subject.<axis> * weights.<axis> * factor` (saturating throughout), where `factor` is `tb.position` for `prefer_early_period` and `1` (gated by position equality) for the two `avoid` axes. The per-axis global multiplier in `ConstraintWeights` stays as the operator-only on / off + global-strength dial; the per-Subject weight is the relative-strength dial exposed in the subject edit dialog.

Pydantic + Zod cap user input to `[0, 10]`. Rust stays free `u32` so a power-user override via env-driven seed is not blocked. Alembic migration backfills existing rows via `CASE WHEN <bool> THEN 1 ELSE 0 END`; downgrade is lossy (any value `>= 1` rounds back to `TRUE`).

## Consequences

Existing schedules' soft scores are byte-identical post-migration because `bool true` becomes `u32 1` and `1 * weights.<axis> = weights.<axis>`. Bench fixtures should not need a `BASELINE.md` refresh; if p50 drifts past 3%, refresh and explain. The frontend dialog gains three small number inputs in place of three checkboxes; one i18n key renames (`preferEarlyPeriods → preferEarlyPeriod`).

Per-class subject preference overrides remain a separate deferral (`school_class_subject_preferences`); per-Subject weights are a stepping stone, not a replacement.
