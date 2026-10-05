/**
 * Tier-1 oracle case — v4's REAL `lib/chat/impersonation-voice-legacy.ts`
 * (NEW at `07b8f0209`, P4.D251): the retired `impersonationVoiceRewrite`
 * boolean translated into `impersonationVoiceMode`.
 *
 * Two pure functions, both driven over a fixed corpus:
 *
 *   - `impersonationVoiceModeFromLegacy(value)` — the six scalars v4's own spec
 *     names (`true`, `1`, `false`, `0`, `null`, `undefined`) plus the edges
 *     that tell `=== 1` from truthiness (`1.0` — one JS number; `2`; the
 *     strings `'1'` and `'true'`).
 *   - `withImpersonationVoiceModeFromLegacy(settings)` — the record cases: no
 *     key (the SAME reference comes back — `same: true`), key `true` with no
 *     mode, key `true` beside an explicit `'always'`, key `false` beside a
 *     `null` mode (JS `??` falls to the legacy value), the key present as
 *     `undefined` (key PRESENCE, not truthiness — it still translates), and
 *     key `1` beside an explicit `'off'` (the `??` keeps `'off'`).
 *
 * Output: one NDJSON line per row. Record outputs are emitted as JSON so the
 * Rust side compares key ORDER too (v4 spreads `...rest` then sets the mode:
 * a mode key the record already carried KEEPS its slot; a mode it lacked is
 * appended LAST — after `createdAt` — wherever the retired key sat). An `undefined` input is spelled `{"undefined": true}` in
 * the `value` slot because JSON has no undefined.
 *
 * Run (Node 24, from the v4 checkout or a pinned worktree):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5W/harness/oracle/cases/impersonation-voice-legacy.ts \
 *     > /tmp/oracle-impersonation-voice-legacy.ndjson
 */

import {
  impersonationVoiceModeFromLegacy,
  withImpersonationVoiceModeFromLegacy,
} from '@/lib/chat/impersonation-voice-legacy';

const out = (row: Record<string, unknown>) => process.stdout.write(JSON.stringify(row) + '\n');

const UNDEFINED = { undefined: true };

// ---- the scalar translation ------------------------------------------------
const scalars: Array<[string, unknown]> = [
  ['true', true],
  ['one', 1],
  ['false', false],
  ['zero', 0],
  ['null', null],
  ['undefined', undefined],
  ['one_point_zero', 1.0],
  ['two', 2],
  ['string_one', '1'],
  ['string_true', 'true'],
];
for (const [label, value] of scalars) {
  out({
    op: 'fromLegacy',
    label,
    value: value === undefined ? UNDEFINED : value,
    result: impersonationVoiceModeFromLegacy(value as never),
  });
}

// ---- the record translation ------------------------------------------------
const records: Array<[string, Record<string, unknown>]> = [
  ['no_key', { id: 'cs-1', impersonationVoiceMode: 'off', createdAt: 't' }],
  ['key_true_no_mode', { id: 'cs-1', impersonationVoiceRewrite: true, createdAt: 't' }],
  ['key_true_mode_always', { id: 'cs-1', impersonationVoiceRewrite: true, impersonationVoiceMode: 'always', createdAt: 't' }],
  ['key_false_mode_null', { id: 'cs-1', impersonationVoiceRewrite: false, impersonationVoiceMode: null, createdAt: 't' }],
  // A `null` mode FALLS to the legacy value (JS `??`) — with legacy `true`, so
  // a port mapping a null mode to "default off" cannot pass (the §3 review of
  // the `07b8f0209` unification).
  ['key_true_mode_null', { id: 'cs-1', impersonationVoiceRewrite: true, impersonationVoiceMode: null, createdAt: 't' }],
  ['key_present_undefined', { id: 'cs-1', impersonationVoiceRewrite: undefined, createdAt: 't' }],
  ['key_one_mode_off', { id: 'cs-1', impersonationVoiceRewrite: 1, impersonationVoiceMode: 'off', createdAt: 't' }],
];
for (const [label, input] of records) {
  const result = withImpersonationVoiceModeFromLegacy(input as never);
  // JSON.stringify drops an `undefined`-valued key, so the INPUT is spelled
  // with the sentinel where the key is present-but-undefined.
  const spelled: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(input)) spelled[k] = v === undefined ? UNDEFINED : v;
  out({
    op: 'withLegacy',
    label,
    input: spelled,
    same: result === input,
    result,
  });
}
