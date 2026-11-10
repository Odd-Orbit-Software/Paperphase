#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo clippy --locked --release -p paperphase-temperature-example --target wasm32-unknown-unknown -- -D warnings
cargo build --locked --release -p paperphase-temperature-example --target wasm32-unknown-unknown

case "${CARGO_TARGET_DIR:-target}" in
    /*) artifact_dir="${CARGO_TARGET_DIR}" ;;
    *) artifact_dir="$(pwd)/${CARGO_TARGET_DIR:-target}" ;;
esac
PAPERPHASE_TEST_PLUGIN="$artifact_dir/wasm32-unknown-unknown/release/paperphase_temperature_example.wasm"
export PAPERPHASE_TEST_PLUGIN
cargo test --locked --workspace -- --include-ignored --test-threads=2
