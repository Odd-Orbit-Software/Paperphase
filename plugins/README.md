# Paperphase plugins

- `weather`: Open-Meteo current conditions plus hourly and daily forecasts.
- `rss`: bounded UTF-8 RSS 2.0 and Atom headline extraction.
- `luas`: Luas stop forecasts, both directions and service messages.
- `bindings.example.json`: local module registration for all three plugins.

All compile to portable Wasm and use host HTTP, settings, dataset publication,
polling, caching, and rendering. Definitions/templates currently live in
`examples/simulator/connectors.json`; package installation and the stable plugin
contract are not finalised. See [setup](../docs/usage/weather-and-rss.md) and
[design](../docs/design/weather-and-rss.md).

Weather's bundled Meteocons artwork, licence, provenance and generated PNG
atlases live in `weather/assets`. The host loads them through the registration's
optional `icon_manifest`. See [icon operation](../docs/usage/plugin-icons.md).

See [Luas setup](../docs/usage/luas.md), [design](../docs/design/luas.md) and its
[data attribution notice](luas/NOTICE.md).
