import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests',
  workers: 1,
  fullyParallel: false,
  timeout: 30_000,
  use: { baseURL: 'http://127.0.0.1:8097', viewport: { width: 1480, height: 1000 }, trace: 'retain-on-failure' },
  webServer: { command: 'node tests/server.mjs', url: 'http://127.0.0.1:8097/api/state', reuseExistingServer: false, timeout: 60_000 },
});
