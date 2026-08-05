'use strict';

// A jest `globalSetup` that pins the process timezone for a zone-legged oracle
// regen — and the ONLY place the pin still works.
//
// Since v4 `f7f1a956`, `jest.config.ts` and `jest.integration.config.ts` assign
// `process.env.TZ = 'UTC'` at config load, before Jest forks its workers. That
// silently clobbers an env-passed `TZ=America/Chicago` on the regen command
// line, so any oracle family with a non-UTC leg would re-record the UTC one.
//
// Setting TZ from inside the test file does NOT recover it:
// `jest-environment-node` hands the test a DEEP COPY of `process`, so an
// in-worker `process.env.TZ = …` writes to a sandbox object libuv never reads.
// (v4's own comment blames ICU caching; the copied env is the nearer cause, and
// either way the in-file pin is inert.) `globalSetup` runs in the MAIN process,
// after the config module and before any worker is forked — the one window
// where the assignment both lands on the real environment and is early enough
// for the workers to inherit it.
//
// The zone comes from `QT_ORACLE_TZ` (or `QT_LLC_LEG`, the llm-log-cleanup
// family's existing leg variable), defaulting to UTC. v4's own `globalSetup`
// still runs: it is resolved from the CWD, which every oracle recipe sets to
// the v4 checkout root, and chained here rather than replaced — dropping it
// would silently disable the native-ABI heal.
//
// Usage (from the v4 checkout):
//   QT_ORACLE_TZ=America/Chicago npx jest \
//     --globalSetup <v5>/harness/oracle/lib/jest-zone-globalsetup.cjs …
//
// The consuming oracle case must still PROVE the zone took (both do, at a
// winter and a summer instant) — this file makes it possible, not certain.

const path = require('node:path');
const fs = require('node:fs');

module.exports = async function zoneGlobalSetup(...args) {
  process.env.TZ = process.env.QT_ORACLE_TZ || process.env.QT_LLC_LEG || 'UTC';

  const basePath = path.join(process.cwd(), 'jest.global-setup.js');
  if (!fs.existsSync(basePath)) return;
  const base = require(basePath);
  const fn = typeof base === 'function' ? base : base && base.default;
  if (typeof fn === 'function') await fn(...args);
};
