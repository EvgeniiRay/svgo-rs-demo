# Fleet on herdr-core (hc)

This backend runs a fleet engagement on herdr-core instead of the
tmux backend: one hc terminal per worker, each in its own worktree.
The playbook (`../SKILL.md`) still owns ceremony, stop conditions,
recovery judgment, and wind-down gates; this doc maps each mechanical
step to herdr-core. `hc` owns cores, machines, and terminals. The
installed hc manuals are the authority for command syntax: defer to
`hc --skill` and the selected subcommand's `--help`, behind the
core-socket guard below. This doc does not copy manual text into the
repository, and it carries no tracker, messaging, or host-specific
integration.

## Identify the stack first

Before any hc command, identify the stack from the environment:

- A nonempty `HC_TERMINAL_ID` means this is an hc terminal. Take
  identity from `HC_TERMINAL_ID`, never from stale `HERDR_*` values.
- With no nonempty `HC_TERMINAL_ID` and `HERDR_ENV=1`, this is an
  old Herdr pane: use `herdr-backend.md`, not this doc.
- With neither signal, do not run Herdr or hc terminal commands;
  the tmux playbook applies on its own.

Old-stack ids keep their old-stack addresses. Never translate an
old-stack id into an hc terminal id, or the reverse, from position,
label similarity, or stale environment values.

## Core-socket guard

Any hc command can start a core when none is running. Before every
hc command, prove a core already exists without running hc:

1. If `HC_SOCKET` is set, that exact path must be a socket.
2. Otherwise let the session be `HC_SESSION` or `default`, and check
   the one location this environment selects:
   `$XDG_RUNTIME_DIR/herdr-core/<session>.sock` when that variable is
   set, else `/run/user/<uid>/herdr-core/<session>.sock` on Linux,
   else `~/.local/state/herdr-core/<session>/<session>.sock` on
   macOS. A socket somewhere this environment would not look proves
   nothing.
3. If no socket exists, treat hc as absent on that machine. Never
   start or stop a core, and never stop an old Herdr server to make
   room for one. Workers already running are bounded by the stop
   condition until a core is present and a session adopts them.

## Identity and addressing

- Re-list before acting: guarded `hc ls --json`, then `hc get` for
  the target. Recheck terminal id, name, cwd, foreground program,
  status, and parent immediately before sending text or closing
  anything.
- Address a remote terminal from the hub as
  `<machine>/<terminal>`. A per-machine terminal id alone is not a
  hub address. On the target machine itself, use its plain local
  terminal id.
- Read before messaging. A fresh listing alone does not prove the
  intended session is still at an address.
- Never type into, close, or force a terminal whose agent is working
  or blocked on a human unless the owner names that exact terminal.
- Never move or close a daemon-owned lane: a lane whose process
  environment marks it as daemon-owned (on this stack,
  `MUSE_LANE_BACKEND`) belongs to its daemon registry, not to
  manual stack migration.

## Step mapping

1. **Intake.** Split the task into single-owner lanes, decide worker
   count per lane, and declare the stop/wind-down condition up
   front. Record it in the engagement record (goal engagements) or
   the state-dir `stop-condition` file (dispatch tasks).
2. **Engagement record.** One local record in the state dir
   (`engagement.md`): task, lane table (lane → builder → reviewer →
   worktree/branch → terminal id and name → session id and launch
   environment), decisions log. It is the durable state.
3. **Human channel.** Agree one channel with the human and note it in
   the record. Humans answer decisions there; never block a lane on
   an answer a worker can prep around.
4. **Workers.** Fleet lanes that must outlive their starter start
   detached with `hc run --bg --no-parent`, then verify `hc get`
   reports no parent. One terminal per worker, one worktree per
   worker, the lane prompt as its first input, the same engine the
   operator runs as. For distinct prompts per lane, steer each lane
   with its own prompt after start, per the installed manual, or run
   one engagement per lane. Record each worker's terminal id and
   name, worktree and branch, session id, and launch environment.
   Stop conditions are end-to-end by default. A worker blocked at
   an approval or question dialog is never auto-answered: read it
   per the installed manual, then answer or relay to the human.
5. **Watcher.** There is none: terminal lifecycle supersedes
   transcript staleness. On each tick, re-list and read status with
   `hc get` for each lane. A lane needing human input is the
   staleness wake's heir: read it, then steer or escalate. No pid
   files exist, so teardown closes only identifiers recorded in
   the engagement record.
6. **Tick.** Same rhythms, hc reads: re-list, `hc get` on blocked
   or otherwise unclear lanes, steer per the installed manual,
   engagement record and state updates. Durable supervision (armed
   schedule plus stop condition) is unchanged. Pressure cleanup
   (playbook step 7) runs unchanged: ask the human before deleting
   anything.
7. **Wind-down.** Verify each lane finished at its declared end
   state and its claim in the engagement record is released, cancel
   the tick schedule, then enforce the gates — every worktree clean
   AND its branch merged to the base or with zero unique commits —
   before closing each worker's terminal by its recorded id and
   verifying its process exited, then removing its worktree and
   branch. Close only terminals the engagement created, unless the
   human explicitly asks otherwise. A missing record is nothing to
   wind down. A refusal names its violators and closes nothing;
   resolve by hand.

## Reviewer workers

A reviewer is an ordinary worker on this backend: its own
terminal, its own worktree, its own reads. It reads the builder's
branch or diff from its own checkout, re-runs the lane's tests and
parity check there, and reports its verdict to the operator. It
never edits the builder's worktree and never merges. The review loop
in the playbook (request changes, then re-review only the changed
files, escalate after two rounds without agreement) is operated
with hc reads and steering, like any other worker intervention.
A reviewer counts toward the worker count, exactly like a builder.
A lane with no reviewer is recorded as unreviewed; it is never
silently treated as reviewed.

## Adding a worker mid-engagement

Update the engagement-record lane table first, start a detached
lane per step 4, record its terminal id and name, then re-run the
coverage check: every live worker's terminal must appear in the
guarded listing and in the record. Fix the record before the next
tick.

## Moving a session

Moving a session is a recreate-and-resume, never a live transfer:
observers first, idle lanes one at a time, record session id and
launch environment, close and verify the old process exited, then
resume the same session without stale `HERDR_*` values.

## Resume contract

The engagement record is the durable state; hc is the live
registry. A fresh operator adopts via: the engagement record,
guarded `hc ls --json` plus `hc get` for each lane, the state dir
(`stop-condition`, `tick-schedule`), and each lane's record state.
Re-steer stalled lanes, re-arm the schedule, continue ticks.
Anything not in the engagement record is presumed lost. If no core
socket exists, hc is absent: do not start a core; workers run
unsupervised, bounded by the step-1 stop condition, until a core
is present and a session adopts them.
