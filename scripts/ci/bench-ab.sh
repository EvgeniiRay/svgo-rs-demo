#!/usr/bin/env bash
# Compare two bench_corpus binaries on the same runner, interleaved A B A B so that
# runner noise hits both sides alike. Prints one JSON line and writes a Markdown table.
#   scripts/ci/bench-ab.sh BASE_BIN HEAD_BIN [rounds] [runs] [table.md]
set -euo pipefail
base=$1 head=$2 rounds=${3:-3} runs=${4:-7} table=${5:-bench-table.md}
b=() h=()
for _ in $(seq "$rounds"); do
  b+=("$("$base" corpus "$runs" | jq -r .median_ms)")
  h+=("$("$head" corpus "$runs" | jq -r .median_ms)")
done
med() { printf '%s\n' "$@" | sort -g | awk '{a[NR]=$1} END {print a[int((NR+1)/2)]}'; }
bm=$(med "${b[@]}") hm=$(med "${h[@]}")
pct=$(awk -v b="$bm" -v h="$hm" 'BEGIN {printf "%.1f", (h - b) / b * 100}')
icons=$(ls corpus/*.svg | wc -l | tr -d ' ')
{
  echo "| | main | this change |"
  echo "| --- | ---: | ---: |"
  echo "| ms per corpus pass ($icons icons, median of $rounds×$runs) | $bm | $hm |"
  echo "| icons per second | $(awk -v m="$bm" -v n="$icons" 'BEGIN {printf "%.0f", n / (m / 1000)}') | $(awk -v m="$hm" -v n="$icons" 'BEGIN {printf "%.0f", n / (m / 1000)}') |"
  echo "| change | | $( [ "${pct#-}" = "$pct" ] && echo "+$pct% slower" || echo "${pct#-}% faster") |"
} > "$table"
jq -nc --argjson base "$bm" --argjson head "$hm" --argjson pct "$pct" '{base_ms: $base, head_ms: $head, change_pct: $pct}'
