# Real lock 0.6.0 fixtures

These lockfiles were produced by the installed `lock` binary so the integration
test exercises the real `lock.v0` output shape without requiring `lock` on the
test runner.

- Lock version: `lock 0.6.0`
- Upstream tools: `vacuum 0.5.0`, `hashbytes 0.5.0`
- Date produced: 2026-11-25
- Candidate directory: `tests/fixtures/candidates/smoke`

Exact commands, run from the benchmark repository root:

```sh
vacuum tests/fixtures/candidates/smoke --no-witness \
  | hashbytes --algorithm sha256 --no-witness \
  | lock --dataset-id benchmark-real-lock-sha256 --no-witness

vacuum tests/fixtures/candidates/smoke --no-witness \
  | hashbytes --algorithm blake3 --no-witness \
  | lock --dataset-id benchmark-real-lock-blake3 --no-witness
```
