# Paperphase repository instructions

- Use short, descriptive commit subjects. Put detailed change descriptions and
  relevant verification in `changelog.md`.
- The current version is `0.1.0`; the earliest stable stage has not been reached.
  Keep this version during the experimental foundation work. Once the earliest
  stable stage is reached and recorded, increment the minor version on every
  commit, including documentation-only commits. Keep `Cargo.toml`, workspace
  package versions in `Cargo.lock`, and changelog version information consistent.
- Keep agreed plans in `docs/planning`. Document implemented features in
  `docs/design`, with concise explanations of data structures, algorithms, and
  diagrams or actual screenshots where useful. Put operator instructions in
  `docs/usage`.
- For Rust changes, the complete validation command is `sh scripts/check.sh`.
  It requires the `wasm32-unknown-unknown` target and loopback socket access.
  See `docs/usage/foundation-harness.md` for details. Keep desktop validation
  distinct from pending Pi, Google account, and physical display validation.
