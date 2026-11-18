---
name: fleet-view
description: "Draw the fleet as a live, colored terminal tree in a Herdr side pane: the coordinator, engagements grouped by project or repository, and every lane with its goal, branch, PR, and status, plus decisions and crons. This copy is Herdr lanes only, with no disk or activity sections. Section numbers follow the sections actually drawn, with no gaps. Use when the user asks to open, show, or launch the fleet view, or when reporting fleet progress with the live view."
---

# Fleet view

One command draws the fleet from live Herdr state. It needs
`herdr`, `gh` (signed in, for PRs), and `ssh` to the Herdr
machines; a missing piece hides that part of the view instead
of failing. Colors turn off when stdout is not a terminal;
`FORCE_COLOR=1` keeps them.

```sh
python3 -I .agents/skills/fleet-view/fleet_view.py                 # colored, every lane
python3 -I .agents/skills/fleet-view/fleet_view.py --watch 30      # live dashboard in place
python3 -I .agents/skills/fleet-view/fleet_view.py --plain         # no colors, for a report
python3 -I .agents/skills/fleet-view/fleet_view.py --json          # the collected data
```

Optional config: `~/.fleet/view.json` (coordinator, orchestrators,
pins, goals, repos). No config is needed: the view discovers
Herdr machines and engagements, and uses the signed-in `gh` user
and the current repo.

## Side panel in Herdr

Inside Herdr (`HERDR_ENV=1`), open the live view beside the
current pane, in the same window. Reuse a pane already named
`fleet-view` if one exists; do not split a second one.

```sh
P=$(herdr pane split --current --direction right --ratio 0.42 --no-focus --cwd "$PWD" \
  | python3 -I -c 'import json,sys;print(json.load(sys.stdin)["result"]["pane"]["pane_id"])')
herdr pane rename "$P" fleet-view
herdr pane run "$P" "while :; do python3 -I $PWD/.agents/skills/fleet-view/fleet_view.py --watch 30 --coordinator-pane $HERDR_PANE_ID; sleep 2; done"
```

On Windows, run everything above under Git Bash with Python 3 on
PATH in the pane.
