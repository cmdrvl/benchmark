#!/usr/bin/env bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
cargo run --quiet -- capabilities --json | jq -e '.schema == "benchmark.doctor.capabilities.v1" and .agent_entrypoints[0].usage == "benchmark --robot-triage"'
cargo run --quiet -- --json | jq -e '.schema == "benchmark.doctor.capabilities.v1"'
