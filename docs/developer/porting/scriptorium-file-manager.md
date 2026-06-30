# The Scriptorium file-manager component (Angular)

Phase-4 (Angular UI) work, but the **component choice is recorded now** because
it has no first-party path on our target stack and we want the decision settled
before the Scriptorium UI is built. This is a settled-now / implemented-later
note, like [`api-boundary.md`](./api-boundary.md) and
[`provider-manifest.md`](./provider-manifest.md).

## The problem

v4 (`quilltap-server`) renders the Scriptorium's document-store browser with
**SVAR File Manager** (`@svar-ui/react-filemanager`, pinned 2.6.0), wrapped by a
framework-agnostic adapter layer under `components/files/svar/`
(`createSvarAdapter.ts`, `event-route-map.ts`, `listing-to-tree.ts`,
`error-translation.ts`, `reindex-after-copy.ts`, `node-id.ts`).

SVAR has **no Angular package and no vanilla-JS core build** — its components are
genuinely framework-native (Svelte original, plus separate React and Vue ports),
not wrappers over a shared vanilla widget. So SVAR cannot come across to v5's
Angular 21 SPA. The UI widget must be replaced; the adapter *thinking* carries
over (the helpers above are framework-agnostic in spirit and map onto whatever we
pick).

## Constraints

- **Free / open-source only.** Self-hosted is Quilltap's ethos; no commercial
  components (this rules out Syncfusion's File Manager despite its clean
  `ajaxSettings` backend hookup — its free tier is a revenue/headcount-gated
  *community license*, not OSS).
- **We always provide the file CRUD API ourselves.** We don't need a built-in
  data-source framework — we need a UI that doesn't fight us when pointed at our
  own HTTP endpoints (v4's mount-points CRUD).
- **Themeable to match our UI.** v5 keeps v4's `qt-*` semantic-class theming and
  the `.qtap-theme` bundle format. A component that drags in a *second* theming
  engine (PrimeNG's `@primeuix/themes` token system) is a cost, not a feature —
  it repeats the `svar-theme-bridge.css` problem in a new place.
- **Angular 21+ (zoneless, signals, standalone).**

## Candidates evaluated (June 2026)

| Option | License | Angular | Theming | Verdict |
|---|---|---|---|---|
| **ngx-explorer** (`artemnih`, npm 5.0.2) | Apache-2.0 | peer `>=17.2` | plain CSS, ~11 hardcoded hex, no token engine | **front-runner** |
| **ngx-voyage** (`mschn`, npm 1.0.6) | MIT | peer `^21.1.3` | PrimeNG `@primeuix/themes` token engine | strong, but adds a design-system + theming layer |
| Syncfusion File Manager | proprietary (community tier) | current | own theme engine | **out** — not OSS |
| joni2back / ng6-file-man / ng2-file-man / @rign | various | AngularJS–Angular 6 | n/a | **out** — years stale, dead Angular versions |
| Build our own | — | — | native `qt-*` | fallback; see below |

## The recommendation: spike **ngx-explorer**, with build-our-own as the fallback

ngx-explorer fits the constraints best *because* it's minimal, despite being the
older release. From unpacking the 5.0.2 tarball:

- **Tiny and self-contained.** 9 presentational components, 2 services, 1
  directive — **~641 lines** of compiled source, ~660 K unpacked (mostly a 12-glyph
  icon font). Peer deps are *only* `@angular/common` + `@angular/core` (+ rxjs,
  tslib). No backend, no provider integrations, no native code, no design system.
- **Themeable by flat CSS override.** The entire styling surface is ~11 hardcoded
  hex colours across three small inline stylesheets (folder `#fdb900`, icon `#555`,
  hover `#d7edff`, selected `#c5e1ff`/`#f1f9ff`, borders `#ccc`, drag `#30a2ff`,
  delete `#ca0801`). No CSS variables and no token engine, so we override `.nxe-*`
  selectors and point them at our tokens — and almost certainly swap the icon font
  for our own. Cruder than `qt-*` (override-by-selector, not set-a-variable) but
  with **no second theming system to reconcile** — the opposite of the PrimeNG /
  SVAR-bridge cost.
- **Adapter contract close to what we already have.** A single injected
  `IDataService<T>` (DI: `{ provide: DataService, useClass: MyDataService }`) with
  seven Observable-returning methods:

  ```ts
  getContent(target: T): Observable<{ files: T[]; dirs: T[] }>;
  createDir(parent: T, name: string): Observable<T>;
  rename(target: T, newName: string): Observable<T>;
  delete(target: T[]): Observable<T>;
  uploadFiles(parent: T, files: FileList): Observable<T>;
  downloadFile(target: T): Observable<T>;
  openTree(data: T): Observable<Array<DataNode<T>>>;
  ```

  This maps almost one-to-one onto v4's mount-points CRUD; v4's `listing-to-tree`
  is essentially what `getContent`/`openTree` want. `NgeExplorerConfig.features`
  gates delete/upload/rename/createDir (handy for capability-driven UI).
- **The staleness fear is bounded here.** "Untouched for ~2 years" is a big risk
  for a library with a large dependency graph or a backend; it is a *small* risk
  for a 641-line, dependency-light, purely-presentational widget. If upstream
  dies, **vendoring it into the repo is a non-event**, not a fork of a sprawling
  project. That escape hatch is exactly what PrimeNG/ngx-voyage does not give us.

### Caveats / what the spike must check

1. **Angular 21 zoneless.** Peer dep is `>=17.2` and last publish was mid-2024.
   It touches no zone-sensitive APIs (state is plain rxjs `BehaviorSubject`), so it
   *should* run clean — but this is the gating check before committing. It's also
   still `NgModule`-based (`NgxExplorerModule`), not standalone; usually interops
   fine, but confirm.
2. **Numeric node ids.** `INode.id` is a `number`, whereas our world is
   UUIDs / relative paths — so the adapter keeps an id↔path map (v4's `node-id.ts`
   already does this shape for SVAR).

### Fallback: build our own — but build it *small*

If the spike fails on Angular 21, build it. The reframing that matters: a
home-grown explorer was a maintenance burden in v4 because we owned *all* of it —
tree, views, selection, drag-drop, **and** the backend glue. The hard part (the
adapter to our API) was always going to be ours regardless. ngx-explorer hands us
the first four for free; and having seen that a competent version of the widget is
only **~640 lines**, that's a useful yardstick for scoping a from-scratch build
against whatever the earlier attempt had ballooned into. Either way the v4
adapter helpers — `listing-to-tree`, `event-route-map`, `error-translation`,
`reindex-after-copy`, `node-id` — port forward; only the UI widget swaps.

## Build order (when Phase 4 reaches the Scriptorium)

1. Spike ngx-explorer in a throwaway Angular-21 harness; confirm it renders under
   zoneless and the `IDataService` adapter wires to the mount-points CRUD.
2. If green: port the v4 adapter helpers, write the `qt-*` override sheet for the
   `.nxe-*` selectors, swap the icon font.
3. If red: build the small bespoke component, reusing the same ported adapter
   helpers.
