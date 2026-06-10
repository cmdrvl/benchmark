# Scorecard Pass 1

| Surface | Before | After | Evidence |
|---|---:|---:|---|
| Bare `benchmark` | 420 | 870 | Long help exits 0 and lists agent entrypoints |
| `benchmark --robot-triage` | 250 | 910 | Returns `benchmark.doctor.triage.v1` JSON |
| `benchmark capabilities --json` | 500 | 900 | Returns `benchmark.doctor.capabilities.v1` JSON |
| `benchmark robot-docs guide` | 500 | 860 | Prints in-tool agent guide |
| `benchmark --jsno` | 520 | 840 | Names `--json`, next command, and robot docs |
| Formula generator | 430 | 850 | Template omits redundant Homebrew `version` |

Median uplift: 170 points. No scored surface regressed.
