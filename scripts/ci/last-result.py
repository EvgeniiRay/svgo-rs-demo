#!/usr/bin/env python3
"""Print the structured result of a `muse exec --json` run.

Reads the JSON event stream on stdin and writes the last non-empty
`run.terminal.completed` text that parses as JSON. Exits 1 if there is none.
"""
import json
import sys

result = None
for line in sys.stdin:
    try:
        event = json.loads(line)
    except ValueError:
        continue
    if event.get("payload_type") != "run.terminal.completed":
        continue
    text = ((event.get("payload") or {}).get("text") or "").strip()
    if not text:
        continue
    try:
        value, _ = json.JSONDecoder().raw_decode(text)
    except ValueError:
        continue
    if isinstance(value, dict):
        result = value

if result is None:
    sys.exit("no structured result in the event stream")
json.dump(result, sys.stdout)
print()
