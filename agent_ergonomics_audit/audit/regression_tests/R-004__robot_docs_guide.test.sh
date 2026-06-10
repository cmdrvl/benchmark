#!/usr/bin/env bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
output="$(cargo run --quiet -- robot-docs guide)"
grep -F "benchmark robot-docs guide" <<<"$output"
grep -F "benchmark capabilities --json" <<<"$output"
grep -F "benchmark <CANDIDATE> --assertions <FILE> --key <COLUMN> --json" <<<"$output"
