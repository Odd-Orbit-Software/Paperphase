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
const busModule = process.env.PAPERPHASE_TEST_DUBLIN_BUS_PLUGIN || resolve('../target/wasm32-unknown-unknown/release/paperphase_dublin_bus_plugin.wasm');
if (!existsSync(busModule)) throw new Error('Build the Dublin Bus guest with scripts/check.sh.');
const bus = createServer((request, response) => {
  if (request.headers['x-api-key'] !== 'fixture-secret-key') { response.writeHead(401); response.end(); return; }
  response.writeHead(200, {'Content-Type':'application/protobuf'}); response.end(Buffer.concat([readFileSync('../plugins/dublin-bus/fixtures/trip-updates.pb'),readFileSync('../plugins/dublin-bus/fixtures/sparse-updates.pb').subarray(readFileSync('../plugins/dublin-bus/fixtures/empty.pb').length)]));
});
const archive = createServer((request,response) => {
  if (request.headers['x-api-key']) { response.writeHead(400); response.end(); return; }
  const zip = readFileSync('../plugins/dublin-bus/fixtures/reference.zip'); const etag='"fixture-v1"';
  if (request.method === 'HEAD') { response.writeHead(request.headers['if-none-match'] === etag ? 304 : 200, {'Content-Length':zip.length, ETag:etag}); response.end(); return; }
  const match = /^bytes=(\d+)-(\d+)$/.exec(request.headers.range ?? '');
  if (!match) { response.writeHead(400); response.end(); return; }
  const first=Number(match[1]),last=Number(match[2]); const bytes=zip.subarray(first,last+1);
  response.writeHead(206, {'Content-Length':bytes.length,'Content-Range':`bytes ${first}-${last}/${zip.length}`,ETag:etag}); response.end(bytes);
});
const feed = createServer((_request, response) => { response.writeHead(200, { 'Content-Type': 'application/rss+xml' }); response.end(readFileSync('../plugins/rss/fixtures/headlines.xml')); });
await Promise.all([weather, feed, luas, bus, archive].map(server => new Promise(resolve => server.listen(0, '127.0.0.1', resolve))));
const feedUrl = `http://127.0.0.1:${feed.address().port}/feed.xml`;
const bindings = join(directory, 'bindings.json');
writeFileSync(bindings, JSON.stringify({ schema_version: 1, bindings: [
  { connector_id: 'example.temperature', wasm: modulePath, source_url: `http://127.0.0.1:${source.address().port}/observation`, stale_after_secs: 60 },
  { connector_id: 'weather.open-meteo', icon_manifest: resolve('../plugins/weather/assets/icons.json'), wasm: weatherModule, source_url: `http://127.0.0.1:${weather.address().port}/forecast`, origin_interval_secs: 2, stale_after_secs: 3600 },
  { connector_id: 'transport.luas', wasm: luasModule, source_url: `http://127.0.0.1:${luas.address().port}/get.ashx`, origin_interval_secs: 60, stale_after_secs: 120 },
  { connector_id:'transport.dublin-bus', wasm:busModule, source_url:`http://127.0.0.1:${bus.address().port}/TripUpdates`,request_headers:{'x-api-key':'api_key'},work_budget:{response_bytes:8388608,fuel:500000000},origin_interval_secs:60,stale_after_secs:120,
    reference_archive:{url:`http://127.0.0.1:${archive.address().port}/reference.zip`,cache:join(directory,'reference.json'),refresh_secs:3600,selection_setting:'stop_numbers',tables:[
      {id:'stops',file:'stops.txt',key:'stop_code',fields:['stop_id','stop_name'],filter:{column:'stop_id',contains:'DB'}},
      {id:'routes',file:'routes.txt',key:'route_id',fields:['route_short_name']},
      {id:'trips',file:'trips.txt',key:'trip_id',fields:['route_id','trip_headsign','service_id']},
      {id:'feed',file:'feed_info.txt',key:'feed_version',fields:['feed_version']},
      {id:'times',file:'stop_times.txt',key:'stop_id',fields:['trip_id','stop_sequence','arrival_time','departure_time','pickup_type'],page_size:32,group:true,streaming:true,select:{column:'stop_id',table:'stops',value_index:0}},
      {id:'calendar',file:'calendar.txt',key:'service_id',fields:['monday','tuesday','wednesday','thursday','friday','saturday','sunday','start_date','end_date']},
      {id:'exceptions',file:'calendar_dates.txt',key:'service_id',key_suffix:['date'],fields:['exception_type']}
    ]}
  },
  { connector_id: 'rss.headlines', wasm: rssModule, source_setting: 'feed_url', origin_interval_secs: 2, stale_after_secs: 21600 },
] }));
// Cold records with later automatic deadlines make Fetch now deterministic.
writeFileSync(join(directory, 'workspace.connections.json'), JSON.stringify({ schema_version: 1, revision: 1, connections: [
  { id: 'temperature.browser', connector_id: 'example.temperature', label: 'Bench sensor', enabled: true, poll_interval_secs: 300, settings: { location: 'Workshop', units: 'imperial' } },
  { id: 'weather.browser', connector_id: 'weather.open-meteo', label: 'Dublin forecast', enabled: false, poll_interval_secs: 900, settings: { location: 'Dublin', latitude: 53.3498, longitude: -6.2603, units: 'metric', forecast_hours: 24, forecast_days: 3, rain_threshold: 50 } },
  { id: 'luas.browser', connector_id: 'transport.luas', label: 'Charlemont trams', enabled: false, poll_interval_secs: 60, settings: {stop:'CHA', direction:'both', max_arrivals:3, destination:''} },
  { id: 'bus.browser', connector_id:'transport.dublin-bus',label:'Dublin buses',enabled:false,poll_interval_secs:60,settings:{stop_numbers:'2813',routes:'',max_arrivals:3,api_key:'fixture-initial-key'} },
  { id: 'rss.browser', connector_id: 'rss.headlines', label: 'Local headlines', enabled: false, poll_interval_secs: 1800, settings: { label: 'Sample headlines', feed_url: feedUrl, max_items: 3 } },
], data: { 'temperature.browser': { generation: 1, next_fetch_at: 1700003600 } } }));
const child = spawn(process.env.PAPERPHASE_TEST_BINARY || resolve('../target/debug/paperphase'), ['serve', '--listen', '127.0.0.1:8097', '--state-file', join(directory, 'workspace.json'), '--connector-bindings', bindings, '--now', '1700000000'], { stdio: 'inherit' });
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => child.kill('SIGINT'));
child.on('exit', code => { source.close(); weather.close(); feed.close(); luas.close(); bus.close(); archive.close(); rmSync(directory, { recursive: true, force: true }); process.exit(code ?? 1); });
child.on('error', error => { console.error(error); source.close(); weather.close(); feed.close(); luas.close(); bus.close(); archive.close(); rmSync(directory, { recursive: true, force: true }); process.exit(1); });
