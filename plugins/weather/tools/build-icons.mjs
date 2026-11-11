// Offline build tool; the service only loads the generated PNG atlases.
import { readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { chromium } from '../../../web/node_modules/@playwright/test/index.mjs';
const assets = new URL('../assets/', import.meta.url);
const provenance = JSON.parse(await readFile(new URL('sources.json', assets), 'utf8'));
const entries = await Promise.all(Object.entries(provenance.mapping).map(async ([key, name]) => [key, await readFile(new URL(`meteocons/${name}.svg`, assets), 'utf8')]));
const sizes = [16, 24, 32, 48, 64];
const browser = await chromium.launch({ headless: true });
try {
  const page = await browser.newPage();
  for (const size of sizes) {
    const png = await page.evaluate(async ({ entries, size }) => {
      const canvas = document.createElement('canvas');
      canvas.width = 8 * size; canvas.height = Math.ceil(entries.length / 8) * size;
      const ctx = canvas.getContext('2d');
      for (let i = 0; i < entries.length; i++) {
        const image = new Image();
        image.src = `data:image/svg+xml;base64,${btoa(entries[i][1])}`;
        await image.decode();
        ctx.drawImage(image, i % 8 * size, Math.floor(i / 8) * size, size, size);
      }
      const pixels = ctx.getImageData(0, 0, canvas.width, canvas.height);
      for (let i = 0; i < pixels.data.length; i += 4) {
        pixels.data[i] = pixels.data[i + 1] = pixels.data[i + 2] = 0;
        pixels.data[i + 3] = pixels.data[i + 3] >= 128 ? 255 : 0;
      }
      ctx.putImageData(pixels, 0, 0);
      return canvas.toDataURL('image/png').split(',')[1];
    }, { entries, size });
    await writeFile(new URL(`icons-${size}.png`, assets), Buffer.from(png, 'base64'));
  }
  const manifest = { schema_version: 1, id: 'weather.open-meteo', name: 'Meteocons weather icons', variants: sizes.map(size => ({ size, image: `icons-${size}.png` })), icons: Object.fromEntries(entries.map(([key], i) => [key, { column: i % 8, row: Math.floor(i / 8) }])) };
  await writeFile(new URL('icons.json', assets), JSON.stringify(manifest, null, 2) + '\n');
  console.log(`Built ${entries.length} icons at ${sizes.join(', ')}px in ${fileURLToPath(assets)}`);
} finally { await browser.close(); }
