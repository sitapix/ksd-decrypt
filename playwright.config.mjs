import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './tests/ui',
  use: { channel: 'chrome', headless: true, viewport: { width: 600, height: 600 } },
  reporter: 'list',
  workers: 1,
});
