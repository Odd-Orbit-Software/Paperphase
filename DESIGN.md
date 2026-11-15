---
name: Paperphase
description: Warm paper and green controls for arranging a calm e-paper display.
colors:
  paper: '#f5f4ef'
  panel: '#fffef9'
  canvas: '#eeeee7'
  ink: '#25352d'
  muted: '#566453'
  green: '#2f5745'
  green-hover: '#244535'
  selected: '#e3ebdd'
  line: '#d4dbcd'
  input-line: '#788b70'
  danger: '#923c30'
  error-surface: '#fff0ec'
  amber: '#795013'
  amber-surface: '#f7edd8'
  focus: '#2e6849'
  topbar: '#faf9f4'
  field: '#fffffc'
  white: '#ffffff'
  element: '#486f8a'
typography:
  headline:
    fontFamily: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe
      UI", sans-serif
    fontSize: 25px
    fontWeight: 650
    lineHeight: 1.25
    letterSpacing: -.02em
  title:
    fontFamily: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe
      UI", sans-serif
    fontSize: 19px
    fontWeight: 650
    lineHeight: 1.25
    letterSpacing: -.02em
  section:
    fontFamily: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe
      UI", sans-serif
    fontSize: 16px
    fontWeight: 650
    lineHeight: 1.25
    letterSpacing: -.02em
  body:
    fontFamily: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe
      UI", sans-serif
    fontSize: 14px
    lineHeight: 1.6
  label:
    fontFamily: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe
      UI", sans-serif
    fontSize: 13px
    fontWeight: 600
  button:
    fontFamily: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe
      UI", sans-serif
    fontSize: 14px
    fontWeight: 600
    lineHeight: 1.3
  help:
    fontFamily: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe
      UI", sans-serif
    fontSize: 13px
    lineHeight: 1.6
  caption:
    fontFamily: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe
      UI", sans-serif
    fontSize: 12px
  brand:
    fontFamily: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe
      UI", sans-serif
    fontSize: 25px
    fontWeight: 700
    letterSpacing: -.035em
  about-headline:
    fontFamily: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe
      UI", sans-serif
    fontSize: 30px
    fontWeight: 650
    lineHeight: 1.25
    letterSpacing: -.02em
rounded:
  badge: 5px
  field: 6px
  button: 7px
  table-row: 8px
  panel: 12px
spacing:
  '4': 4px
  '6': 6px
  '8': 8px
  '10': 10px
  '12': 12px
  '14': 14px
  '16': 16px
  '18': 18px
  '20': 20px
  '22': 22px
  '24': 24px
  '28': 28px
  '32': 32px
  '40': 40px
  '48': 48px
components:
  button-primary:
    backgroundColor: '{colors.green}'
    textColor: '{colors.white}'
    typography: '{typography.button}'
    rounded: '{rounded.button}'
    padding: 9px 14px
  button-primary-hover:
    backgroundColor: '{colors.green-hover}'
  button-secondary:
    backgroundColor: '{colors.panel}'
    textColor: '{colors.ink}'
    typography: '{typography.button}'
    rounded: '{rounded.button}'
    padding: 9px 14px
  button-secondary-hover:
    backgroundColor: '{colors.selected}'
  button-quiet:
    backgroundColor: transparent
    textColor: '{colors.ink}'
    typography: '{typography.button}'
    rounded: '{rounded.button}'
    padding: 9px 14px
  button-small:
    typography: '{typography.label}'
    rounded: '{rounded.button}'
    padding: 7px 10px
  field:
    backgroundColor: '{colors.field}'
    textColor: '{colors.ink}'
    rounded: '{rounded.field}'
    padding: 10px 11px
  nav-active:
    backgroundColor: '{colors.selected}'
    textColor: '{colors.green}'
    typography: '{typography.button}'
    rounded: '{rounded.button}'
    padding: 9px 14px
  badge:
    backgroundColor: '{colors.selected}'
    textColor: '{colors.ink}'
    typography: '{typography.caption}'
    rounded: '{rounded.badge}'
    padding: 5px 9px
  badge-draft:
    backgroundColor: '{colors.amber-surface}'
    textColor: '{colors.amber}'
    typography: '{typography.caption}'
    rounded: '{rounded.badge}'
    padding: 5px 9px
  panel:
    backgroundColor: '{colors.panel}'
    textColor: '{colors.ink}'
    rounded: '{rounded.panel}'
  inspector-tab:
    backgroundColor: transparent
    textColor: '{colors.ink}'
    padding: 12px 8px
  component-item-selected:
    backgroundColor: '{colors.selected}'
    textColor: '{colors.ink}'
    rounded: '{rounded.button}'
    padding: 8px 10px
    width: 150px
  canvas-selection:
    backgroundColor: transparent
---

# Design System: Paperphase

## Overview

**Creative North Star: "Warm paper, green, calm utilitarian desktop editor"**

Paperphase is a warm paper and green, calm utilitarian desktop editor. It uses familiar native controls, clear labels and quiet borders so owners can arrange a readable e-paper display. System fonts keep the local application self-contained and consistent with the approved code-led prototype.

Paperphase leads the workspace identity. The supplied Odd Orbit logo and “Developed by Odd Orbit Software” attribution provide a modest maker signature throughout; Orbi appears on About only. The parent brand’s dark and violet artwork stays within its supplied asset and About maker card.

**Key Characteristics:**
- Warm paper surfaces, forest green actions and dark green ink.
- Restrained controls, fine borders and limited shadows.
- Desktop mouse and keyboard editing with accessible surrounding controls at narrower widths.
- Original supplied maker artwork, with Orbi on About only.

This captures the implemented system on branch `UIUX`, following the owner's
approved direction in [PRODUCT.md](PRODUCT.md) and
[uiux-direction.md](docs/planning/uiux-direction.md). The normative implementation
is [app.css](web/src/app.css), with structure and behavior in
[index.html](web/src/index.html) and [app.ts](web/src/app.ts). Frontmatter records
current primitives, including recurring literal CSS values; it does not introduce
new CSS variables. Token values take precedence over descriptions. Review captures
in `.impeccable/review/` use synthetic states and document local desktop rendering,
not physical display or account validation.

## Colors

Warm neutral layers support a single forest green action palette. Functional
error, warning and inner-element selection colors remain distinct.

### Primary

- **Forest green** (`green`): primary actions, links, accent/caret color and component tags.
- **Deep forest green** (`green-hover`): enabled primary-button hover.
- **Selection wash** (`selected`): active navigation, selected dock items and secondary-button hover.
- **Focus green** (`focus`): keyboard focus rings and selected component outlines.

### Neutral

- **Warm paper** (`paper`): application background and browser theme color.
- **Near-white paper** (`panel`): panels, buttons, inspector and form action bars.
- **Pale canvas** (`canvas`): display surround and caption, distinct from the frame itself.
- **Green ink** (`ink`): primary text.
- **Muted green** (`muted`): explanatory copy, captions and metadata.
- **Quiet line** (`line`): surface outlines and section dividers.
- **Field line** (`input-line`): stronger input boundaries and hovered button borders.
- **Topbar paper** (`topbar`) and **field white** (`field`): implemented local surface variants.
- **Frame white** (`white`): monochrome display background and primary-action text.

Error text uses `danger` with `error-surface`. Draft, busy and warning states use
`amber` with `amber-surface`. The inner-element editor uses `element` blue to
distinguish element geometry from green component geometry; it is a functional
editing cue, not an additional brand accent. Status dots supplement text.

**The Paperphase Identity Rule.** Keep the editor's paper and green palette; the
Odd Orbit artwork is a contained maker signature, not a replacement UI palette.

## Typography

Use the exact system sans-serif stack in the frontmatter for all application
roles. No downloaded fonts are required. The display's rendered monochrome type
comes from the display renderer and is not the editor's UI type system.

Headlines and section titles use compact negative tracking and medium-heavy
weight. Body paragraphs use a generous line height and a maximum measure of
75ch. Native fields inherit the surrounding font; within the current labelled
forms that includes the label's size and weight. Keep that inherited behavior
when extending these forms.

The hierarchy is: page headline, panel title, section heading, body, label/help,
then caption. Inspector property headings are a local 15px variant; h4 uses
14px. About has its own larger headline. The wordmark shrinks to 23px at the
small-screen breakpoint. Zoom percentages use tabular numerals.

## Layout

Desktop editing is primary. The implemented Display surface has compact top
navigation, a canvas with the inspector to the right and a component dock below
the canvas. Content, Position and Style tabs keep content editing and precision
controls reachable. Data sources and Device reuse the same panel language with
a left context/sidebar column. This describes these surfaces; it does not make
their exact composition mandatory for every future page.

The topbar is 72px high with 28px horizontal padding and gaps. Main content has
28px horizontal gutters, a centered 2000px maximum width and minimum height
`calc(100vh - 124px)`. Page headings have an 88px minimum height with 18px
vertical padding. The studio uses `minmax(0, 1fr) 320px`, a 20px gap, a 610px
minimum height and `calc(100vh - 232px)` height. The canvas toolbar is at least
50px; the final dock override is at least 78px with 10px vertical padding.
The earlier canvas-heading row is intentionally hidden by the final stylesheet.

The viewport has 12px padding, automatic overflow and a 230px minimum height.
Fit scales the actual frame to available space; zoom ranges from 10% to 200%,
with automatic fit capped at 100% for components and 200% for inner content.
Manual zoom changes by 10 percentage points. The display remains a
two-dimensional surface and may scroll. Dock items have a 150px flex basis,
minimum 52px height and an 8px gap in a horizontally scrolling strip.

Inspector headings use 22px/20px/18px padding; property sections use 20px.
The inspector scrolls independently and its tabs are sticky on wide desktops.
Data sources and Device use a 285px context column and a 20px gap. Connection
fields use 28px/32px padding, settings sections use 28px, and ordinary settings
form controls are capped at 700px. Source settings use two columns with a 24px
gap; table, secret, URL and multiline fields span the full width. Connection
actions sit below the scrolling form content with 18px/28px padding.

Responsive behavior follows the sidecar's exact media boundaries:

- At 1700px and above, the inspector widens to 360px.
- At 1250px and below, topbar/main gutters become 20px, studio gap becomes
  16px and inspector width becomes 300px. Host address and dock heading hide.
- At 950px and below, the inspector stacks below the canvas, studio height is
  automatic, tabs stop sticking, and the service summary hides. Source/Device
  columns become 230px plus flexible content with 14px gaps; source settings
  become one column. Form height/overflow restrictions relax. About uses a
  240px-wide, 360px-high portrait.
- At 620px and below, navigation wraps onto a full-width second row; topbar
  minimum height is 110px and main gutters become 12px. Heading actions,
  toolbar, caption and dock wrap; the dock strip takes a full row. Source,
  Device, table cells and About stack into one column. The canvas panel has a
  560px minimum height. Footer becomes vertical. The source sidebar can scroll
  within 360px; the picker moves to 12px side margins and 20px from the bottom.

Body minimum width is 320px. These adaptations preserve access to navigation,
forms and the canvas; full phone/tablet editing remains outside the initial
release promise. Spacing is a compact observed pixel vocabulary, not a strict
single-multiple grid.

## Elevation & Depth

Most surfaces are flat, separated by tonal changes and one-pixel borders.
The display frame has a restrained shadow (`0 5px 18px #263d2c0d`). The floating
component picker uses a stronger shadow (`0 12px 40px #263d2c26`) to signal its
position above the workspace. Confirmation uses a tinted native dialog backdrop
(`#25352d66`). Full values also live in sidecar extensions and snippets.

There are no CSS transitions or decorative entrance animations in the current
system. Selection, outline, expansion and text changes provide feedback. The
reduced-motion query forces automatic scroll behavior.

## Shapes

Use fine, visible borders with modest rounding. Frontmatter records observed
radii for badges, fields, buttons, table rows and panels. Inspector tabs have
square corners with an active bottom border. Status dots are circular. Canvas
component/element rectangles keep square corners so selection follows the
physical display geometry. The frame itself remains white and pixelated.

The picker and confirmation dialog share panel rounding. The picker is fixed
above the dock, at most 560px wide and `min(650px, 75vh)` high, with scrolling.
The confirmation dialog is at most 480px wide with 20px minimum viewport side
allowance. Avoid turning flat panels into raised cards.

## Components

### Buttons

Familiar and restrained. Base buttons have a one-pixel outline, 40px minimum
height, an 8px internal gap and the frontmatter padding/radius. Primary actions
use white text on green; secondary actions use ink on panel paper; quiet actions
have transparent backgrounds and borders. Enabled hover uses selection wash
and the stronger field boundary, with deep green for primary actions. Disabled
buttons use opacity .45 and a default cursor. A selected dock/source button may
retain full opacity when disabled so its current selection remains legible.
Small buttons are at least 36px high. Toolbar controls use 34px, zoom controls
32px; retain accessible names for icon-only controls. Icons are 19px inline SVGs.

### Inputs / Fields

Fields have a one-pixel field-line boundary, near-white background and the
frontmatter padding/radius. Labels remain visible above controls with a 7px
label-to-field gap; help follows below. Fields and textareas have line height
1.5; textareas are at least 84px tall and resize vertically. Checkboxes remain
native, 17px square, with green accent; their label rows are at least 36px high.
Use native select, time, numeric and password controls for their corresponding
values. Secret inputs remain masked; a saved-secret hint and explicit clear
checkbox explain state without revealing the secret.

All buttons, links, fields, summaries and focusable canvas controls share a
2px focus-green outline offset by 3px. The skip link appears on focus. Labels,
help, errors and notices carry meaning beyond color. Error notices use the
error surface and danger text; warnings use amber text and surface, a
one-pixel warm border and button-like rounding.

### Navigation

Top navigation uses transparent buttons; the current page has selection wash,
green text and `aria-current="page"`. Canvas view buttons use the same selected
treatment with `aria-pressed`. Inspector tabs use 13px medium text; the selected
tab adds a two-pixel green bottom border and weight 650. Tabs expose tablist,
tab and tabpanel relationships, support arrows/Home/End, and keep only the
selected tab in the normal tab order. Keep these states distinct from hover.

### Badges and notices

Small, quiet status aids. Count badges use the selection wash; Editing draft
uses the amber surface and text. The badge is 12px text, compact padding and
modest rounding. Notices use readable text and status/live-region semantics;
never make a color dot the sole statement of service state.

### Panels and forms

Panel paper, quiet outlines and panel rounding recur across the canvas,
inspector, source sidebar and settings. Internal sections divide with a line
rather than a shadow. Advanced settings use native details/summary disclosures.
The Data sources workspace is a page-like nonmodal dialog with static position,
transparent background and no outer dialog frame; confirmation alone uses the
modal overlay treatment.

### Canvas selection and component dock

The real monochrome frame remains the working surface. Component overlays are
transparent until hovered (subtle green wash and dashed green outline) or
selected (two-pixel solid green outline). Inner elements use blue outlines.
Resize handles are 12px squares with two-pixel outlines, positioned 5px outside
the lower-right corner. Keyboard-focused component/element tags become visible;
tags use white text on the matching selection color. Invalid placement uses a
danger boundary and tinted fill and restores the last valid placement.

The optional grid has an 8px pitch. Selection and movement work through the
canvas and the component dock; arrow keys move one display pixel, Shift plus
arrows moves eight. Numeric geometry remains available in Position. Dock items
show a clipped name and a smaller geometry summary; selected items retain a
stronger green-gray border with selection wash. Entering content editing swaps
the dock to inner elements and makes the return-to-components action explicit.

### Maker attribution and About

Keep the Paperphase wordmark and mark in the application header. The footer
uses a 32px Odd Orbit logo tile and the exact “Developed by Odd Orbit Software”
wording. About uses a larger 64px tile within its supplied dark maker card and
a portrait with `object-fit: cover`, `object-position: center 25%` and panel
rounding. The desktop portrait is 300px by 430px. Orbi appears on About only.

Artwork is supplied, approved owner material, not newly generated artwork.
Source and usage provenance are recorded in
[web/assets/brand/README.md](web/assets/brand/README.md): `odd-orbit.png` is the
exact `Odd-Orbit-Brand/avatars/odd-orbit-avatar-128.png` artwork; `orbi.png` comes
from `Orbi/hd/reference-panels/hero-portrait.png`. Preserve their proportions,
colors and opaque backgrounds. These assets are embedded and served locally;
About loads the portrait lazily. Parent-brand dark colors in the artwork and
maker card are local attribution treatments, not Paperphase theme tokens.

## Do's and Don'ts

### Do:

- Do use the current paper/green tokens and local system font stack.
- Do pair state color with text, selection borders or accessible control state.
- Do keep content labels visible and advanced controls separately expandable.
- Do preserve explicit save/discard actions and distinguish saved, pending and displayed state.
- Do preserve the supplied Odd Orbit artwork, proportions and opaque backgrounds.

### Don't:

- Don't extend Odd Orbit’s dark/violet palette across the Paperphase workspace.
- Don't put Orbi in the routine editor, Data sources or Device pages.
- Don't add decorative entrance animation or replace the local font stack with externally loaded fonts.
- Don't blur or smooth the monochrome display frame; retain its pixelated rendering.
- Don't present narrow-screen adaptations as a promise of full phone/tablet editing.
