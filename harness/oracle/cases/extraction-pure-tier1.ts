/**
 * Tier-1 ORACLE for the pure leaves of v4's ONE memory-extraction pipeline
 * (`lib/memory/extraction/{window,parse}.ts` + `prompts/{shared,self,other,
 * episode}.ts`, `ac6348c5f` + bug 245 `64743ee13`; work order P4.D296 Tier 1
 * #1–#3). Rewrites the IMPORT-DEAD `memory-tasks-tier1.test.ts` (its subject,
 * `cheap-llm-tasks/memory-tasks.ts`, was deleted by Phase D); the Rust family
 * keeps its name, `memory_tasks_equivalence` (the order's L-20).
 *
 * Drives v4's REAL `renderClockStamp` / `renderClockBlock`,
 * `buildSelfTurnMessages`, `buildOtherTurnMessages`, `buildOtherFoldMessages`,
 * `buildEpisodeFoldMessages`, `renderTurnContext`, `renderStretch`, `passCap`,
 * `parseSelfDrafts` / `parseOtherDraftsBySubject` / `parseEpisodeDrafts`,
 * `firstValidIso`, `isFoldEligibleMessage`, `messagesPastCursor`,
 * `windowPastCursor`, `sliceFoldWindow`, `foldPassRange`, `stretchWindow`,
 * `turnWindow` over the committed corpus
 * (`harness/oracle/fixtures/extraction-pure-tier1.json`), one NDJSON row per
 * op: `{ section, name, out }`. Nothing is mocked; the builders are pure at
 * the pin.
 *
 * A tsx case, not jest: jest's config forces `TZ=UTC`, and the clock rows
 * whose `timeZone` is ABSENT read v4's `getMemoryTimeZone()` fallback
 * (`QUILLTAP_TIMEZONE`, set per row from `envZone`) — memory note
 * `oracle-second-zone-needs-tsx`.
 *
 * Run from the v4 checkout (or a pin) under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   $N/npx tsx <v5>/harness/oracle/cases/extraction-pure-tier1.ts > /tmp/oracle-extraction-pure.ndjson
 */

import * as fs from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

import { renderClockBlock, renderClockStamp } from '@/lib/memory/extraction/prompts/shared';
import { buildSelfTurnMessages } from '@/lib/memory/extraction/prompts/self';
import { buildOtherFoldMessages, buildOtherTurnMessages } from '@/lib/memory/extraction/prompts/other';
import { buildEpisodeFoldMessages } from '@/lib/memory/extraction/prompts/episode';
import {
  passCap,
  parseEpisodeDrafts,
  parseOtherDraftsBySubject,
  parseSelfDrafts,
} from '@/lib/memory/extraction/parse';
import {
  firstValidIso,
  foldPassRange,
  isFoldEligibleMessage,
  messagesPastCursor,
  renderStretch,
  renderTurnContext,
  sliceFoldWindow,
  stretchWindow,
  turnWindow,
  windowPastCursor,
} from '@/lib/memory/extraction/window';
import { resetMemoryTimeZoneCacheForTests } from '@/lib/memory/time/calendar';

type Any = any; // eslint-disable-line @typescript-eslint/no-explicit-any

const here = dirname(fileURLToPath(import.meta.url));
const corpus = JSON.parse(
  fs.readFileSync(join(here, '..', 'fixtures', 'extraction-pure-tier1.json'), 'utf8'),
) as Any;

const lines: string[] = [];
const emit = (section: string, name: string, out: unknown) =>
  lines.push(JSON.stringify({ section, name, out: out === undefined ? null : out }));

const ids = (ms: Any[]) => ms.map((m) => m.id);
const messages = (ms: Any[] | null) => (ms === null ? null : ms.map((m) => ({ role: m.role, content: m.content })));

resetMemoryTimeZoneCacheForTests();
const savedEnv = process.env.QUILLTAP_TIMEZONE;
for (const c of corpus.clocks) {
  if (c.envZone) process.env.QUILLTAP_TIMEZONE = c.envZone;
  else delete process.env.QUILLTAP_TIMEZONE;
  emit('clocks', c.name, { stamp: renderClockStamp(c.clock), block: renderClockBlock(c.clock) });
}
if (savedEnv === undefined) delete process.env.QUILLTAP_TIMEZONE;
else process.env.QUILLTAP_TIMEZONE = savedEnv;
emit('clocks', 'no_clock', { stamp: null, block: renderClockBlock(undefined) });

for (const p of corpus.prompts) {
  let out: Any;
  if (p.op === 'self') {
    out = messages(
      buildSelfTurnMessages({
        transcript: corpus.transcripts[p.transcript],
        holderId: p.holderId,
        canonBlock: p.canonBlock,
        cap: p.cap,
        inAutonomousRoom: p.inAutonomousRoom,
        orienting: p.orienting,
        clock: p.clock,
      }),
    );
  } else if (p.op === 'other-turn') {
    out = messages(
      buildOtherTurnMessages({
        transcript: corpus.transcripts[p.transcript],
        observer: p.observer,
        subjects: p.subjects,
        cap: p.cap,
        inAutonomousRoom: p.inAutonomousRoom,
        orienting: p.orienting,
        clock: p.clock,
      }),
    );
  } else if (p.op === 'other-fold') {
    out = messages(
      buildOtherFoldMessages({
        stretch: p.stretch,
        observer: p.observer,
        subjects: p.subjects,
        cap: p.cap,
        inAutonomousRoom: p.inAutonomousRoom,
        orienting: p.orienting,
        clock: p.clock,
      }),
    );
  } else {
    out = messages(buildEpisodeFoldMessages({ stretch: p.stretch, clock: p.clock }));
  }
  emit('prompts', p.name, out);
}

for (const r of corpus.passCap) {
  emit('passCap', `base_${r.base}_max_${r.maxTokens}`, passCap(r.base, r.maxTokens ?? undefined));
}

for (const p of corpus.parse) {
  let out: Any;
  if (p.op === 'self') out = parseSelfDrafts(p.raw, p.cap);
  else if (p.op === 'episode') out = parseEpisodeDrafts(p.raw, p.cap);
  else out = [...parseOtherDraftsBySubject(p.raw, p.subjectIds, p.cap).entries()];
  emit('parse', p.name, out);
}

const noNames = new Map<string, string>();
for (const w of corpus.window) {
  let out: Any;
  switch (w.op) {
    case 'firstValidIso':
      out = firstValidIso(...w.isos);
      break;
    case 'eligible':
      out = w.messages.map((m: Any) => isFoldEligibleMessage(m));
      break;
    case 'pastCursor':
      out = ids(messagesPastCursor(w.messages, w.cursor));
      break;
    case 'windowPastCursor': {
      const r = windowPastCursor(w.window, w.all, w.cursor);
      out = { window: ids(r.window), applied: r.applied };
      break;
    }
    case 'slice':
      out = sliceFoldWindow(w.eligible, w.max, w.final).map((s) => ({
        messages: ids(s.messages),
        coverThroughMessageId: s.coverThroughMessageId,
      }));
      break;
    case 'range': {
      const r = foldPassRange(w.window, w.all, w.cursor);
      out = { messages: ids(r.messages), applied: r.applied, gap: r.gap };
      break;
    }
    case 'stretchWindow': {
      const s = stretchWindow(w.messages, noNames, w.cover, w.nowMs);
      out = {
        messages: ids(s.messages),
        rendered: s.rendered,
        nowIso: s.nowIso,
        startIso: s.startIso,
        coverThroughMessageId: s.coverThroughMessageId,
      };
      break;
    }
    case 'turnWindow':
      out = { nowIso: (turnWindow(corpus.transcripts[w.transcript], w.nowMs) as Any).nowIso };
      break;
    default:
      throw new Error(`unknown window op ${w.op}`);
  }
  emit('window', w.name, out);
}

for (const r of corpus.render) {
  emit('render', r.name, r.op === 'stretch' ? renderStretch(r.stretch) : renderTurnContext(corpus.transcripts[r.transcript]));
}

process.stdout.write(lines.join('\n') + '\n');
