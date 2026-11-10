import { build } from 'esbuild';
import { copyFile } from 'node:fs/promises';
await build({ entryPoints: ['src/app.ts'], outfile: 'dist/app.js', bundle: true, minify: true, target: 'es2022', legalComments: 'none' });
await Promise.all(['index.html', 'app.css'].map(file => copyFile(`src/${file}`, `dist/${file}`)));
