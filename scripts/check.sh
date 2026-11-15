#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo clippy --locked --release -p paperphase-temperature-example -p paperphase-weather-plugin -p paperphase-rss-plugin -p paperphase-luas-plugin -p paperphase-dublin-bus-plugin -p paperphase-air-quality-plugin -p paperphase-daily-quote-plugin -p paperphase-generic-json-plugin -p paperphase-google-calendar-plugin --target wasm32-unknown-unknown -- -D warnings
cargo build --locked --release -p paperphase-temperature-example -p paperphase-weather-plugin -p paperphase-rss-plugin -p paperphase-luas-plugin -p paperphase-dublin-bus-plugin -p paperphase-air-quality-plugin -p paperphase-daily-quote-plugin -p paperphase-generic-json-plugin -p paperphase-google-calendar-plugin --target wasm32-unknown-unknown

case "${CARGO_TARGET_DIR:-target}" in
    /*) artifact_dir="${CARGO_TARGET_DIR}" ;;
    *) artifact_dir="$(pwd)/${CARGO_TARGET_DIR:-target}" ;;
esac
PAPERPHASE_TEST_PLUGIN="$artifact_dir/wasm32-unknown-unknown/release/paperphase_temperature_example.wasm"
PAPERPHASE_TEST_WEATHER_PLUGIN="$artifact_dir/wasm32-unknown-unknown/release/paperphase_weather_plugin.wasm"
PAPERPHASE_TEST_RSS_PLUGIN="$artifact_dir/wasm32-unknown-unknown/release/paperphase_rss_plugin.wasm"
PAPERPHASE_TEST_LUAS_PLUGIN="$artifact_dir/wasm32-unknown-unknown/release/paperphase_luas_plugin.wasm"
PAPERPHASE_TEST_DUBLIN_BUS_PLUGIN="$artifact_dir/wasm32-unknown-unknown/release/paperphase_dublin_bus_plugin.wasm"
PAPERPHASE_TEST_AIR_QUALITY_PLUGIN="$artifact_dir/wasm32-unknown-unknown/release/paperphase_air_quality_plugin.wasm"
PAPERPHASE_TEST_DAILY_QUOTE_PLUGIN="$artifact_dir/wasm32-unknown-unknown/release/paperphase_daily_quote_plugin.wasm"
PAPERPHASE_TEST_GENERIC_JSON_PLUGIN="$artifact_dir/wasm32-unknown-unknown/release/paperphase_generic_json_plugin.wasm"
PAPERPHASE_TEST_GOOGLE_CALENDAR_PLUGIN="$artifact_dir/wasm32-unknown-unknown/release/paperphase_google_calendar_plugin.wasm"
export PAPERPHASE_TEST_GOOGLE_CALENDAR_PLUGIN
export PAPERPHASE_TEST_PLUGIN PAPERPHASE_TEST_WEATHER_PLUGIN PAPERPHASE_TEST_RSS_PLUGIN PAPERPHASE_TEST_LUAS_PLUGIN
export PAPERPHASE_TEST_DUBLIN_BUS_PLUGIN PAPERPHASE_TEST_AIR_QUALITY_PLUGIN PAPERPHASE_TEST_DAILY_QUOTE_PLUGIN PAPERPHASE_TEST_GENERIC_JSON_PLUGIN
cargo test --locked --workspace -- --include-ignored --test-threads=2
