/**
 * Oracle case (P4.D37, v4 `424a7381`): the pure ad-hoc-announcement speaker
 * attribution leaves.
 *
 * Drives the REAL exported functions from v4's
 * `lib/chat/context/announcement-attribution.ts`:
 *   - resolveAnnouncerName
 *   - collectAnnouncerCharacterIds
 *   - attributeAdhocAnnouncements
 *
 * The corpus is v4's own 109-line jest suite (tagging a character / a custom
 * display name, an un-announced message left alone, an unresolvable character
 * passed through unnamed, idempotency across a re-run, every other field
 * preserved) widened where the suite relies on TypeScript rather than a runtime
 * check: a blank/whitespace display name, an empty-string `characterId`, a name
 * that needs trimming, an absent `content`, a body that starts with the tag but a
 * DIFFERENT name, and the first-appearance ordering of the id collection.
 *
 * `attributeAdhocAnnouncements` is emitted as the WHOLE output message array, so
 * "every other field survives" is diffed rather than asserted.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx <v5-worktree>/harness/oracle/cases/announcement-attribution.ts \
 *     > /tmp/oracle-announcement-attribution.ndjson
 */

import {
  attributeAdhocAnnouncements,
  collectAnnouncerCharacterIds,
  resolveAnnouncerName,
} from '@/lib/chat/context/announcement-attribution';

type Msg = Record<string, unknown>;

const ARIEL = '14710fac-c074-45f1-bc03-c19fa66fc18e';
const BEA = '2b2b2b2b-0000-4000-8000-000000000002';

/** The shared name map for every case (the caller's up-front lookup). */
const NAME_ENTRIES: Array<[string, string]> = [
  [ARIEL, 'Ariel'],
  [BEA, '  Bea  '], // stored untrimmed: v4 trims at resolve time, not at set time
  ['blank-name', '   '],
  // A name under the EMPTY-STRING key, so v4's `!announcer.characterId` falsy
  // guard is observable: without it, an empty `characterId` would resolve to a
  // name. (Without this entry the guard is a no-op the map already covers, and
  // the arm cannot be mutation-proven.)
  ['', 'Nobody At All'],
];
const NAMES = new Map(NAME_ENTRIES);

const lines: string[] = [];

// ---------------------------------------------------------------------------
// resolveAnnouncerName
// ---------------------------------------------------------------------------

const resolveCases: Array<{ name: string; announcer: unknown; systemSender?: string | null }> = [
  { name: 'character_named', announcer: { kind: 'character', characterId: ARIEL } },
  { name: 'character_name_is_trimmed', announcer: { kind: 'character', characterId: BEA } },
  { name: 'character_name_blank_is_null', announcer: { kind: 'character', characterId: 'blank-name' } },
  { name: 'character_unknown_id_is_null', announcer: { kind: 'character', characterId: 'gone' } },
  { name: 'character_null_id_is_null', announcer: { kind: 'character', characterId: null } },
  { name: 'character_empty_id_is_null', announcer: { kind: 'character', characterId: '' } },
  { name: 'character_absent_id_is_null', announcer: { kind: 'character' } },
  { name: 'custom_display_name', announcer: { kind: 'custom', displayName: 'The Narrator' } },
  { name: 'custom_display_name_trimmed', announcer: { kind: 'custom', displayName: '  The Narrator  ' } },
  { name: 'custom_blank_display_name_is_null', announcer: { kind: 'custom', displayName: '   ' } },
  { name: 'custom_absent_display_name_is_null', announcer: { kind: 'custom' } },
  { name: 'announcer_null', announcer: null },
  { name: 'announcer_undefined', announcer: undefined },
  // Bug 28 (v4 `99d5fc7d`): the Staff-sender fallback. No `customAnnouncer`, a
  // `systemSender` names the speaker through the staff-name table.
  { name: 'staff_sender_host', announcer: null, systemSender: 'host' },
  { name: 'staff_sender_suparna', announcer: null, systemSender: 'suparna' },
  // An unknown sender falls back to the raw tag (staffDisplayName `?? sender`).
  { name: 'staff_sender_unknown_is_raw', announcer: null, systemSender: 'newcomer' },
  // JS-truthy but whitespace-only → staffDisplayName returns the raw tag, `.trim()`
  // empties it, `|| null` nulls it.
  { name: 'staff_sender_whitespace_is_null', announcer: null, systemSender: '   ' },
  // Empty string is falsy → the `if (systemSender)` guard is skipped → null.
  { name: 'staff_sender_empty_is_null', announcer: null, systemSender: '' },
  // An announcer, when present, wins over the sender (returns from the announcer
  // branch before the fallback is reached).
  { name: 'announcer_wins_over_sender', announcer: { kind: 'character', characterId: ARIEL }, systemSender: 'host' },
];

for (const c of resolveCases) {
  lines.push(
    JSON.stringify({
      kind: 'resolve',
      name: c.name,
      announcer: c.announcer ?? null,
      systemSender: c.systemSender ?? null,
      names: NAME_ENTRIES,
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      output: resolveAnnouncerName(c.announcer as never, NAMES, c.systemSender ?? undefined),
    }),
  );
}

// ---------------------------------------------------------------------------
// collectAnnouncerCharacterIds
// ---------------------------------------------------------------------------

const collectCases: Array<{ name: string; messages: Msg[] }> = [
  {
    name: 'gathers_referenced_ids_once_each',
    messages: [
      { customAnnouncer: { kind: 'character', characterId: ARIEL } },
      { customAnnouncer: { kind: 'character', characterId: ARIEL } },
      { customAnnouncer: { kind: 'custom', displayName: 'The Narrator' } },
      { customAnnouncer: null },
      {},
    ],
  },
  {
    name: 'first_appearance_order',
    messages: [
      { customAnnouncer: { kind: 'character', characterId: BEA } },
      { customAnnouncer: { kind: 'character', characterId: ARIEL } },
      { customAnnouncer: { kind: 'character', characterId: BEA } },
    ],
  },
  {
    name: 'skips_empty_and_null_ids',
    messages: [
      { customAnnouncer: { kind: 'character', characterId: '' } },
      { customAnnouncer: { kind: 'character', characterId: null } },
      { customAnnouncer: { kind: 'character' } },
    ],
  },
  { name: 'empty_list', messages: [] },
];

for (const c of collectCases) {
  lines.push(
    JSON.stringify({
      kind: 'collect',
      name: c.name,
      messages: c.messages,
      output: collectAnnouncerCharacterIds(c.messages as never),
    }),
  );
}

// ---------------------------------------------------------------------------
// attributeAdhocAnnouncements
// ---------------------------------------------------------------------------

const attributeCases: Array<{ name: string; messages: Msg[]; twice?: boolean }> = [
  {
    name: 'tags_a_character_announcement',
    messages: [
      { content: '*She drifts closer.*', customAnnouncer: { kind: 'character', characterId: ARIEL } },
    ],
  },
  {
    name: 'tags_a_custom_announcement',
    messages: [{ content: 'A bell tolls.', customAnnouncer: { kind: 'custom', displayName: 'The Narrator' } }],
  },
  {
    name: 'leaves_unannounced_messages_untouched',
    messages: [{ content: 'Prospero opens his ledger.' }, { content: 'hi', customAnnouncer: null }],
  },
  {
    name: 'leaves_an_unresolvable_character_alone',
    messages: [{ content: 'x', customAnnouncer: { kind: 'character', characterId: 'gone' } }],
  },
  {
    name: 'idempotent_across_a_rerun',
    twice: true,
    messages: [{ content: 'hello', customAnnouncer: { kind: 'character', characterId: ARIEL } }],
  },
  {
    name: 'a_different_name_in_brackets_is_not_the_tag',
    messages: [
      { content: '[Bea] hello', customAnnouncer: { kind: 'character', characterId: ARIEL } },
    ],
  },
  {
    name: 'preserves_every_other_field',
    messages: [
      {
        content: 'hi',
        customAnnouncer: { kind: 'character', characterId: ARIEL },
        id: 'm1',
        role: 'ASSISTANT',
        targetParticipantIds: ['p1'],
        systemKind: 'announcement',
        participantId: null,
      },
    ],
  },
  {
    name: 'absent_content_becomes_the_bare_tag',
    messages: [{ id: 'm2', customAnnouncer: { kind: 'custom', displayName: 'The Narrator' } }],
  },
  {
    name: 'a_mixed_list_in_one_pass',
    messages: [
      { content: 'u01', role: 'USER' },
      { content: 'a01', role: 'ASSISTANT', customAnnouncer: { kind: 'character', characterId: ARIEL } },
      { content: 'a02', role: 'ASSISTANT', customAnnouncer: { kind: 'custom', displayName: 'A Distant Bell' } },
      { content: 'a03', role: 'ASSISTANT', customAnnouncer: { kind: 'character', characterId: 'gone' } },
    ],
  },
  // ---- Bug 28: the Staff-sender fallback + the opaque body ----
  {
    // A staff-signed ad-hoc announcement (no customAnnouncer, systemKind
    // 'announcement') is tagged with the resolved staff name.
    name: 'tags_a_staff_announcement',
    messages: [
      { content: 'The hour grows late.', systemSender: 'host', systemKind: 'announcement' },
    ],
  },
  {
    // An ordinary Staff whisper (a NON-announcement systemKind) carries a
    // systemSender too, but names itself in its own prose — it must NOT be tagged.
    name: 'leaves_non_announcement_staff_whispers_untouched',
    messages: [
      { content: 'A Lantern image was shared.', systemSender: 'lantern', systemKind: 'image' },
    ],
  },
  {
    // The prefix lands on opaqueContent as well as content, so the opaque-anywhere
    // body swap (normalizeWhisperRoles) still reaches the model attributed.
    name: 'tags_both_content_and_opaque_content',
    messages: [
      {
        content: 'A bell tolls thrice.',
        opaqueContent: 'A bell tolls thrice.',
        systemSender: 'host',
        systemKind: 'announcement',
      },
    ],
  },
  {
    // A null opaqueContent is not a string — only content is tagged.
    name: 'null_opaque_content_is_left_alone',
    messages: [
      {
        content: 'Only content here.',
        opaqueContent: null,
        customAnnouncer: { kind: 'custom', displayName: 'The Narrator' },
      },
    ],
  },
  {
    // Idempotent across a re-run for the staff arm too (retry / regenerate).
    name: 'idempotent_staff_across_a_rerun',
    twice: true,
    messages: [
      {
        content: 'The doors close.',
        opaqueContent: 'The doors close.',
        systemSender: 'suparna',
        systemKind: 'announcement',
      },
    ],
  },
];

for (const c of attributeCases) {
  const once = attributeAdhocAnnouncements(c.messages as never, NAMES);
  const out = c.twice ? attributeAdhocAnnouncements(once, NAMES) : once;
  lines.push(
    JSON.stringify({
      kind: 'attribute',
      name: c.name,
      twice: c.twice ?? false,
      names: NAME_ENTRIES,
      messages: c.messages,
      output: out,
    }),
  );
}

for (const l of lines) process.stdout.write(l + '\n');
