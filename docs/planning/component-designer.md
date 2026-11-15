# Compact component designer

Branch: `component-designer`. Mode: Operate, extending the approved Paperphase
canvas/task-dock design. Keep DESIGN.md and the earlier UX review's hierarchy,
contrast, keyboard-focus, recovery and draft-isolation requirements.

Owner decisions: compact but readable starting text around 14 px; fit the box
closely to its content; save named reusable designs per plugin on Paperphase.
Advanced controls remain available without leading the everyday workflow.

The designer opens independently of display space, from a loaded plugin or an
existing connection, with a blank start or a copy of a supplied template. A
compact start reduces padding/gap and uses readable text; it never silently
drops information to force a fit. The shared Rust renderer measures current
content. Size and roomier text remain adjustable, with precise controls separate.
Fit is based on current values, not a guarantee about future longer values.

Named designs persist separately from layouts/connections. Bindings are stored
relative to the plugin, then rebound to a chosen connection when used. Saving a
design does not update the display. Adding/applying explicitly validates size,
content and whole-component non-overlap, retaining the design on failure. Cancelling restores the
previous layout draft and its history. Blank and oversized drafts remain editable;
only valid designs may be saved or inserted.

Restore mouse-selected Delete for components and internal elements, protect text
input and dialogs, retain last-element rules and undo. Expose a clear visual
internal-designer action from ordinary component content editing.

Validate full/crowded displays, oversized/overflowing templates, plugins with no
templates or no connection yet, named save/reuse/restart, connection rebinding,
failed placement and cancellation, keyboard/focus and existing plugin/account
flows. Capture the actual UI and run two independent UX reviews; fix and recheck
all material findings. Keep Pi, real OAuth and physical-panel checks separate.

Implemented and desktop-validated on this branch. See
[design and screenshots](../design/component-designer.md),
[review outcomes](../design/component-designer-review.md), and
[owner instructions](../usage/component-designer.md). Final validation: 176 Rust
tests and 36 Chromium tests. Pi,
real Google and physical-display checks remain separate.

Owner follow-up: internal items may overlap because their parent shares a redraw
area; the non-overlap rule applies to whole components. Tight visible text must
not be rejected for unused font leading. Reusable design saves permit clipping
with an item-specific warning; fit repairs clipped boxes without moving items.

Owner acceptance: the designer is approved, and the owner authorized committing
and pushing this work after merging it into `main`.
