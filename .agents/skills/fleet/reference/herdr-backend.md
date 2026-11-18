# Fleet on Herdr (backend mapping)

This backend runs a fleet engagement on Herdr instead of tmux: one
Herdr workspace supervises the engagement, while each worker agent
pane lives in its linked worktree workspace. Selecting a worktree
item in Herdr therefore shows that worker's transcript rather than a
bare shell. Herdr is the fleet view: the supervision workspace, the
worktree workspaces, and the worker lifecycle states are what a
person watches. The playbook (`../SKILL.md`) still owns ceremony, stop
conditions, recovery judgment, and wind-down gates; this doc maps
each mechanical step to Herdr verbs, operated through the
`fleet-herdr` command. The `fleet-herdr` command is vendored in
this kit (`../herdr/fleet-herdr`) and this kit does not depend on
any other service. Engine names map directly to worker kinds (`claude`,
`codex`, `muse`): pass `--kind` explicitly for the engine the
engagement uses (for example `muse` for local Muse Code) so no model
probing happens. Without `--kind`, `up` probes `claude` then `codex`
on host defaults, which needs `timeout(1)`. Start defaults are
`--yolo` for `muse` and `--dangerously-skip-permissions` for
`claude`; no network or sandbox default is added, so pass one after
`--` when the engine's launcher needs it. `--machine <label>` runs a
verb on a saved Herdr machine. The command refuses to run outside
Herdr and passes `--no-focus` on every layout call.

Names: an engagement name matches `^[a-z][a-z0-9-]{0,19}$` and a lane
name matches `^[a-z][a-z0-9-]{0,10}$`, so an agent name
`<engagement>-<lane>` fits the 32-character Herdr limit. Lane names in
this kit are short slugs (for example `perf`, `bugfix`, `newattr`).

## Windows (Git Bash only)

On Windows, run `fleet-herdr` under Git Bash, never PowerShell or
cmd. The script detects the host (`MINGW*`/`MSYS*`/`CYGWIN*`),
turns MSYS argument conversion off for the native tools it drives,
converts repo paths to the `C:/...` form bash and `herdr.exe` both
open, puts `TMPDIR` in that form, reads Herdr replies through
`jq -b` so no `\r` reaches a pane id or the record, retries a
not-yet-ready pane shell 30 times instead of 6, and allows the
model ladder 300 seconds per probe instead of 90. Prerequisites
on Windows: Herdr for Windows, git, jq 1.7 or newer
(`winget install jqlang.jq` — an older jq without `-b` is refused
rather than trusted), Git Bash coreutils `timeout` for ladder
runs (pass `--kind` explicitly to skip the ladder entirely), and
LF line endings in the vendored script (re-check out with
`core.autocrlf=false` if a CRLF checkout breaks it). State lives
under `$FLEET_HERDR_STATE_ROOT` when set, else `/tmp` — the Git
Bash temp dir on Windows. Paths in commands use forward slashes;
never pass a backslash path.

## Startup and cleanup

Initial prompts wait until every worker and its record exist. On
startup failure, cleanup keeps worktrees that are dirty, have new
commits, or changed branch. It names any workspace it could not
close. Shutdown checks each worker's current HEAD against the base
and rechecks it after closing sessions. Preserved work is reported
for recovery; it is not discarded by retry.

## Human and operator responsibilities

The human starts with `fleet <task>`. A complete request skips any
interview; missing choices are settled before dispatch, one question
at a time. The fleet operator runs `fleet-herdr up`, creates every
worktree and agent, submits the worker prompts, monitors them, and
winds the engagement down. The human does not need to run worktree,
agent, status, or workspace commands. The human may open the Herdr
interface to watch the work, but that is optional. Commands in this
guide are operator instructions. Present them to the human only when
the human asks to take manual control or when operator recovery
requires a human-run command.

## Step mapping

1. **Intake.** Split the task into single-owner lanes, decide worker
   count per lane, and declare the stop/wind-down condition up
   front. Record it in the engagement record (goal engagements) or
   the state-dir `stop-condition` file (dispatch tasks — create the
   state dir when you record it). A worker count that includes
   reviewers: reviewers count toward the total, exactly like
   builders.
2. **Engagement record.** One local record in the state dir
   (`engagement.md`): task, lane table (lane → builder → reviewer →
   worktree/branch → agent name), decisions log. It is the durable
   state. If the project uses an issue tracker and the human asks
   for a mirror there, copy the record there too; the local record
   stays the authority.
3. **Human channel.** Agree one channel with the human and note it in
   the record. Herdr adds no messaging requirement.
4. **Workers.** `fleet-herdr up --engagement <name> --workers N
   --repo <repo> --base <ref> --kind <kind> [--cwd-rel <rel>]
   [--env KEY=VALUE]... [--prompt ...] [-- <agent-args...>]`.
   This creates the supervision workspace (`fleet-<name>`) and one
   git worktree per worker (`<repo>/.worktrees/<name>-w<i>`,
   branch `<name>-w<i>`). Herdr opens each worktree as its own
   workspace with a bare shell; `fleet-herdr` starts the agent in
   a staging pane, moves that pane into the matching worktree
   workspace, closes the bare shell, and then submits the prompt.
   The fleet carries its instructions: `--lane-prompts
   <file>` (a JSON object mapping lane names to prompt text)
   sends each lane its own prompt once every worker exists, so
   no steering is needed for the initial assignment; a lane
   with no entry gets the shared `--prompt`/`--prompt-file`
   text. Steering afterwards is only for follow-ups and
   corrections.
   `-- <agent-args...>` passes arguments through to every agent
   start. A Herdr pane gets the Herdr server's environment, not
   the operator shell's, so `--env KEY=VALUE` (repeatable) is how
   a worker gets a variable: it goes to the staging pane's shell,
   and the agent and every restart in that pane inherit it. The
   engagement record (`/tmp/fleet-<name>/herdr.json`) holds the
   supervision workspace and tab identifiers, the kinds list,
   default agent arguments and environment, plus each worker's
   moved pane identifier, agent name, worktree, branch, kind,
   resolved arguments, and environment. Transcript paths are
   unnecessary on this backend — reads go through agent reads.
   A failed `up` or `add-worker` rolls back both sides of what
   that run created: its workspaces and any unmoved staging pane,
   then its worktrees and branches, and — for `up` — the record,
   when cleanup was safe; preserved work keeps its record for
   recovery.
5. **Watcher.** There is none: agent lifecycle supersedes transcript
   staleness. `fleet-herdr status` maps each worker to
   `idle|working|blocked|done|unknown` (from agent state);
   `fleet-herdr wait` blocks to the first match. A `blocked`
   worker is the staleness wake's heir: read it (`fleet-herdr
   read`), then steer or escalate — never auto-answer an approval
   dialog. `read` defaults to the recent unwrapped output, 120
   lines. No pid files exist, so the validated kill from the
   playbook has no Herdr form; teardown closes only identifiers
   recorded in `herdr.json`.
6. **Tick.** Same rhythms, Herdr reads: `status`, then `read` on
   blocked or unknown workers, `steer` to intervene, engagement
   record and state updates. `steer --wait` submits through the
   agent surface and waits for the turn to settle — it replaces
   the tmux send sequence outright; the delivery receipt is the
   `steer` wait resolving plus the turn visible in `read`.
   Pressure cleanup (playbook step 7) runs unchanged: ask the
   human before deleting anything. Durable supervision (armed
   schedule plus stop condition) is unchanged — Herdr keeps no
   scheduler of its own.
7. **Wind-down.** `fleet-herdr wind-down --engagement <name>`:
   verifies each lane finished at its declared end state and its
   claim in the engagement record is released, cancels the tick
   schedule, then enforces the gates — every worktree clean AND
   its branch merged to the base or with zero unique commits —
   before closing each worker's worktree workspace by identifier
   (the workspace qualifier of its recorded pane), then the
   supervision workspace as a group, before the worktrees and
   branches are removed. The by-identifier close is required:
   when another workspace already owns the repo checkout, Herdr
   links worktree workspaces to that workspace's group, and a
   group close on the supervision workspace does not reach them.
   An already-closed workspace is skipped, and a missing record
   (a rolled-back `up`) is nothing to wind down. A refusal names
   its violators and closes nothing; resolve by hand.

## Reviewer workers

A reviewer is an ordinary worker on this backend: it starts with
`add-worker --lane <lane> --prompt <review prompt>` (or as part of
an engagement whose prompt names the review role), reads the
builder's branch or diff from its own worktree, re-runs the lane's
tests and parity check there, and reports its verdict to the
operator through the agent surface. It never edits the builder's
worktree and never merges. It runs on the same host and in the same
engagement as the builders — there is no separate reviewer host in
this kit. The review loop in the playbook (request changes, then
re-review only the changed files, escalate after two rounds without
agreement) is operated with `read`, `steer`, and `status`, like any
other worker intervention.

## Adding a worker mid-engagement

`fleet-herdr add-worker --engagement <name> --lane <lane>
[--kind <kind>] [--env KEY=VALUE]... [--prompt ...] [--
<agent-args...>]`: the same staging, start, move, and close flow
into a new linked worktree workspace, appended to `herdr.json`.
Without `--kind` the worker continues the engagement's kind; without
`--env` it reuses `up`'s environment. Update the engagement-record
lane table first (same rule as tmux). No coverage check is needed:
`status` lists every recorded worker, and a missing agent reports
as missing.

## Restarting a worker

`fleet-herdr restart --engagement <name> --worker <lane> [--prompt
...] [-- <agent-args...>]` ends the worker's agent and starts it
again under the same name, in the same pane. It sends the kind's
exit command and waits for Herdr to free the name, then starts the
same kind with the recorded arguments (or new ones after `--`) and
re-sends the prompt. A worker blocked at a dialog is refused: read
it and ask the human. A worker whose agent already exited just
starts.

## Steering

`fleet-herdr steer --engagement <name> --worker <lane> --text
'<text>' [--wait] [--until ...] [--timeout MS]`. No composer
clearing, no Enter splitting — the agent surface owns delivery.
`--wait` requires an observed lifecycle change within 5 seconds;
a `blocked` agent rejects the submission before any input is
sent — inspect with `read` and ask the human before answering it.

## Recovery

Same recipes as tmux, translated: `blocked` at an approval dialog
→ read, then answer or relay; `unknown` → read the pane before
touching it; crashed or stuck agent → `fleet-herdr restart` (same
name, same pane) with the lane prompt; infra weather → diagnose,
fix the environment, resume. Never restart lanes to clear infra
errors.

## Resume contract

The engagement record is the durable state; Herdr is the live
registry. A fresh operator adopts via: the engagement record,
`fleet-herdr status` (plus an agent list for anything unrecorded),
the state dir (`stop-condition`, `herdr.json`), and each lane's
record state. Adopt the names as-is (agent names are unique among
live agents); re-steer, re-arm the schedule, continue ticks.
Anything not in the engagement record is presumed lost. A lapsed
schedule bounds workers by the step-1 stop condition, same as
tmux.
