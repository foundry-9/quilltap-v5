/**
 * @jest-environment node
 *
 * P4.D169 item 5 ORACLE: the Pascal side-effect APPLIER. Drives v4's REAL
 * `lib/pascal/side-effects.ts` (`applyCustomToolEffects`) with
 * `getRepositories` and `writeGeneralState` mocked to RECORDERS, so what a run
 * would have written to each store becomes a comparand without a database.
 *
 * ## What each row records
 *
 * - `characterWrite` — the whole `metadata` object the ONE `characters.update`
 *   carried, or null when no character write happened. This is the load-bearing
 *   claim of the progress branch: a `progress.<id>.<field>` effect is a metadata
 *   write wearing a hat, so it folds into the same replace rather than issuing a
 *   write of its own.
 * - `stateWrites` — every state store touched, in v4's fixed commit order
 *   (chat → project → group → general), each with the id it was written under.
 * - `applied` — the `AppliedEffect[]` the applier returned, which is what
 *   `pascalMeta.effects` persists.
 * - `warns` — `logger.warn` calls, message and bag. The post-validation
 *   rollback is invisible in `applied` alone (a dropped write and a write that
 *   never applied look identical there), so the warn is the discriminator.
 * - `snapshotMutated` — whether the caller's `metadataSnapshot` came back
 *   changed. v4 asserts this once; recording it per row makes every row a probe.
 *
 * ## Why a mocked repository and not a fixture database
 *
 * v4's own two applier suites mock `getRepositories`, and the comparand they
 * assert on is the ARGUMENT of the write, not its effect on a table. v5 splits
 * the applier into a pure planning pass and an impure commit for exactly this
 * reason (P4.D35), so both sides here observe the same thing: the plan. The
 * commit half — one write per touched store, per-store fail-soft — is proven
 * on the Rust side by `side_effects.rs`'s own tests, and its three v4 cases are
 * deliberately absent from the corpus (see the corpus `note`).
 *
 * Run (v4 @ 25f534c0b, Node 24, from a PINNED worktree — jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-pascal-sidefx-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/pascal-side-effects.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server
 *   QT_SIDE_EFFECTS_CORPUS="$V5W/harness/oracle/fixtures/pascal-side-effects.json" \
 *   QT_ORACLE_OUT=/tmp/oracle-pascal-side-effects.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- pascal-side-effects
 */

import * as fs from 'fs';

jest.mock('@/lib/repositories/factory', () => ({ getRepositories: jest.fn() }));
jest.mock('@/lib/mount-index/general-state', () => ({ writeGeneralState: jest.fn() }));
jest.mock('@/lib/logger', () => {
  // `child` matters: modules loaded transitively by the applier build service
  // loggers off this one at import time, and a mock without it fails the whole
  // suite before a single assertion runs. (v4's own suite says the same.)
  const base = { debug: jest.fn(), info: jest.fn(), warn: jest.fn(), error: jest.fn() };
  return { logger: { ...base, child: jest.fn(() => ({ ...base, child: jest.fn() })) } };
});

import { applyCustomToolEffects } from '@/lib/pascal/side-effects';
import type { ResolvedEffect } from '@/lib/pascal/custom-tools';
import type { StateCascadeResult } from '@/lib/state/state-cascade';
import type { EffectTarget } from '@/lib/pascal/custom-tool.types';
import { getRepositories } from '@/lib/repositories/factory';
import { writeGeneralState } from '@/lib/mount-index/general-state';

const { logger } = require('@/lib/logger') as { logger: Record<string, jest.Mock> };
const mockGetRepositories = getRepositories as jest.Mock;
const mockWriteGeneralState = writeGeneralState as jest.Mock;

const OUT = process.env.QT_ORACLE_OUT;
const CORPUS = process.env.QT_SIDE_EFFECTS_CORPUS;
const rows: unknown[] = [];

/** A corpus value, tagged so NaN survives JSON. */
type CorpusValue = { n: number } | { s: string } | { b: boolean } | { nan: true };

function value(v: CorpusValue): number | string | boolean {
  if ('nan' in v) return Number.NaN;
  if ('n' in v) return v.n;
  if ('s' in v) return v.s;
  return v.b;
}

interface CorpusEffect {
  index: number;
  target?: EffectTarget;
  value?: CorpusValue;
  skipped?: string;
}

interface CorpusCase {
  name: string;
  chatId: string;
  toolName: string;
  characterId: string | null;
  metadataSnapshot: Record<string, unknown>;
  cascade: StateCascadeResult | null;
  effects: CorpusEffect[];
  nowMs?: number;
}

function repos() {
  return {
    chats: { update: jest.fn() },
    projects: { update: jest.fn() },
    groups: { update: jest.fn() },
    characters: { update: jest.fn() },
  };
}

describe('applyCustomToolEffects', () => {
  it('emits', async () => {
    if (!CORPUS) throw new Error('set QT_SIDE_EFFECTS_CORPUS');
    const corpus = JSON.parse(fs.readFileSync(CORPUS, 'utf8')) as {
      nowMs: number;
      cases: CorpusCase[];
    };

    for (const c of corpus.cases) {
      jest.clearAllMocks();
      const db = repos();
      mockGetRepositories.mockReturnValue(db);

      const effects: ResolvedEffect[] = c.effects.map((e) =>
        e.skipped !== undefined
          ? ({ index: e.index, skipped: e.skipped } as ResolvedEffect)
          : ({ index: e.index, target: e.target, value: value(e.value!) } as ResolvedEffect),
      );

      // The snapshot the applier is handed, and an untouched twin to compare it
      // against afterwards — v4 promises it never mutates the caller's copy.
      const snapshot = JSON.parse(JSON.stringify(c.metadataSnapshot));
      const snapshotBefore = JSON.stringify(c.metadataSnapshot);

      let applied: unknown = null;
      let threw: string | null = null;
      try {
        applied = await applyCustomToolEffects({
          chatId: c.chatId,
          toolName: c.toolName,
          effects,
          cascade: c.cascade,
          characterId: c.characterId,
          metadataSnapshot: snapshot,
          nowMs: c.nowMs ?? corpus.nowMs,
        });
      } catch (e) {
        threw = (e as Error).message;
      }

      // The state writes, in v4's fixed commit order.
      const stateWrites: Array<{ store: string; id: string | null; state: unknown }> = [];
      for (const [store, mock] of [
        ['chat', db.chats.update],
        ['project', db.projects.update],
        ['group', db.groups.update],
      ] as const) {
        for (const call of mock.mock.calls) {
          stateWrites.push({ store, id: call[0] as string, state: call[1].state });
        }
      }
      for (const call of mockWriteGeneralState.mock.calls) {
        stateWrites.push({ store: 'general', id: null, state: call[0] });
      }

      const characterCalls = db.characters.update.mock.calls;
      const characterWrite =
        characterCalls.length > 0 ? (characterCalls[0][1].metadata as unknown) : null;

      const warns = logger.warn.mock.calls.map((call) => ({
        message: call[0] as string,
        bag: (call[1] ?? null) as unknown,
      }));

      rows.push({
        name: c.name,
        applied,
        threw,
        characterWrite,
        characterWriteCount: characterCalls.length,
        characterWriteId: characterCalls.length > 0 ? (characterCalls[0][0] as string) : null,
        stateWrites,
        warns,
        snapshotMutated: JSON.stringify(snapshot) !== snapshotBefore,
      });
    }
  });
});

afterAll(() => {
  if (!OUT) throw new Error('set QT_ORACLE_OUT');
  fs.writeFileSync(OUT, rows.map((r) => JSON.stringify(r)).join('\n') + '\n');
});
