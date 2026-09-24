/**
 * The v4-side recorder behind `src/app/chat/mentions/mention-typeahead.oracle.spec.ts`.
 *
 * v4 `3376b3dfa` added the composer's `@` character typeahead with its pure logic
 * in `lib/mentions/mention-typeahead.ts` — a CLIENT module despite its `lib/`
 * home. v5's `chat/mentions/mention-typeahead.ts` is a transcription, and THIS
 * corpus, recorded from v4's REAL module, is what proves it: every exported
 * function over v4's own vectors (`lib/mentions/__tests__/mention-typeahead.test.ts`)
 * plus a wider corpus along each rule's edges.
 *
 * Row kinds (the `kind` key): `trigger` (`findMentionTrigger`), `rank`
 * (`rankMentionCandidates`, compared as the ordered id list), `candidates`
 * (`mentionCandidatesFor`, compared whole), `classify`
 * (`classifyLineStartMention`), `canKeep` (`canKeepLineStartAt`), `constants`
 * (`MENTION_TRIGGER` + `BRAHMA_MENTION`).
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/lib/...` and
 * would not compile in the SPA's own tsconfig.
 *
 * Run it from a pinned v4 worktree at or after `3376b3dfa` (Node 24 at
 * `~/.nvm/versions/node/v24.13.1/bin`):
 *
 * ```bash
 * PIN=/tmp/qt-v4-pin-<order>-<sha>
 * git -C ~/source/quilltap-server worktree add --detach "$PIN" <sha>
 * ln -sfn ~/source/quilltap-server/node_modules "$PIN/node_modules"
 * cp <V5>/apps/web/oracle/mention-typeahead.recorder.ts "$PIN/"
 * cd "$PIN" && npx tsx mention-typeahead.recorder.ts \
 *   > <V5>/apps/web/src/testing/fixtures/mention-typeahead.ndjson
 * ```
 *
 * Expect 116 lines; a shorter file means the recorder errored and the redirect
 * already truncated the old one (the empty-file trap). Pin marker: the module
 * does not exist before `3376b3dfa`, so a pre-feature tree cannot run this at all.
 */

import {
  BRAHMA_MENTION,
  MENTION_TRIGGER,
  canKeepLineStartAt,
  classifyLineStartMention,
  findMentionTrigger,
  mentionCandidatesFor,
  rankMentionCandidates,
  type MentionCandidate,
} from '@/lib/mentions/mention-typeahead';

const rows: unknown[] = [];
const seen = new Set<string>();
function emit(row: { id: string; kind: string } & Record<string, unknown>): void {
  if (seen.has(row.id)) throw new Error(`duplicate vector label: ${row.id}`);
  seen.add(row.id);
  rows.push(row);
}

// --- constants ----------------------------------------------------------------
emit({
  id: 'constants',
  kind: 'constants',
  trigger: {
    opener: MENTION_TRIGGER.opener,
    queryPattern: { source: MENTION_TRIGGER.queryPattern.source, flags: MENTION_TRIGGER.queryPattern.flags },
    minQueryLength: MENTION_TRIGGER.minQueryLength,
    maxQueryLength: MENTION_TRIGGER.maxQueryLength,
    closer: MENTION_TRIGGER.closer,
    lowercaseQuery: MENTION_TRIGGER.lowercaseQuery,
    queryStartPattern: MENTION_TRIGGER.queryStartPattern ?? null,
  },
  brahma: BRAHMA_MENTION,
});

// --- findMentionTrigger ---------------------------------------------------------
const TRIGGERS: Array<[string, string]> = [
  // v4's vectors, verbatim
  ['trigger: bare @', '@'],
  ['trigger: @ari', '@ari'],
  ['trigger: hello @Ari', 'hello @Ari'],
  ['trigger: (@zo', '(@zo'],
  ['trigger: line one\\n@Vi', 'line one\n@Vi'],
  ['trigger: @Zoë', '@Zoë'],
  ['trigger: name@example.com', 'name@example.com'],
  ['trigger: hello @ari + space', 'hello @ari '],
  ['trigger: plain text', 'plain text'],
  ['trigger: x@', 'x@'],
  // widening
  ['trigger: combining mark in query (NFD)', '@Zoe\u0301'],
  ['trigger: Cyrillic query', 'привет @Ив'],
  ['trigger: CJK query', '@李'],
  ['trigger: digits query', '@42'],
  ['trigger: underscore query', '@r2_d'],
  ['trigger: hyphen query', '@jean-l'],
  ['trigger: apostrophe ends the query', "@o'ne"],
  ['trigger: curly apostrophe ends the query', '@o’ne'],
  ['trigger: full stop ends the query', '@mr.s'],
  ['trigger: 48-char query', '@' + 'a'.repeat(48)],
  ['trigger: 49-char query', '@' + 'a'.repeat(49)],
  ['trigger: after a tab', 'x\t@ab'],
  ['trigger: after a bracket', '[@ab'],
  ['trigger: after a brace', '{@ab'],
  ['trigger: after a double quote', '"@ab'],
  ["trigger: after a single quote", "'@ab"],
  ['trigger: after a curly double quote', '“@ab'],
  ['trigger: after a curly single quote', '‘@ab'],
  ['trigger: after an em dash', '—@ab'],
  ['trigger: after an en dash', '–@ab'],
  ['trigger: after a colon', 'x:@ab'],
  ['trigger: after a closing paren', ')@ab'],
  ['trigger: double @', '@@ab'],
  ['trigger: after U+FFFC', '\uFFFC@ab'],
  ['trigger: after a no-break space', 'x\u00A0@ab'],
  ['trigger: emoji ends the query', '@ab🦉'],
  ['trigger: emoji before the opener', '🦉@ab'],
  ['trigger: empty string', ''],
  ['trigger: mixed case preserved', '@ArIsT'],
];
for (const [id, text] of TRIGGERS) emit({ id, kind: 'trigger', text, out: findMentionTrigger(text) });

// --- rankMentionCandidates ------------------------------------------------------
const CAST: MentionCandidate[] = [
  { id: '1', name: 'Aristarchus' },
  { id: '2', name: 'Lady Arabella' },
  { id: '3', name: 'Barnaby' },
  { id: '4', name: 'Arden' },
  { id: '5', name: '' },
];
const WIDE: MentionCandidate[] = [
  { id: 'a', name: 'Zoë' },
  { id: 'b', name: 'Zoe' },
  { id: 'c', name: 'zoey' },
  { id: 'd', name: 'Jean-Luc' },
  { id: 'e', name: "O'Neil" },
  { id: 'f', name: 'O’Brien' },
  { id: 'g', name: 'Mr. Smith' },
  { id: 'h', name: 'R2_D2' },
  { id: 'i', name: '   ' },
  { id: 'j', name: 'Ébène' },
  { id: 'k', name: 'ebony' },
  { id: 'l', name: 'Émile Zola' },
  { id: 'm', name: 'Иван Грозный' },
  { id: 'd', name: 'Jean-Luc duplicate id' },
  { id: 'n', name: 'Lady\tVivienne' },
  { id: 'o', name: 'Ab' },
  { id: 'p', name: 'ab' },
  { id: 'q', name: 'AB' },
];
const RANKS: Array<[string, MentionCandidate[], string, string[], number]> = [
  // v4's vectors
  ['rank: empty query, alphabetical', CAST, '', [], 10],
  ['rank: AR — name prefix over word prefix, no mid-word', CAST, 'AR', [], 10],
  ['rank: ar with the cast floated', CAST, 'ar', ['2'], 10],
  ['rank: zz matches nothing', CAST, 'zz', [], 10],
  ['rank: empty query, limit 2', CAST, '', [], 2],
  // widening
  ['rank: limit 0', CAST, '', [], 0],
  ['rank: limit 1', CAST, 'ar', [], 1],
  ['rank: a priority id not in the list', CAST, 'ar', ['zzz'], 10],
  ['rank: two priority ids', CAST, '', ['3', '2'], 10],
  ['rank: priority beats tier', CAST, 'ar', ['2', '1'], 10],
  ['rank: blank priority name still skipped', CAST, '', ['5'], 10],
  ['rank: wide, empty query', WIDE, '', [], 50],
  ['rank: wide, zo', WIDE, 'zo', [], 50],
  ['rank: wide, ZOË', WIDE, 'ZOË', [], 50],
  ['rank: wide, luc (hyphen word)', WIDE, 'luc', [], 50],
  ['rank: wide, neil (apostrophe word)', WIDE, 'neil', [], 50],
  ['rank: wide, brien (curly apostrophe word)', WIDE, 'brien', [], 50],
  ['rank: wide, smith (full stop word)', WIDE, 'smith', [], 50],
  ['rank: wide, d2 (underscore word)', WIDE, 'd2', [], 50],
  ['rank: wide, eb (accent folding by base sort)', WIDE, 'eb', [], 50],
  ['rank: wide, zola', WIDE, 'zola', [], 50],
  ['rank: wide, гр (Cyrillic word)', WIDE, 'гр', [], 50],
  ['rank: wide, vivienne (tab word)', WIDE, 'vivienne', [], 50],
  ['rank: wide, ab ties by base sensitivity', WIDE, 'ab', [], 50],
  ['rank: wide, jean-luc (the whole hyphenated query)', WIDE, 'jean-luc', [], 50],
  ['rank: wide, duplicate id keeps the first', WIDE, 'jean', [], 50],
  ['rank: wide, NFD query does not match NFC name', WIDE, 'Zoe\u0301', [], 50],
];
for (const [id, candidates, query, priority, limit] of RANKS) {
  emit({
    id,
    kind: 'rank',
    candidates,
    query,
    priority,
    limit,
    out: rankMentionCandidates(candidates, query, new Set(priority), limit).map((c) => c.id),
  });
}

// --- mentionCandidatesFor --------------------------------------------------------
const CANDIDATE_SETS: Array<[string, MentionCandidate[], boolean]> = [
  ['candidates: not at line start', [{ id: '1', name: 'Aristarchus' }], false],
  ['candidates: at line start adds Brahma', [{ id: '1', name: 'Aristarchus' }], true],
  ['candidates: a padded brahma suppresses it', [{ id: '1', name: 'Aristarchus' }, { id: '2', name: ' brahma ' }], true],
  ['candidates: BRAHMA upper-case suppresses it', [{ id: '2', name: 'BRAHMA' }], true],
  ['candidates: Brahma Console does not suppress it', [{ id: '2', name: 'Brahma Console' }], true],
  ['candidates: a tab-padded Brahma suppresses it', [{ id: '2', name: '\tBrahma\n' }], true],
  ['candidates: empty list at line start', [], true],
  ['candidates: empty list mid-line', [], false],
  ['candidates: brahma mid-line is just a character', [{ id: '2', name: 'brahma' }], false],
];
for (const [id, characters, atLineStart] of CANDIDATE_SETS) {
  emit({ id, kind: 'candidates', characters, atLineStart, out: mentionCandidatesFor(characters, atLineStart) });
}

// --- classifyLineStartMention ----------------------------------------------------
const CLASSIFY: Array<[string, string, string]> = [
  // v4's vectors
  ['classify: @Aristarchus', '@Aristarchus', 'Aristarchus'],
  ['classify: @Aristarchus:', '@Aristarchus:', 'Aristarchus'],
  ['classify: @Aristarchus?', '@Aristarchus?', 'Aristarchus'],
  ['classify: @Aristarchus: what is the date?', '@Aristarchus: what is the date?', 'Aristarchus'],
  ['classify: @Aristarchus? a whisper', '@Aristarchus? a whisper', 'Aristarchus'],
  ['classify: @Aristarchus + space', '@Aristarchus ', 'Aristarchus'],
  ['classify: @Aristarchus,', '@Aristarchus,', 'Aristarchus'],
  ['classify: @Aristarchuss', '@Aristarchuss', 'Aristarchus'],
  ['classify: @Aristarchus:x', '@Aristarchus:x', 'Aristarchus'],
  ['classify: @Aristarch', '@Aristarch', 'Aristarchus'],
  ['classify: Aristarchus', 'Aristarchus', 'Aristarchus'],
  ['classify: @Jean-Luc bare', '@Jean-Luc', 'Jean-Luc'],
  ['classify: @Jean-Luc: hello', '@Jean-Luc: hello', 'Jean-Luc'],
  ['classify: @Zoë bare', '@Zoë', 'Zoë'],
  ['classify: @Zoë: hello', '@Zoë: hello', 'Zoë'],
  ["classify: @O'Neil bare", "@O'Neil", "O'Neil"],
  ["classify: @O'Neil: hello", "@O'Neil: hello", "O'Neil"],
  ['classify: @X bare', '@X', 'X'],
  ['classify: @X: hello', '@X: hello', 'X'],
  ['classify: @Lady Arabella: hello', '@Lady Arabella: hello', 'Lady Arabella'],
  ['classify: @Lady Arabella said', '@Lady Arabella said', 'Lady Arabella'],
  ['classify: @Brahma? hi', '@Brahma? hi', 'Brahma'],
  // widening
  ['classify: a tab after the separator keeps', '@Aristarchus:\tnow', 'Aristarchus'],
  ['classify: a no-break space after the separator keeps', '@Aristarchus:\u00A0now', 'Aristarchus'],
  ['classify: a newline after the separator keeps', '@Aristarchus:\nnow', 'Aristarchus'],
  ['classify: a doubled separator strips', '@Aristarchus::', 'Aristarchus'],
  ['classify: ?: strips', '@Aristarchus?:', 'Aristarchus'],
  ['classify: a leading space abandons', ' @Aristarchus', 'Aristarchus'],
  ['classify: a case change abandons', '@aristarchus', 'Aristarchus'],
  ['classify: an empty line abandons', '', 'Aristarchus'],
  ['classify: a name ending in space', '@Trail : hi', 'Trail '],
  ['classify: a two-char name keeps', '@Ab: hi', 'Ab'],
  ['classify: the empty name', '@: hi', ''],
  ['classify: a trailing ! strips', '@Aristarchus!', 'Aristarchus'],
];
for (const [id, line, name] of CLASSIFY) {
  emit({ id, kind: 'classify', line, name, out: classifyLineStartMention(line, name) });
}

// --- canKeepLineStartAt -----------------------------------------------------------
const KEEP: Array<[string, string]> = [
  ['canKeep: Aristarchus', 'Aristarchus'],
  ['canKeep: Lady Arabella', 'Lady Arabella'],
  ['canKeep: Jean-Luc', 'Jean-Luc'],
  ['canKeep: Zoë', 'Zoë'],
  ['canKeep: X', 'X'],
  ['canKeep: Brahma', 'Brahma'],
];
for (const [id, name] of KEEP) emit({ id, kind: 'canKeep', name, out: canKeepLineStartAt(name) });

for (const row of rows) console.log(JSON.stringify(row));
