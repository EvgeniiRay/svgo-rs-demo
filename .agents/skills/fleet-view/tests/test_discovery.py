#!/usr/bin/env python3
"""Discovery test for fleet_view.py: a dash-less named agent is its own
engagement, the same fleet reported by two Herdr machines is shown once,
and an unreachable machine reads as None.

Run from the repo root: python3 .agents/skills/fleet-view/tests/test_discovery.py
No network, no Herdr: fleet_view.run is stubbed.
"""
import importlib.util
import json
import os
import pathlib
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("fleet_view", HERE.parent / "fleet_view.py")
fv = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fv)

LOCAL_AGENTS = {"result": {"agents": [
    {"name": "fable", "agent_status": "working", "cwd": "/repo", "pane_id": "w1:pV", "agent": "claude"},
    {"name": None, "agent_status": "working", "cwd": "/repo", "pane_id": "w1:pQ", "agent": "muse"},
    {"name": "demo-w1", "agent_status": "idle", "cwd": "/repo/.worktrees/demo-w1", "pane_id": "w1T:p2", "agent": "muse"},
]}}
REMOTE_AGENTS = {"result": {"agents": [
    {"name": "rev-w1", "agent_status": "done", "cwd": "/srv/other/.worktrees/rev-w1", "pane_id": "w6X:p2", "agent": "claude"},
    {"name": "rev-w3", "agent_status": "working", "cwd": "/srv/other/.worktrees/rev-w3", "pane_id": "w6Z:p2", "agent": "claude"},
]}}
MACHINE_LIST = (
    "id\tstudio\tstudio\tdefault\tenabled\n"
    "id\tbackup\tbackup\tdefault\tenabled\n"
    "id\toffline\toffline\tdefault\tdisabled\n"
)


def fake_run(cmd, timeout=60):
    if cmd[:3] == ["herdr", "machine", "list"]:
        return MACHINE_LIST
    if cmd == ["herdr", "agent", "list"]:
        return json.dumps(LOCAL_AGENTS)
    if cmd[:2] == ["herdr", "--machine"]:
        # Both enabled machines report the same agents: one engagement.
        return json.dumps(REMOTE_AGENTS)
    raise AssertionError(f"unexpected command: {cmd}")


def main():
    old_env = os.environ.get("HERDR_ENV")
    os.environ["HERDR_ENV"] = "1"
    real_run = fv.run
    fv.run = fake_run
    try:
        orchs = fv.discovered_orchestrators()
        got = {(o["engagement"], o["machine"]) for o in orchs}
        assert ("fable", "local") in got, f"dash-less fable missing: {got}"
        assert ("demo", "local") in got, f"demo missing: {got}"
        assert ("", "local") not in got, f"unnamed agent became an engagement: {got}"
        rev = [o for o in orchs if o["engagement"] == "rev"]
        assert len(rev) == 1 and rev[0]["machine"] == "studio", f"dedupe wrong: {rev}"
        assert "offline" not in {o["machine"] for o in orchs}, f"disabled machine listed: {got}"
        print("DISCOVERY OK")
    finally:
        fv.run = real_run
        if old_env is None:
            del os.environ["HERDR_ENV"]
        else:
            os.environ["HERDR_ENV"] = old_env


if __name__ == "__main__":
    main()
