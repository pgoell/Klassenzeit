---
title: 'CI: GitHub repo settings drift detected'
labels:
  - ci-audit
  - bug
assignees:
  - pgoell
---
<!--
  DO NOT edit the title in this file or in .github/workflows/audit.yml.
  The title is the dedup key for `JasonEtco/create-an-issue@v2` with
  `update_existing: true`. Changing it would open duplicate issues on
  the next failure and orphan the existing one.
-->

The scheduled GitHub repo settings drift-check ([`.github/workflows/audit.yml`](../../.github/workflows/audit.yml), `drift-check` job) failed.

- **Latest failing run:** [#{{ env.RUN_NUMBER }} attempt {{ env.RUN_ATTEMPT }}]({{ env.RUN_URL }})
- **Trigger:** `{{ env.TRIGGER_EVENT }}`

## What happened

`bash scripts/apply-github-settings.sh --check` returned exit code 5: the live branch-protection settings on `master` no longer match `.github/branch-protection.json`. The job's "Check repo settings drift" step prints a unified diff between the two normalized documents.

## What to do

1. Open the run log linked above and scroll to the `Check repo settings drift` step. The unified diff identifies which fields changed.
2. Decide which side is the source of truth:
   - **The JSON is right** (someone toggled a setting in the GitHub UI by mistake): run `mise run repo:apply-settings` locally to push the JSON back to GitHub.
   - **The live setting is right** (the JSON is stale, e.g. a workflow rename made a required-check name obsolete): edit `.github/branch-protection.json` to match the live setting, push the change, and merge.
3. Re-trigger this workflow with `gh workflow run audit.yml`. If the re-run is green, close this issue.

## History

Comments on this issue are appended by the workflow every time the scheduled run fails. The issue body is overwritten on each run to reflect the latest failing run; earlier failures live in the comment stream below.
