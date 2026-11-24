# Storybook Previews (static HTML)

This folder holds hand-written, self-contained HTML pages that preview AnchorKit UI patterns: config form, status monitor, wallet connector, loading, error and success states, and so on. Despite the folder name, it is **not** a [Storybook](https://storybook.js.org/) project. There is no `package.json`, no build step, and no dev server.

## Opening the previews

Open a page straight from disk in any browser. No server is needed:

```bash
open storybook/index.html        # macOS
xdg-open storybook/index.html    # Linux
```

Or drag the file into a browser window. `index.html` is the landing page and links to most previews. `anchor-capability-card.html`, `json-viewer.html` and `precision-fintech.html` are not linked from it, so open them directly.

## What the previews are, and are not

- **Static and data-mocked.** Every value shown (addresses, transaction IDs, statuses, config fields) is hard-coded in the HTML. The pages make no network requests. Nothing is connected to a live anchor, a Soroban RPC endpoint, a wallet, or a deployed contract.
- **Not the live components.** The React components in [`ui/components/`](../ui/components/) are the shipped implementation. These pages are separate hand-written mock-ups. They do not import or render those components and can drift from them.
- **Interactivity is simulated.** Only `status-monitor.html` and `sdk-config-form.html` contain scripts. The status monitor's "refresh" picks random states, and the config form updates its on-page preview and shows the generated JSON in a browser alert. Nothing is saved or sent. All other pages are HTML and CSS only.
- **Not tied to the Rust SDK.** Rust or TSX snippets shown on the pages are illustrative text. They are not compiled or executed, and nothing here exercises the `anchorkit` crate or CLI.

## Validating the previews

```bash
bash scripts/validate_storybook.sh
```

The script runs offline and checks that:

1. every expected page exists,
2. each page is non-empty and contains `<html>` (a missing `<title>` is a warning),
3. relative links to other `.html` files in this folder resolve, and
4. no `src="/..."` absolute paths are present, since those break under `file://` (warning only).

It does not render pages, run their scripts, check visual output or accessibility, check external links, or compare pages against `ui/components/`. It is not currently wired into the CI workflows, so run it by hand after editing a page.

## Editing

Edit the relevant `.html` file directly; each page is self-contained. If you add a page, also add it to `REQUIRED_PAGES` in `scripts/validate_storybook.sh`.
