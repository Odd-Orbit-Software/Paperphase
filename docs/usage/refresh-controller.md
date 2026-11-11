# Simulated refresh policy

The information below the canvas shows the active or last completed refresh
mode and its reason. Hover the mode to see its pixel region and layout revision.
Saved content can be queued while an older refresh is active; **On display**
continues to show the last completed frame until that operation finishes.

In **Edit layout → Display settings**, adjust:

- **Minimum refresh interval (ms)**: minimum wait after a successful update before
  another starts. The simulator enforces a floor of 250 ms.
- **Full refresh after partial updates**: number of successful partial updates
  allowed before the next changed frame requires full refresh; maximum 20 in
  the simulator profile.
- **Maintenance interval (seconds)**: full-refresh age that requires full refresh
  when content next changes; the profile caps this at 7200 seconds.
- **Largest partial area (%)**: maximum aligned region eligible for partial
  refresh; the profile caps this at 60% of display area.

Save applies these settings; Undo and Discard work as for layout edits. More
permissive settings never bypass profile limits. These are synthetic simulator
values, not recommended settings for any Waveshare panel.

Select a component and use **Refresh request** to choose Automatic, Prefer
partial, or Request full. Requests apply when its area changes. The host can
promote partial to full for maintenance, area, capability, or recovery reasons.

**Refresh display** reevaluates saved content. Unchanged pixels require no
operation, and the button obeys the same interval, retry, and quiet-hours rules.
Fetching is separate and continues during quiet hours. An operation that already
started finishes before the pause; queued content waits until quiet hours end.

The simulator initialises from the saved layout on restart and resets its
in-memory refresh history. It does not read a physical display. Driver failure
recovery is exercised through Rust tests; there is no operator fault-injection
control in this release.
