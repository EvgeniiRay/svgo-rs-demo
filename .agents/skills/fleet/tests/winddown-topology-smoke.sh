#!/bin/bash
# Guards SKILL.md operator-lifecycle wording: the wind-down teardown against
# the window/session contradiction — workers are tmux WINDOWS of the
# engagement session (launcher section), so wind-down must tear down the
# engagement SESSION by exact name, never "worker sessions", which do not
# exist — and the resume read list against quiet-vs-broken-wake-source
# blindness: a fresh operator must read wake-source.err, where the
# sub-threshold wake-source check failures live.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../../../.."
SKILL=.agents/skills/fleet/SKILL.md
fail() { echo "FAIL: $1" >&2; exit 1; }

grep -qi 'kill worker sessions\?' "$SKILL" && fail "wind-down still says 'kill worker sessions' but workers are windows"
grep -q 'its own tmux window' "$SKILL" || fail "launcher topology sentence lost (workers must stay windows)"

WD=$(awk '/^[0-9]+\. \*\*Wind-down\.\*\*/,/^## /' "$SKILL")
[ -n "$WD" ] || fail "wind-down step not found in SKILL.md"
grep -q "tmux kill-session -t '=" <<<"$WD" || fail "wind-down must kill the engagement session in exact-match form (tmux kill-session -t '=...')"
grep -q 'worktree-cleanup gates' <<<"$WD" || fail "wind-down lost the worktree-cleanup gates"

RESUME=$(awk '/^## Resume contract/,0' "$SKILL")
[ -n "$RESUME" ] || fail "resume contract not found in SKILL.md"
grep -q 'wake-source\.err' <<<"$RESUME" || fail "resume contract omits wake-source.err (fresh operator cannot tell quiet from broken wake source)"

echo "winddown-topology-smoke: ok"
