---
name: coordinator
description: "Demo-project coordinator setup and close: `/coordinator` prepares this session as the coordinator for the svgo-rs coordinator demo in this project and launches the fleet view in a Herdr pane on the right; `/coordinator close` winds the engagement down and closes the view pane. It reads the fleet playbook, the Herdr backend mapping, and the demo plan, checks the Herdr stack, checkout, and launcher, prepares the engagement record, and opens the live view. It launches no workers itself; lane setup is automatic when work is entered later in chat (see "Automatic spin-up" below). Takes no task argument and runs no other project's setup. Use only when the user invokes /coordinator in this demo project."
---

# Coordinator preparation (demo project only)

Scope: this skill is project-scoped to the svgo-rs demo in this
workspace, and it does two things: prepare this session as the
coordinator, and launch the fleet view in a Herdr pane on the
right. It takes no arguments and splits no task. Text after
`/coordinator` is not work: do not treat it as a task, and do not
launch workers from it. For any other task or any other project,
do not use this skill — use the `fleet` skill instead. Do not
install, copy, or reuse this skill outside this project.

`/coordinator` launches no workers. No worktree, branch, Herdr
workspace, or agent is created by this invocation, and no
`fleet-herdr` verb that starts, steers, reads, or checks a worker
runs. The one pane it creates is the fleet-view pane on the
right. Work is entered later, in chat; lane setup happens only
then, under "When work arrives" below.

The one other invocation is `/coordinator close` — the exact
word `close` after `/coordinator`. It runs only the close flow
under "Close" below: wind down, then close the view pane. Any
other text after `/coordinator` is not work and not a command:
ignore it and run the prepare-plus-view setup above.

You are the coordinator once prepared. You supervise, steer, and
wind down after work arrives. Only you judge what a worker
produced.

## Automatic spin-up

Every work-bearing message in this chat automatically spins up
a worker, with no separate launch request from the human and no
manual prompting step. Work-bearing means the message names a
deliverable or an action: a task, a change, an investigation,
a build, a fix, or the demo task. The coordinator then, in this
order: splits the message's work into single-owner lanes;
launches a worker per lane with its instructions carried in
the launch invocation itself: the prompts from the message,
forwarded verbatim as the shared `--prompt`, as
`--lane-prompts-json` built from the message's per-lane
prompts, or as a per-lane `--prompt` on `add-worker`. No
prompts are stored anywhere; the coordinator only delegates
what the human sent. It then verifies with `fleet-herdr
status` that each lane is listed with a pane, agent, worktree,
and branch and is `working` or `idle`; and reports the lane,
its pane id, and that it appears in the fleet view on the next
refresh (within 30 seconds — never claim it is already visible
before a refresh shows it). Steering afterwards is only for
follow-ups and corrections, never for the initial assignment.

Do not spin up a worker for a message that is not work:
`/coordinator close`, questions about this coordinator, the
fleet view, or a skill itself, status-only requests, greetings,
or corrections to the coordinator's own behaviour (implement
the correction; do not spawn a worker for it). When a message
continues an existing lane's work, steer that lane instead of
spinning up a new worker. When a message holds several distinct
lanes, spin up one worker per lane. Never spin up for the
coordinator's own checks, and never launch work the
human has not given: a question with no deliverable gets an
answer, not a worker.

## Prepare the coordinator

1. **Read.** Read the fleet playbook and the backend mapping for
   the stack you are on, and the demo plan:

   - `../fleet/SKILL.md` — ceremony, stop conditions, recovery,
     wind-down.
   - `../fleet/reference/herdr-backend.md` — Herdr setup, when
     `HERDR_ENV=1` (old Herdr pane) or `HC_TERMINAL_ID` is set (hc
     terminal: use `../fleet/reference/herdr-core-backend.md`
     instead).
   - `../../../docs/coordinator-demo.md` — the demo task, lanes,
     and stop condition.

   Reading only: do not launch, steer, or check any worker while
   reading.
2. **Identify the stack.** This demo runs in Herdr. If neither
   Herdr signal is present, stop and report that the demo needs
   Herdr; do not fall back to tmux, and do not launch anything.
3. **Check.** From this repo's root
   (`git rev-parse --show-toplevel`): confirm the base checkout
   is clean (`git status --porcelain` is empty) and the vendored
   launcher exists and is executable
   (`../fleet/herdr/fleet-herdr`). If the checkout is dirty or
   the launcher is missing, stop and report it; launch nothing.
   Do not run any `fleet-herdr` verb during preparation.
4. **Prepare the record.** The state dir is
   `${FLEET_HERDR_STATE_ROOT:-/tmp}/fleet-demo` — under Git Bash
   on Windows, `/tmp` is Git Bash's temp dir, which is also where
   `fleet-herdr` keeps its state by default. Create it
   (`mkdir -p` that dir) and write the engagement record shell
   (`engagement.md` inside it): the demo task, the fixed lane
   table below with every lane marked "not launched" (no agent,
   worktree, or branch yet), the stop condition, and an empty
   decisions log. This file is preparation only: it creates no
   engagement state in Herdr and no `herdr.json`.

## Windows (Git Bash only)

On Windows, everything in this skill runs under Git Bash: the
Herdr pane, the coordinator session, and `fleet-herdr` itself.
Never run it from PowerShell or cmd, and never launch a worker
from a different shell than the coordinator's. During
preparation, in addition to the checks above, confirm:

- Git Bash is the shell (`uname -s` prints `MINGW*`, `MSYS*`,
  or `CYGWIN*`); Herdr for Windows, git, and jq are installed.
- `jq --version` is 1.7 or newer. An older jq, or one without
  `-b`, writes CRLF that corrupts pane ids and records;
  `fleet-herdr` refuses it on Windows with the install command
  (`winget install jqlang.jq`) — report that refusal, do not
  work around it.
- `fleet-herdr` has LF line endings (a CRLF checkout breaks
  bash before it can run). If it does not, re-check out the
  skill with `core.autocrlf=false`; do not edit line endings by
  hand in the vendored script.
- The model ladder is never needed here: lane setup always
  passes `--kind` explicitly, so no `timeout(1)` probe runs.
  If a ladder run is ever requested, it needs coreutils
  `timeout` (Git Bash coreutils) or `FLEET_HERDR_TIMEOUT_CMD`.

Paths in commands use forward slashes. `fleet-herdr` converts
a Git Bash repo path to the `C:/...` form both bash and
`herdr.exe` open; never pass a backslash path or a PowerShell
drive path to it. The fleet view runs with `python3` in the Git
Bash pane; ensure Python 3 is on PATH there.
5. **Launch the fleet view on the right.** Open the live view
   in a Herdr pane on the right of the current pane, following
   `../fleet-view/SKILL.md` ("Side panel in Herdr"). First check
   `herdr pane list` for a pane already named `fleet-view` and
   reuse it if one exists; otherwise split the current pane to
   the right (`--direction right --ratio 0.42 --no-focus --cwd`
   the repo root), rename the new pane `fleet-view`, and run the
   view's `--watch 30` loop in it with `--coordinator-pane
   $HERDR_PANE_ID`, so the coordinator's latest message shows
   under Coordinator. Keep focus in the coordinator pane.
   Confirm the view pane exists and its visible output shows
   the `FLEET` header before reporting; if it does not draw,
   read the pane and report the error instead of claiming the
   view is running. The view shows whatever lanes Herdr has
   now — with no work entered yet, the demo engagement is
   empty, which is a valid frame.
6. **Report readiness.** End with one short report: what was
   read, what was checked and passed, the engagement name
   (`demo`), the fixed lanes and stop condition, the fleet-view
   pane id, and the one line: "Coordinator ready. Fleet view is
   running on the right. No workers launched. Enter work in
   chat." Name the wind-down command
   (`fleet-herdr wind-down --engagement demo`) for later; do not
   run it.

## Demo lanes (fixed, not launched by this skill)

Engagement `demo`, base `main`, kind = your engine, in steer
order once work arrives:

1. **bugfix** — fix the `convertColors` short-hex bug: `#aabbcd`
   must not become `#abc`. Add a regression test first.
2. **perf** — run the corpus parity check and benchmark gate;
   report timings, change nothing unless a regression is proven.
3. **docs** — document the behaviour the bugfix lane lands, only
   after the fix is verified.
4. **reviewer** — starts after bugfix reports done; reviews the
   bugfix branch from its own worktree.

Stop condition: regression test green, corpus byte-identical to
SVGO, docs updated, reviewer agrees.

## When work arrives (later, in chat)

Work is a later chat message, not the `/coordinator` invocation.
Only when it arrives, and only for the demo task above, do the
lane setup (see "Automatic spin-up" above), from Git Bash on
Windows (see above): run the vendored `fleet-herdr up` with
`--engagement demo --workers 3 --repo "$PWD" --base main
--kind <your engine>` and the prompts from the chat message
carried in the launch (`--lane-prompts-json` built from the
message's per-lane prompts, or a shared `--prompt`), so each
lane's instructions ride with the launch (the worker works
only its lane, and reports `STATUS:` lines and
`BLOCKED(HUMAN):` questions; no steering is needed for the
initial assignment, and no prompts are stored anywhere);
verify
with `fleet-herdr status` that every lane is listed with a
pane, agent, worktree, and branch, and is `working` or `idle`;
roll back and report failure if a lane is missing, never
reporting setup as done with a lane missing. Start the reviewer only
after the bugfix lane reports done, with
`fleet-herdr add-worker`; it reads the bugfix branch from its
own worktree, re-runs the lane's tests, and reports a verdict.
It never edits the builder's worktree and never merges.

After launch, supervise per the fleet playbook. Do not merge,
answer an approval dialog, or restart a lane to clear an
infrastructure error. A lane blocked at a dialog is read and
escalated, never auto-answered.

## Test-gate decision

When a test run completes — a lane's `cargo test`, the parity
check, or the reviewer's re-run — a decision must come up in
the fleet view before anything proceeds past it. The
coordinator writes it to the decisions file
(`~/.fleet/decisions.json`, or the config's `decisions_file`)
as one entry in the file's JSON list, preserving every other
entry and writing atomically (temp file beside it, then
rename). The entry states the actual result from the test
output — pass/fail counts, never invented numbers — in this
shape:

```json
{"q": "cargo test: 283 passed, 0 failed — accept this test run and proceed to review?",
 "options": ["Accept and proceed to review", "Re-run the tests", "Stop the demo"],
 "recommend": "Accept and proceed to review", "lane": "demo"}
```

If an open entry with the same `q` and `lane` already exists,
write nothing: one decision per test run, never a duplicate.
If the tests failed, the question says so and the options are
fix-and-rerun or stop — never accept. The owner answers in the
view (`d`, then a digit and a letter); the coordinator never
answers its own decision, and review does not start until the
decision is answered or removed.

## Close (`/coordinator close`)

Closes the engagement and the view. Run every command from this
repo's root, under Git Bash on Windows. The coordinator session
itself stays open; only the workers and the view pane close.
Order matters: wind down first, so the gates decide what may
close, then close the view.

1. **Wind down.** Run `fleet-herdr wind-down --engagement demo`.
   Its gates decide: every worktree clean AND the branch merged
   to the base or to `origin/main`, or with zero unique commits.
   A refusal names its violators and closes nothing — stop
   there, report the violators, close neither workers nor view,
   and resolve by hand. With no record, wind-down reports
   nothing to wind down and succeeds; proceed to the view.
   "Killing" the workers means this step: wind-down closes
   their worktree workspaces, which ends their pane processes,
   after the gates pass. No signal is sent, no remote branch is
   deleted, and no dirty or unmerged work is discarded.
2. **Close the view pane.** Find its id from the pane list,
   selecting the pane whose label is `fleet-view`:

   ```sh
   herdr pane list --workspace "$HERDR_WORKSPACE_ID" \
     | jq -r '.result.panes[] | select(.label == "fleet-view") | .pane_id'
   ```

   If no pane matches, the view is already closed: report that
   and stop. Otherwise close exactly that pane:

   ```sh
   herdr pane close <pane-id>
   ```

   Never close a pane by position, title guess, or any other
   label, never close the coordinator's own pane, and never
   close another pane. If the close fails, report the pane id
   and the error; do not retry it blindly.
3. **Verify and report.** Check `herdr pane list` shows no
   `fleet-view` pane, and `fleet-herdr status --engagement demo`
   shows no live workers (or reports no record). Report: the
   engagement wound down (or there was nothing to wind down),
   the view pane id that was closed, and that the coordinator
   session stays open. The engagement record
   (`/tmp/fleet-demo/engagement.md`) and `herdr.json` are kept;
   close deletes neither.

If wind-down refuses and the human explicitly says to force
close anyway, confirm back first, naming the preserved work
from the refusal (dirty worktrees, unmerged branches), and
proceed only on their confirmation: close each worker pane by
its recorded pane id with `herdr pane close`, leave every
worktree, branch, and record in place for recovery, then close
the view as above. Force is never the default.
