/**
 * Client oracle — the Concierge's per-chat state derivation, EXECUTED out of
 * v4's own `lib/services/dangerous-content/chat-override.ts` rather than
 * transcribed from it (P4.D229).
 *
 * v4 imports that module into BOTH its server and its React components; v5's
 * client twin is `apps/web/src/app/chat/concierge-state.ts`. Until P4.D229 the
 * twin's spec was a hand transcription of v4's test file. This recorder
 * imports v4's REAL module (its only imports are `import type`, which Node 24's
 * type stripping erases — no bundler, no aliases, no jest) and runs every
 * client-reachable getter over a corpus of every mode × setBy × reason ×
 * legacy-pair combination, in BOTH shapes the helpers accept (the stored
 * columns and the server-derived payload keys) and with the two disagreeing.
 * The SPA spec `concierge-state.oracle.spec.ts` diffs v5 against these bytes.
 *
 * (`deriveConciergeModeFromLegacy` / `withConciergeModeFromLegacy` are the
 * import/restore mapping — server-only; the Rust chain lane ports them.)
 *
 * REGEN RECIPE (run from anywhere; writes the committed oracle in place):
 *
 *   export PATH=~/.nvm/versions/node/v24.13.1/bin:$PATH
 *   QT_V4_PIN=acadcc7cd node ~/source/quilltap-v5/harness/oracle/cases/concierge-chat-override.mjs \
 *     > ~/source/quilltap-v5/apps/web/src/app/chat/concierge-state.v4.json
 *
 * Override the checkout or the pin with QT_V4_CHECKOUT / QT_V4_PIN. Reading
 * through `git show` at the pin makes the recipe independent of the v4 working
 * tree (drift-ledger §5.1's PIN REQUIRED rule is satisfied by construction).
 * Expect `rows` of length 484.
 */

import { execFileSync } from 'node:child_process';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const CHECKOUT = process.env.QT_V4_CHECKOUT ?? `${process.env.HOME}/source/quilltap-server`;
const PIN = process.env.QT_V4_PIN ?? 'acadcc7cd';
const SOURCE = 'lib/services/dangerous-content/chat-override.ts';

const src = execFileSync('git', ['-C', CHECKOUT, 'show', `${PIN}:${SOURCE}`], {
  encoding: 'utf8',
});

const dir = mkdtempSync(join(tmpdir(), 'qt-concierge-chat-override-'));
const file = join(dir, 'chat-override.ts');
writeFileSync(file, src);

const mod = await import(pathToFileURL(file).href);

// `undefined` cannot survive JSON; the corpus spells an absent key by leaving it
// out of `chat` altogether, so ABSENT and `null` stay distinguishable.
const MODES = [undefined, null, 'moderated', 'unmoderated', 'locked', 'flagged'];
const SET_BYS = [undefined, null, 'operator', 'concierge', 'bogus'];
const REASONS = [undefined, null, 'manual', 'refusals', 'classifier', 'migration'];
const LEGACY = [
  undefined,
  { conciergeOverride: 'UNCENSORED', isDangerousChat: true },
  { conciergeOverride: 'OFF', isDangerousChat: false },
  { conciergeOverride: null, isDangerousChat: true },
];

const KEYS = {
  column: ['conciergeMode', 'conciergeModeSetBy', 'conciergeModeReason'],
  payload: ['conciergeState', 'conciergeSetBy', 'conciergeReason'],
};

function build(form, mode, setBy, reason, legacy) {
  const chat = {};
  const [m, s, r] = KEYS[form];
  if (mode !== undefined) chat[m] = mode;
  if (setBy !== undefined) chat[s] = setBy;
  if (reason !== undefined) chat[r] = reason;
  if (legacy) Object.assign(chat, legacy);
  return chat;
}

function answer(chat) {
  return {
    state: mod.getConciergeState(chat),
    provenance: mod.getConciergeProvenance(chat),
    reason: mod.getConciergeReason(chat),
    uncensoredRoute: mod.shouldUseUncensoredRoute(chat),
    dangerStyling: mod.shouldShowDangerStyling(chat),
    classifierOnDuty: mod.isClassifierOnDuty(chat),
    mayFailOver: mod.mayFailOver(chat),
  };
}

const rows = [];
// The null/undefined chat (v4's "chatless call").
rows.push({ id: 'chat-null', chat: null, out: answer(null) });
rows.push({ id: 'chat-undefined', chat: '__undefined__', out: answer(undefined) });

// 1. Full product, each shape alone: 6 × 5 × 6 × 2 = 360 rows.
for (const form of ['column', 'payload']) {
  for (const mode of MODES) {
    for (const setBy of SET_BYS) {
      for (const reason of REASONS) {
        const chat = build(form, mode, setBy, reason);
        rows.push({ id: `${form}`, chat, out: answer(chat) });
      }
    }
  }
}

// 2. The legacy pair riding on each mode in each shape: 6 × 3 × 2 = 36 rows —
//    "ignores the legacy pair entirely".
for (const form of ['column', 'payload']) {
  for (const mode of MODES) {
    for (const legacy of LEGACY.slice(1)) {
      const chat = build(form, mode, 'concierge', 'classifier', legacy);
      rows.push({ id: `${form}+legacy`, chat, out: answer(chat) });
    }
  }
}

// 3. Columns AND payload disagreeing — "prefers the column": 6 × 6 = 36 rows
//    for the state, plus setBy/reason precedence over an Unmoderated column
//    (5 × 5 + 6 × 6 would be large; the two diagonals that matter): 35 rows.
for (const colMode of MODES) {
  for (const payMode of MODES) {
    const chat = { ...build('column', colMode, undefined, undefined), ...build('payload', payMode, undefined, undefined) };
    rows.push({ id: 'column-vs-payload-state', chat, out: answer(chat) });
  }
}
for (const colBy of SET_BYS) {
  for (const payBy of SET_BYS) {
    if (colBy === payBy) continue;
    const chat = { conciergeMode: 'unmoderated', ...(colBy !== undefined ? { conciergeModeSetBy: colBy } : {}), ...(payBy !== undefined ? { conciergeSetBy: payBy } : {}) };
    rows.push({ id: 'column-vs-payload-setBy', chat, out: answer(chat) });
  }
}
for (const colReason of REASONS) {
  for (const payReason of REASONS) {
    if (colReason === payReason) continue;
    const chat = { conciergeState: 'locked', ...(colReason !== undefined ? { conciergeModeReason: colReason } : {}), ...(payReason !== undefined ? { conciergeReason: payReason } : {}) };
    rows.push({ id: 'column-vs-payload-reason', chat, out: answer(chat) });
  }
}

const STATES = ['moderated', 'unmoderated', 'locked', 'flagged'];
const stateOnly = STATES.map((s) => ({
  state: s,
  usesUncensoredRoute: mod.conciergeStateUsesUncensoredRoute(s),
  mayFailOver: mod.conciergeStateMayFailOver(s),
}));

const out = [
  '{',
  `  "_source": ${JSON.stringify({ checkout: 'quilltap-server', pin: PIN, file: SOURCE })},`,
  `  "states": ${JSON.stringify(mod.CONCIERGE_STATES)},`,
  `  "stateOnly": ${JSON.stringify(stateOnly)},`,
  '  "rows": [',
  rows.map((r) => `    ${JSON.stringify(r)}`).join(',\n'),
  '  ]',
  '}',
];
process.stdout.write(out.join('\n') + '\n');
