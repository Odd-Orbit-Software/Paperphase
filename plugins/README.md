# Paperphase plugins

- `weather`: Open-Meteo current conditions plus hourly and daily forecasts.
- `rss`: bounded UTF-8 RSS 2.0 and Atom headline extraction.
- `luas`: Luas stop forecasts, both directions and service messages.
- `dublin-bus`: numbered multi-stop NTA bus forecasts with shared reference data.
- `air-quality`: modelled AQI, pollutant forecasts and seasonal pollen.
- `daily-quote`: an online daily quote with optional personal/mixed selection.
- `generic-json`: typed JSON Pointer mappings with optional private headers.
- `google-calendar`: host-owned OAuth and privacy-filtered calendar agendas.
- `bindings.example.json`: local module registration for all eight plugins.

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

Dublin Bus needs an NTA key in its private settings form. Its public reference
cache is host-managed. See [setup](../docs/usage/dublin-bus.md) and
[design](../docs/design/dublin-bus.md).

See [air/quote/JSON setup](../docs/usage/air-quotes-json.md) and
[design](../docs/design/air-quotes-json.md) for the next plugin stretch.

Google Calendar has [browser-only setup](../docs/usage/google-calendar.md), a
[self-deployed HTTPS callback](../services/google-oauth-callback/README.md) and
[account/aggregation design](../docs/design/google-calendar.md). Real Google
consent and renewal still need account validation.
