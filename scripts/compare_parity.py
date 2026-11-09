#!/usr/bin/env python3
"""Byte-compare JS SVGO output vs Rust output over the corpus.

Usage: python scripts/compare_parity.py [js_dir] [rs_dir] [corpus_dir]
Prints match/mismatch counts, the first differing byte per mismatching
file, and a classified sample of diffs.
"""
import re
import sys
from pathlib import Path

js_dir = Path(sys.argv[1] if len(sys.argv) > 1 else "parity-js")
rs_dir = Path(sys.argv[2] if len(sys.argv) > 2 else "parity-rs")
corpus_dir = Path(sys.argv[3] if len(sys.argv) > 3 else "corpus")

files = sorted(p.name for p in corpus_dir.glob("*.svg"))
match = 0
mismatches = []
for name in files:
    a = (js_dir / name).read_bytes()
    b = (rs_dir / name).read_bytes()
    if a == b:
        match += 1
        continue
    pos = next((i for i, (x, y) in enumerate(zip(a, b)) if x != y), min(len(a), len(b)))
    mismatches.append((name, pos, a, b))

print(f"total={len(files)} byte-identical={match} mismatched={len(mismatches)}")
num_re = re.compile(rb"-?\d+\.?\d*")
only_numeric = 0
for name, pos, a, b in mismatches:
    # strip all numbers; if equal afterwards the diff is purely numeric
    if num_re.sub(b"#", a) == num_re.sub(b"#", b):
        only_numeric += 1
print(f"mismatches differing only in numbers: {only_numeric}")
print(f"mismatches with structural/textual diffs: {len(mismatches) - only_numeric}")
for name, pos, a, b in mismatches[:15]:
    lo = max(0, pos - 40)
    print(f"\n{name} first-diff@{pos}")
    print(f"  js: ...{a[lo:pos + 40]!r}")
    print(f"  rs: ...{b[lo:pos + 40]!r}")
