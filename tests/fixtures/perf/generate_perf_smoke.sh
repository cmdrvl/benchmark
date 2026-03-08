#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "usage: $0 <output-dir> [row-count]" >&2
  exit 64
fi

output_dir="$1"
row_count="${2:-256}"
candidate_path="${output_dir}/perf_candidate.csv"
assertions_path="${output_dir}/perf_assertions.jsonl"

mkdir -p "$output_dir"

printf 'comp_id,property_name,cap_rate\n' > "$candidate_path"
: > "$assertions_path"

for index in $(seq 1 "$row_count"); do
  comp_id=$(printf 'comp_%04d' "$index")
  property_name=$(printf 'Perf Property %04d' "$index")
  rate=$(printf '5.%02d%%' $((index % 100)))

  printf '%s,%s,%s\n' "$comp_id" "$property_name" "$rate" >> "$candidate_path"
  printf '{"entity":"%s","field":"property_name","expected":"%s"}\n' \
    "$comp_id" "$property_name" >> "$assertions_path"
done
