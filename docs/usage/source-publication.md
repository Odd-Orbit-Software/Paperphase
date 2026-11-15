# Source publication audit

Audit date: 2026-11-15. This is a review of the current local source candidate
and reachable Git history, not a permanent certification of later changes.

## Result

No actual credentials were found in the files Git would include or the 12
reachable commits. The private `.paperphase/` directory, validation evidence,
deployment archives and local runtime data are excluded from ordinary Git adds.
No private credential/runtime artifact paths were found in history.

The audit used Gitleaks 8.30.1 with the default rules, redacted reports, recursive
decoding and archive traversal. Its official Linux archive was verified against
published SHA-256 `551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb`.
It scanned an isolated copy of Git-includable files and all reachable commits.
A separate high-confidence pattern scan checked common Google/GitHub/AWS tokens
and private keys. The current checkout had 228 includable files before adding
this report, and 229 afterward.

Gitleaks reported one current finding and six historical findings. All seven
were confirmed false positives: minified JavaScript property assignments after
`autocomplete="new-password"`, setting an input's `maxLength`. No bundle-wide
scanner exception was added.

Three live Google credential values were compared using one-way fingerprints
against token-like literals in 559 source/history text versions. No matches were
found. Secret values stayed on the Pi and were not returned to the session.
The new screenshots and credential-related settings screenshots were visually
reviewed; fields are blank and display/agenda data are fixtures. Some examples
show local usernames/paths and private LAN addresses; these are metadata, not
credentials. The callback domain is intentionally public.

## Keeping private files out

`.gitignore` now also excludes downloaded `client_secret*.json` and
`google-client*.json`, private `*.google.json` / `*.connections.json` runtime
files, `.env` files and common private-key files. `.env.example` files and
normal source/fixture paths remain includable. Twelve private-path checks and
five public-source/example checks passed.

Ignore rules do not remove files already committed, block assistant access or
prevent an explicit forced add. Keep real credentials outside the source tree,
and review the exact staged files before publishing. Scan later changes and
review screenshots separately; text scanners do not establish what an image
contains. Keep scanner reports containing findings private and redacted.

The local redacted audit evidence is under the ignored
`.paperphase/github-publication-audit-2026-11-15/` directory. No commit or push
was performed by this audit. Runtime validation remains the separately recorded
166 Rust tests and 20 browser tests; this audit changed ignore rules and docs.
