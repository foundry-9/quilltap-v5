# Quilltap SPA (`apps/web`)

The Angular 21 (standalone + zoneless + signals) single-page app served inside
the Tauri webview **and** by the axum HTTP host (`quilltap-web`). This is the
Phase-4 P4.5 foundation: the `CoreClient` transport seam, the SSE stream
reducer, the ported `qt-*` CSS + bundled themes, the UI primitive set, and the
startup-gate → unlock → setup → shell screens.

The SPA talks to exactly one seam, `CoreClient`, over three server surfaces:
`POST /api/dispatch` (typed actions), `GET /api/events` (the one global
scope-tagged SSE stream), and `GET /health` (readiness). The Rust contract in
`crates/quilltap-core/src/api/types.rs` is the source of truth; the TS mirror is
`src/app/core/core-contract.ts`.

## Development loops

### 1. Proxy dev loop (fast inner loop)

Run the Rust host and the Angular dev server side by side; `ng serve` proxies
`/api` and `/health` to the host (see `proxy.conf.json`).

```bash
# terminal 1 — the Rust host (any unlocked/locked instance)
cargo run -p quilltap-web -- --port 3000 --data-dir /path/to/instance

# terminal 2 — the Angular dev server (http://localhost:4200)
cd apps/web
npm install
npm start
```

Open <http://localhost:4200>. Hot-reloads on source changes; API/SSE calls go to
the host on :3000.

### 2. Built-dist loop (production shape)

Build the SPA and let the host serve it directly via `--spa-dir`:

```bash
cd apps/web
npm run build                       # → dist/quilltap/browser
cargo run -p quilltap-web -- --port 3000 \
  --data-dir /path/to/instance \
  --spa-dir apps/web/dist/quilltap/browser
```

Open <http://localhost:3000>. The host serves the built assets with the
SPA-index fallback.

## Unit / component tests

```bash
npm test        # ng test — Vitest + jsdom, single run
```

Covers the stream reducer (against the committed frame-trace fixture),
`CoreClient` parsing (dispatch envelope, error, Locked-503 body, SSE frame
parse, `/health` vocabulary), `ThemeService`, and the setup wizard + unlock +
startup gate against a mocked `CoreClient`.

## End-to-end (real server)

Playwright drives a real `quilltap-web` serving the built SPA against a
passphrase-locked copy of the committed chat-send fixture
(`crates/quilltap-web/tests/fixtures/`, copied — never mutated).

Prerequisites:

```bash
# 1. build the Rust binaries (base commit — the e2e uses the CLI to migrate the
#    fixture schema and the web host to serve it):
cargo build -p quilltap-web -p quilltap-cli

# 2. build the SPA and install the Playwright browser (first run only):
cd apps/web
npm run build
npm run e2e:install
```

Then:

```bash
npm run e2e     # playwright test — global-setup builds the locked instance +
                # launches the host; global-teardown stops it + cleans up.
```

The walk: locked → unlock screen → wrong-passphrase error → correct passphrase →
the shell with the fixture's chats → the theme switcher applying a bundled pack.

## Theming notes / documented divergences

The six bundled theme packs (`public/themes/<id>/`) were copied from v4
`themes/bundled/`. Two rewrites were made this round (to reconcile when the
server themes service lands in the Settings vertical):

- Every `/api/themes/assets/bundle:<id>/…` texture URL was rewritten to
  `/themes/<id>/…` (in the six packs + two art-deco refs in
  `src/styles/qt-components/_surfaces.css`).
- Theme fonts are injected at runtime by `ThemeService` from each pack's
  `theme.json`, pointing at `/themes/<id>/fonts/…`.

`ThemeService` persists the choice in `localStorage` for now; its `listThemes` →
descriptors / `applyTheme(id)` shape mirrors the future
`GET /api/v1/themes` server read.
