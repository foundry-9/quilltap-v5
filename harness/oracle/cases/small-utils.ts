/**
 * Oracle case #12 (Wave 1 / B4): small pure leaf utilities.
 *
 * Drives REAL pure functions from five v4 files:
 *   - lib/schemas/chat.types.ts: isHelpLikeChatType, isModerationExemptChatType,
 *     isParticipantPresent, canReceiveWhisper, migrateIsActiveToStatus
 *   - lib/utils/semver.ts: parseVersion, compareVersions (parseable pairs only)
 *   - lib/characters/pronoun-gender.ts: genderFromPronouns, genderNounFromPronouns,
 *     genderPrefixFromPronouns
 *   - lib/tags/styles.ts: mergeWithDefaultTagStyle
 *   - lib/utils/char-count.ts: charCountClass
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/small-utils.ts \
 *     > /tmp/oracle-small-utils.ndjson
 */

import {
  isHelpLikeChatType,
  isModerationExemptChatType,
  isParticipantPresent,
  canReceiveWhisper,
  migrateIsActiveToStatus,
  type ParticipantStatus,
} from '@/lib/schemas/chat.types';
import { parseVersion, compareVersions } from '@/lib/utils/semver';
import {
  genderFromPronouns,
  genderNounFromPronouns,
  genderPrefixFromPronouns,
} from '@/lib/characters/pronoun-gender';
import { mergeWithDefaultTagStyle } from '@/lib/tags/styles';
import { charCountClass } from '@/lib/utils/char-count';
import type { Pronouns } from '@/lib/schemas/character.types';
import type { TagVisualStyle } from '@/lib/schemas/types';

const pron = (subject: string): Pronouns => ({ subject } as unknown as Pronouns);

type WireStyle = {
  emoji: string | null;
  foregroundColor: string;
  backgroundColor: string;
  emojiOnly: boolean;
  bold: boolean;
  italic: boolean;
  strikethrough: boolean;
};

type Row =
  | { kind: 'chatPred'; id: string; fn: 'help' | 'moderation'; chatType: string | null; out: boolean }
  | { kind: 'statusPred'; id: string; fn: 'present' | 'whisper'; status: string; out: boolean }
  | { kind: 'migrate'; id: string; isActive: boolean; removedAt: string | null; out: string }
  | { kind: 'parseVer'; id: string; version: string; out: { major: number; minor: number; patch: number } | null }
  | { kind: 'compareVer'; id: string; a: string; b: string; out: number }
  | { kind: 'gender'; id: string; subject: string | null; out: { gender: string | null; noun: string | null; prefix: string } }
  | { kind: 'tagStyle'; id: string; style: Partial<WireStyle> | null; out: WireStyle }
  | { kind: 'charCount'; id: string; current: number; max: number; out: string };

const rows: Row[] = [];

// chat-type predicates
for (const [id, ct] of [['help', 'help'], ['brahma', 'brahma'], ['salon', 'salon'], ['null', null], ['autonomous', 'autonomous']] as Array<[string, string | null]>) {
  rows.push({ kind: 'chatPred', id: `help-${id}`, fn: 'help', chatType: ct, out: isHelpLikeChatType(ct) });
  rows.push({ kind: 'chatPred', id: `mod-${id}`, fn: 'moderation', chatType: ct, out: isModerationExemptChatType(ct) });
}

// participant-status predicates
for (const status of ['active', 'silent', 'absent', 'removed'] as ParticipantStatus[]) {
  rows.push({ kind: 'statusPred', id: `present-${status}`, fn: 'present', status, out: isParticipantPresent(status) });
  rows.push({ kind: 'statusPred', id: `whisper-${status}`, fn: 'whisper', status, out: canReceiveWhisper(status) });
}

// migrateIsActiveToStatus — note: removedAt '' is falsy → absent.
const migCases: Array<[string, boolean, string | null]> = [
  ['active', true, null],
  ['active-with-removed', true, '2026-01-01T00:00:00.000Z'], // active wins
  ['removed', false, '2026-01-01T00:00:00.000Z'],
  ['absent', false, null],
  ['empty-removed', false, ''], // '' falsy → absent
];
for (const [id, isActive, removedAt] of migCases) {
  rows.push({ kind: 'migrate', id, isActive, removedAt, out: migrateIsActiveToStatus(isActive, removedAt) });
}

// parseVersion
const pvCases: Array<[string, string]> = [
  ['plain', '1.2.3'],
  ['v-prefix', 'v2.0.1'],
  ['prerelease', '1.4.0-beta.2'],
  ['extra-segment', '1.2.3.4'],
  ['leading-zero', '01.02.03'],
  ['too-short', '1.2'],
  ['garbage', 'not-a-version'],
  ['vv', 'vv1.2.3'],
  ['empty', ''],
];
for (const [id, version] of pvCases) {
  rows.push({ kind: 'parseVer', id, version, out: parseVersion(version) });
}

// compareVersions — parseable pairs, plus malformed pairs that hit the
// `localeCompare` fallback (now ICU-backed, so non-ASCII / mixed-case pairs are
// faithful, not just code-unit). ICU en-US/tertiary: "apple" < "Banana" (-1,
// where code-unit gives +1 since 'a'=97 > 'B'=66), and accents interleave.
const cvCases: Array<[string, string, string]> = [
  ['lt-major', '1.0.0', '2.0.0'],
  ['gt-minor', '1.3.0', '1.2.9'],
  ['eq', '1.2.3', '1.2.3'],
  ['lt-patch', '1.2.3', '1.2.4'],
  ['v-prefix-eq', 'v1.2.3', '1.2.3'],
  ['suffix-ignored', '1.2.3-rc1', '1.2.3+build'],
  // localeCompare fallback (at least one side unparseable):
  ['fallback-case', 'apple', 'Banana'],
  ['fallback-case-rev', 'Banana', 'apple'],
  ['fallback-accent', 'apple', 'äpple'],
  ['fallback-equal-text', 'notaversion', 'notaversion'],
  ['fallback-one-parses', '1.2.3', 'notaversion'],
];
for (const [id, a, b] of cvCases) {
  rows.push({ kind: 'compareVer', id, a, b, out: compareVersions(a, b) });
}

// pronoun-gender (all three derivations per subject)
const gCases: Array<[string, string | null]> = [
  ['he', 'he'],
  ['she', 'she'],
  ['He-caps', 'He'],
  ['SHE-pad', '  SHE  '],
  ['they', 'they'],
  ['neo', 'xe'],
  ['empty', ''],
  ['none', null],
];
for (const [id, subject] of gCases) {
  const pronouns = subject === null ? null : pron(subject);
  rows.push({
    kind: 'gender',
    id,
    subject,
    out: {
      gender: genderFromPronouns(pronouns),
      noun: genderNounFromPronouns(pronouns),
      prefix: genderPrefixFromPronouns(pronouns),
    },
  });
}

// mergeWithDefaultTagStyle
const tsCases: Array<[string, Partial<WireStyle> | null]> = [
  ['null', null],
  ['empty-obj', {}],
  ['emoji-set', { emoji: '🔥' }],
  ['emoji-empty', { emoji: '' }], // empty → null
  ['colors', { foregroundColor: '#000000', backgroundColor: '#ffffff' }],
  ['color-empty', { foregroundColor: '' }], // '' falsy → default
  ['flags-false', { bold: false, italic: false, emojiOnly: false }], // ?? keeps false
  ['flags-true', { bold: true, strikethrough: true, emojiOnly: true }],
];
for (const [id, style] of tsCases) {
  rows.push({ kind: 'tagStyle', id, style, out: mergeWithDefaultTagStyle(style as Partial<TagVisualStyle> | null) as WireStyle });
}

// charCountClass
const ccCases: Array<[string, number, number]> = [
  ['under', 50, 100], // secondary
  ['at-90', 90, 100], // 90 not > 90 → secondary
  ['over-90', 91, 100], // warning
  ['at-max', 100, 100], // 100 not > 100 → warning (>90)
  ['over-max', 101, 100], // destructive
];
for (const [id, current, max] of ccCases) {
  rows.push({ kind: 'charCount', id, current, max, out: charCountClass(current, max) });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
