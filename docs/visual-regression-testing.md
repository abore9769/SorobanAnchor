# Visual Regression Testing

AnchorKit uses [Percy](https://percy.io) + [Playwright](https://playwright.dev) to catch unintended UI changes across PRs.

## How it works

1. On every PR that touches `static/`, `storybook/`, or `visual-tests/`, the `visual-regression.yml` CI workflow runs.
2. Playwright opens each HTML page in Chromium and Firefox (desktop 1280×800) and Chrome/Safari (mobile 375×667).
3. Percy uploads the screenshots and compares them against the baseline from `main`.
4. If any visual diff is detected, the Percy check on the PR is marked **pending** until a team member reviews and approves (or rejects) the diff in the Percy dashboard.
5. Merging is blocked until the Percy check passes.

## Pages covered

| Category | Pages |
|---|---|
| Storybook components | index, sdk-config-form, status-monitor, webhook-monitor, wallet-connector, anchor-capability-card, json-viewer, precision-fintech |
| UI state variants | loading-states, error-states, success-states |
| Static app pages | sdk_config_form, status-monitor, webhook_monitor |

## Setup (first time)

### 1. Create a Percy project

1. Sign in at [percy.io](https://percy.io) and create a new project linked to this repository.
2. Copy the **PERCY_TOKEN** from the project settings.

### 2. Add the token to GitHub secrets

Go to **Settings → Secrets and variables → Actions** and add:

```
PERCY_TOKEN = <your token>
```

### 3. Establish the baseline

Merge this PR to `main`. Percy will run and set the initial baseline snapshots automatically — no manual step needed.

## Running locally

```bash
# Install dependencies (one-time)
npm install
npx playwright install --with-deps chromium firefox

# Run with Percy (uploads snapshots — requires PERCY_TOKEN)
PERCY_TOKEN=<your_token> npm run percy

# Run without Percy (Playwright only, no upload)
npm run test:visual

# Update local snapshots
npm run test:visual:update
```

## Viewports

Both desktop and mobile are tested on every run, configured in `.percy.yml`:

| Viewport | Width |
|---|---|
| Mobile | 375 px |
| Desktop | 1280 px |

## Freezing animations

The `loading-states` test injects a CSS rule to set all animation and transition durations to `0s` before snapshotting, ensuring stable pixel-level comparisons.
