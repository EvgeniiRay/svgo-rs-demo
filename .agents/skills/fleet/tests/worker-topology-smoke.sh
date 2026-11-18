#!/bin/bash
# Guards the fleet worker-launch topology: a fleet worker IS a standalone
# main session in its own tmux window — never a spawned subagent. Asserts
# stable anchor phrases so a future edit dropping the rule goes red;
# kill-verify by deleting the sentence in turn (every deletion must fail).
# Reviewers follow the same rule: a reviewer is a standalone worker, never
# a subagent of the builder it reviews.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../../../.."
SKILL=.agents/skills/fleet/SKILL.md
fail() { echo "FAIL: $1" >&2; exit 1; }

[ -f "$SKILL" ] || fail "skill playbook not found at $SKILL"

NORM=$(tr -s '[:space:]' ' ' < "$SKILL")
anchor() { # $1 = literal phrase, $2 = failure message
  grep -qF -- "$1" <<<"$NORM" || fail "$2"
}

# 1. The positive rule: workers are standalone tmux-window sessions.
anchor 'main session in its own tmux window' \
  "missing main-session worker rule"

# 2. The explicit ban: never a spawned subagent (the shortcut the skill
#    must never leave unnamed, or operators keep taking it).
anchor 'never a spawned subagent' \
  "missing never-a-subagent ban"

# 3. Negative: no line affirmatively launches a worker as a subagent.
BAD=$(grep -niE '(launch|spawn|start|run)[^.]*worker[^.]*as a subagent' "$SKILL" \
  | grep -viE 'never' || true)
[ -z "$BAD" ] || fail "SKILL.md affirmatively launches a worker as a subagent: $BAD"

# 4. Reviewers share the topology: never a subagent of the builder.
anchor 'never a subagent of the builder' \
  "missing reviewer topology rule"

echo "worker-topology-smoke: ok"
