import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawn } from 'node:child_process';

const directory = mkdtempSync(join(tmpdir(), 'paperphase-editor-'));
const child = spawn(process.env.PAPERPHASE_TEST_BINARY || resolve('../target/debug/paperphase'), ['serve', '--listen', '127.0.0.1:8097', '--state-file', join(directory, 'workspace.json'), '--now', '1700000000'], { stdio: 'inherit' });
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => child.kill('SIGINT'));
child.on('exit', code => { rmSync(directory, { recursive: true, force: true }); process.exit(code ?? 1); });
child.on('error', error => { console.error(error); rmSync(directory, { recursive: true, force: true }); process.exit(1); });
