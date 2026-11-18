# Coordinator demo: one coordinator, several lanes

## Concept

One coordinator session owns the task. It splits the task into
single-owner lanes, starts one worker agent per lane in its own
worktree, and supervises them. The audience watches the coordinator
work: the split, the launch, the steering, the verdict, and the
wind-down. Nobody watches individual worker panes unless a lane
needs attention.

The coordinator is the only judge of what a worker produced. Workers
never judge each other, never merge, and never take a lane that is
not theirs. When a lane is blocked or stalls, the coordinator reads
it, steers it, or escalates to the human — that intervention, in the
open, is the demo.

## Cast

| Role | Who | Does what |
|---|---|---|
| Coordinator | The invoking (operator) session, running `fleet <task>` | Splits lanes, starts workers, steers, winds down |
| Lane worker | One agent per lane, own worktree and branch | Works only its assigned lane, reports `STATUS:` / `BLOCKED(HUMAN):` |
| Reviewer | One lane, same as any worker | Re-reads a builder's branch, re-runs its tests, reports a verdict, never merges |

## Lanes for this repo

Three lanes plus one reviewer, all inside `svgo-rs-demo`:

1. **bugfix** — fix the `convertColors` short-hex bug in
   [demo-issue.md](/Users/evgeniiray/Coding/svgo-rs-demo/docs/demo-issue.md):
   `#aabbcd` must not become `#abc`. Add a regression test first.
2. **perf** — run the corpus parity check and the benchmark gate
   from the README; report timings, change nothing unless a
   regression is proven.
3. **docs** — document the behaviour the bugfix lane lands, in the
   README's known-limitations area, only after the fix is verified.
4. **reviewer** — starts after the bugfix lane reports done; reviews
   the bugfix branch from its own worktree and re-runs the tests.

Lanes are short slugs. Engagement name: `demo`. Worker names are
`demo-bugfix`, `demo-perf`, `demo-docs`, `demo-reviewer`.

## Prerequisites

- Inside Herdr (`HERDR_ENV=1`), with `herdr`, `jq`, and `git` on
  PATH. The vendored launcher is
  [.agents/skills/fleet/herdr/fleet-herdr](/Users/evgeniiray/Coding/svgo-rs-demo/.agents/skills/fleet/herdr/fleet-herdr);
  see [herdr-backend.md](/Users/evgeniiray/Coding/svgo-rs-demo/.agents/skills/fleet/reference/herdr-backend.md).
- A clean checkout on the base branch. Every lane gets a fresh
  worktree under `.worktrees/demo-w<i>`; nothing lands on `main`
  during the demo except through the reviewer's verdict and the
  human's decision.

## Setup: one invocation, then work in chat

Setup is a basic skill invocation. Open Herdr, start the
coordinator session, and invoke `/coordinator`. That one
invocation prepares the coordinator for this demo project and
launches the fleet view in a Herdr pane on the right: it reads
the fleet playbook, the Herdr backend mapping, and this plan;
checks the Herdr stack, the checkout, and the launcher;
prepares the engagement record; and opens the live view from the
vendored [fleet-view skill](/Users/evgeniiray/Coding/svgo-rs-demo/.agents/skills/fleet-view/SKILL.md).
It launches no workers. No manual launcher commands, no
interview. The skill is project-scoped to this demo
([coordinator skill](/Users/evgeniiray/Coding/svgo-rs-demo/.agents/skills/coordinator/SKILL.md));
it takes no task argument and sets up no other project.

Lane setup happens only when work is entered later, in chat:
the coordinator then runs `fleet-herdr up`, steers each worker
its lane prompt, and verifies every lane with `fleet-herdr
status`. The reviewer lane is recorded during preparation and
started later, after the bugfix lane reports done.

## Windows

On Windows, run everything under Git Bash — the Herdr pane, the
coordinator session, and `fleet-herdr` — never PowerShell or
cmd. Prerequisites: Herdr for Windows, git, jq 1.7 or newer
(`winget install jqlang.jq`), and Git Bash coreutils. The
vendored `fleet-herdr` detects Git Bash, turns MSYS argument
conversion off, converts paths to `C:/...`, reads Herdr replies
through `jq -b`, and uses longer start retries and probe caps;
pass `--kind` explicitly (as the skill does) so no `timeout`
probe runs. Keep the script's line endings LF: re-check out
with `core.autocrlf=false` if a CRLF checkout breaks it. State
lives under `$FLEET_HERDR_STATE_ROOT` when set, else `/tmp`
(the Git Bash temp dir).

## Run of show (about 18 minutes, inside a 15–20 minute slot)

| Beat | Time | What happens | What the audience sees |
|---|---|---|---|
| 1. The problem | 0:00–2:00 | Coordinator states the task and the stop condition out loud: the bug fixed with a regression test, parity still green, docs updated. | Coordinator pane only; no lanes exist yet. |
| 2. Split | 2:00–3:30 | Coordinator names the three lanes and the reviewer, and why each lane has one owner. | Nothing moves yet — the split is a decision, said aloud before any agent starts. |
| 3. Prepare + view | 3:30–5:00 | The human invokes `/coordinator`; it prepares the coordinator — reads, checks, engagement record — and launches the fleet view in a pane on the right. No workers, no worktrees, no branches yet. | Fleet view on the right shows the live fleet (other engagements, if any); the `demo` engagement is still empty. |
| 4. Work entered, launch | 5:00–7:00 | Work is entered in chat, with its prompts. The coordinator automatically spins up a worker per lane, carrying those prompts in the `fleet-herdr up` launch (no manual steering for the assignment, no stored prompts), then verifies status. Three worktrees, three panes, three agents. | `fleet-herdr status` lists three lanes under `demo`, each `working`. |
| 5. Work | 7:00–10:30 | Workers work. Coordinator polls with `fleet-herdr status` and reads a lane with `fleet-herdr read` only when its status changes. Say the polling out loud; do not narrate pane contents you have not read. When a test run completes, a decision comes up in the fleet view with the actual result, and the coordinator waits for the owner's answer before anything proceeds past it. | Statuses flip `working` → `idle`/`done` lane by lane in the status output, then a numbered decision appears under DECISIONS. |
| 6. Blocked | 10:30–13:00 | If no lane blocks on its own, the coordinator steers the docs lane to surface one genuine decision — the wording of the documented behaviour — as `BLOCKED(HUMAN):` and wait, rather than guess. Coordinator reads it, answers the question in the open, steers it on. If a real block happens earlier, use that one instead — never stage a second. | One lane shows `blocked` in the status output, then returns to `working` after the coordinator's answer. This is the point of the demo. |
| 7. Review | 13:00–15:00 | Only after the test-gate decision is answered, the coordinator adds the reviewer lane, which re-runs the bugfix lane's tests from its own worktree and reports its verdict. | A fourth lane appears in the status output; the bugfix lane is not merged by its own worker. |
| 8. Close | 15:00–16:30 | Coordinator verifies the stop condition, then invokes `/coordinator close`: it winds the engagement down (`fleet-herdr wind-down --engagement demo`, gates first) and closes the fleet-view pane. Worktrees and branches go only after every worktree is clean and merged or empty; a refusal closes nothing. | A final status check shows no lanes remain, and the view pane is gone. |
| 9. Questions | 16:30–18:00 | Buffer for questions. If none, close on the one line: the coordinator, not the workers, decided when the task was done. | Coordinator pane only. |

## Speech

Read this as the script, not as notes. Lines in quotes are spoken.
Stage directions are in brackets.

> "One task, several workers, one person in charge. I am the
> coordinator. I do not write the fix — I decide who does, and I
> decide when it is done."

[Beat 1. State the task and the stop condition before starting
anything. Point at the coordinator pane; no lanes exist yet.]

> "The bug: `convertColors` turns `#aabbcd` into `#abc`, which is a
> different colour. Done means three things: a regression test that
> fails before the fix and passes after it, the full corpus still
> byte-identical to SVGO, and the docs updated. Nothing else counts
> as done."

[Beat 2. Name the lanes. One lane, one owner, no overlap.]

> "Three lanes. Bugfix owns the fix and its test. Perf owns the
> numbers and changes nothing. Docs owns the words, and only after
> the fix is proven. A fourth lane, the reviewer, will check the
> fix from its own checkout. No lane reviews its own work."

[Beat 3. Prepare + view. Invoke `/coordinator`. It prepares
the coordinator and launches the fleet view on the right. No
workers yet.]

> "I am set up as the coordinator. I have read the playbook, the
> checks pass, and the lanes are planned — but nothing has
> launched. Look right: that is the live fleet view, and it
> will fill when work arrives. Work comes next, in chat, and
> only then do lanes start."

[Beat 4. Work is entered in chat. The coordinator automatically
spins up the lanes — instructions ride with `up`, no steering
needed — then checks status before speaking again.]

> "Three agents, three worktrees, three branches — and I started
> none of them by hand before the work arrived. Each worker got
> the same launch prompt, and now each one gets its own lane,
> from me, in the open. If a worker drifts into another lane,
> that is my failure to steer, and you will see it here. That
> status output is live Herdr state: every lane, its branch,
> its status."

[Beat 5. Let them work. Resist narrating. Poll status, say so,
and wait.]

> "I am not doing their work. I am watching for three things: a
> lane finishing, a lane asking, a lane going quiet. Status checks
> tell me all three without me opening a single worker pane."

[Beat 6. The blocked lane. Read it with `fleet-herdr read`
before answering anything.]

> "There — one lane is blocked. It asked a question instead of
> guessing. I will read exactly what it asked, answer it here,
> and steer it on. A blocked lane is the system working, not
> failing. What would fail is a worker guessing past a question
> I never saw."

[Beat 7. Review. Add the reviewer lane only after the bugfix
lane reports done.]

> "The fix is claimed done. I do not take the builder's word for
> it. A separate lane, a separate checkout, re-runs the tests
> and reports back. The builder never merges its own work."

[Beat 8. Close. State the verdict against the stop condition,
then run `/coordinator close`: wind down first, view pane second.]

> "Regression test green, corpus identical, docs updated,
> reviewer agrees. That is the stop condition I stated at the
> start — so I wind the fleet down. Worktrees and branches go
> only because every lane is clean and accounted for. No lanes
> remain, and the coordinator is the one who closed them."

## If something goes wrong

- **A lane stalls with no status change:** read it with
  `fleet-herdr read`. Do not restart it to clear the stall;
  diagnose first, then `fleet-herdr restart` with the lane prompt
  only if the agent itself crashed.
- **A lane is blocked at an approval dialog:** never auto-answer
  it. Read it, answer it yourself, or relay the question to the
  human.
- **Wind-down refuses:** it names the violating lane and closes
  nothing. Resolve by hand — usually an unmerged branch or a dirty
  worktree — and re-run it. Do not force-close workspaces.
- **Status shows no lanes while they run:** `fleet-herdr status`
  reads the engagement record, not the coordinator's notes. Check
  that the record exists and that `herdr agent list` shows the
  demo agents; do not restart the lanes to fix a status read.
- **A worker edits outside its lane:** steer it back with the
  lane prompt, and say so. The engagement record, not the
  worker's word, is the authority for who owns what.

## After the demo

- Close with `/coordinator close`: it winds the engagement down
  and closes the fleet-view pane (the coordinator session stays
  open). Then confirm no `.worktrees/demo-*` directories or
  `demo-*` branches remain, and no `fleet-view` pane is listed.
- Keep the engagement record in `/tmp/fleet-demo/` until the
  human has seen the verdict; it is the only durable account of
  who did what.
- File follow-ups, if any, as issues against the lanes that own
  them — not as edits to this script.
