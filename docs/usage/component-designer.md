# Design a reusable component

Open **Component designs** from Display. Choose **Create design** beside a plugin
or **Text & time**. You can also choose **Create custom design** beneath a source
in the Add component picker. A source does not need a supplied template.

1. Choose a source now or later, and start blank or from a supplied design.
2. Add text, a data field or time. Edit the selected item in Content. The preview
   fits closely around stacked items, starting at readable 14 px text.
3. Give it a useful design name. **Text & size** changes text size and width;
   **Fit to contents** calculates a compact box using current values. Leave room
   for longer future information. Position and Style contain precise controls.
4. **Save design** stores a reusable design on this Paperphase. It does not
   change the display. You may save without a configured source, then choose one
   when using it later.
5. **Add to display** puts the design in your layout draft. Review it and choose
   **Save to display** to update the screen. If there is no free area, your design
   stays open; save it for later or make it smaller.

Use **Arrange freely** to move individual items inside their own boxes. Items
may overlap inside one component; whole components must remain separate. Use
**Stack items** to return to automatic stacking. Select an item with the mouse
and press Delete to remove it; Undo restores it. Delete in a text field edits
text. The last remaining item is protected.

For an existing component, choose **Edit layout**, select it, then **Design this
component** in Content. **Apply to layout** replaces that component in your draft.
**Close designer** restores the layout draft you had before entering. Unsaved
design changes require confirmation before being discarded.

Clipping warnings identify affected items but do not block Save design.
**Fit to contents** enlarges clipped item boxes while keeping their positions and
font sizes. You can also adjust an individual box in Position or its font in Style.

Saved designs appear in Component designs and under compatible sources in the
Add picker. Reuse them with another connection of the same plugin. Deleting a
saved design requires confirmation and keeps components already on the display.
If another tab changes the library, reload saved designs and save again; your
current design edits remain available.

Back up the adjacent `*.designs.json` file with your workspace. Designs may contain
personal text; keep this backup private.

For a changing icon, select the icon item and choose **Icon selection → Dataset
field**. Pick the information group and a text field containing icon names from
the chosen pack. Weather provides **Current weather → Icon key**; this is the
default when switching a weather icon to a data field. Numeric readings cannot
directly select an icon without a plugin field that maps them to icon names.
