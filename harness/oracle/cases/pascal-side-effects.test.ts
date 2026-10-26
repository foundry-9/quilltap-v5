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
// P4.D306 (v4 `f0db8cbb8`): the wardrobe pass is its own module; here it is a
// RECORDER returning the row's SCRIPTED `IndexedAppliedEffect[]`, so the split
// (`applyWriteEffects` + the pass) and the stable index merge are what is
// compared — the applier itself is `pascal_wardrobe_effects_equivalence`'s.
jest.mock('@/lib/pascal/wardrobe-effects', () => ({ applyWardrobeEffects: jest.fn(async () => []) }));
// P4.D329 (v4 `e1872c94f`): the inventory pass, likewise a RECORDER returning
// the row's SCRIPTED `IndexedAppliedEffect[]` — the third list and the
// three-way merge are what is compared; the applier itself is
// `pascal_inventory_effects_equivalence`'s. WITHOUT this mock the pre-existing
// rows would reach the real applier through the mocked `getRepositories`.
jest.mock('@/lib/pascal/inventory-effects', () => ({ applyInventoryEffects: jest.fn(async () => []) }));
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
import { applyWardrobeEffects } from '@/lib/pascal/wardrobe-effects';
import { applyInventoryEffects } from '@/lib/pascal/inventory-effects';

const { logger } = require('@/lib/logger') as { logger: Record<string, jest.Mock> };
const mockGetRepositories = getRepositories as jest.Mock;
const mockWriteGeneralState = writeGeneralState as jest.Mock;
const mockApplyWardrobeEffects = applyWardrobeEffects as jest.Mock;
const mockApplyInventoryEffects = applyInventoryEffects as jest.Mock;

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
  /** P4.D306: a resolved wardrobe gesture (`ResolvedWardrobeGesture`). */
  wardrobe?: Record<string, unknown>;
  /** P4.D329: a resolved inventory gesture (`ResolvedInventoryGesture`). */
  inventory?: Record<string, unknown>;
  /** P4.D331: a resolved log line (`{ index, log }`). */
  log?: string;
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
  /** P4.D306: v4's REQUIRED `userId` (defaults to `user-1`, v4's suites' value). */
  userId?: string;
  /** P4.D306: what the mocked wardrobe pass returns (`IndexedAppliedEffect[]`). */
  wardrobeReturns?: unknown[];
  /** P4.D306: the turn's announcement set, when the run is inside a model turn. */
  pendingWardrobeAnnouncements?: string[];
  /** P4.D329: what the mocked inventory pass returns (`IndexedAppliedEffect[]`). */
  inventoryReturns?: unknown[];
  /** P4.D329: the run's context rode the call (a sentinel stands in for it). */
  inventory?: boolean;
  /** P4.D329: the turn's pending Inventory map, when inside a model turn. */
  pendingInventoryAnnouncements?: boolean;
  /**
   * P4.D331: the log pass's repository. `logEntries.create` is a RECORDER that
   * answers `{ id: "log-<n>", …data, createdAt, updatedAt }` for its n-th call
   * — or throws `logCreateError` (every call; v4's degraded / absent guard
   * sentence, or a refused create) when set.
   */
  logCreateError?: string;
}

function repos(logCreateError?: string) {
  let n = 0;
  return {
    chats: { update: jest.fn() },
    projects: { update: jest.fn() },
    groups: { update: jest.fn() },
    characters: { update: jest.fn() },
    // P4.D331: the log pass's write door reaches this through the REAL
    // `writeLogEntry` (its trim / clip run; only the repository is mocked).
    logEntries: {
      create: jest.fn(async (data: Record<string, unknown>) => {
        if (logCreateError !== undefined) throw new Error(logCreateError);
        n += 1;
        return { id: `log-${n}`, ...data, createdAt: '2026-10-20T00:00:00.000Z', updatedAt: '2026-10-20T00:00:00.000Z' };
      }),
    },
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
      const db = repos(c.logCreateError);
      mockGetRepositories.mockReturnValue(db);

      mockApplyWardrobeEffects.mockResolvedValue(c.wardrobeReturns ?? []);
      mockApplyInventoryEffects.mockResolvedValue(c.inventoryReturns ?? []);

      const effects: ResolvedEffect[] = c.effects.map((e) =>
        e.skipped !== undefined
          ? ({ index: e.index, skipped: e.skipped } as ResolvedEffect)
          : e.wardrobe !== undefined
            ? ({ index: e.index, wardrobe: e.wardrobe } as unknown as ResolvedEffect)
            : e.inventory !== undefined
              ? ({ index: e.index, inventory: e.inventory } as unknown as ResolvedEffect)
              : e.log !== undefined
                ? ({ index: e.index, log: e.log } as unknown as ResolvedEffect)
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
          userId: c.userId ?? 'user-1',
          chatId: c.chatId,
          toolName: c.toolName,
          effects,
          cascade: c.cascade,
          characterId: c.characterId,
          metadataSnapshot: snapshot,
          nowMs: c.nowMs ?? corpus.nowMs,
          ...(c.pendingWardrobeAnnouncements
            ? { pendingWardrobeAnnouncements: new Set(c.pendingWardrobeAnnouncements) }
            : {}),
          // P4.D329: the run's context and the turn's pending map, by sentinel.
          ...(c.inventory ? { inventory: { sentinel: 'the run bag' } as never } : {}),
          ...(c.pendingInventoryAnnouncements ? { pendingInventoryAnnouncements: new Map() } : {}),
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
        // P4.D331: the log pass's create calls, in order (the data object).
        logCreates: db.logEntries.create.mock.calls.map(([data]) => data),
        // P4.D306: the wardrobe pass's EXACT argument objects (the set as its
        // sorted members, key kept only when v4 passed one).
        wardrobeCalls: mockApplyWardrobeEffects.mock.calls.map(([arg]) => {
          const { pendingWardrobeAnnouncements: set, ...rest } = arg as Record<string, unknown>;
          return {
            ...rest,
            ...(set ? { pendingWardrobeAnnouncements: [...(set as Set<string>)].sort() } : {}),
          };
        }),
        // P4.D329: the inventory pass's EXACT argument objects — `inventory`
        // and the pending map reported by PRESENCE (each stands in a sentinel).
        inventoryCalls: mockApplyInventoryEffects.mock.calls.map(([arg]) => {
          const { inventory, pendingInventoryAnnouncements: pending, ...rest } = arg as Record<string, unknown>;
          return {
            ...rest,
            ...(inventory !== undefined ? { inventory: inventory === null ? null : 'the run bag' } : {}),
            ...(pending ? { pendingInventoryAnnouncements: true } : {}),
          };
        }),
      });
    }
  });
});

afterAll(() => {
  if (!OUT) throw new Error('set QT_ORACLE_OUT');
  fs.writeFileSync(OUT, rows.map((r) => JSON.stringify(r)).join('\n') + '\n');
});
