#!/usr/bin/env bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
set +e
cargo run --quiet -- --jsno >/tmp/benchmark-r005.out 2>/tmp/benchmark-r005.err
code=$?
set -e
test "$code" -eq 2
grep -F 'hint: did you mean `--json`?' /tmp/benchmark-r005.err
grep -F "next: benchmark capabilities --json" /tmp/benchmark-r005.err
grep -F "help: benchmark robot-docs guide" /tmp/benchmark-r005.err
