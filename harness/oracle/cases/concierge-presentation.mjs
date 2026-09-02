/**
 * Client oracle — the Concierge presentation table, EXECUTED out of v4's own
 * module rather than transcribed from it.
 *
 * v4 `lib/services/dangerous-content/concierge-state-presentation.ts` (new at
 * `c43d3b1b4`) is the single source for every word, icon and tone the four
 * Concierge states wear on screen. v5's twin lives at
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
 *   node ~/source/quilltap-v5/harness/oracle/cases/concierge-presentation.mjs \
 *     > ~/source/quilltap-v5/apps/web/src/app/chat/concierge-state-presentation.v4.json
 *
 * Override the checkout or the pin with QT_V4_CHECKOUT / QT_V4_PIN. The default
 * pin is this lane's target commit; reading through `git show` means the recipe
 * is independent of whatever the v4 working tree happens to hold (drift-ledger
 * §5.1's PIN REQUIRED rule is satisfied by construction).
 */

import { execFileSync } from 'node:child_process';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const CHECKOUT = process.env.QT_V4_CHECKOUT ?? `${process.env.HOME}/source/quilltap-server`;
const PIN = process.env.QT_V4_PIN ?? 'c43d3b1b4';
const SOURCE = 'lib/services/dangerous-content/concierge-state-presentation.ts';

const src = execFileSync('git', ['-C', CHECKOUT, 'show', `${PIN}:${SOURCE}`], {
  encoding: 'utf8',
});

const dir = mkdtempSync(join(tmpdir(), 'qt-concierge-presentation-'));
const file = join(dir, 'concierge-state-presentation.ts');
writeFileSync(file, src);

const mod = await import(pathToFileURL(file).href);

const STATES = ['monitored', 'flagged', 'vouched', 'uncensored'];
const TONES = ['danger', 'muted', 'info', 'success'];

/** Every describeConciergeState shape the SPA can ask for. */
const describeCases = [];
for (const state of STATES) {
  describeCases.push({ state, dangerCategories: undefined, result: mod.describeConciergeState(state) });
  describeCases.push({ state, dangerCategories: [], result: mod.describeConciergeState(state, []) });
  describeCases.push({
    state,
    dangerCategories: ['NSFW', 'Violence'],
    result: mod.describeConciergeState(state, ['NSFW', 'Violence']),
  });
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
