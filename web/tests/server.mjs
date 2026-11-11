import { mkdtempSync, rmSync, writeFileSync, existsSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';

const directory = mkdtempSync(join(tmpdir(), 'paperphase-editor-'));
const modulePath = process.env.PAPERPHASE_TEST_PLUGIN || resolve('../target/wasm32-unknown-unknown/release/paperphase_temperature_example.wasm');
if (!existsSync(modulePath)) throw new Error('Build the compiled guest with sh scripts/check.sh before browser tests.');
const source = createServer((_request, response) => { response.writeHead(200, { 'Content-Type': 'application/json' }); response.end('{"temperature_c":18.5}'); });
await new Promise(resolve => source.listen(0, '127.0.0.1', resolve));
const weatherModule = process.env.PAPERPHASE_TEST_WEATHER_PLUGIN || resolve('../target/wasm32-unknown-unknown/release/paperphase_weather_plugin.wasm');
const rssModule = process.env.PAPERPHASE_TEST_RSS_PLUGIN || resolve('../target/wasm32-unknown-unknown/release/paperphase_rss_plugin.wasm');
for (const module of [weatherModule, rssModule]) if (!existsSync(module)) throw new Error('Build weather/RSS guests with scripts/check.sh.');
const weather = createServer((_request, response) => { response.writeHead(200, { 'Content-Type': 'application/json' }); response.end(readFileSync('../plugins/weather/fixtures/forecast.json')); });
const luasModule = process.env.PAPERPHASE_TEST_LUAS_PLUGIN || resolve('../target/wasm32-unknown-unknown/release/paperphase_luas_plugin.wasm');
if (!existsSync(luasModule)) throw new Error('Build the Luas guest with scripts/check.sh.');
const luas = createServer((_request, response) => { response.writeHead(200, { 'Content-Type': 'text/xml' }); response.end(readFileSync('../plugins/luas/fixtures/forecast.xml', 'utf8').replace('stop="Ranelagh"', 'stop="Charlemont"').replace('stopAbv="RAN"', 'stopAbv="CHA"')); });
const feed = createServer((_request, response) => { response.writeHead(200, { 'Content-Type': 'application/rss+xml' }); response.end(readFileSync('../plugins/rss/fixtures/headlines.xml')); });
await Promise.all([weather, feed, luas].map(server => new Promise(resolve => server.listen(0, '127.0.0.1', resolve))));
const feedUrl = `http://127.0.0.1:${feed.address().port}/feed.xml`;
const bindings = join(directory, 'bindings.json');
writeFileSync(bindings, JSON.stringify({ schema_version: 1, bindings: [
  { connector_id: 'example.temperature', wasm: modulePath, source_url: `http://127.0.0.1:${source.address().port}/observation`, stale_after_secs: 60 },
  { connector_id: 'weather.open-meteo', icon_manifest: resolve('../plugins/weather/assets/icons.json'), wasm: weatherModule, source_url: `http://127.0.0.1:${weather.address().port}/forecast`, origin_interval_secs: 2, stale_after_secs: 3600 },
  { connector_id: 'transport.luas', wasm: luasModule, source_url: `http://127.0.0.1:${luas.address().port}/get.ashx`, origin_interval_secs: 60, stale_after_secs: 120 },
  { connector_id: 'rss.headlines', wasm: rssModule, source_setting: 'feed_url', origin_interval_secs: 2, stale_after_secs: 21600 },
] }));
// Cold records with later automatic deadlines make Fetch now deterministic.
writeFileSync(join(directory, 'workspace.connections.json'), JSON.stringify({ schema_version: 1, revision: 1, connections: [
  { id: 'temperature.browser', connector_id: 'example.temperature', label: 'Bench sensor', enabled: true, poll_interval_secs: 300, settings: { location: 'Workshop', units: 'imperial' } },
  { id: 'weather.browser', connector_id: 'weather.open-meteo', label: 'Dublin forecast', enabled: false, poll_interval_secs: 900, settings: { location: 'Dublin', latitude: 53.3498, longitude: -6.2603, units: 'metric', forecast_hours: 24, forecast_days: 3, rain_threshold: 50 } },
  { id: 'luas.browser', connector_id: 'transport.luas', label: 'Charlemont trams', enabled: false, poll_interval_secs: 60, settings: {stop:'CHA', direction:'both', max_arrivals:3, destination:''} },
  { id: 'rss.browser', connector_id: 'rss.headlines', label: 'Local headlines', enabled: false, poll_interval_secs: 1800, settings: { label: 'Sample headlines', feed_url: feedUrl, max_items: 3 } },
], data: { 'temperature.browser': { generation: 1, next_fetch_at: 1700003600 } } }));
const child = spawn(process.env.PAPERPHASE_TEST_BINARY || resolve('../target/debug/paperphase'), ['serve', '--listen', '127.0.0.1:8097', '--state-file', join(directory, 'workspace.json'), '--connector-bindings', bindings, '--now', '1700000000'], { stdio: 'inherit' });
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => child.kill('SIGINT'));
child.on('exit', code => { source.close(); weather.close(); feed.close(); luas.close(); rmSync(directory, { recursive: true, force: true }); process.exit(code ?? 1); });
child.on('error', error => { console.error(error); source.close(); weather.close(); feed.close(); luas.close(); rmSync(directory, { recursive: true, force: true }); process.exit(1); });
