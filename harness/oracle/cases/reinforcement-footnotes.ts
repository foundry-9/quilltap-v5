/**
 * Tier-1 ORACLE for v4's reinforcement footnotes (P4.D289 unit 5, pin
 * `83334b9a5`) — `lib/memory/reinforcement-footnotes.ts` (`e0ac7493a`; the
 * `cap` argument from `8af764514`): `MAX_REINFORCEMENT_FOOTNOTES`,
 * `countReinforcementFootnotes`, and `appendCappedFootnotes(content, details,
 * cap)` with bugs 231 / 234's skip-present + within-call dedupe.
 *
 * Drives v4's REAL module, imported by FILE path. Rows: a detail already
 * present, a duplicate within the call, room 0, a cap below the present
 * count, `\r\n` content (its `[+] X\r` lines never match the detail `X`),
 * `[+]x` without the space, an empty `details`, the default cap, and a
 * 0 / 7 / 8 / 9-footnote × 0 / 1 / 3-detail grid.
 *
 * Run from inside a v4 checkout (or the round's pin):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   TZ=UTC $N/npx tsx "$V5W/harness/oracle/cases/reinforcement-footnotes.ts" > /tmp/oracle-reinforcement-footnotes.ndjson
 */

import {
  MAX_REINFORCEMENT_FOOTNOTES,
  appendCappedFootnotes,
  countReinforcementFootnotes,
} from '@/lib/memory/reinforcement-footnotes';

const rows: unknown[] = [];
rows.push({ kind: 'constant', id: 'max-footnotes', out: MAX_REINFORCEMENT_FOOTNOTES });

// cap === null → the call omits `cap` (v4's default).
const append = (id: string, content: string, details: string[], cap: number | null) => {
  rows.push({
    kind: 'append',
    id,
    content,
    details,
    cap,
    count: countReinforcementFootnotes(content),
    out: cap === null ? appendCappedFootnotes(content, details) : appendCappedFootnotes(content, details, cap),
  });
};

const footnotes = (n: number): string => Array.from({ length: n }, (_, i) => `\n[+] Detail${i}`).join('');

// The grid: 0 / 7 / 8 / 9 present × 0 / 1 / 3 details, default cap.
const DETAILS: Record<string, string[]> = { none: [], one: ['Lisbon'], three: ['Lisbon', 'Porto', 'Faro'] };
for (const existing of [0, 7, 8, 9]) {
  for (const [dname, details] of Object.entries(DETAILS)) {
    append(`grid-${existing}-${dname}`, `Mara sails.${footnotes(existing)}`, details, null);
  }
}

// Bugs 231 / 234: a detail already present is skipped; a duplicate within the call once.
append('already-present', 'Mara sails.\n[+] Lisbon', ['Lisbon', 'Porto'], 8);
append('already-present-all', 'Mara sails.\n[+] Lisbon\n[+] Porto', ['Porto', 'Lisbon'], 8);
append('duplicate-within-call', 'Mara sails.', ['Lisbon', 'Lisbon', 'Porto', 'Lisbon'], 8);
append('present-skip-does-not-spend-room', `Mara sails.${footnotes(6)}\n[+] Lisbon`, ['Lisbon', 'Porto', 'Faro'], 8);
append('present-as-prose-not-footnote', 'Mara sails to Lisbon.', ['Lisbon'], 8);
append('present-without-space', 'Mara sails.\n[+]Lisbon', ['Lisbon'], 8);
// Room 0, and a cap below the present count.
append('room-zero', `Mara sails.${footnotes(3)}`, ['Lisbon'], 3);
append('cap-below-count', `Mara sails.${footnotes(5)}`, ['Lisbon'], 2);
append('cap-zero', 'Mara sails.', ['Lisbon'], 0);
append('cap-one-of-three', 'Mara sails.', ['Lisbon', 'Porto', 'Faro'], 1);
append('cap-large', 'Mara sails.', ['Lisbon', 'Porto', 'Faro'], 100);
// `\r\n` content: lines keep their `\r`, so `[+] Lisbon\r` is not `[+] Lisbon`.
append('crlf', 'Mara sails.\r\n[+] One\r\n[+] Two\r\n', ['Lisbon'], 8);
append('crlf-present-detail', 'Mara sails.\r\n[+] Lisbon\r\n[+] Two', ['Lisbon', 'Two'], 8);
append('crlf-at-cap', `Mara sails.\r\n${Array.from({ length: 8 }, (_, i) => `[+] D${i}`).join('\r\n')}`, ['Lisbon'], 8);
// `[+]x` without the space is prose.
append('no-space', 'Mara sails.\n[+]x\n[+] y', ['Lisbon'], 8);
append('no-space-many', `Mara sails.${Array.from({ length: 9 }, (_, i) => `\n[+]x${i}`).join('')}`, ['Lisbon'], 8);
// Empty details, empty content, odd shapes.
append('empty-details', 'Mara sails.\n[+] One', [], 8);
append('empty-content', '', ['Lisbon'], 8);
append('empty-detail-string', 'Mara sails.', ['', 'Lisbon'], 8);
append('leading-footnote', '[+] first\nbody', ['first', 'second'], 8);
append('indented', 'body\n  [+] indented', ['indented'], 8);
append('trailing-newline', 'body\n', ['Lisbon'], 8);
append('non-bmp-detail', 'body', ['\u{1F600} Lisbon', 'Café'], 8);

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
