# Changelog

## Unreleased

Current development version: `0.1.0`. The earliest stable stage has not been
reached; minor-version increments per commit begin after that milestone.

### Compiled plugin foundation

- Created the Paperphase Rust workspace, CLI, experimental guest SDK, and a Rust
  temperature example compiled to portable WebAssembly.
- Added Wasmtime/Cranelift preparation with fresh invocation state, shared host
  HTTP networking, explicit origin grants, and strict typed-dataset validation.
- Enforced guest memory, stack, table, fuel, request, response, and publication
  limits. Applied fuel before module startup, disabled redirects/proxies/retries,
  and retained published data only after a successful invocation.
- Added a deterministic local API demo and JSON reports separating runtime
  setup, compilation, invocation, guest memory, and whole-process RSS measurements.
- Added 18 boundary tests, host/guest formatting and lint checks, a check script,
  a locked dependency graph, and GitHub Actions validation.
- Documented the implemented ABI, data structures, execution flow, limits,
  development commands, initial desktop measurements, and outstanding hardware
  and account-linking validation. Added the project README and identity.

Validation recorded: all 18 boundary tests passed in an isolated x86-64 Linux
container, including the compiled Rust guest; formatting, host/guest Clippy,
release builds, and a ten-invocation release demo passed. A Ranelagh Luas HTTPS
forecast response was parsed successfully. Pi measurements, Google OAuth,
provider polling allowances, and physical display validation remain pending.

### Development workflow

- Use short, descriptive commit subjects and keep detailed change descriptions
  in this file.
- Keep the version at `0.1.0` during the current experimental foundation work.
  After the earliest stable stage, increment the minor version on every commit
  and update the workspace package entries in `Cargo.lock` accordingly.

### CI and build outputs

- Include the GitHub Actions workflow that installs the Rust guest target and
  runs the complete host/guest check script on pushes and pull requests.
- Ignore Cargo build output, local caches, compiled native Wasm caches, and
  profiling output so generated files stay out of source commits.

### Shared model and display simulator

- Extracted datasets and validation into `paperphase-core`, independent of
  Wasmtime. Added typed dataset publication to the Rust guest SDK and updated
  the temperature example without changing the experimental host-call ABI.
- Added versioned layouts, stable component/block identities, account-scoped
  data bindings, validation error paths, and bounded geometry/content checks.
- Added snapshot freshness using fetch and optional observation times, source
  errors, stale-content indicators, and distinct missing/empty-data states.
- Added a shared monochrome renderer with a licensed bundled DejaVu font,
  clipped text, wrapping/ellipsis, fixed font sizes, alignment, bounded glyph
  caching, and a packed one-bit framebuffer.
- Added PNG previews and PBM exports from the same pixels, plus change bounds
  covering moved and removed content. Added `paperphase simulate` and sample
  layouts/datasets, implementation diagrams, a generated frame, and usage docs.
- Added model, renderer, and CLI tests, including a compiled-plugin-to-renderer
  workflow. The browser editor, service, scheduling, persistence, and hardware
  validation remain pending; this is still experimental version `0.1.0`.

Validation: `sh scripts/check.sh` passed on x86-64 Linux: formatting, host/guest
Clippy, compiled guest build, and all 40 tests (18 boundary, 7 model, 10 renderer,
5 CLI). Checked every decoded PNG pixel against packed frame data, exercised
input/output alias protection, visually inspected the generated sample frame,
and verified the bundled font checksum and documentation links. Pi and physical
display measurements remain pending.
