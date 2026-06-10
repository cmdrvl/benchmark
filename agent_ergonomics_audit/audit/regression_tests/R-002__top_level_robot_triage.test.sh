#!/usr/bin/env bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
cargo run --quiet -- --robot-triage | jq -e '.schema == "benchmark.doctor.triage.v1" and .ok == true and .read_only == true'
