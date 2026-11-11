# Bundled weather artwork

Static monochrome Meteocons by Bas Milius, MIT licensed:
https://github.com/basmilius/meteocons

`meteocons/` contains the selected original SVG files and upstream MIT licence.
`sources.json` pins `@meteocons/svg-static` 0.1.0, its npm SHA-512 integrity and
the mapping from Paperphase semantic keys to source filenames. The downloaded
npm tarball was checked against that integrity before extracting these files.

`icons.json` and `icons-{16,24,32,48,64}.png` are generated monochrome atlases.
Rebuild using `node plugins/weather/tools/build-icons.mjs` from the repo root.
Original files are unmodified; generated atlases rasterise and threshold them.
Some conditions intentionally share drawings. See the complete mapping in
`sources.json` and the [design](../../../docs/design/plugin-icons.md).
