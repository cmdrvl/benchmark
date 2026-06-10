#!/usr/bin/env bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
output="$(cargo run --quiet -- 2>/dev/null)"
grep -F "Agent entrypoints:" <<<"$output"
grep -F "benchmark --robot-triage" <<<"$output"
grep -F "benchmark capabilities --json" <<<"$output"
grep -F "benchmark robot-docs guide" <<<"$output"
