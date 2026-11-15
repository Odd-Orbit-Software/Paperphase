# UI and UX review — 15 November 2026

Method: dual-agent (A: `/root/ux_review` · B: `/root/ui_audit`).
Assessment A completed before the parent read Assessment B's findings. Both
reviewers independently inspected the local synthetic fixture and source.
These are expert assessments, not user-study measurements.

## Brief and verdict

The owner confirmed an everyday-owner experience, with technical controls
separate and desktop editing the priority. Keep Paperphase's warm paper/green
identity. Add the approved Odd Orbit logo and the literal attribution
“Developed by Odd Orbit Software”. Orbi's placement remains a feedback choice.

The visual identity already suits a calm e-paper display. The main problem is
interaction hierarchy: changing content requires navigating layout-engine
controls, and connecting a calendar leads into OAuth administration. Improve
these journeys while preserving precise editing for people who need it.

Three strengths to preserve:

- The real monochrome display frame is the proof, with separate displayed and
  draft states and honest simulator disclosure.
- Undo, discard, overlap prevention and retention of valid geometry protect work.
- Warm surfaces, green emphasis and restrained branding form a coherent identity.

## Baseline scores

Higher is better; each heuristic is scored out of four.

| Design heuristic | Score |
| --- | ---: |
| Visibility of system status | 3 |
| Match with everyday language and tasks | 2 |
| User control and freedom | 2 |
| Consistency and standards | 3 |
| Error prevention | 2 |
| Recognition rather than recall | 2 |
| Efficiency | 2 |
| Minimalism and hierarchy | 2 |
| Error diagnosis and recovery | 2 |
| Help and documentation | 2 |
| **Design total** | **22/40** |

Assessment B scored accessibility 2/4, performance 3/4, theming 1/4,
responsive behaviour 1/4 and implementation integrity 3/4: **10/20**.
The narrow-width result requires control access and desktop zoom improvements;
it does not establish a requirement for full phone editing or dark mode.

## Five design priorities

1. **P1 — Put content first.** Selecting Weather initially exposes geometry,
   spacing and refresh policy before content. “Edit contents” silently converts
   flow to positioned layout and dirties the draft. Lead with Content, then
   Position and Style. Make layout conversion an explicit, explained choice.
   Evidence: `web/src/app.ts:145`, `web/src/app.ts:438`.
2. **P1 — Start with useful data, retain setup context.** New connection defaults
   to a demo connector. Calendar creation with no account leads to an empty
   chooser and requires abandoning the form to link. Offer service choices,
   separate demo/testing entries, check prerequisites before opening a form,
   and return to the intended calendar task after linking. Keep current private
   OAuth configuration available under Advanced. Evidence:
   `web/src/app.ts:888`, `web/src/app.ts:978`.
3. **P1 — Preserve keyboard context and use human names.** Component selection,
   edits, movement and connection polling replace focused DOM controls. Both
   reviewers reproduced lost focus. Accessible component/element labels use IDs
   rather than visible names. Preserve stable controls and logical focus; label
   objects with visible names and context. Evidence: `web/src/app.ts:250`,
   `web/src/app.ts:439`, `web/src/app.ts:950`.
4. **P1 — Protect saved connections and account removal.** Delete/disconnect
   immediately issues DELETE with no confirmation or undo. Add a named
   confirmation explaining the consequences and separate destructive actions
   from Save/Discard. Ordinary reversible layout edits should retain undo rather
   than gain confirmation prompts. Source-inspected without executing deletion:
   `web/src/app.ts:914`, `web/src/app.ts:1097`, `web/src/app.ts:1141`.
5. **P2 — Recover space and improve reading comfort.** The header uses about
   220 px before the workspace; functional labels/help are frequently 8–11 px.
   Compact the header, enlarge controls/type and make technical sections
   collapsible. Keep surrounding controls accessible at narrower effective
   desktop widths. Evidence: `web/src/app.css:165`, `web/src/app.css:677`.

Assessment B adds concrete implementation requirements:

- Secondary text includes measured contrast ratios of 3.98:1 and 2.76:1.
  Normal text needs at least 4.5:1 under
  [WCAG contrast minimum](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html).
  Establish semantic text/surface/state tokens and verify every functional pair.
- Clustered block buttons measure 17 × 22 px with 3 px gaps, failing the
  [24 px target/spacing requirement](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html).
  Increase targets and spacing; numeric alternatives already cover resize handles.
- An optional `/api/icon-packs` failure rejects the six-request startup
  `Promise.all`, hiding an otherwise valid workspace. Load essential state
  independently, retain the preview and offer local recovery for failed catalogs.
- Use deliberate loading, empty, retry and unavailable states; avoid loading
  messages and broken-image chrome surviving an initialization failure.

## Journey and cognitive load

The initial display feels calm and trustworthy. Selecting content exposes a
dense inspector; entering element editing changes layout semantics. Google
setup is the deepest interruption, replacing the intended calendar task with
OAuth configuration and a form detour. Five of eight cognitive-load checklist
items failed for a populated flow component: focus, chunking, hierarchy,
one-task-at-a-time and number of choices.

- **Jordan, first-time owner:** cannot readily distinguish real services from
  demo connectors; dataset IDs, row indices and OAuth details dominate simple tasks.
- **Sam, keyboard/low-vision owner:** loses focus after ordinary edits and even
  while waiting in Connections; tiny low-contrast labels add reading effort.
- **Alex, experienced tinkerer:** benefits from save/undo/nudging, but deep
  property scrolling and implicit conversion undermine precision work.

## Coverage and limits

| Flow | Review performed |
| --- | --- |
| Display and draft | Actual frame, saved/draft switching, selection, undo/discard |
| Component editing | Flow, positioned items, bindings, text/numeric edits, template insertion |
| Recovery | Overlap rejection, dirty-dialog close; browser-local empty/503 simulations |
| Data sources | Lists, new/edit forms, enabled state, secret-field presentation and actions |
| Google accounts | Existing setup UI, empty-account calendar form, prerequisite journey |
| Display settings | Quiet hours, timezone, refresh policy and draft semantics |
| Input/layout | Keyboard focus, labels, target sizes; desktop 1280–1920 px; 720 px observation |

No real saves, deletes, external fetches, refreshes or account linking were
executed. Real OAuth consent, Pi operation, physical display, screen reader,
actual 200% zoom and physical touch-device validation remain separate. Browser
touch emulation confirmed move/cancel; it is not hardware validation.

The detector ran once: 56 raw records (49 warnings, seven advisories), not 56
independent defects. Browser/source inspection rejected generic cream-palette,
legitimate canvas-grid, loaded-image and padding false positives. Reviewers
used headless Chromium; an overlay was injected in a dedicated review context,
not a browser visible to the owner. Production CSP was unchanged.

## Visual directions for feedback

These are static, code-rendered proposals using synthetic data, not implemented
application screens. All seven were rendered at 1440 × 900, checked for loaded
images/document overflow and reviewed for material clipping and state conflicts.
The baseline is [Display](ui-ux-review-2026-11-15/before-display.png) and
[Google setup](ui-ux-review-2026-11-15/before-google.png).

| Direction | Structure | Tradeoff |
| --- | --- | --- |
| [A — Canvas + task dock](ui-ux-review-2026-11-15/canvas-dock.png) | Top navigation, content inspector right, components below canvas | Recommended for the confirmed owner audience; long component lists need browsing |
| [B — Bottom inspector](ui-ux-review-2026-11-15/bottom-inspector.png) | Side navigation, wide canvas, properties below | Clear horizontal editing space; less vertical preview space |
| [C — Studio workbench](ui-ux-review-2026-11-15/studio-workbench.png) | Components left, canvas centre, inspector right | Familiar precision workflow; three persistent columns |

Shared proposals:
[Data sources](ui-ux-review-2026-11-15/data-sources.png),
[Google accounts](ui-ux-review-2026-11-15/google-accounts.png),
[quiet hours](ui-ux-review-2026-11-15/display-settings.png),
[Odd Orbit/Orbi About placement](ui-ux-review-2026-11-15/brand-about.png).

Seven grounded workspace structures were considered: three-pane studio,
contextual canvas drawer, guided setup followed by studio, bottom inspector,
draft/current comparison, source-centred workspace and canvas/task dock.
Surface seed `seed:c31a67cdbd74cef9` dealt structures 7, 4 and 1. The proposals
keep the existing visual identity; seeded unrelated visual worlds were unsuitable
for this established utility.

Approved artwork came from `../Odd-Orbit-Brand/avatars/odd-orbit-avatar-128.png`
and `../Orbi/hd/reference-panels/hero-portrait.png`. Existing opaque backgrounds
and logo proportions are preserved. Orbi is a framed approved reference for
placement feedback, not a newly prepared production asset.

## Decisions and implementation order

Confirmed: everyday-owner focus, desktop priority, paper/green palette,
separate advanced controls and Odd Orbit attribution.

Owner decision on 15 November 2026: **A — Canvas + task dock**, with the
recommended branding: approved logo/attribution footer and Orbi on About only.
Implementation is authorised on branch `UIUX`.

After feedback: implement the chosen shell and semantic styling, content-first
editing, focus preservation, data-source/Google journeys, recovery and destructive
action protection. Verify with meaningful browser tests and screenshots across
existing flows. Document implemented behaviour in `docs/design`. Run the full
Rust check if Rust changes are needed. Keep version `0.1.0`.

Standalone component design before placement, shared public Google OAuth,
client JSON import and GitHub release updates remain separate planned features.
This review does not claim them as implemented. No commit or push is included.
