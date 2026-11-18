#!/usr/bin/env python3
"""Repo-scope tests for fleet_view.py: by default the view shows only
lanes in this checkout's repo; --all shows every agent.

Run from the repo root: python3 .agents/skills/fleet-view/tests/test_repo_scope.py
No network, no Herdr, no gh: run/github/branches helpers are stubbed.
"""
import importlib.util
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("fleet_view", HERE.parent / "fleet_view.py")
fv = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fv)

TOPS = {
    "/repo/svgo-rs-demo": "/repo/svgo-rs-demo",
    "/repo/svgo-rs-demo/.worktrees/demo-w1": "/repo/svgo-rs-demo",
    "/srv/other/.worktrees/rev-w1": "/srv/other",
    "/home/booth/games": "/home/booth/games",
}


def test_predicate():
    assert fv.lane_repo_matches("/repo/svgo-rs-demo/.worktrees/demo-w1", TOPS, "svgo-rs-demo")
    assert fv.lane_repo_matches("/repo/svgo-rs-demo", TOPS, "svgo-rs-demo")
    assert not fv.lane_repo_matches("/srv/other/.worktrees/rev-w1", TOPS, "svgo-rs-demo")
    assert not fv.lane_repo_matches("/repo/svgo-rs-demo/.worktrees/demo-w1", {}, "svgo-rs-demo")
    assert not fv.lane_repo_matches("/home/booth/games", TOPS, "svgo-rs-demo")
    assert fv.lane_repo_matches("/srv/other/.worktrees/rev-w1", TOPS, "svgo-rs-demo", hand=True)
    print("predicate OK")


def test_current_repo_basename():
    real_run = fv.run
    try:
        fv.run = lambda cmd, timeout=60: "/repo/svgo-rs-demo\n" if cmd[:2] == ["git", "rev-parse"] else ""
        assert fv.current_repo_basename() == "svgo-rs-demo"
        fv.run = lambda cmd, timeout=60: ""
        import os
        assert fv.current_repo_basename() == os.path.basename(os.getcwd().rstrip("/"))
    finally:
        fv.run = real_run
    print("current_repo_basename OK")


AGENTS = [
    {"name": "fable", "agent_status": "working", "cwd": "/repo/svgo-rs-demo", "pane_id": "w1:pV", "agent": "claude"},
    {"name": "demo-w1", "agent_status": "idle", "cwd": "/repo/svgo-rs-demo/.worktrees/demo-w1", "pane_id": "w1T:p2", "agent": "muse"},
    {"name": "rev-w1", "agent_status": "done", "cwd": "/srv/other/.worktrees/rev-w1", "pane_id": "w6X:p2", "agent": "claude"},
]
CFG = {
    "orchestrators": [
        {"engagement": "fable", "machine": "local", "label": "fable"},
        {"engagement": "demo", "machine": "local", "label": "demo"},
        {"engagement": "rev", "machine": "local", "label": "rev"},
    ],
    "coordinator": {}, "repos": [], "github_author": "@me",
    "goals": {}, "pins": {}, "roles": {}, "pulse": False,
}


def test_collect_scope():
    real = (fv.agents_on, fv.branches_on, fv.github, fv.cards_cached,
            fv.ready_cached, fv.cron_marks, fv.current_repo_basename)
    fv.agents_on = lambda m: list(AGENTS)
    fv.branches_on = lambda ssh, dirs, here=False: (
        {d: ("demo-w1" if "demo" in d else "main") for d in dirs},
        None,
        {d: TOPS.get(d, d) for d in dirs},
        [],
    )
    fv.github = lambda name, *a, **k: ({}, []) if name == "prs" else {}
    fv.cards_cached = lambda cfg: []
    fv.ready_cached = lambda cfg: {}
    fv.cron_marks = lambda cfg: []
    fv.current_repo_basename = lambda: "svgo-rs-demo"
    try:
        scoped = fv.collect(dict(CFG))
        scoped_engs = {o["engagement"]: [l["name"] for l in o["lanes"]] for o in scoped["orchestrators"]}
        assert scoped_engs == {"fable": ["fable"], "demo": ["demo-w1"]}, scoped_engs
        unscoped = fv.collect(dict(CFG), all_repos=True)
        all_engs = {o["engagement"]: [l["name"] for l in o["lanes"]] for o in unscoped["orchestrators"]}
        assert all_engs == {"fable": ["fable"], "demo": ["demo-w1"], "rev": ["rev-w1"]}, all_engs
    finally:
        (fv.agents_on, fv.branches_on, fv.github, fv.cards_cached,
         fv.ready_cached, fv.cron_marks, fv.current_repo_basename) = real
    print("collect scope OK")


def main():
    test_predicate()
    test_current_repo_basename()
    test_collect_scope()
    print("REPO-SCOPE OK")


if __name__ == "__main__":
    main()
