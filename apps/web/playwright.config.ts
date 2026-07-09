import { defineConfig, devices } from '@playwright/test';

import { BASE_URL } from './e2e/support/env';

/**
 * Playwright e2e against the REAL axum server (tier-4 #2). `globalSetup` builds a
 * passphrase-locked copy of the committed chat-send fixture and launches
 * `quilltap-web --spa-dir <dist> --data-dir <copy>`; the tests then walk
 * locked → unlock → shell + theme switch. See apps/web/README.md for the cargo +
 * `npm run build` prerequisites (the setup fails loud if they are missing).
 */
export default defineConfig({
  testDir: './e2e',
  testMatch: '**/*.spec.ts',
  fullyParallel: false,
  workers: 1,
  retries: 0,
  timeout: 30_000,
  globalSetup: './e2e/global-setup.ts',
  globalTeardown: './e2e/global-teardown.ts',
  reporter: [['list']],
  use: {
    baseURL: BASE_URL,
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
});
