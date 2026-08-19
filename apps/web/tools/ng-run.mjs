#!/usr/bin/env node
/**
 * ng-run — run an `ng` command and actually get the shell back.
 *
 * ## Why this exists
 *
 * `ng build` and `ng test` (`@angular/build` 21.x) finish their work, print
 * their summary, and then **never exit**. Measured 2026-08-19: after the
 * completion line, `process.getActiveResourcesInfo()` reports exactly
 * `["PipeWrap","PipeWrap","ProcessWrap"]` and nothing else — the
 * `esbuild --service=… --ping` child and its two stdio pipes, still ref'd.
 * No timers, no sockets, no worker threads. Killing that one child makes
 * `ng build` exit immediately with code 0.
 *
 * It is not esbuild's bug: its shim unrefs the service child at spawn and
 * only refs it while an operation is in flight (`esbuild/lib/main.js:2299`),
 * and a standalone `context() → rebuild() → dispose()` script exits cleanly.
 * The retained ref is `@angular/build`'s: on the non-watch path it defers
 * `result.dispose()` into a generator `finally`, which does not reliably run.
 * **Fixed upstream in `@angular/build` 22.0.4**, which disposes eagerly
 * before yielding; NOT backported to v21 LTS (21.2.21's `src/` tree is
 * byte-identical to 21.2.19's). Taking that fix means Angular 21 → 22 AND
 * TypeScript 5.9 → 6.0, so it is tabled — hence this wrapper.
 *
 * ## What it does
 *
 * Runs ng as a child, streams its output through untouched, and watches for
 * the command's terminal marker. Once the marker lands the work is COMPLETE
 * and the artifacts are written, so a lingering process carries no
 * information: we give it a short grace period to exit on its own (and use
 * its real exit code if it does), then terminate it and exit with the code
 * the marker itself encodes. Angular prints `complete` vs `failed` from its
 * own `hasError`, so the derived code is not a guess.
 *
 * Delete this file when the Angular 22 upgrade lands.
 *
 * Usage:  node tools/ng-run.mjs build [...args]
 *         node tools/ng-run.mjs test --watch=false [...args]
 * Env:    NG_RUN_TIMEOUT_MS   overall cap (default 1800000 = 30 min)
 *         NG_RUN_GRACE_MS     wait for a natural exit after the marker (default 2000)
 */

import { spawn } from 'node:child_process';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const args = process.argv.slice(2);
if (args.length === 0) {
  console.error('ng-run: expected an ng command, e.g. `node tools/ng-run.mjs build`');
  process.exit(2);
}

const TIMEOUT_MS = Number(process.env.NG_RUN_TIMEOUT_MS ?? 30 * 60 * 1000);
const GRACE_MS = Number(process.env.NG_RUN_GRACE_MS ?? 2000);

// Strip ANSI SGR codes before matching: vitest's summary is colour-soaked and
// separates the words we match on with escape sequences.
const stripAnsi = (s) => s.replace(/\x1b\[[0-9;]*m/g, '');

/**
 * Terminal markers, per command. Each returns `{ done, ok }` for a chunk of
 * ANSI-stripped output, or null when the command has no known marker (in
 * which case we simply wait for a natural exit, as today).
 */
function markerFor(command) {
  if (command === 'build') {
    // `Application bundle generation ${hasError ? 'failed' : 'complete'}.`
    // — @angular/build/src/builders/application/index.js
    return (text) => {
      const m = /Application bundle generation (complete|failed)\./.exec(text);
      return m ? { done: true, ok: m[1] === 'complete' } : { done: false };
    };
  }
  if (command === 'test') {
    // vitest's summary block. `Duration` is the last line and survives
    // colouring intact; the verdict comes from the `Test Files` line.
    return (text) => {
      if (!/^\s*Duration\s/m.test(text)) return { done: false };
      const files = /^\s*Test Files\s+(.*)$/m.exec(text);
      const failed = files ? /\bfailed\b/.test(files[1]) : /\bfailed\b/.test(text);
      return { done: true, ok: !failed };
    };
  }
  return null;
}

// A watch/serve run is SUPPOSED to outlive its first build — the marker would
// fire on build #1 and kill it. Those commands pass straight through.
const watching =
  args[0] === 'serve' ||
  args.some((a) => a === '--watch' || a === '--watch=true' || a === '-w');
const marker = watching ? null : markerFor(args[0]);

let ngBin;
try {
  ngBin = require.resolve('@angular/cli/bin/ng.js');
} catch {
  console.error('ng-run: cannot resolve @angular/cli — run from apps/web with node_modules installed');
  process.exit(2);
}

// `detached` puts ng in its own process group, so one signal reaps ng AND
// its esbuild service grandchild together. Without it the grandchild is
// orphaned and lingers for several seconds until its own `--ping` keepalive
// notices the dead parent — harmless, but they pile up across a gate.
const child = spawn(process.execPath, [ngBin, ...args], {
  stdio: ['inherit', 'pipe', 'pipe'],
  env: process.env,
  detached: true,
});

/** Signal ng's whole process group, falling back to the bare pid. */
function signalGroup(sig) {
  try {
    process.kill(-child.pid, sig);
  } catch {
    try {
      child.kill(sig);
    } catch {
      /* already gone */
    }
  }
}

// A detached child no longer receives the terminal's Ctrl+C, so relay it.
for (const sig of ['SIGINT', 'SIGTERM']) {
  process.on(sig, () => {
    signalGroup(sig);
    process.exit(130);
  });
}

let buffer = '';
let verdict = null; // { ok } once the marker lands
let settling = false;

function absorb(chunk, stream) {
  stream.write(chunk);
  if (!marker || verdict) return;
  // Keep only a tail: markers are single lines and the full log can be large.
  buffer = (buffer + stripAnsi(chunk.toString('utf8'))).slice(-64 * 1024);
  const state = marker(buffer);
  if (state.done) {
    verdict = { ok: state.ok };
    settle();
  }
}

child.stdout.on('data', (c) => absorb(c, process.stdout));
child.stderr.on('data', (c) => absorb(c, process.stderr));

let graceTimer = null;
let killTimer = null;

/** The work is done; reclaim the shell. */
function settle() {
  if (settling) return;
  settling = true;
  graceTimer = setTimeout(() => {
    // It did not leave on its own — the expected case on 21.x.
    signalGroup('SIGTERM');
    killTimer = setTimeout(() => signalGroup('SIGKILL'), 3000);
  }, GRACE_MS);
  graceTimer.unref?.();
}

const overall = setTimeout(() => {
  console.error(`\nng-run: no terminal marker after ${TIMEOUT_MS} ms — killing \`ng ${args.join(' ')}\``);
  verdict = { ok: false, timedOut: true };
  signalGroup('SIGKILL');
}, TIMEOUT_MS);
overall.unref?.();

child.on('error', (err) => {
  console.error(`ng-run: failed to start ng — ${err.message}`);
  process.exit(2);
});

child.on('exit', (code, signal) => {
  clearTimeout(overall);
  clearTimeout(graceTimer);
  clearTimeout(killTimer);
  if (verdict?.timedOut) process.exit(124);
  // A natural exit before we intervened is authoritative.
  if (!settling && code !== null) process.exit(code);
  // It exited inside the grace window, before any signal from us.
  if (code !== null && !signal) process.exit(code);
  // We terminated it after the marker: the marker carries the verdict.
  process.exit(verdict?.ok ? 0 : 1);
});
