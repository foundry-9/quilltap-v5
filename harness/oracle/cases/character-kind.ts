/**
 * Tier-1 oracle case — character kind (v4 `lib/characters/kind.ts`,
 * `b09ca46bb` + `96156566b` / `3ae4cf582`).
 *
 * Drives v4's REAL module over a fixed corpus and emits one NDJSON row per
 * call: `{ id, fn, value?, out }`. An `undefined` input is emitted as an ABSENT
 * `value` key. The Rust port (quilltap_core::characters::kind) must produce the
 * same `out` bytes per row.
 *
 * Run from the v4 server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/character-kind.ts \
 *     > /tmp/oracle-character-kind.ndjson
 */

import {
  CHARACTER_KINDS,
  DEFAULT_CHARACTER_KIND,
  characterKindOrDefault,
  characterKindPolicy,
  isAssistantCharacter,
  kindPolicy,
  resolveCharacterKind,
} from '@/lib/characters/kind';

type Row = { id: string; fn: string; value?: unknown; out: unknown };
const rows: Row[] = [];

function row(id: string, fn: string, value: unknown, out: unknown) {
  const r: Row = { id, fn, out };
  if (value !== undefined) r.value = value;
  rows.push(r);
}

// characterKindOrDefault — "normalizes %p to %p"
const kindValues: Array<[string, unknown]> = [
  ['roleplay', 'roleplay'],
  ['assistant', 'assistant'],
  ['mixed-case', 'Assistant'],
  ['leading-space', ' assistant'],
  ['empty', ''],
  ['robot', 'robot'],
  ['null', null],
  ['undefined', undefined],
  ['number', 5],
  ['bool', true],
  ['array', []],
  ['object', {}],
  ['object-with-kind', { kind: 'assistant' }],
];
for (const [suffix, v] of kindValues) {
  row(`normalizes-${suffix}`, 'characterKindOrDefault', v, characterKindOrDefault(v));
}

// resolveCharacterKind / isAssistantCharacter — "resolves a character …",
// "answers whether a character is an assistant"
const characters: Array<[string, unknown]> = [
  ['null', null],
  ['undefined', undefined],
  ['empty', {}],
  ['assistant', { kind: 'assistant' }],
  ['roleplay', { kind: 'roleplay' }],
  ['mixed-case', { kind: 'Assistant' }],
  ['kind-null', { kind: null }],
  ['kind-number', { kind: 5 }],
  ['bare-string', 'assistant'],
  ['bare-number', 5],
];
for (const [suffix, c] of characters) {
  row(`resolves-${suffix}`, 'resolveCharacterKind', c, resolveCharacterKind(c as never));
  row(`is-assistant-${suffix}`, 'isAssistantCharacter', c, isAssistantCharacter(c as never));
}

// kindPolicy — "derives the policy for each kind"
row('policy-roleplay', 'kindPolicy', 'roleplay', kindPolicy('roleplay'));
row('policy-assistant', 'kindPolicy', 'assistant', kindPolicy('assistant'));
row('policy-robot', 'kindPolicy', 'robot', kindPolicy('robot' as never));

// characterKindPolicy — "derives a character's policy, defaulting to roleplay"
const policyCharacters: Array<[string, unknown]> = [
  ['null', null],
  ['empty', {}],
  ['assistant', { kind: 'assistant' }],
  ['unknown', { kind: 'x' }],
];
for (const [suffix, c] of policyCharacters) {
  row(`character-policy-${suffix}`, 'characterKindPolicy', c, characterKindPolicy(c as never));
}

// "defaults to roleplay"
row('defaults-kinds', 'CHARACTER_KINDS', undefined, CHARACTER_KINDS);
row('defaults-default', 'DEFAULT_CHARACTER_KIND', undefined, DEFAULT_CHARACTER_KIND);

for (const r of rows) {
  process.stdout.write(JSON.stringify(r) + '\n');
}
