# Render a simulated display

The simulator renders a saved layout with sample or supplied datasets. It does
not require a Pi, physical display, API credentials, or browser engine.

From the repository root:

```sh
cargo run --locked -p paperphase -- simulate \
  --layout examples/simulator/layout.json \
  --data examples/simulator/data.json \
  --now 1700000000 \
  --output /tmp/paperphase-preview.png \
  --pbm /tmp/paperphase-frame.pbm
```

Open `/tmp/paperphase-preview.png` in an image viewer. PNG and PBM contain the
same monochrome pixels. The JSON report prints display dimensions, packed byte
count, render time, and each component's freshness and warnings.

The example uses fictional/sample data. With the supplied render time, weather
and headlines are fresh and the Luas estimate is stale. Its `STALE DATA` label
is deliberate. The calendar text is static sample content, not a connected
Google Calendar. No live clock component is implemented yet.

Use `--now` for reproducible freshness checks. Omitting it uses the host's current
Unix time; these fixed sample snapshots will then become stale. Omitting `--data`
renders literal text normally and marks unresolved field components `NO DATA`.

## Edit a layout

Change `examples/simulator/layout.json`, or copy it to your own file. Geometry
is expressed in native display pixels. Components cannot overlap or extend
outside the display. Each component contains vertically arranged text blocks.

- Change a block's literal text or its connection/dataset/field binding.
- Change its font size, alignment (`left`, `center`, `right`), and overflow
  (`wrap`, `clip`, `ellipsis`).
- Adjust component dimensions, padding, gaps, and optional borders.
- Keep IDs stable when changing captions or formatting.

This is a file-based workflow while the browser editor is being built. Invalid
input is rejected before image creation; an existing output is preserved when
layout validation fails. Path checks also reject output aliases of the inputs
or another output; Unix hard-link identity is checked as well. Inputs are capped
at 1 MiB each. Output-image writes
are not a transactional layout/configuration store.

The 800 × 480 example is an explicitly simulated profile. It does not select or
confirm a Waveshare panel model. You may change its dimensions within simulator
limits to exercise other sizes.

## Supply data

Each snapshot combines a `connection_id`, typed `dataset`, successful-fetch time,
stale interval, optional source `observed_at`, and optional `last_error`. All
timestamps are Unix seconds. Keep the last good dataset and update `last_error`
when modelling a failed fetch; its content stays visible with a stale label.

Bindings match the complete connection/dataset/field identity. Dataset names
can repeat across different connections. Numeric precision and literal
prefix/suffix settings affect presentation without refetching data.

For the schemas, algorithms, and generated frame, see the
[shared data and renderer design](../design/data-and-rendering.md). The
[foundation harness](foundation-harness.md) describes compiling and executing
the actual Rust guest plugin.

## Checks

```sh
sh scripts/check.sh
```

The complete check builds the guest and includes the compiled-plugin-to-renderer
workflow, alongside model, renderer, CLI, and Wasm boundary tests. It needs the
Wasm target and permission to open loopback sockets. It is desktop validation;
Pi resource measurements and physical panel tests are still separate work.
