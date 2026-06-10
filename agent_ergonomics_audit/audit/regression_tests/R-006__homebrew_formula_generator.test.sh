#!/usr/bin/env bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
grep -F 'blocks.append(f"  {os_block} do")' .github/workflows/release.yml
if grep -F 'version "{bare}"' .github/workflows/release.yml; then
  echo "formula generator must not emit an explicit stable version line" >&2
  exit 1
fi
