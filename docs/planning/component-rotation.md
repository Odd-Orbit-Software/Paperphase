# Alternating components and making better use of display space

Status: owner approved recording all six ideas below as potential screen
improvement TODOs, to revisit after the physical display works reliably.
Automatic full-refresh integration is now working; partial-refresh and longer
panel qualification remain ahead of this backlog. These are not implemented
features or a commitment to build every option immediately.
Detailed UX, cadence, priority within this group and acceptance criteria will be
settled using the real panel's refresh/ghosting measurements.

## Potential screen improvements — after physical-display integration

- [ ] Alternate components in one area, with order, timing and browser preview.
- [ ] Combine multiple feeds into one source-labelled headline list.
- [ ] Add display pages while keeping important information pinned.
- [ ] Schedule different views for different times or days.
- [ ] Prioritise relevant upcoming events, departures and meaningful changes.
- [ ] Alternate compact summaries with fuller detail views, retaining readable text.

Start by evaluating pinned information plus one rotating news area. The
scheduler/driver work can account for future rotation deadlines without adding
these user-facing features before hardware validation.

## Requested capability: show components in turns

Let an owner select a component and choose **Show in turns**, then add compatible
existing or saved designs to the same display area. Show a simple ordered list,
a **Switch every** setting, and previous/next preview controls in the browser.
The display is not assumed to have touch controls. A first version should keep
one fixed outer rectangle and fit every member inside it; it must not silently
shrink text, crop a design or move neighbouring components.

Model this as one display-area owner containing multiple alternate component
presentations. Its outer rectangle obeys the existing no-overlap rule against
other components. Internal items retain the newly approved overlap behavior.
Only the active presentation renders; hide inactive content entirely. Keep
source/attribution visible where required. An optional position cue such as
“2 of 4” can explain that more information is available without taking much room.

Prefer a slow, hardware-appropriate default (five minutes is a proposal, not a
validated panel setting). Keep time-critical information pinned while lower
urgency news rotates. Allow pausing/turning off rotation and previewing every
member without waiting for physical refreshes. Advanced scheduling can be added
later rather than leading the first-use workflow.

## Reliability and acceptance

- Rotate cached snapshots; do not tie provider fetches to presentation changes.
- Share the normal host renderer, serial refresh controller, quiet hours, rate
  limits, maintenance/full-refresh rules and retry/backoff behavior.
- Coalesce eligible changes where possible; do not race independent timers or
  replay a backlog of missed transitions after sleep/restart.
- Give a successfully shown component its readable dwell time. Keep the last
  completed image if a replacement frame or panel operation fails. Treat an
  unchanged frame as a logical transition without unnecessary panel activity.
- Distinguish a successful empty source from unavailable data. An optional
  skip-empty policy must leave a meaningful fallback if every member is empty;
  stale last-good information must remain labelled rather than disappearing
  silently. Reuse existing data-status/privacy behavior.
- Retain named designs and connection bindings; deleting a design/connection
  must not silently switch a group to unrelated content. Define usable recovery
  for missing members and incompatible dimensions.
- Test manual preview, reorder/remove, persistence/restart, all-empty/unavailable
  members, varying data lengths, source errors, busy/failed refreshes, clock
  changes, quiet-hours resume and physical readability/ghosting.

## Details for the deferred improvement backlog

The owner approved retaining these options as potential TODOs after hardware
work; their final implementation scope remains to be decided:

1. **Combined source lists:** merge selected feeds into one headline list, retain
   source labels, and show only the useful top few entries. Complements alternate
   custom designs when owners prefer one consistent news layout.
2. **Display pages with pinned information:** rotate broader views such as
   Calendar, Travel and News, while keeping a small clock/next-event area fixed.
3. **Scheduled views:** show travel information on weekday mornings and a
   different view in the evening, with clear timezone and manual override.
4. **Relevant information first:** show upcoming events/departures and meaningful
   changes such as imminent rain rather than distant or unchanged detail. Keep
   ranking/eligibility understandable and critical information visible.
5. **Summary and detail:** compact “next / later / count” summaries, with slower
   detail pages for longer headlines or agendas. Preserve the owner-approved
   readable text floor rather than automatically squeezing fonts smaller.

Suggested starting combination: pinned important information, one rotating news
area, and combined feeds where a shared template is sufficient. Evaluate the
physical refresh results before adding several independent rotation areas or
whole-screen cycling. Avoid scrolling text/animations on this display.
