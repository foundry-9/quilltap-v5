/**
 * Client oracle — the Concierge presentation table, EXECUTED out of v4's own
 * module rather than transcribed from it.
 *
 * v4 `lib/services/dangerous-content/concierge-state-presentation.ts` is the
 * single source for every word, icon and tone the Concierge states wear on
 * screen — THREE states since v4 `4d370a90f` (#75), the `info` tone retired at
 * `3b463d6b1` (#76), and `describeConciergeState(state, provenance, categories)`
 * picking Unmoderated's sentence by provenance. v5's twin lives at
 * `apps/web/src/app/chat/concierge-state-presentation.ts` (shared contract §B:
 * the table lives ONCE, in the SPA — there is no Rust twin). This recorder
 * imports v4's REAL module and emits every string it can produce, so the SPA
 * spec diffs against v4's bytes instead of against a retyping of them.
 *
 * The module is client-safe and its only imports are `import type`, which Node
 * 24's type stripping erases outright — so it runs with no bundler, no path
 * aliases and no jest.
 *
 * REGEN RECIPE (run from anywhere; writes the committed oracle in place):
 *
 *   export PATH=~/.nvm/versions/node/v24.13.1/bin:$PATH
 *   QT_V4_PIN=acadcc7cd node ~/source/quilltap-v5/harness/oracle/cases/concierge-presentation.mjs \
 *     > ~/source/quilltap-v5/apps/web/src/app/chat/concierge-state-presentation.v4.json
 *
 * Override the checkout or the pin with QT_V4_CHECKOUT / QT_V4_PIN. Reading
 * through `git show` at the pin makes the recipe independent of the v4 working
 * tree (drift-ledger §5.1's PIN REQUIRED rule is satisfied by construction).
 * Pin marker: the table carries three rows and NO `info` tone. Expect 144
 * `describe` rows (3 states × 16 provenance shapes × 3 category shapes).
 */

import { execFileSync } from 'node:child_process';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const CHECKOUT = process.env.QT_V4_CHECKOUT ?? `${process.env.HOME}/source/quilltap-server`;
const PIN = process.env.QT_V4_PIN ?? 'acadcc7cd';
const SOURCE = 'lib/services/dangerous-content/concierge-state-presentation.ts';

const src = execFileSync('git', ['-C', CHECKOUT, 'show', `${PIN}:${SOURCE}`], {
  encoding: 'utf8',
});

const dir = mkdtempSync(join(tmpdir(), 'qt-concierge-presentation-'));
const file = join(dir, 'concierge-state-presentation.ts');
writeFileSync(file, src);

const mod = await import(pathToFileURL(file).href);

const STATES = ['moderated', 'unmoderated', 'locked'];
const TONES = ['danger', 'muted', 'success'];

/**
 * Every provenance note a caller can pass: none (the list marks' Moderated/
 * Locked and the New Chat helper), the operator's, and the Concierge's for each
 * reason — `refusals` with no count, the counts either side of the
 * number-word cutoff (0, 1, 2, 10, 11) and `null`.
 */
const PROVENANCES = [
  { id: 'omitted', note: undefined },
  { id: 'empty', note: {} },
  { id: 'operator-manual', note: { setBy: 'operator', reason: 'manual' } },
  { id: 'operator-migration', note: { setBy: 'operator', reason: 'migration' } },
  { id: 'operator-refusals-2', note: { setBy: 'operator', reason: 'refusals', refusalCount: 2 } },
  { id: 'concierge-refusals-nocount', note: { setBy: 'concierge', reason: 'refusals' } },
  { id: 'concierge-refusals-null', note: { setBy: 'concierge', reason: 'refusals', refusalCount: null } },
  { id: 'concierge-refusals-0', note: { setBy: 'concierge', reason: 'refusals', refusalCount: 0 } },
  { id: 'concierge-refusals-1', note: { setBy: 'concierge', reason: 'refusals', refusalCount: 1 } },
  { id: 'concierge-refusals-2', note: { setBy: 'concierge', reason: 'refusals', refusalCount: 2 } },
  { id: 'concierge-refusals-10', note: { setBy: 'concierge', reason: 'refusals', refusalCount: 10 } },
  { id: 'concierge-refusals-11', note: { setBy: 'concierge', reason: 'refusals', refusalCount: 11 } },
  { id: 'concierge-classifier', note: { setBy: 'concierge', reason: 'classifier' } },
  { id: 'concierge-migration', note: { setBy: 'concierge', reason: 'migration' } },
  { id: 'concierge-null-reason', note: { setBy: 'concierge', reason: null } },
  { id: 'setby-null', note: { setBy: null, reason: 'manual' } },
];

const CATEGORIES = [undefined, [], ['NSFW', 'Violence']];

/** Every describeConciergeState shape the SPA can ask for. */
const describeCases = [];
for (const state of STATES) {
  for (const { id, note } of PROVENANCES) {
    for (const dangerCategories of CATEGORIES) {
      describeCases.push({
        state,
        provenance: id,
        note: note ?? null,
        dangerCategories: dangerCategories ?? null,
        result: mod.describeConciergeState(state, note, dangerCategories),
      });
    }
  }
}

process.stdout.write(
  JSON.stringify(
    {
      _source: { checkout: 'quilltap-server', pin: PIN, file: SOURCE },
      presentation: mod.CONCIERGE_STATE_PRESENTATION,
      toneSuffix: Object.fromEntries(TONES.map((t) => [t, mod.conciergeToneSuffix(t)])),
      toneTextClass: Object.fromEntries(TONES.map((t) => [t, mod.conciergeToneTextClass(t)])),
      describe: describeCases,
    },
    null,
    2,
  ) + '\n',
);
