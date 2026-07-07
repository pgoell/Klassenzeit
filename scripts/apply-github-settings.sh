#!/usr/bin/env bash
# Apply GitHub repo + branch-protection settings from .github/*.json.
#
# Reading branch protection (used by --check and the post-apply verify) requires
# a token with the "Administration" (read) permission. A local `gh auth login`
# with the `repo` scope has it; the Actions GITHUB_TOKEN does not, so the
# drift-check job in .github/workflows/audit.yml supplies a REPO_ADMIN_TOKEN.
set -euo pipefail

cd "$(dirname "$0")/.."

DRY_RUN=0
SKIP_VERIFY=0
CHECK=0

usage() {
  cat <<'EOF'
Usage: scripts/apply-github-settings.sh [--check | --dry-run | --skip-verify] [--help]

Applies .github/repo-settings.json via PATCH /repos/:owner/:repo, then
.github/branch-protection.json via PUT /repos/:owner/:repo/branches/:default/protection.
Reads branch protection back and diffs the normalized result against the source
JSON. Exits non-zero on drift.

Flags (mutually exclusive):
  --check         Read branch protection back and diff against branch-protection.json.
                  Do not apply. Exit 5 on drift, 0 on match.
  --dry-run       Print the gh api commands that would run, do not mutate.
  --skip-verify   Apply settings but skip the readback + drift diff.

  --help          Show this message and exit.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --check) CHECK=1; shift ;;
    --dry-run) DRY_RUN=1; shift ;;
    --skip-verify) SKIP_VERIFY=1; shift ;;
    --help|-h) usage; exit 0 ;;
    --*) echo "unknown flag: $1" >&2; usage >&2; exit 2 ;;
    *)   echo "unexpected positional argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if (( DRY_RUN + SKIP_VERIFY + CHECK > 1 )); then
  echo "--check, --dry-run, and --skip-verify are mutually exclusive" >&2
  usage >&2
  exit 2
fi

# --- Preflight ----------------------------------------------------------------
command -v gh >/dev/null || { echo "gh is required; install https://cli.github.com/" >&2; exit 2; }
command -v jq >/dev/null || { echo "jq is required" >&2; exit 2; }
gh auth status >/dev/null 2>&1 || {
  echo "gh is not authenticated; run 'gh auth login'" >&2
  exit 3
}

# --- Resolve repo -------------------------------------------------------------
REPO_INFO=$(gh repo view --json nameWithOwner,defaultBranchRef \
  --jq '.nameWithOwner + " " + .defaultBranchRef.name' 2>/dev/null) || {
  echo "could not resolve current repo via 'gh repo view'; is this a GitHub repo clone?" >&2
  exit 4
}
OWNER_REPO=${REPO_INFO% *}
DEFAULT_BRANCH=${REPO_INFO##* }

REPO_SETTINGS=.github/repo-settings.json
BRANCH_PROTECTION=.github/branch-protection.json

echo "target: $OWNER_REPO (branch: $DEFAULT_BRANCH)"

# --- Verifier ----------------------------------------------------------------
verify_branch_protection() {
  echo "→ verifying branch protection"
  local actual_raw actual_norm expected_norm
  actual_raw=$(mktemp)
  actual_norm=$(mktemp)
  expected_norm=$(mktemp)
  trap 'rm -f "$actual_raw" "$actual_norm" "$expected_norm"' RETURN

  gh api "/repos/$OWNER_REPO/branches/$DEFAULT_BRANCH/protection" > "$actual_raw"

  local filter=scripts/lib/normalize-branch-protection.jq
  jq -S -f "$filter" "$actual_raw"        > "$actual_norm"
  jq -S -f "$filter" "$BRANCH_PROTECTION" > "$expected_norm"

  if diff -u "$expected_norm" "$actual_norm" >&2; then
    echo "✔ branch protection matches branch-protection.json"
    return 0
  else
    echo "✖ drift detected between branch-protection.json and GitHub" >&2
    return 5
  fi
}

# --- Check-only path ----------------------------------------------------------
if [[ "$CHECK" == "1" ]]; then
  verify_branch_protection
  exit $?
fi

# --- Apply or describe --------------------------------------------------------
if [[ "$DRY_RUN" == "1" ]]; then
  echo "→ would run: gh api --method PATCH /repos/$OWNER_REPO --input $REPO_SETTINGS"
  echo "→ would run: gh api --method PUT  /repos/$OWNER_REPO/branches/$DEFAULT_BRANCH/protection --input $BRANCH_PROTECTION"
  echo "✔ dry run complete, no changes made"
  exit 0
fi

echo "→ applying repo-level settings"
gh api --method PATCH "/repos/$OWNER_REPO" --input "$REPO_SETTINGS" >/dev/null

echo "→ applying branch protection"
gh api --method PUT "/repos/$OWNER_REPO/branches/$DEFAULT_BRANCH/protection" \
  --input "$BRANCH_PROTECTION" >/dev/null

if [[ "$SKIP_VERIFY" == "1" ]]; then
  echo "✔ applied (verify skipped)"
  exit 0
fi

verify_branch_protection
exit $?
