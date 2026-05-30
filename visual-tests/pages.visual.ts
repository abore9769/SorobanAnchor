import { test } from '@playwright/test';
import percySnapshot from '@percy/playwright';

/**
 * Visual regression tests for AnchorKit UI pages.
 *
 * Snapshots are uploaded to Percy on every CI run.
 * Percy blocks the PR merge until visual diffs are reviewed and approved.
 *
 * Baseline: established on the first successful run against `main`.
 * Viewports: desktop (1280×800) and mobile (375×667) — configured in playwright.config.ts.
 */

// ---------------------------------------------------------------------------
// Storybook component pages
// ---------------------------------------------------------------------------

test.describe('Storybook components', () => {
  test('index — component overview', async ({ page }) => {
    await page.goto('/storybook/index.html');
    await page.waitForLoadState('networkidle');
    await percySnapshot(page, 'Storybook — Index');
  });

  test('sdk-config-form', async ({ page }) => {
    await page.goto('/storybook/sdk-config-form.html');
    await page.waitForLoadState('networkidle');
    await percySnapshot(page, 'Storybook — SDK Config Form');
  });

  test('status-monitor', async ({ page }) => {
    await page.goto('/storybook/status-monitor.html');
    await page.waitForLoadState('networkidle');
    await percySnapshot(page, 'Storybook — Status Monitor');
  });

  test('webhook-monitor', async ({ page }) => {
    await page.goto('/storybook/webhook-monitor.html');
    await page.waitForLoadState('networkidle');
    await percySnapshot(page, 'Storybook — Webhook Monitor');
  });

  test('wallet-connector', async ({ page }) => {
    await page.goto('/storybook/wallet-connector.html');
    await page.waitForLoadState('networkidle');
    await percySnapshot(page, 'Storybook — Wallet Connector');
  });

  test('anchor-capability-card', async ({ page }) => {
    await page.goto('/storybook/anchor-capability-card.html');
    await page.waitForLoadState('networkidle');
    await percySnapshot(page, 'Storybook — Anchor Capability Card');
  });

  test('json-viewer', async ({ page }) => {
    await page.goto('/storybook/json-viewer.html');
    await page.waitForLoadState('networkidle');
    await percySnapshot(page, 'Storybook — JSON Viewer');
  });

  test('precision-fintech', async ({ page }) => {
    await page.goto('/storybook/precision-fintech.html');
    await page.waitForLoadState('networkidle');
    await percySnapshot(page, 'Storybook — Precision Fintech');
  });
});

// ---------------------------------------------------------------------------
// State variant pages
// ---------------------------------------------------------------------------

test.describe('UI state variants', () => {
  test('loading-states', async ({ page }) => {
    await page.goto('/storybook/loading-states.html');
    await page.waitForLoadState('networkidle');
    // Freeze CSS animations for a stable snapshot
    await page.addStyleTag({ content: '*, *::before, *::after { animation-duration: 0s !important; transition-duration: 0s !important; }' });
    await percySnapshot(page, 'States — Loading');
  });

  test('error-states', async ({ page }) => {
    await page.goto('/storybook/error-states.html');
    await page.waitForLoadState('networkidle');
    await percySnapshot(page, 'States — Error');
  });

  test('success-states', async ({ page }) => {
    await page.goto('/storybook/success-states.html');
    await page.waitForLoadState('networkidle');
    await percySnapshot(page, 'States — Success');
  });
});

// ---------------------------------------------------------------------------
// Static app pages
// ---------------------------------------------------------------------------

test.describe('Static app pages', () => {
  test('sdk_config_form', async ({ page }) => {
    await page.goto('/static/sdk_config_form.html');
    await page.waitForLoadState('networkidle');
    await percySnapshot(page, 'App — SDK Config Form');
  });

  test('status-monitor', async ({ page }) => {
    await page.goto('/static/status-monitor.html');
    await page.waitForLoadState('networkidle');
    await percySnapshot(page, 'App — Status Monitor');
  });

  test('webhook-monitor', async ({ page }) => {
    await page.goto('/static/webhook_monitor.html');
    await page.waitForLoadState('networkidle');
    await percySnapshot(page, 'App — Webhook Monitor');
  });
});
