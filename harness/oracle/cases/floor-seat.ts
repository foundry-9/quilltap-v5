/**
 * Oracle case: `resolveFloorSeatId` — which seat the "your turn" banner speaks
 * for, and whose turn its Skip passes (v4 `2075242f9`, bug 146).
 *
 * Drives the REAL exported function from v4's lib/chat/turn-manager (the
 * re-export in `index.ts` is the surface v4's own consumer imports).
 *
 * The corpus carries v4's own seven `floor-seat.test.ts` cases PLUS the shapes
 * that suite does not ask — the three prose-invisible ones especially: the
 * fallback is returned VERBATIM and un-validated (a seat not in the room, or an
 * LLM seat, comes back as given — the caller gates afterwards); an EMPTY-string
 * `nextSpeakerId` is falsy in JS and falls straight through; and `find` takes
 * the FIRST id match when ids repeat.
 *
 * ⚠ PIN REQUIRED at the TARGET `2075242f9` — `resolveFloorSeatId` does not exist
 * at the `ffb6b3119` baseline, so a baseline-pinned run fails to import and that
 * failure IS the pin verification.
 *
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   rm -f /tmp/oracle-floor-seat.ndjson
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5W/harness/oracle/cases/floor-seat.ts \
 *     > /tmp/oracle-floor-seat.ndjson
 */

import * as turnManager from '@/lib/chat/turn-manager';
import { resolveFloorSeatId } from '@/lib/chat/turn-manager/utils';
import type { ChatParticipantBase } from '@/lib/schemas/types';

/**
 * ⚠ The import is the DEFINING module, not the `@/lib/chat/turn-manager`
 * barrel v4's own consumer uses — and that is a tsx resolution fact, not a
 * choice. An entry script living in the v5 repo with cwd inside the pinned v4
 * worktree gets the barrel transpiled as CJS: `import * as tm` yields
 * `{ default, 'module.exports' }` and a NAMED import off it fails outright
 * (`does not provide an export named 'resolveFloorSeatId'`), while the deep
 * path resolves as ESM and works. The committed `turn-pause-filters.ts` recipe
 * imports the deep path for the same reason.
 *
 * So the barrel is pinned SEPARATELY, by the `reexport` row below: it reaches
 * the barrel through the CJS-interop `default` and asserts the function object
 * is IDENTICAL to the one this corpus drives. That is v4 `2075242f9`'s
 * `index.ts:83` hunk, measured rather than assumed.
 */
type WirePart = { id: string; controlledBy: string; status: string };
const asParts = (ps: WirePart[]) => ps as unknown as ChatParticipantBase[];

/** How the call spelled an absent-ish argument — JS tells `null` from `undefined`. */
type Form = 'value' | 'null' | 'undefined';

type Row =
  | {
      /** v4 `index.ts:83` — the barrel re-exports the SAME function object. */
      kind: 'reexport';
      id: string;
      out: boolean;
    }
  | {
  kind: 'floor';
  id: string;
  nextSpeakerId: string | null;
  nextSpeakerForm: Form;
  participants: WirePart[];
  impersonating: string[] | null;
  impersonatingForm: Form;
  speakingSeatId: string | null;
  speakingSeatForm: Form;
  out: string | null;
    };

const rows: Row[] = [];

const p = (id: string, controlledBy: string, status = 'active'): WirePart => ({
  id,
  controlledBy,
  status,
});

/** The shape of the chat that produced bug 146: two seats the human drives. */
const charlie = p('charlie', 'user');
const helene = p('helene', 'user');
const wahno = p('wahno', 'llm');
const room = [wahno, charlie, helene];

const lorian = p('lorian', 'llm');
const withLorian = [...room, lorian];
const departed = p('departed', 'user', 'removed');
const silentUser = p('silent-user', 'user', 'silent');
const absentUser = p('absent-user', 'user', 'absent');

/**
 * Emit one row. `next`/`impersonating`/`speaking` are given as
 * `[form, value]` so the call can spell `undefined` (the argument omitted or
 * passed as `undefined`) distinctly from `null`.
 */
function emit(
  id: string,
  next: [Form, string | null],
  participants: WirePart[],
  imp: [Form, string[] | null],
  speaking: [Form, string | null],
): void {
  const nextArg = next[0] === 'undefined' ? undefined : next[1];
  const impArg = imp[0] === 'undefined' ? undefined : imp[1];
  const speakingArg = speaking[0] === 'undefined' ? undefined : speaking[1];
  rows.push({
    kind: 'floor',
    id,
    nextSpeakerId: next[1],
    nextSpeakerForm: next[0],
    participants,
    impersonating: imp[1],
    impersonatingForm: imp[0],
    speakingSeatId: speaking[1],
    speakingSeatForm: speaking[0],
    out: resolveFloorSeatId(nextArg, asParts(participants), impArg, speakingArg),
  });
}

const V = (s: string): [Form, string] => ['value', s];
const NUL: [Form, null] = ['null', null];
const UNDEF: [Form, null] = ['undefined', null];
const EMPTY_IMP: [Form, string[]] = ['value', []];
const impOf = (...ids: string[]): [Form, string[]] => ['value', ids];

// ---------------------------------------------------------------------------
// v4's own seven cases, `__tests__/unit/lib/chat/turn-manager/floor-seat.test.ts`
// ---------------------------------------------------------------------------

// (1) prefers the seat the rotation landed on over the composer's seat.
emit('v4-1-rotation-wins', V(charlie.id), room, EMPTY_IMP, V(helene.id));
// (2) keeps the composer's seat when the floor belongs to an LLM (bug 123).
emit('v4-2-llm-floor-keeps-composer', V(wahno.id), room, EMPTY_IMP, V(helene.id));
// (3) keeps the composer's seat when there is no selection yet.
emit('v4-3a-null-floor', NUL, room, EMPTY_IMP, V(helene.id));
emit('v4-3b-undefined-floor', UNDEF, room, EMPTY_IMP, V(charlie.id));
// (4) agrees with the composer when both name the same seat.
emit('v4-4-agreement', V(helene.id), room, EMPTY_IMP, V(helene.id));
// (5) honours the impersonation overlay, not the bare controlledBy column.
emit('v4-5a-overlay-wins', V(lorian.id), withLorian, impOf('lorian'), V(charlie.id));
emit('v4-5b-no-overlay-falls-back', V(lorian.id), withLorian, EMPTY_IMP, V(charlie.id));
// (6) falls back when the rotation names a seat that has left the room.
emit('v4-6-departed-floor', V(departed.id), [...room, departed], EMPTY_IMP, V(helene.id));
// (7) null when neither the floor nor the composer names a seat.
emit('v4-7a-llm-floor-no-composer', V(wahno.id), room, EMPTY_IMP, NUL);
emit('v4-7b-nothing-at-all', NUL, room, EMPTY_IMP, NUL);

// ---------------------------------------------------------------------------
// Shapes v4's suite does not ask
// ---------------------------------------------------------------------------

// JS truthiness: an empty-string floor id is falsy → straight to the fallback,
// even though a seat with that id is in the room and user-driven.
const blankIdSeat = p('', 'user');
emit('empty-string-floor', V(''), room, EMPTY_IMP, V(helene.id));
emit('empty-string-floor-matching-seat', V(''), [...room, blankIdSeat], EMPTY_IMP, V(helene.id));
emit('empty-string-floor-null-composer', V(''), room, EMPTY_IMP, NUL);

// Presence is active OR silent: a SILENT user seat on the floor still wins.
emit('silent-user-floor-wins', V(silentUser.id), [...room, silentUser], EMPTY_IMP, V(helene.id));
// …while an ABSENT one is not present, so the composer's seat is kept.
emit('absent-user-floor-falls-back', V(absentUser.id), [...room, absentUser], EMPTY_IMP, V(helene.id));
// A silent seat on the floor with no composer at all.
emit('silent-user-floor-no-composer', V(silentUser.id), [...room, silentUser], EMPTY_IMP, NUL);
emit('absent-user-floor-no-composer', V(absentUser.id), [...room, absentUser], EMPTY_IMP, NUL);

// The overlay naming a seat that is ALSO `controlledBy: 'user'` — belt and
// braces, and the column alone already answers.
emit('overlay-names-an-owner-seat', V(charlie.id), room, impOf('charlie'), V(helene.id));
emit('overlay-names-several', V(lorian.id), withLorian, impOf('wahno', 'lorian'), V(helene.id));
emit('overlay-names-someone-else', V(lorian.id), withLorian, impOf('wahno'), V(helene.id));

// The three spellings of "no overlay": `[]`, `null`, omitted.
emit('imp-empty-array-llm-floor', V(lorian.id), withLorian, EMPTY_IMP, V(helene.id));
emit('imp-null-llm-floor', V(lorian.id), withLorian, NUL, V(helene.id));
emit('imp-undefined-llm-floor', V(lorian.id), withLorian, UNDEF, V(helene.id));
emit('imp-null-user-floor', V(charlie.id), room, NUL, V(helene.id));
emit('imp-undefined-user-floor', V(charlie.id), room, UNDEF, V(helene.id));

// The fallback is returned VERBATIM and un-validated — the caller gates it.
emit('fallback-not-in-room', V(wahno.id), room, EMPTY_IMP, V('ghost-seat'));
emit('fallback-is-an-llm-seat', V(wahno.id), room, EMPTY_IMP, V(wahno.id));
emit('fallback-is-a-removed-seat', V(wahno.id), [...room, departed], EMPTY_IMP, V(departed.id));
emit('fallback-empty-string', V(wahno.id), room, EMPTY_IMP, V(''));
emit('fallback-undefined', V(wahno.id), room, EMPTY_IMP, UNDEF);
emit('no-floor-fallback-not-in-room', NUL, room, EMPTY_IMP, V('ghost-seat'));
emit('no-floor-fallback-undefined', NUL, room, EMPTY_IMP, UNDEF);

// `find` takes the FIRST id match when ids repeat.
const dupLlmFirst = [p('dup', 'llm'), p('dup', 'user'), ...room];
const dupUserFirst = [p('dup', 'user'), p('dup', 'llm'), ...room];
const dupAbsentFirst = [p('dup', 'user', 'absent'), p('dup', 'user'), ...room];
emit('duplicate-ids-llm-first', V('dup'), dupLlmFirst, EMPTY_IMP, V(helene.id));
emit('duplicate-ids-user-first', V('dup'), dupUserFirst, EMPTY_IMP, V(helene.id));
emit('duplicate-ids-absent-first', V('dup'), dupAbsentFirst, EMPTY_IMP, V(helene.id));

// An empty room: nothing can be on the floor, so the fallback is all there is.
emit('empty-room-with-composer', V(charlie.id), [], EMPTY_IMP, V(helene.id));
emit('empty-room-no-composer', V(charlie.id), [], EMPTY_IMP, NUL);
emit('empty-room-null-floor', NUL, [], EMPTY_IMP, V(helene.id));

// A floor id naming nobody in the room.
emit('unknown-floor-id', V('nobody'), room, EMPTY_IMP, V(helene.id));
emit('unknown-floor-id-no-composer', V('nobody'), room, EMPTY_IMP, NUL);
emit('unknown-floor-id-overlaid', V('nobody'), room, impOf('nobody'), V(helene.id));

// An all-LLM room: no seat can ever be on the floor without the overlay.
const allLlm = [p('l1', 'llm'), p('l2', 'llm')];
emit('all-llm-floor', V('l1'), allLlm, EMPTY_IMP, NUL);
emit('all-llm-floor-overlaid', V('l1'), allLlm, impOf('l1'), NUL);
emit('all-llm-floor-with-composer', V('l1'), allLlm, EMPTY_IMP, V('l2'));

// Removed and absent seats the overlay names — presence is checked FIRST, so
// the overlay cannot resurrect a seat that has left.
const departedImpersonated = p('gone-imp', 'llm', 'removed');
emit(
  'overlay-cannot-resurrect-removed',
  V(departedImpersonated.id),
  [...room, departedImpersonated],
  impOf(departedImpersonated.id),
  V(helene.id),
);
const absentImpersonated = p('away-imp', 'llm', 'absent');
emit(
  'overlay-cannot-resurrect-absent',
  V(absentImpersonated.id),
  [...room, absentImpersonated],
  impOf(absentImpersonated.id),
  V(helene.id),
);

// The composer and the floor naming the same LLM seat — the floor conjunct
// fails, and the verbatim fallback hands back that same LLM seat.
emit('floor-and-composer-agree-on-an-llm', V(wahno.id), room, EMPTY_IMP, V(wahno.id));

// Both seats the human drives, each on the floor in turn.
emit('charlie-floor-helene-composer', V(charlie.id), room, EMPTY_IMP, V(helene.id));
emit('helene-floor-charlie-composer', V(helene.id), room, EMPTY_IMP, V(charlie.id));
emit('charlie-floor-charlie-composer', V(charlie.id), room, EMPTY_IMP, V(charlie.id));
emit('helene-floor-no-composer', V(helene.id), room, EMPTY_IMP, NUL);
emit('charlie-floor-undefined-composer', V(charlie.id), room, EMPTY_IMP, UNDEF);

// ---------------------------------------------------------------------------
// v4 `index.ts:83` — the utils re-export block gained `resolveFloorSeatId`.
// Reached through the CJS-interop `default` (see the header): under a real ESM
// resolution the namespace itself carries the keys, so both spellings are tried.
// ---------------------------------------------------------------------------
const barrel = ((turnManager as unknown as { default?: Record<string, unknown> }).default ??
  (turnManager as unknown as Record<string, unknown>)) as Record<string, unknown>;
rows.push({
  kind: 'reexport',
  id: 'index-barrel-exports-the-same-function',
  out: barrel['resolveFloorSeatId'] === resolveFloorSeatId,
});

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
