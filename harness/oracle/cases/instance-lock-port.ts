/**
 * ORACLE for P4.D299 — v4 bug 239 (`93c5af6ad`): the shared instance lock's
 * optional `port`, driven through v4's REAL
 * `lib/database/backends/sqlite/instance-lock.ts` (`acquireInstanceLock`, the
 * heartbeat's own interval callback).
 *
 * Every row runs in a fresh scratch dir with its own lock path. A row may
 * PLANT a lock first: the plant is a pretty-JSON TEMPLATE recorded in the row
 * (so the Rust side rebuilds it byte-for-byte) with two tokens — the quoted
 * `"__PID__"` (replaced, quotes and all, by a PID: a just-exited child's for a
 * `dead` plant, this process's own for a `self` plant) and `__HOST__` inside a
 * string (this machine's `os.hostname()`). `PORT` is set (or deleted) per row
 * before the actions; the row records v4's own `resolveListeningPort()` under
 * it as `listeningPort` (`null` for v4's `undefined`) — the value the Rust side
 * hands `acquire_instance_lock`, so the PORT → port mapping is v4's code, not a
 * re-implementation. The actions:
 *
 *   - `acquire`   — `acquireInstanceLock(lockPath)`. `setInterval` is
 *                   captured before v4's module loads, so the heartbeat it
 *                   starts never runs a real timer.
 *   - `heartbeat` — invokes the interval callback `startLockHeartbeat`
 *                   registered (v4's 60 s tick, run once, synchronously).
 *
 * Recorded per action: the lock file's text with the volatile values masked
 * and re-serialized in the file's own key order (`masked` — top-level `pid`,
 * `hostname`, `startedAt`, `lastHeartbeat`, `environment`, `processTitle`,
 * `processArgv0` → `"<m>"`; each history entry's `pid`, `hostname`,
 * `timestamp` → `"<m>"` and every digit run in its `detail` → `<n>`; every
 * other key, `port` and the unknown ones included, raw), plus every line the
 * module logs (`module: 'database:instance-lock'` — the child's binding first,
 * as v4's `log()` merges it; a `pid` field masked; winston's dropped
 * `undefined` fields dropped here too).
 *
 * P4.171 (C-6) adds rows for v4's lenient `readLockFile` (a lock missing
 * `processArgv0`, an unknown or absent `environment` / `startedAt`, a `pid`
 * that is not a number, text that is not JSON, a JSON `null`) and records an
 * action that THROWS as `threw: true` (the file and lines still compared).
 *
 * Run (Node 24, from the v4 checkout or a PINNED worktree — a `tsx` script):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-instance-lock-port.ndjson \
 *     $N/npx tsx $V5W/harness/oracle/cases/instance-lock-port.ts
 */

import * as fs from 'node:fs';
import { mkdtempSync, rmSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { hostname as osHostname, tmpdir } from 'node:os';
import { join } from 'node:path';

type Line = { level: string; message: string; fields: Record<string, unknown> };

type Plant = { pid: 'dead' | 'self'; template: string } | null;

type Spec = {
  name: string;
  /** `PORT` for the row; `null` deletes it. */
  port: string | null;
  plant: Plant;
  actions: Array<'acquire' | 'heartbeat'>;
};

const MODULE = 'database:instance-lock';

/** A plant: the standard v4 key order, `port` / extras spliced where named. */
function plantTemplate(opts: {
  host?: string;
  /** Any JSON value — v4 never type-checks it; `'__NOW_MS__'` → `Date.now()`. */
  lastHeartbeat?: unknown;
  port?: unknown;
  before?: Record<string, unknown>;
  after?: Record<string, unknown>;
  /** P4.171: keys deleted after the build; `environment` overridden. */
  omit?: string[];
  environment?: unknown;
}): string {
  const obj: Record<string, unknown> = { ...(opts.before ?? {}) };
  obj.pid = '__PID__';
  obj.hostname = opts.host ?? '__HOST__';
  obj.startedAt = '2026-01-02T03:04:05.000Z';
  obj.lastHeartbeat = 'lastHeartbeat' in opts ? opts.lastHeartbeat : '2026-01-02T03:04:05.000Z';
  obj.environment = 'local';
  obj.processTitle = 'node';
  obj.processArgv0 = '/usr/bin/node';
  if ('port' in opts) obj.port = opts.port;
  obj.history = [
    {
      event: 'acquired',
      pid: 4242,
      hostname: 'old-host',
      timestamp: '2026-01-02T03:04:05.000Z',
      detail: 'Clean acquisition — no prior lock',
    },
  ];
  Object.assign(obj, opts.after ?? {});
  if ('environment' in opts) obj.environment = opts.environment;
  for (const k of opts.omit ?? []) delete obj[k];
  return JSON.stringify(obj, null, 2) + '\n';
}

const EXTRAS_BEFORE = { aaFirst: true };
const EXTRAS_AFTER = { zzExtra: { a: 1, list: [1, 'two', null] } };

function specs(): Spec[] {
  const out: Spec[] = [];
  // Fresh writes under every PORT shape v4's resolver distinguishes.
  for (const [label, port] of [
    ['3005', '3005'],
    ['unset', null],
    ['empty', ''],
    ['banana', 'banana'],
    ['zero', '0'],
    ['70000', '70000'],
    ['float', '3005.5'],
  ] as Array<[string, string | null]>) {
    out.push({ name: `fresh port ${label}`, port, plant: null, actions: ['acquire'] });
  }
  out.push({ name: 'fresh port 3005 then heartbeat', port: '3005', plant: null, actions: ['acquire', 'heartbeat'] });

  // Stale claims (same host, dead PID) of every planted port shape.
  const claimPlants: Array<[string, string]> = [
    ['with port 3001', plantTemplate({ port: 3001 })],
    ['portless', plantTemplate({})],
    ['malformed port string', plantTemplate({ port: 'abc' })],
    ['malformed port float', plantTemplate({ port: 3000.5 })],
    ['malformed port 70000', plantTemplate({ port: 70000 })],
    ['malformed port null', plantTemplate({ port: null })],
    ['malformed port object', plantTemplate({ port: {} })],
    ['unknown keys with port', plantTemplate({ port: 3001, before: EXTRAS_BEFORE, after: EXTRAS_AFTER })],
    ['unknown keys portless', plantTemplate({ before: EXTRAS_BEFORE, after: EXTRAS_AFTER })],
  ];
  for (const [label, template] of claimPlants) {
    for (const [plabel, port] of [
      ['valid', '3007'],
      ['invalid', 'banana'],
    ] as Array<[string, string]>) {
      out.push({
        name: `claim ${label} / ${plabel} PORT`,
        port,
        plant: { pid: 'dead', template },
        actions: ['acquire', 'heartbeat'],
      });
    }
  }
  // A foreign-host lock with no recent heartbeat — the other claim arm.
  out.push({
    name: 'claim foreign stale with port / valid PORT',
    port: '3007',
    plant: {
      pid: 'dead',
      template: plantTemplate({ host: 'elsewhere-host', lastHeartbeat: '2020-01-01T00:00:00.000Z', port: 3001 }),
    },
    actions: ['acquire'],
  });

  // P4.171 (C-6) follow-up: a foreign lock's NON-STRING `lastHeartbeat` —
  // v4 (`instance-lock.ts:683-685`) takes `existing.lastHeartbeat ? Date.now()
  // - new Date(existing.lastHeartbeat).getTime() : Infinity` over the raw
  // value, so a fresh epoch-ms number REFUSES (two hosts must never open one
  // DB); a falsy `0` is `Infinity` (`last: never`); `{}` is a `NaN` age
  // (claimed, `last: [object Object]`); `true` is `new Date(1)` (claimed).
  for (const [label, hb, threw] of [
    ['fresh numeric', '__NOW_MS__', true],
    ['numeric zero', 0, false],
    ['object', {}, false],
    ['boolean true', true, false],
  ] as Array<[string, unknown, boolean]>) {
    out.push({
      name: `${threw ? 'refused' : 'claim'} foreign ${label} heartbeat / valid PORT`,
      port: '3007',
      plant: {
        pid: 'dead',
        template: plantTemplate({ host: 'elsewhere-host', lastHeartbeat: hb, port: 3001 }),
      },
      actions: ['acquire'],
    });
  }

  // P4.171 (C-6): v4's lenient read — a stale claim of a lock missing fields
  // v4 never checks (the WARN drops `undefined` ones), an unknown environment,
  // and the two refusals (the invalid-structure WARN, the unreadable DEBUG),
  // each of which ends in v4's EEXIST re-read throw.
  for (const [label, template] of [
    ['missing processArgv0', plantTemplate({ omit: ['processArgv0'] })],
    ['missing environment + startedAt', plantTemplate({ omit: ['environment', 'startedAt'] })],
    ['unknown environment', plantTemplate({ environment: 'kubernetes' })],
    ['numeric environment', plantTemplate({ environment: 7 })],
  ] as Array<[string, string]>) {
    out.push({
      name: `claim ${label} / valid PORT`,
      port: '3007',
      plant: { pid: 'dead', template },
      actions: ['acquire', 'heartbeat'],
    });
  }
  for (const [label, template] of [
    ['pid string', plantTemplate({}).replace('"__PID__"', '"12"')],
    ['history missing', plantTemplate({ omit: ['history'] })],
    ['an array', '[1, 2]\n'],
    ['not json', '{ "pid": 12,\n'],
    ['json null', 'null\n'],
  ] as Array<[string, string]>) {
    out.push({
      name: `refused ${label}`,
      port: '3007',
      plant: { pid: 'dead', template },
      actions: ['acquire'],
    });
  }

  // Re-entrant (same PID, same host) — `port` is never touched.
  for (const [label, template] of [
    ['malformed port + unknown keys', plantTemplate({ port: 'abc', before: EXTRAS_BEFORE, after: EXTRAS_AFTER })],
    ['port 3001', plantTemplate({ port: 3001 })],
    ['portless', plantTemplate({})],
  ] as Array<[string, string]>) {
    for (const [plabel, port] of [
      ['valid', '3007'],
      ['invalid', 'banana'],
    ] as Array<[string, string]>) {
      out.push({
        name: `reentrant ${label} / ${plabel} PORT`,
        port,
        plant: { pid: 'self', template },
        actions: ['acquire', 'heartbeat'],
      });
    }
  }
  return out;
}

const MASKED_TOP = new Set([
  'pid',
  'hostname',
  'startedAt',
  'lastHeartbeat',
  'environment',
  'processTitle',
  'processArgv0',
]);

function maskLock(text: string): string {
  // P4.171: a refused plant stays on disk as written — text that is not JSON
  // is recorded raw, a JSON value that is not an object compactly.
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    return `RAW:${text}`;
  }
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return JSON.stringify(value);
  const parsed = value as Record<string, unknown>;
  const masked: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(parsed)) {
    if (MASKED_TOP.has(k)) {
      masked[k] = '<m>';
    } else if (k === 'history' && Array.isArray(v)) {
      masked[k] = v.map((entry: Record<string, unknown>) => {
        const e: Record<string, unknown> = {};
        for (const [ek, ev] of Object.entries(entry)) {
          if (ek === 'pid' || ek === 'hostname' || ek === 'timestamp') e[ek] = '<m>';
          else if (ek === 'detail' && typeof ev === 'string') e[ek] = ev.replace(/\d+/g, '<n>');
          else e[ek] = ev;
        }
        return e;
      });
    } else {
      masked[k] = v;
    }
  }
  return JSON.stringify(masked);
}

function deadPid(): number {
  const r = spawnSync('true');
  return r.pid as number;
}

async function main(): Promise<void> {
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('set QT_ORACLE_OUT');
  delete process.env.QUILLTAP_JOB_CHILD;
  process.env.LOG_LEVEL = 'error';

  // Capture the heartbeat's interval instead of running a real 60 s timer.
  let tick: (() => void) | null = null;
  const realSetInterval = globalThis.setInterval;
  const realClearInterval = globalThis.clearInterval;
  const DUMMY = { unref() { return DUMMY; }, ref() { return DUMMY; }, __dummy: true };
  (globalThis as unknown as Record<string, unknown>).setInterval = (fn: () => void) => {
    tick = fn;
    return DUMMY;
  };
  (globalThis as unknown as Record<string, unknown>).clearInterval = (h: unknown) => {
    if (h && (h as { __dummy?: boolean }).__dummy) return;
    realClearInterval(h as Parameters<typeof realClearInterval>[0]);
  };
  void realSetInterval;

  const lock = await import('@/lib/database/backends/sqlite/instance-lock');
  const { Logger } = await import('@/lib/logger');

  let sink: Line[] | null = null;
  for (const level of ['error', 'warn', 'info', 'debug'] as const) {
    const original = (Logger.prototype as unknown as Record<string, unknown>)[level];
    (Logger.prototype as unknown as Record<string, unknown>)[level] = function (
      this: { context?: Record<string, unknown> },
      message: string,
      context?: Record<string, unknown>,
      ...rest: unknown[]
    ) {
      if (sink) {
        const merged: Record<string, unknown> = { ...(this.context ?? {}), ...(context ?? {}) };
        if (merged.module === MODULE) {
          const fields: Record<string, unknown> = {};
          for (const [k, v] of Object.entries(merged)) {
            if (v === undefined) continue;
            fields[k] = k === 'pid' ? '<m>' : v;
          }
          sink.push({ level, message, fields });
        }
      }
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    };
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-instance-lock-port-oracle-'));
  const outLines: string[] = [];
  const g = globalThis as unknown as Record<string, unknown>;
  try {
    let n = 0;
    for (const spec of specs()) {
      n += 1;
      const dir = join(scratch, `row-${n}`);
      fs.mkdirSync(dir, { recursive: true });
      const lockPath = join(dir, 'quilltap.lock');
      // Fresh module state per row (v4 keeps it on globalThis).
      g.__quilltapInstanceLockOwner = undefined;
      g.__quilltapInstanceLockPath = undefined;
      g.__quilltapInstanceHeartbeatInterval = undefined;
      tick = null;

      if (spec.port === null) delete process.env.PORT;
      else process.env.PORT = spec.port;
      const listening = lock.resolveListeningPort();

      if (spec.plant) {
        const pid = spec.plant.pid === 'dead' ? deadPid() : process.pid;
        const text = spec.plant.template
          .replace('"__PID__"', String(pid))
          .replace('"__NOW_MS__"', String(Date.now()))
          .split('__HOST__')
          .join(osHostname());
        fs.writeFileSync(lockPath, text);
      }

      const actions: Array<{ action: string; masked: string | null; lines: Line[]; threw?: boolean }> = [];
      for (const action of spec.actions) {
        sink = [];
        let threw = false;
        if (action === 'acquire') {
          try {
            lock.acquireInstanceLock(lockPath);
          } catch {
            threw = true;
          }
        } else {
          if (!tick) throw new Error(`${spec.name}: no heartbeat registered`);
          (tick as () => void)();
        }
        const lines = sink;
        sink = null;
        const masked = fs.existsSync(lockPath) ? maskLock(fs.readFileSync(lockPath, 'utf8')) : null;
        actions.push({ action, masked, lines, ...(threw && { threw: true }) });
      }
      lock.stopLockHeartbeat();

      outLines.push(
        JSON.stringify({
          kind: 'row',
          name: spec.name,
          port: spec.port,
          listeningPort: listening === undefined ? null : listening,
          plant: spec.plant,
          actions,
        }),
      );
    }
  } finally {
    delete process.env.PORT;
    (globalThis as unknown as Record<string, unknown>).setInterval = realSetInterval;
    (globalThis as unknown as Record<string, unknown>).clearInterval = realClearInterval;
    rmSync(scratch, { recursive: true, force: true });
  }

  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
}

main().then(
  () => process.exit(0),
  (error) => {
    console.error(error);
    process.exit(1);
  },
);
