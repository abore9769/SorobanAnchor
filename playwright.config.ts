import { defineConfig, devices } from '@playwright/test';

/**
 * Playwright configuration for AnchorKit visual regression tests.
 * Serves static HTML files locally and captures snapshots via Percy.
 *
 * Percy token must be set as PERCY_TOKEN in CI secrets.
 * Run locally: npx percy exec -- playwright test
 */
export default defineConfig({
  testDir: './visual-tests',
  testMatch: '**/*.visual.ts',

  // Run tests in parallel for speed
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: process.env.CI ? 2 : undefined,

  reporter: [
    ['list'],
    ['html', { outputFolder: 'visual-tests/report', open: 'never' }],
  ],

  use: {
    // Local static file server (started in globalSetup)
    baseURL: 'http://localhost:3000',

    // Capture trace on first retry to aid debugging
    trace: 'on-first-retry',

    // Consistent rendering: disable animations for stable snapshots
    actionTimeout: 10_000,
  },

  projects: [
    // Desktop viewports
    {
      name: 'chromium-desktop',
      use: {
        ...devices['Desktop Chrome'],
        viewport: { width: 1280, height: 800 },
      },
    },
    {
      name: 'firefox-desktop',
      use: {
        ...devices['Desktop Firefox'],
        viewport: { width: 1280, height: 800 },
      },
    },

    // Mobile viewports
    {
      name: 'mobile-chrome',
      use: { ...devices['Pixel 5'] },
    },
    {
      name: 'mobile-safari',
      use: { ...devices['iPhone 13'] },
    },
  ],

  // Start a local static file server before running tests
  webServer: {
    command: 'npx serve . --listen 3000 --no-clipboard',
    url: 'http://localhost:3000',
    reuseExistingServer: !process.env.CI,
    timeout: 30_000,
  },
});
