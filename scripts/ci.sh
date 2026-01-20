#!/bin/bash
# CI script for rvr
# Runs all checks: format, lint, tests, and examples

set -e

echo "=== Checking format ==="
cargo fmt --check

echo "=== Running clippy ==="
cargo clippy --workspace --all-targets -- -D warnings

echo "=== Running tests ==="
cargo test --workspace

echo "=== Building examples ==="
cargo build --examples -p rvr

echo "=== Building in release mode ==="
cargo build --release --workspace

echo "=== All checks passed ==="
