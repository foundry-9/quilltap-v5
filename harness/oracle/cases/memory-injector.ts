/**
 * Oracle case: the scene-state + summary context formatters (chat-orchestration
 * wave 1.3; RE-AIMED by P4.D291).
 *
 * Drives the REAL pure functions from v4's lib/chat/context/scene-state.ts —
 * formatCurrentSceneState, formatSummaryForContext — over a fixed deterministic
 * corpus, printing results as NDJSON. The Rust port (`memory_injector.rs`, which
 * keeps exactly these two formatters byte-for-byte) feeds the SAME corpus and
 * asserts byte-equal output (tier-1 exact).
 *
 * v4 Phase E moved these two verbatim out of `memory-injector.ts` (deleted) into
 * `scene-state.ts`; every memory formatter the old corpus drove now lives in
 * `lib/memory/recall/render.ts` and is pinned by `cases/recall-render.ts`.
 *
 * IMPORTANT — imports the actual app code, does not reimplement it. Run from the
 * server checkout so `@/` aliases resolve:
 *
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/memory-injector.ts \
 *     > /private/tmp/.../scratchpad/oracle-memory-injector.ndjson
 *
 * DETERMINISM: the scene-state formatter calls `new Date()` internally (each
 * character's `emittedAt`). We pin the no-arg `Date` constructor to a fixed
 * instant BEFORE importing the module, so every `new Date()` returns FIXED_NOW
 * while `new Date(string)` still parses normally.
 */

// ---- Pin `new Date()` (no-arg) to a fixed instant --------------------------
const FIXED_NOW_ISO = '2026-06-27T12:00:00.000Z';
const FIXED_NOW_MS = Date.parse(FIXED_NOW_ISO);
const RealDate = Date;
// Replace the global Date so no-arg construction is deterministic. Preserve
// every other behaviour (parsing a string/number, Date.now, static methods).
class PinnedDate extends RealDate {
  constructor(...args: unknown[]) {
    if (args.length === 0) {
      super(FIXED_NOW_MS);
    } else {
      // @ts-expect-error forward all args to the real constructor
      super(...args);
    }
  }
  static now(): number {
    return FIXED_NOW_MS;
  }
}
// eslint-disable-next-line @typescript-eslint/no-explicit-any
;(globalThis as any).Date = PinnedDate;

import {
  formatCurrentSceneState,
  formatSummaryForContext,
  type SceneStateEmissionEntry,
} from '@/lib/chat/context/scene-state';
import type { SceneState } from '@/lib/schemas/chat.types';

const rows: unknown[] = [];
function emit(row: unknown) {
  rows.push(row);
}


// ===========================================================================
// formatCurrentSceneState
// ===========================================================================
type SceneCharWire = {
  characterId: string;
  characterName: string;
  action: string;
  clothing: string | null;
};
function sceneCase(
  id: string,
  scene: SceneState | null | undefined,
  time: string | null | undefined,
  liveClothing: Array<[string, string]>,
  priorEmission: Array<[string, SceneStateEmissionEntry]>,
) {
  const liveMap = new Map(liveClothing);
  const priorMap = new Map(priorEmission);
  const res = formatCurrentSceneState(scene, time, undefined, liveMap, priorMap);
  emit({
    kind: 'scene',
    id,
    scene: scene
      ? {
          location: scene.location,
          characters: (scene.characters ?? []).map((c) => ({
            characterId: c.characterId,
            characterName: c.characterName,
            action: c.action,
            clothing: c.clothing,
          })) as SceneCharWire[],
        }
      : null,
    time: time ?? null,
    nowIso: FIXED_NOW_ISO,
    liveClothing,
    priorEmission,
    out: {
      content: res.content,
      tokenCount: res.tokenCount,
      emittedByCharacter: Array.from(res.emittedByCharacter.entries()).map(([k, v]) => ({
        characterId: k,
        actionHash: v.actionHash,
        clothingHash: v.clothingHash,
        emittedAt: v.emittedAt,
      })),
    },
  });
}
function sc(
  characterId: string,
  characterName: string,
  action: string,
  clothing: string | null,
): SceneState['characters'][number] {
  return { characterId, characterName, action, clothing, appearance: null } as SceneState['characters'][number];
}
function scene(location: string, characters: SceneState['characters']): SceneState {
  return { location, characters, updatedAt: FIXED_NOW_ISO, updatedAtMessageCount: 0 } as unknown as SceneState;
}

sceneCase('scene-null', null, null, [], []);
sceneCase('scene-empty', scene('', []), null, [], []);
sceneCase(
  'scene-basic',
  scene('The Study', [sc('c1', 'Ada', 'reading a book', 'a tweed jacket')]),
  '3:00 PM',
  [],
  [],
);
sceneCase(
  'scene-no-time',
  scene('The Study', [sc('c1', 'Ada', 'reading', 'tweed')]),
  null,
  [],
  [],
);
sceneCase(
  'scene-blank-time',
  scene('The Study', [sc('c1', 'Ada', 'reading', 'tweed')]),
  '   ',
  [],
  [],
);
sceneCase(
  'scene-trim-time',
  scene('The Study', [sc('c1', 'Ada', 'reading', 'tweed')]),
  '  midnight  ',
  [],
  [],
);
sceneCase(
  'scene-unspecified',
  scene('Parlour', [sc('c1', 'Ada', '   ', null)]),
  null,
  [],
  [],
);
sceneCase(
  'scene-multi',
  scene('The Ballroom', [
    sc('c1', 'Ada', 'dancing', 'a gown'),
    sc('c2', 'Bertie', 'watching', null),
    sc('', 'NoId', 'lurking', 'a cloak'),
    sc('c4', '', 'ignored (no name)', 'invisible'),
  ]),
  'evening',
  [],
  [],
);
sceneCase(
  'scene-live-clothing',
  scene('The Study', [sc('c1', 'Ada', 'reading', 'tweed')]),
  null,
  [['c1', '  a freshly-pressed waistcoat  ']],
  [],
);
sceneCase(
  'scene-live-clothing-blank',
  scene('The Study', [sc('c1', 'Ada', 'reading', 'tweed')]),
  null,
  [['c1', '   ']],
  [],
);
// Unchanged path: prior hashes match current (action+clothing).
{
  const action = 'reading';
  const clothing = 'tweed';
  // Compute the hashes the same way the module does: sha256 hex slice(0,16).
  // We reuse the module by first emitting a full case, then feeding its hashes.
  const first = formatCurrentSceneState(
    scene('The Study', [sc('c1', 'Ada', action, clothing)]),
    null,
    undefined,
    new Map(),
    new Map(),
  );
  const prior = first.emittedByCharacter.get('c1')!;
  // Pin a distinct prior emittedAt so we can prove it's preserved.
  const priorPinned: SceneStateEmissionEntry = { ...prior, emittedAt: '2020-01-01T00:00:00.000Z' };
  sceneCase(
    'scene-unchanged',
    scene('The Study', [sc('c1', 'Ada', action, clothing)]),
    null,
    [],
    [['c1', priorPinned]],
  );
  // Changed action → full section, new emittedAt.
  sceneCase(
    'scene-changed',
    scene('The Study', [sc('c1', 'Ada', 'now writing', clothing)]),
    null,
    [],
    [['c1', priorPinned]],
  );
}
sceneCase(
  'scene-astral',
  scene('🏰 Castle', [sc('c1', 'Zöe', 'casting a 🔮 spell', 'robes of midnight')]),
  '𝟏𝟐 o’clock',
  [],
  [],
);

// ===========================================================================
// formatSummaryForContext
// ===========================================================================
function summaryCase(id: string, summary: string, maxTokens: number) {
  const out = formatSummaryForContext(summary, maxTokens, undefined as never);
  emit({ kind: 'summary', id, summary, maxTokens, out });
}

summaryCase('sum-empty', '', 1000);
summaryCase('sum-blank', '   ', 1000);
summaryCase('sum-fits', 'A short recap of the tale so far.', 1000);
summaryCase('sum-truncate', 'word '.repeat(200).trim(), 30);
summaryCase('sum-truncate-tiny', 'word '.repeat(200).trim(), 12);
summaryCase('sum-astral', '🎩 A soirée with Zöe and a café.', 1000);

// ---- emit ------------------------------------------------------------------
for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
