#!/usr/bin/env bash
# Profile one bench_corpus binary over the corpus and print the top functions by self time.
#   scripts/ci/profile.sh BIN [top]   (perf when the kernel allows it, else callgrind on a sample)
set -euo pipefail
bin=$1 top=${2:-25}
if perf record -q -g -o "${TMPDIR:-/tmp}/perf.data" -- "$bin" corpus 3 >/dev/null 2>&1; then
  perf report -i "${TMPDIR:-/tmp}/perf.data" --stdio --no-children --sort symbol --percent-limit 0.8 2>/dev/null \
    | grep -E '^ +[0-9]' | head -n "$top" | sed -E 's/ +$//'
else
  mkdir -p "${TMPDIR:-/tmp}/sample" && ls corpus/*.svg | head -n 300 | xargs -I{} cp {} "${TMPDIR:-/tmp}/sample/"
  valgrind --tool=callgrind --callgrind-out-file="${TMPDIR:-/tmp}/cg.out" "$bin" "${TMPDIR:-/tmp}/sample" 1 >/dev/null 2>&1
  callgrind_annotate "${TMPDIR:-/tmp}/cg.out" 2>/dev/null | grep -E '^ *[0-9,]+ \(' | head -n "$top"
fi
