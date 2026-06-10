# Benchmark Agent Ergonomics Playbook

Pass 1 focused on first-try automation surfaces, keeping scoring semantics untouched.

Top applied changes:

1. `benchmark` with no args exits 0 and prints useful long help.
2. `benchmark --robot-triage` emits compact read-only triage JSON.
3. `benchmark capabilities --json` exposes the machine-readable CLI contract.
4. `benchmark --json` with no candidate returns the same capabilities contract.
5. `benchmark robot-docs guide` provides an in-tool agent guide.
6. Common `--json` typos now get an exact correction and next command.
7. Doctor capabilities include top-level aliases, scoring commands, exit codes, and refusal recovery.
8. Release formula generation now matches the strict-audit pattern used by the newer spine releases.

The scoring path, refusal taxonomy, lock verification, and summary math were intentionally not changed.
