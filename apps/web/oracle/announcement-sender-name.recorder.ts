/**
 * The v4-side recorder behind `src/app/chat/system-message-labels.oracle.spec.ts`
 * (P4.D320).
 *
 * `a05c6f755` lets a Pascal outcome be ANNOUNCED by the rolling character
 * (`announceAsCharacter`): the posted row carries `customAnnouncer: { kind:
 * 'character', characterId }` BESIDE `systemSender: 'pascal'`, and v4's
 * `getAnnouncementSenderName` (`app/salon/[id]/components/
 * system-message-labels.ts`) puts that announcer's name on the Staff bar and
 * the collapsed chip — or falls back to the Staff member rather than
 * inventing one. This drives v4's REAL function, including its Staff
 * fallback over every `systemSender` value v5's `SystemSender` type lists, so
 * a staff-name drift trips here too.
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/app/...`.
 * The bracketed path must be quoted in any shell that touches it (memory note
 * `v4-path-quoting-traps`); the import below is a string, so tsx is unbothered.
 *
 * Each row: `{ id, message, withNames, names, out }` — `withNames: false`
 * means the second argument was not passed at all.
 *
 * Run it from a pinned v4 worktree at `e8e17b2d9` (Node 24):
 *
 * ```bash
 * PIN=${PIN:?a detached v4 worktree at e8e17b2d9}
 * V5W=${V5W:-$HOME/source/quilltap-v5}
 * cp "$V5W/apps/web/oracle/announcement-sender-name.recorder.ts" "$PIN/"
 * OUT="$V5W/apps/web/src/testing/fixtures/announcement-sender-name.oracle.ndjson"
 * rm -f "$OUT"
 * (cd "$PIN" && PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH \
 *   npx tsx announcement-sender-name.recorder.ts > "$OUT")
 * wc -l "$OUT"                 # expect 32
 * grep -c '"out":"Tester"' "$OUT"   # > 0
 * ```
 *
 * Expect 32 lines (20 announcer rows + 12 Staff-fallback rows).
 */

import { getAnnouncementSenderName } from '@/app/salon/[id]/components/system-message-labels';

type Msg = Parameters<typeof getAnnouncementSenderName>[0];

const NAMES = { 'char-1': 'Tester', 'char-pad': '  Padded Name  ', 'char-blank': '   ', 'char-empty': '' };

const ROWS: Array<[string, Msg, boolean, Record<string, string> | undefined]> = [
  // v4's three tests
  ['v4-nobody-announces', { systemSender: 'pascal', customAnnouncer: null }, true, NAMES],
  ['v4-character-announces', { systemSender: 'pascal', customAnnouncer: { kind: 'character', characterId: 'char-1' } }, true, NAMES],
  ['v4-announcer-unknown', { systemSender: 'pascal', customAnnouncer: { kind: 'character', characterId: 'gone' } }, true, NAMES],
  // the custom arm
  ['custom-named', { systemSender: 'host', customAnnouncer: { kind: 'custom', displayName: 'Town Crier' } }, true, NAMES],
  ['custom-padded', { systemSender: 'host', customAnnouncer: { kind: 'custom', displayName: '  Town Crier ' } }, true, NAMES],
  ['custom-blank', { systemSender: 'host', customAnnouncer: { kind: 'custom', displayName: '' } }, true, NAMES],
  ['custom-whitespace', { systemSender: 'host', customAnnouncer: { kind: 'custom', displayName: '   ' } }, true, NAMES],
  ['custom-null-name', { systemSender: 'host', customAnnouncer: { kind: 'custom', displayName: null } }, true, NAMES],
  ['custom-absent-name', { systemSender: 'pascal', customAnnouncer: { kind: 'custom' } }, true, NAMES],
  ['custom-with-character-id', { systemSender: 'pascal', customAnnouncer: { kind: 'custom', characterId: 'char-1', displayName: 'Crier' } }, true, NAMES],
  ['custom-no-names-arg', { systemSender: 'pascal', customAnnouncer: { kind: 'custom', displayName: 'Crier' } }, false, undefined],
  // the character arm
  ['character-null-id', { systemSender: 'pascal', customAnnouncer: { kind: 'character', characterId: null } }, true, NAMES],
  ['character-empty-id', { systemSender: 'pascal', customAnnouncer: { kind: 'character', characterId: '' } }, true, NAMES],
  ['character-absent-id', { systemSender: 'pascal', customAnnouncer: { kind: 'character' } }, true, NAMES],
  ['character-padded-name', { systemSender: 'pascal', customAnnouncer: { kind: 'character', characterId: 'char-pad' } }, true, NAMES],
  ['character-whitespace-name', { systemSender: 'pascal', customAnnouncer: { kind: 'character', characterId: 'char-blank' } }, true, NAMES],
  ['character-empty-name', { systemSender: 'pascal', customAnnouncer: { kind: 'character', characterId: 'char-empty' } }, true, NAMES],
  ['character-with-display-name-ignored', { systemSender: 'pascal', customAnnouncer: { kind: 'character', characterId: 'gone', displayName: 'Crier' } }, true, NAMES],
  ['character-names-absent', { systemSender: 'pascal', customAnnouncer: { kind: 'character', characterId: 'char-1' } }, false, undefined],
  ['character-names-empty', { systemSender: 'pascal', customAnnouncer: { kind: 'character', characterId: 'char-1' } }, true, {}],
];

// The Staff fallback's own table: every systemSender v5 knows, plus null.
const SENDERS = [
  'lantern', 'aurora', 'librarian', 'concierge', 'prospero', 'host',
  'commonplaceBook', 'ariel', 'carina', 'suparna', 'pascal', null,
] as const;
for (const sender of SENDERS) {
  ROWS.push([`staff-${sender ?? 'null'}`, { systemSender: sender, customAnnouncer: null } as Msg, true, NAMES]);
}

for (const [id, message, withNames, names] of ROWS) {
  const out = withNames ? getAnnouncementSenderName(message, names) : getAnnouncementSenderName(message);
  console.log(JSON.stringify({ id, message, withNames, names: names ?? null, out }));
}
