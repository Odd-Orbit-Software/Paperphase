import { mkdtempSync, rmSync, writeFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';

const directory = mkdtempSync(join(tmpdir(), 'paperphase-editor-'));
const modulePath = process.env.PAPERPHASE_TEST_PLUGIN || resolve('../target/wasm32-unknown-unknown/release/paperphase_temperature_example.wasm');
if (!existsSync(modulePath)) throw new Error('Build the compiled guest with sh scripts/check.sh before browser tests.');
const source = createServer((_request, response) => { response.writeHead(200, { 'Content-Type': 'application/json' }); response.end('{"temperature_c":18.5}'); });
await new Promise(resolve => source.listen(0, '127.0.0.1', resolve));
const bindings = join(directory, 'bindings.json');
writeFileSync(bindings, JSON.stringify({ schema_version: 1, bindings: [{ connector_id: 'example.temperature', wasm: modulePath, source_url: `http://127.0.0.1:${source.address().port}/observation`, stale_after_secs: 60 }] }));
// A cold record with a later automatic deadline makes Fetch now deterministic.
writeFileSync(join(directory, 'workspace.connections.json'), JSON.stringify({ schema_version: 1, revision: 1, connections: [{ id: 'temperature.browser', connector_id: 'example.temperature', label: 'Bench sensor', enabled: true, poll_interval_secs: 300, settings: { location: 'Workshop', units: 'imperial' } }], data: { 'temperature.browser': { generation: 1, next_fetch_at: 1700003600 } } }));
const child = spawn(process.env.PAPERPHASE_TEST_BINARY || resolve('../target/debug/paperphase'), ['serve', '--listen', '127.0.0.1:8097', '--state-file', join(directory, 'workspace.json'), '--connector-bindings', bindings, '--now', '1700000000'], { stdio: 'inherit' });
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => child.kill('SIGINT'));
child.on('exit', code => { source.close(); rmSync(directory, { recursive: true, force: true }); process.exit(code ?? 1); });
child.on('error', error => { console.error(error); source.close(); rmSync(directory, { recursive: true, force: true }); process.exit(1); });
