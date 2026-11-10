# Run the foundation harness

This is a developer CLI for validating compiled plugins. It does not yet run a
display or serve the planned editor. See the
[implementation design](../design/plugin-foundation.md).

## Build and check

Use Rust 1.96 or newer with Cargo, Rustfmt, and Clippy. Install the guest target:

```sh
rustup target add wasm32-unknown-unknown
```

Run the complete check from the repository root:

```sh
sh scripts/check.sh
```

This checks formatting and lints, builds the Rust example into portable Wasm,
then runs all tests including the real compiled guest. Tests create loopback
HTTP servers; the environment must permit local sockets. No provider credentials
or external API access are needed for tests.

`cargo test --workspace` alone runs the other tests and reports the compiled
guest tests as ignored. The full script supplies the artifact and enables them.

## Deterministic demo

```sh
cargo build --locked --release -p paperphase-temperature-example --target wasm32-unknown-unknown
cargo build --locked --release -p paperphase
./target/release/paperphase demo \
  --plugin target/wasm32-unknown-unknown/release/paperphase_temperature_example.wasm \
  --iterations 10
```

The command creates a temporary loopback API, runs the plugin against it, prints
a JSON report, and shuts the API down. The sample observation is always 18.5 °C.
The plugin fetches the observation through the host and converts it into a
typed dataset. No external weather service is contacted.

The report separates runtime setup, native module compilation, and invocation
times. It includes architecture, build profile, fuel consumption, HTTP request
count, and guest memory size. Linux RSS/high-water readings cover the whole
process and are not plugin-only measurements. The example server's scheduling
also contributes to invocation latency.

To run against another explicitly selected source:

```sh
./target/release/paperphase run \
  --plugin target/wasm32-unknown-unknown/release/paperphase_temperature_example.wasm \
  --source-url http://127.0.0.1:8080/observations
```

This example plugin expects an object with a numeric `temperature_c` field.
The host grants only the configured URL's origin and rejects redirects. Provider
polling and rate-limit scheduling are not implemented yet; use the deterministic
demo for repeated benchmarking.

## Zero 2 W validation

On 64-bit Raspberry Pi OS Lite, use a native AArch64 host build. A portable guest
Wasm artifact can be built on another machine and copied to the Pi; the host
then compiles it to the Pi's native instructions during preparation.

Keep host compilation separate from application runtime measurements. Building
the full Rust toolchain/dependency graph on a 512 MB Pi may require more memory
than running the resulting application. Build on a suitable AArch64 machine or
use a properly configured cross-compilation toolchain when necessary.

Run a release demo on the Pi and save the report outside the repository's
tracked files. Record OS, architecture, available memory, host/guest binary
sizes, setup/compilation times, RSS, and repeated invocation measurements. Use
external process monitoring as well to examine transient compilation peaks.
The harness does not yet measure a production install path or long-running idle
service because those are not implemented.

## Initial desktop measurement

These historical measurements used the original guest before the shared typed
contract/SDK refactor. They are not measurements of the current guest or renderer.

One release demo on 2026-11-10, x86-64 Linux in an unprivileged container with
no external network, produced these results for the 82 KiB Rust guest:

| Measurement | Observed value |
| --- | --- |
| Runtime setup | 1.73 ms |
| Native guest compilation | 106.11 ms |
| Ten invocations, including local fixture HTTP | 4.78–5.39 ms each |
| Fuel consumed | 96,040 per invocation |
| Guest linear memory | 1,310,720 bytes (1.25 MiB) |
| Whole-process peak RSS | 14,488 KiB (about 14.1 MiB) |

Compilation happened once before the ten fresh stores. This is a small,
deterministic connector workload, not a whole-application or Raspberry Pi
benchmark. The fixture accepts connections on a five-millisecond polling loop,
which contributes to observed HTTP latency. Repeat on the target hardware and
with representative plugins before choosing production budgets.

## Foundation progress

| Check | Result |
| --- | --- |
| Rust guest builds to binary Wasm | Passed locally |
| Host compiles guest with Cranelift and fetches through shared HTTP | Passed on x86-64 Linux |
| Boundary suite | 18 passed, including compiled guest |
| Format / Clippy | Passed locally |
| Zero 2 W resource measurements | Pending hardware access |
| Google multi-account OAuth and refresh-token renewal | Pending credentials and setup-flow validation |
| Luas forecast endpoint availability | One HTTPS response parsed successfully for Ranelagh on 2026-11-10; two directions and seven tram entries |
| Luas polling allowance and attribution requirements | Still need verification before integration |
| Panel revision / physical refresh | Pending hardware inspection |

The Luas check used the endpoint published in the
[TII forecasting dataset](https://data.gov.ie/dataset/luas-forecasting-api), with
`action=forecast`, `stop=ran`, and `encrypt=false`. A successful response is an
availability check, not a guarantee about future uptime or permitted polling.
