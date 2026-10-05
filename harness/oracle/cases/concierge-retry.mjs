/**
 * Client oracle — "Try uncensored"'s pure helpers, EXECUTED out of v4's own
 * `app/salon/[id]/concierge-retry.ts` (new at `ce2f1dabf`, #77) rather than
 * transcribed from it (P4.D229).
 *
 * v5's twin is `apps/web/src/app/chat/concierge-retry.ts`. The module's only
 * import is `import type { Message } from './types'`, which Node 24's type
 * stripping erases — so it runs with no bundler, no aliases and no jest. The
 * recorder emits `describeRetryRefusal` over every token a dispatch error could
 * carry (and a few it could not), `isLanternBackgroundRefusal` over every
 * sender × kind pair that matters, and v4's retry URL for the record (v5 has
 * no URL — `retryUncensoredTurnRequest` builds §S.3's dispatch request; the
 * SPA spec pins the one-to-one mapping of its two ids).
 *
 * REGEN RECIPE (run from anywhere; writes the committed oracle in place):
 *
 *   export PATH=~/.nvm/versions/node/v24.13.1/bin:$PATH
 *   QT_V4_PIN=acadcc7cd node ~/source/quilltap-v5/harness/oracle/cases/concierge-retry.mjs \
 *     > ~/source/quilltap-v5/apps/web/src/app/chat/concierge-retry.v4.json
 *
 * Override the checkout or the pin with QT_V4_CHECKOUT / QT_V4_PIN. Reading
 * through `git show` at the pin makes the recipe independent of the v4 working
 * tree. Expect 12 `refusal` rows and 20 `lantern` rows.
 */

import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const CHECKOUT = process.env.QT_V4_CHECKOUT ?? `${process.env.HOME}/source/quilltap-server`;
const PIN = process.env.QT_V4_PIN ?? 'acadcc7cd';
const SOURCE = 'app/salon/[id]/concierge-retry.ts';

const src = execFileSync('git', ['-C', CHECKOUT, 'show', `${PIN}:${SOURCE}`], {
  encoding: 'utf8',
});

const dir = mkdtempSync(join(tmpdir(), 'qt-concierge-retry-'));
process.on('exit', () => rmSync(dir, { recursive: true, force: true }));
const file = join(dir, 'concierge-retry.ts');
writeFileSync(file, src);

const mod = await import(pathToFileURL(file).href);

// `undefined` cannot survive JSON; it is spelled by the `absent` flag.
const REFUSALS = [
  { absent: true },
  { error: null },
  { error: 'no-understudy' },
  { error: 'locked' },
  { error: 'Locked' },
  { error: 'no-understudy ' },
  { error: 'LOCKED' },
  { error: '' },
  { error: 'Chat not found' },
  { error: 'Only assistant messages can be retried' },
  { error: 'Staff and system messages cannot be regenerated' },
  { error: 409 },
];

const SENDERS = [null, 'lantern', 'concierge', 'aurora', 'host'];
const KINDS = [null, 'background-refused', 'background', 'refusal'];

const out = {
  _source: { checkout: 'quilltap-server', pin: PIN, file: SOURCE },
  url: { chatId: 'c1', messageId: 'm1', out: mod.retryUncensoredTurnUrl('c1', 'm1') },
  refusal: REFUSALS.map((r) => ({
    ...r,
    out: mod.describeRetryRefusal(r.absent ? undefined : r.error),
  })),
  lantern: SENDERS.flatMap((systemSender) =>
    KINDS.map((systemKind) => ({
      systemSender,
      systemKind,
      out: mod.isLanternBackgroundRefusal({ systemSender, systemKind }),
    })),
  ),
};

process.stdout.write(JSON.stringify(out, null, 2) + '\n');
