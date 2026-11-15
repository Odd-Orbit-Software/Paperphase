# Paperphase product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Everyday owners of a Raspberry Pi e-paper display who want to connect useful
information and arrange it without understanding the implementation. Advanced
controls remain available separately. Confirmed during the 2026-11-15 UI review.
Desktop browsers, mouse and keyboard are the primary editing environment; full
phone/tablet editing is not an initial requirement.

## Product Purpose

Configure a calm information display: connect data, arrange readable components,
and explicitly save an arrangement that the display applies through its refresh
policy. The physical display representation is the main working surface.

## Operating Context

The Rust service runs on a Pi and serves a browser editor over the owner's LAN.
Development and this UI review run locally against synthetic fixtures. The
current service uses a simulated display backend; a separate one-shot physical
full refresh has been tested. Automatic physical updates remain planned.

## Capabilities and Constraints

- Current desktop flows include saved/draft views, drag/resize, keyboard movement,
  numeric geometry, undo/redo, component content editing, data bindings, plugin
  templates, connection forms, fetching/status, quiet hours and refresh settings.
- Component rectangles do not overlap; an invalid placement restores the last
  valid position. Explicit save/discard and saved/pending/displayed distinctions
  must remain clear and accurate.
- Calendar linking with a private Google OAuth client works. Shared public OAuth
  and client JSON import remain planned; do not present them as implemented.
- Component design before placement is the next functional feature, separate from
  this review of current flows. Preserve its agreed scope in docs/planning.
- Keep secrets masked and out of screenshots, logs, source assets and fixtures.
  Local review data and account states must be synthetic.
- Experimental version remains 0.1.0; early stable release has not been reached.

## Brand Commitments

Paperphase remains the product identity. Preserve the paper-inspired palette
and its green character. Add restrained Odd Orbit attribution and a clear
“Developed by Odd Orbit Software” footer; do not make the app match the parent
brand's dark/violet palette throughout. Orbi is available as a supporting mascot;
the owner approved Orbi on the About page only on 2026-11-15. The routine
workspace uses the logo and attribution without mascot illustrations.

Approved company references: ../Odd-Orbit-Brand/guide/BRAND-GUIDE.md and its exact
logo artwork. Mascot references: ../Orbi/CHARACTER-BIBLE.md,
../Orbi/STYLE-GUIDE.md and ../Orbi/canonical/approved-references/. Existing raster
assets have opaque backgrounds and must not be represented as transparent art.

## Evidence on Hand

The local compiled-service fixture harness, 20 browser tests, source and current
screenshots provide evidence of existing flows. The last full runtime validation
passed 166 Rust tests. Pi login and physical bring-up evidence are separate from
this local UI review; they do not prove every Calendar or display lifecycle case.

## Product Principles

- Make the display and its content the focus of the workspace.
- Use owner-facing language; reveal technical settings when needed.
- Protect drafts and explain the consequence of save, discard and refresh.
- Show state accurately, with useful recovery steps rather than vague errors.
- Let Paperphase lead while Odd Orbit provides a modest maker signature.

## Open Decisions

On 2026-11-15 the owner chose option A: top navigation, a canvas with a content
inspector on the right, and a component strip below. Implement on branch UIUX,
keeping advanced controls separate. Branding uses the approved Odd Orbit logo
and attribution throughout, with Orbi on About only. Screenshots and the review
contract are recorded in docs/planning/ui-ux-review-2026-11-15.md.
