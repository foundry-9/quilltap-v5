/**
 * Oracle case (tier 1): the pure half of v4's scenarios-from-anywhere slice
 * (`545e299bb`, pinned at `83334b9a5`) — every pure export the scenario
 * location, the catalogue, the recipe resolver and the chat's scene-keeping
 * rest on, driven through v4's REAL modules:
 *
 *   - `decodeURIComponent` itself (V8) — the decode `normalizeScenarioPath`
 *     runs first (ruling R-Z);
 *   - `normalizeScenarioPath`, `characterScenarioIdForPath`
 *     (`lib/scenarios/location.ts`);
 *   - `scenarioExcerpt` (`lib/scenarios/everywhere.ts`) — recorded as UTF-16
 *     code UNITS (`excerptUnits`), so a lone surrogate v4 leaves at a cut
 *     survives NDJSON (serde refuses a lone `\udXXX`), for the
 *     `EXCERPT_LONE_SURROGATE` table (R-AA);
 *   - `hashScenarioBody`, `sameResolvedScenario`
 *     (`lib/chat/scenario-selection.ts`);
 *   - `combineScenarioText`, `recoverPresetBody` (`lib/chat/scenario-text.ts`);
 *   - `keepCurrentScene` (`app/api/v1/chats/[id]/actions/scenario.ts`).
 *
 * No clock, no randomness, no database. Both sides replay the recorded inputs.
 *
 * Run (Node 24), from the v4 checkout (a pin worktree when v4 HEAD is past
 * the round's baseline):
 *   cd ~/source/quilltap-server
 *   TZ=UTC npx tsx <V5>/harness/oracle/cases/scenario-pure.ts \
 *     > /tmp/oracle-scenario-pure.ndjson
 * Rust side:
 *   QT_ORACLE_SCENARIO_PURE=/tmp/oracle-scenario-pure.ndjson \
 *     cargo test -p quilltap-harness --test scenario_pure_equivalence
 */

import { scenarioExcerpt } from '@/lib/scenarios/everywhere';
import { characterScenarioIdForPath, normalizeScenarioPath } from '@/lib/scenarios/location';
import { hashScenarioBody, sameResolvedScenario } from '@/lib/chat/scenario-selection';
import { combineScenarioText, recoverPresetBody } from '@/lib/chat/scenario-text';
import { keepCurrentScene } from '@/app/api/v1/chats/[id]/actions/scenario';

const out = (row: Record<string, unknown>) => process.stdout.write(`${JSON.stringify(row)}\n`);
const units = (s: string) => Array.from({ length: s.length }, (_, i) => s.charCodeAt(i));

// ---------------------------------------------------------------------------
// decodeURIComponent — V8's accept-or-throw
// ---------------------------------------------------------------------------

const DECODE = [
  '', 'plain', 'a%20b', 'a%2Fb', '%2F', '%2f', '%2E%2E', 'Scenarios%2Fa.md', '%41%42', '%e2%80%a6',
  '%E2%80%A6', '%F0%9F%8E%AD', 'caf%C3%A9', 'café', '100%', '%', '%%', '%2', '%G0', '%ZZ', '%E0',
  '%E0%A0', '%E0%A0%80', '%C0%80', '%C1%BF', '%C2%80', '%DF%BF', '%E0%80%80', '%ED%9F%BF',
  '%ED%A0%80', '%ED%BF%BF', '%EE%80%80', '%EF%BF%BF', '%F0%90%80%80', '%F4%8F%BF%BF',
  '%F4%90%80%80', '%F5%80%80%80', '%F8%80%80%80%80', '%80', '%BF', '%C2', '%C2%41', '%C2%C2%80',
  '%E2%82', '%E2%82%AC%20', 'a%20%20b', '%2525', '+', '%2B', 'a b', '%0A', '%00', '%7F',
  '%e2%80%A6', '%C2%A0', '%EF%BB%BF', '%E2%80%A8',
];
for (const input of DECODE) {
  try {
    out({ kind: 'decode', input, ok: true, output: decodeURIComponent(input) });
  } catch (error) {
    out({ kind: 'decode', input, ok: false, error: error instanceof Error ? `${error.name}: ${error.message}` : String(error) });
  }
}

// ---------------------------------------------------------------------------
// normalizeScenarioPath — decode, trim, the Scenarios/<file>.md rules
// ---------------------------------------------------------------------------

const NORMALIZE = [
  'a', 'a.md', 'a.MD', 'Scenarios/a.md', 'Scenarios/A.Md', '/a', '//a', '///a', 'Scenarios//a.md',
  '  a  ', '', '   ', ' a ', '﻿a', '..', '../x', 'Scenarios/../x.md', 'a..b', 'a/b',
  'Scenarios/a/b.md', 'Scenarios/', 'Scenarios', 'scenarios/a.md', 'a%20b', 'Scenarios/a%20b.md',
  'Scenarios%2Fa.md', 'a%2Fb', '%2E%2E', '%2E%2E%2Fx', '%20', '%20a%20', 'caf%C3%A9', 'a%', '%E0',
  'Scenarios/%E0.md', '%ED%A0%80', 'a.mdx', '.md', 'Scenarios/.md', 'x\ny', '%0Aa%0A',
  'Scenarios/a%2520b.md',
];
for (const input of NORMALIZE) {
  try {
    const r = normalizeScenarioPath(input);
    out(r.ok
      ? { kind: 'normalize', input, threw: false, ok: true, path: r.path }
      : { kind: 'normalize', input, threw: false, ok: false, error: r.error });
  } catch (error) {
    out({ kind: 'normalize', input, threw: true, error: error instanceof Error ? `${error.name}: ${error.message}` : String(error) });
  }
}

// ---------------------------------------------------------------------------
// characterScenarioIdForPath — the vault parser's id, from a ref
// ---------------------------------------------------------------------------

const MP = '7c1d2a4e-0f3b-4c5d-8e9f-0a1b2c3d4e5f';
for (const [mp, path] of [
  [MP, 'Scenarios/a.md'],
  [MP, 'Scenarios/A.md'],
  [MP, 'Scenarios/café.md'],
  ['00000000-0000-4000-8000-000000000000', 'Scenarios/a.md'],
  ['', ''],
  [MP, 'Scenarios/a b.md'],
]) {
  out({ kind: 'idForPath', mountPointId: mp, path, id: characterScenarioIdForPath(mp, path) });
}

// ---------------------------------------------------------------------------
// scenarioExcerpt — JS `\s` collapse, trim, the 240-unit cut
// ---------------------------------------------------------------------------

const x = (n: number) => 'x'.repeat(n);
const EXCERPT: Array<[string, string]> = [
  ['empty', ''],
  ['all_whitespace', ' \t\n\r ﻿ 　 '],
  ['short', 'A short scene.'],
  ['runs_collapse', '  The   tale\n\nbegins\t\there.  '],
  ['nbsp_feff_ls', 'a  b﻿c d e f g　h i j'],
  ['nel_not_js_ws', 'a\u0085b\u0085\u0085c'],
  ['zwsp_not_ws', 'a​b'],
  ['exactly_240', x(240)],
  ['exactly_241', x(241)],
  ['long_words', 'word '.repeat(80)],
  ['space_at_cut', `${x(239)} ${x(20)}`],
  ['spaces_before_cut', `${x(230)}          ${x(20)}`],
  ['collapsed_under_240', `${x(200)}${' '.repeat(100)}${x(30)}`],
  ['emoji_straddles_239_240', `${x(239)}\u{1F3AD}${x(10)}`],
  ['emoji_at_238_239', `${x(238)}\u{1F3AD}${x(10)}`],
  ['emoji_at_240_241', `${x(240)}\u{1F3AD}`],
  ['emoji_ends_at_240_exactly', `${x(238)}\u{1F3AD}`],
  ['emoji_run_straddles', '\u{1F3AD}'.repeat(125)],
  ['cjk_long', '漢'.repeat(300)],
  ['ws_run_crosses_cut', `${x(235)} \n\t ${x(10)}`],
  ['leading_ws_counts_after_trim', `   ${x(240)}`],
  ['nbsp_at_cut', `${x(239)} ${x(5)}`],
  ['multiline_long', 'Line one of the scene.\n'.repeat(20)],
];
for (const [name, body] of EXCERPT) {
  const excerpt = scenarioExcerpt(body);
  out({ kind: 'excerpt', name, body, excerptUnits: units(excerpt) });
}

// ---------------------------------------------------------------------------
// hashScenarioBody
// ---------------------------------------------------------------------------

for (const body of ['', 'abc', 'The tale begins.', 'café \u{1F3AD}', 'line\nline\r\n', '﻿x']) {
  out({ kind: 'hash', body, hex: hashScenarioBody(body) });
}

// ---------------------------------------------------------------------------
// combineScenarioText / recoverPresetBody
// ---------------------------------------------------------------------------

const COMBINE: Array<[string | null, string | null]> = [
  [null, null], ['', ''], ['Body', null], [null, 'Notes'], ['Body', 'Notes'], ['  Body  ', '  Notes  '],
  ['Body\n\n', '\nNotes\n'], ['Body﻿', null], [null, '﻿Notes﻿'], ['Body\u0085', null],
  [null, '\u0085Notes'], ['Body ', ' Notes'], ['   ', '   '], ['Body', '   '], ['\tBody', null],
];
for (const [preset, free] of COMBINE) {
  out({ kind: 'combine', preset, free, output: combineScenarioText(preset, free) ?? null });
}

const RECOVER: Array<[string | null, string | null]> = [
  [null, 'n'], ['', 'n'], ['body', null], ['body', ''], ['body', '   '], ['body\n\nnotes', 'notes'],
  ['body\n\nnotes', '  notes  '], ['body\nnotes', 'notes'], ['notes', 'notes'], ['\n\nnotes', 'notes'],
  ['body\n\nnotes\n', 'notes'], ['body\n\n﻿notes', '﻿notes'], ['body\n\nnotes', '﻿notes﻿'],
  ['body\n\n\u0085notes', '\u0085notes'], ['a\n\nb\n\nb', 'b'], ['x', 'x '],
];
for (const [text, notes] of RECOVER) {
  out({ kind: 'recover', text, notes, output: recoverPresetBody(text, notes) });
}

// ---------------------------------------------------------------------------
// sameResolvedScenario / keepCurrentScene
// ---------------------------------------------------------------------------

type Src = { mountPointId: string; path: string; bodySha256: string } | null;
type Recipe = { scenarioText: string | null; scenarioSource: Src; scenarioNotes: string | null };
const MP2 = '11111111-2222-4333-8444-555555555555';
const src = (path: string, body: string, mp = MP): Src => ({ mountPointId: mp, path, bodySha256: hashScenarioBody(body) });
const recipe = (scenarioText: string | null, scenarioSource: Src, scenarioNotes: string | null): Recipe => ({ scenarioText, scenarioSource, scenarioNotes });

const SAME: Array<[string, Recipe, Recipe]> = [
  ['both_empty', recipe(null, null, null), recipe(null, null, null)],
  ['text_differs', recipe('a', null, null), recipe('b', null, null)],
  ['text_vs_null', recipe('a', null, null), recipe(null, null, null)],
  ['notes_differ', recipe('a', src('Scenarios/a.md', 'a'), 'n'), recipe('a', src('Scenarios/a.md', 'a'), null)],
  ['source_vs_none', recipe('a', src('Scenarios/a.md', 'a'), null), recipe('a', null, null)],
  ['source_equal', recipe('a', src('Scenarios/a.md', 'a'), null), recipe('a', src('Scenarios/a.md', 'a'), null)],
  ['source_mount_differs', recipe('a', src('Scenarios/a.md', 'a'), null), recipe('a', src('Scenarios/a.md', 'a', MP2), null)],
  ['source_path_differs', recipe('a', src('Scenarios/a.md', 'a'), null), recipe('a', src('Scenarios/b.md', 'a'), null)],
  ['source_hash_differs', recipe('a', src('Scenarios/a.md', 'a'), null), recipe('a', src('Scenarios/a.md', 'b'), null)],
  ['empty_text_vs_null', recipe('', null, null), recipe(null, null, null)],
];
for (const [name, a, b] of SAME) {
  out({ kind: 'same', name, a, b, output: sameResolvedScenario(a, b) });
}

const OLD = 'The original scene.';
const NEW = 'The revised scene.';
const P = 'Scenarios/tale.md';
const KEEP: Array<[string, Recipe, Recipe]> = [
  ['revised_kept_new_notes', recipe(`${NEW}\n\nfresh`, src(P, NEW), 'fresh'), recipe(`${OLD}\n\nold notes`, src(P, OLD), 'old notes')],
  ['revised_kept_no_notes', recipe(NEW, src(P, NEW), null), recipe(OLD, src(P, OLD), null)],
  ['revised_kept_notes_removed', recipe(NEW, src(P, NEW), null), recipe(`${OLD}\n\nold notes`, src(P, OLD), 'old notes')],
  ['unchanged_hash_nothing_to_keep', recipe(OLD, src(P, OLD), null), recipe(OLD, src(P, OLD), null)],
  ['different_path', recipe(NEW, src('Scenarios/other.md', NEW), null), recipe(OLD, src(P, OLD), null)],
  ['different_mount', recipe(NEW, src(P, NEW, MP2), null), recipe(OLD, src(P, OLD), null)],
  ['no_stored_source', recipe(NEW, src(P, NEW), null), recipe(OLD, null, null)],
  ['no_incoming_source', recipe('custom', null, null), recipe(OLD, src(P, OLD), null)],
  ['stored_text_no_longer_ends_in_notes', recipe(NEW, src(P, NEW), null), recipe(`${OLD}\n\nedited`, src(P, OLD), 'old notes')],
  ['stored_body_hash_mismatch', recipe(NEW, src(P, NEW), null), recipe('Hand-edited scene.', src(P, OLD), null)],
  ['stored_text_null', recipe(NEW, src(P, NEW), null), recipe(null, src(P, OLD), null)],
  ['stored_text_empty', recipe(NEW, src(P, NEW), null), recipe('', src(P, OLD), null)],
  ['notes_whitespace_kept_body', recipe(`${NEW}\n\nn`, src(P, NEW), 'n'), recipe(OLD, src(P, OLD), '   ')],
];
for (const [name, next, current] of KEEP) {
  const chat = { id: 'chat-1', scenarioText: current.scenarioText, scenarioSource: current.scenarioSource, scenarioNotes: current.scenarioNotes };
  out({ kind: 'keep', name, next, current, output: keepCurrentScene(next, chat as never) });
}
