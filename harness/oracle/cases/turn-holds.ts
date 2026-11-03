/**
 * Oracle case (P4.D348): the turn holds — v4 `a8214b253` (bug 262),
 * `lib/services/chat-message/turn-holds.service.ts`'s two pure exports, at
 * `236c04e35`.
 *
 * Drives v4's REAL `resolveTurnHold` and `qualifiesForFairRotationCheck` over
 * full input grids (computed on both sides, never transcribed):
 *   - `resolve`: `chat.isPaused` × `chat.turnMode` × `continueMode` × whisper
 *     targets × `neverPauseForUser` (absent arms omit the key);
 *   - `qualify`: `continueMode` × `respondingParticipantId` × targets ×
 *     `nudge` × `content` × `hold` × five participant rosters (the
 *     `isMultiCharacterChat` conjunct).
 *
 * Run from inside the server checkout (a pinned worktree under PIN REQUIRED):
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   npx tsx $V5W/harness/oracle/cases/turn-holds.ts \
 *     > /tmp/oracle-turn-holds.ndjson
 */

import {
  qualifiesForFairRotationCheck,
  resolveTurnHold,
} from '@/lib/services/chat-message/turn-holds.service';

type Opt<T> = { tag: string; value: T | undefined };
const opt = <T>(tag: string, value: T | undefined): Opt<T> => ({ tag, value });

function withKeys(pairs: Array<[string, unknown]>): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [k, v] of pairs) if (v !== undefined) out[k] = v;
  return out;
}

const write = (row: unknown) => process.stdout.write(JSON.stringify(row) + '\n');

// ---- resolveTurnHold -------------------------------------------------------
const PAUSED = [opt('absent', undefined), opt('true', true), opt('false', false)];
const TURN_MODE = [
  opt<string | null>('absent', undefined),
  opt<string | null>('null', null),
  opt<string | null>('rotation', 'rotation'),
  opt<string | null>('storyteller', 'storyteller'),
];
const CONTINUE = [opt('absent', undefined), opt('true', true), opt('false', false)];
const TARGETS = [
  opt<string[] | null>('absent', undefined),
  opt<string[] | null>('null', null),
  opt<string[] | null>('empty', []),
  opt<string[] | null>('one', ['p-1']),
];
const NEVER = [opt('absent', undefined), opt('true', true), opt('false', false)];

for (const p of PAUSED)
  for (const m of TURN_MODE)
    for (const c of CONTINUE)
      for (const t of TARGETS)
        for (const n of NEVER) {
          const chat = withKeys([['isPaused', p.value], ['turnMode', m.value]]);
          const options = withKeys([
            ['continueMode', c.value],
            ['targetParticipantIds', t.value],
            ['neverPauseForUser', n.value],
          ]);
          write({
            kind: 'resolve',
            id: `resolve|paused=${p.tag}|mode=${m.tag}|continue=${c.tag}|targets=${t.tag}|never=${n.tag}`,
            chat,
            options,
            out: resolveTurnHold(chat as never, options as never),
          });
        }

// ---- qualifiesForFairRotationCheck -----------------------------------------
const seat = (id: string, controlledBy: string, extra: Record<string, unknown> = {}) => ({
  id,
  type: 'CHARACTER',
  characterId: `char-${id}`,
  controlledBy,
  status: 'active',
  ...extra,
});
const ROSTERS: Array<{ tag: string; participants: unknown[] }> = [
  { tag: 'solo-user', participants: [seat('u1', 'user')] },
  { tag: 'one-llm', participants: [seat('u1', 'user'), seat('a1', 'llm')] },
  { tag: 'two-users', participants: [seat('u1', 'user'), seat('u2', 'user')] },
  { tag: 'absent-llm', participants: [seat('u1', 'user'), seat('a1', 'llm', { status: 'absent' })] },
  {
    tag: 'llm-without-character',
    participants: [seat('u1', 'user'), seat('a1', 'llm', { characterId: null })],
  },
];
const RESPONDING = [opt<string>('absent', undefined), opt<string>('empty', ''), opt<string>('set', 'p-2')];
const NUDGE = [opt('absent', undefined), opt('true', true), opt('false', false)];
const CONTENT = [opt<string>('absent', undefined), opt<string>('empty', ''), opt<string>('text', 'Hello')];
const HOLDS: Array<string | null> = [null, 'paused', 'storyteller'];
const QUALIFY_TARGETS = [
  opt<string[] | null>('absent', undefined),
  opt<string[] | null>('empty', []),
  opt<string[] | null>('one', ['p-1']),
];
const QUALIFY_CONTINUE = [opt('absent', undefined), opt('true', true)];

for (const r of ROSTERS)
  for (const c of QUALIFY_CONTINUE)
    for (const rp of RESPONDING)
      for (const t of QUALIFY_TARGETS)
        for (const n of NUDGE)
          for (const ct of CONTENT)
            for (const hold of HOLDS) {
              const options = withKeys([
                ['continueMode', c.value],
                ['respondingParticipantId', rp.value],
                ['targetParticipantIds', t.value],
                ['nudge', n.value],
                ['content', ct.value],
              ]);
              write({
                kind: 'qualify',
                id: `qualify|roster=${r.tag}|continue=${c.tag}|responding=${rp.tag}|targets=${t.tag}|nudge=${n.tag}|content=${ct.tag}|hold=${hold}`,
                participants: r.participants,
                options,
                hold,
                out: qualifiesForFairRotationCheck(
                  { participants: r.participants } as never,
                  options as never,
                  hold as never,
                ),
              });
            }

process.exit(0);
