# Component designer review — 15 November 2026

Two independent agents reviewed the implementation in fresh browser contexts.
Assessment A evaluated task flow/design and Nielsen heuristics. Assessment B
collected detector output, browser behavior, responsive geometry and hit testing.
Neither read the other's findings before completion. Review used synthetic local
data; no Pi, live Google account, VPS or physical panel was involved.

The user approved the existing paper/green layout direction after viewing the
local designer. This extension preserves DESIGN.md and its token sidecar.

| Material finding | Change | Evidence |
| --- | --- | --- |
| Keyboard entry/add/close loses task focus | Name field receives entry focus; new item receives content focus; close restores the initiating action | Both reviewers rechecked the complete keyboard transitions |
| Laptop canvas clips the item strip | Designer panel grows; preview/dock do not shrink into clipping | B measured the entire 40 px Add control and a correct center hit at 1024×768 and 1366×768, including long error feedback |
| Saved designs buried beneath creation choices | A populated library puts saved designs first | A verified populated browser-local fixture ordering |
| Empty name exposes a renderer path | Field-associated guidance and stable label; preview omits incomplete optional label | Both reviewers checked aria-invalid/describedby and retained pixels |
| Saved library does not update after save/close | Rebuild saved rows and counts before restoring return focus | Finish reviewer identified the return-path bug; targeted new-save and rename coverage added |
| Minimum-area fitting produces extreme wrapping | Bounded wrapping penalty, preserving explicit widths and all content | Parent measured identical weather blocks at automatic 192×244 versus explicit 96×381; font sizes stayed 14 px |

A's initial score was 27/40 before these corrections. Rechecks scored the named
fixes resolved, with A explicitly limiting its weather comparison because data
changed during that review. The later same-values parent measurement closes that
comparison gap; it does not establish that every future value fits.

The single deterministic source scan returned two advisory records and zero
non-advisory records. `repeating-stripes-gradient` refers to the functional
reserved data-status band; `codex-grid-background` refers to the opt-in geometry
canvas grid. Both remain documented functional exceptions. Existing file-scoped
image/padding/paper-palette exceptions were preserved; no new rules or files were
disabled. The attempted live overlay was blocked by the app's self-only script
CSP. Browser behavior, screenshots, geometry and hit testing supplied the fallback;
no visible detector overlay is claimed. The temporary overlay server was stopped.

Review evidence is in `.impeccable/review/component-designer` (local working
artifacts); durable final screens are linked from [the design](component-designer.md).

A fresh source/screenshot finish reviewer initially returned FIX for the stale
library return and suggested friendlier field labels. After the correction and
recapture, it scored both named fixes resolved and returned SHIP within the
validated desktop scope. The verdict pass inspected the affected library,
weather and laptop captures; it did not repeat browser testing or certify Pi.

Validation: complete `sh scripts/check.sh` passed 173 Rust tests and host/Wasm
checks. A fresh sequential browser run passed all 34 tests; after the last narrow
fix, the expanded save/close/rename workflow and affected weather/RSS workflow
passed again. TypeScript/frontend build and embedded Rust build passed. Final
captures were checked for image loading, page errors and correct state. Temporary
overlay/browser scripts were cleaned up; raw review artifacts and test logs are
retained locally. The synthetic preview remains running for owner review.

## Owner follow-up: saving, internal overlap and icons

The owner’s later screenshot showed Save disabled for a readable arranged
component. Their evidence superseded the earlier completion review. They also
confirmed that internal boxes may overlap; only whole components need separation.
The follow-up fixes visible-ink clipping, allows warned saves, repairs clipped
boxes without moving them, refreshes numeric fields after Fit, and prefers Icon
key when binding weather icons. Core/render tests check overlapping ink, retained
component boundaries and actual pixel clipping. Operator docs describe the icon
field path and its requirement for icon names rather than numeric readings.

Complete validation passed 176 Rust tests with host/Wasm checks and 36 browser
flows. Browser automation ran on a separate fixture/port from the owner’s active
preview. Five actual desktop/laptop reproductions used browser-local fixture
responses and read-only preview/fit requests; they are not exports of the owner’s
unsaved draft. A fresh reviewer inspected those captures, the original owner
image and source, and returned SHIP for this narrow desktop follow-up with no
material issue requiring another fix. No detector rerun or new suppression was
used; Pi, real Google and physical-panel behavior remain outside this verdict.
