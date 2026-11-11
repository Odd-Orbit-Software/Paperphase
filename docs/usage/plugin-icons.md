# Use and replace plugin icons

The weather registration in `plugins/bindings.example.json` includes
`"icon_manifest": "weather/assets/icons.json"`. This path is relative to the
registration file, like its Wasm path. Keep the plugin asset directory with the
module when moving an installation. Restart after changing registration/assets.
The host binary contains no Meteocons artwork.

Add a new Weather now, Hourly forecast or Daily forecast template to use icons.
Existing saved components retain their layout and font symbols. To change one,
select it, choose **Edit contents**, select/add an element, then choose **Icon**
as its content source. Choose the pack, size and either a fixed icon or a dataset
field. For weather, bind `icon-key` from the desired current/hourly/daily row.
Move or resize its element box as usual, then **Save to display**.

After updating an existing installation, older cached weather datasets will not
have `icon-key` until their next permitted successful fetch. Fetch status still
controls when a request is allowed. A missing pack produces a visible warning in
the inspector and a `?` in the preview; load its manifest and restart.

## Substitute a sprite sheet

Replace the PNGs and update the manifest's variants and cell coordinates. Keep
pack ID `weather.open-meteo` and the documented semantic keys to reuse existing
bindings. One variant is sufficient; multiple sizes improve readability. Each
variant uses a uniform square grid, with zero-based rows/columns. Use black ink
on white or transparent backgrounds. Transparent/light pixels do not draw ink.
Icon and element sizes remain explicit, so give the element enough room.

See the [manifest and condition table](../design/plugin-icons.md). Asset licences
belong with their plugin and remain separate from Paperphase's code licence.
Meteocons' MIT notice is retained in `plugins/weather/assets/meteocons/LICENSE`.

## Rebuild the bundled pack

Original SVGs and package version/integrity are pinned in `assets/sources.json`.
From the repository root, with the web development dependencies and Playwright's
Chromium installed:

```sh
node plugins/weather/tools/build-icons.mjs
```

The helper works offline and overwrites only the five PNG atlases and manifest.
`PLAYWRIGHT_BROWSERS_PATH` may point to an existing Chromium cache. Browser
rasterisation is a development step; Rust reads the generated PNGs at runtime.
Review regenerated artwork because browser rasterisation can vary by version.
