#!/usr/bin/env bash
# One command to run every gate. CI-friendly, no arguments.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"

echo "==> fmt"
cargo fmt --all --check
echo "==> frontier"
python3 -m unittest discover -s scripts/tests -p 'test_*.py' -v
echo "==> clippy"
cargo clippy --all-targets --all-features -- -D warnings
echo "==> test"
cargo test --all
echo "==> all gates passed"
