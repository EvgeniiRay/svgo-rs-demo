#!/usr/bin/env python3
"""Numbering test for fleet_view.py: visible section numbers are sequential
over the sections actually drawn, with no gaps from hidden sections, and
the digit keys fold the section shown with that number.

Run from the repo root: python3 .agents/skills/fleet-view/tests/test_numbering.py
No Herdr, gh, or ssh calls: render/fit run on fixed data.
"""
import importlib.util
import pathlib
import re
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("fleet_view", HERE.parent / "fleet_view.py")
fv = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fv)
ANSI = re.compile(r"\x1b\[[0-9;]*m|\x1b\]8;;[^\x1b]*\x1b\\")


def pr(num):
    return {"number": num, "state": "OPEN", "isDraft": False, "ci": "SUCCESS",
            "unresolved": 0, "title": "feat: something", "url": f"https://github.com/org/app/pull/{num}",
            "repository": {"name": "app"}, "mergedAt": None, "headRefName": f"b{num}"}


DATA = {
    "coordinator": {"name": "operator", "machine": "mac (local)", "engagement": "rel"},
    "at": "12:00:00", "coord_pulse": "", "coord_ask": "",
    "orchestrators": [{
        "engagement": "rel-a", "label": "rel-a", "machine": "box1", "reachable": True,
        "stats": {"load": 12.0, "cores": 96, "mem_used": 80000, "mem_total": 240000},
        "repo": None, "project": None, "tz": None,
        "lanes": [
            {"lane": "ui", "name": "rel-a-ui", "goal": "Settings polish", "branch": "123-settings",
             "pr": pr(123), "pane": "w1:p1", "status": "working", "agent": "muse", "title": ""},
            {"lane": "old", "name": "rel-a-old", "goal": "Done thing", "branch": "-",
             "pr": None, "pane": "w1:p2", "status": "done", "agent": "muse", "title": ""},
        ],
    }],
    "decisions": [], "unreadable": [], "answered": [], "answered_links": [],
    "relays": [], "repos": [], "my_prs": [], "github_author": "",
    "max_prs": 20, "hidden": [], "show_drafts": False, "repo_order": ["app"],
    "crons": [], "cron_marks": [], "reviewers": [], "gaps": [], "github": None,
}


def main():
    numbering = {}
    out = fv.render(DATA, interactive=True, numbering=numbering)
    plain = ANSI.sub("", out)
    marks = []
    for line in plain.splitlines():
        m = re.search(r"[▾▸](\d)", line)
        if m and ("FLEET" in line or "DECISIONS" in line or "YOUR OPEN PRS" in line):
            marks.append(m.group(1))
    assert marks == ["1", "2", "3"], f"gap in visible numbering: {marks}"
    assert numbering == {"1": "tree", "2": "decisions", "3": "prs"}, numbering
    assert "DISK" not in plain and "ACTIVITY" not in plain, "removed section still drawn"
    assert "YOUR OPEN PRS" in plain

    view = {"folds": {}, "collapsed": set(), "scroll": 0}
    fv.fit(DATA, 40, view, frame=0, show_all=True, links=False)
    assert view.get("numbering") == {"1": "tree", "2": "decisions", "3": "prs"}, view.get("numbering")
    fv.apply_keys(view, "3")
    assert view["folds"].get("prs") is True, f"key 3 did not fold prs: {view['folds']}"
    print("NUMBERING OK")


if __name__ == "__main__":
    main()
