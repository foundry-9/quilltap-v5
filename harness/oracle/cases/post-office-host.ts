/**
 * Tier-1 differential oracle for the W4.6b Host-notifications PURE content /
 * opaqueContent builders — driving v4's REAL exports from
 * `lib/services/host-notifications/writer.ts` so the byte-exact steampunk /
 * Wodehouse strings are proven against the Rust port
 * (`services::host_notifications`).
 *
 * Pure-only: no DB. Every function here is a pure v4 export. The `add` builders
 * are async but hit NO DB when `characterDocumentMountPointId` is absent
 * (`readVaultIdentity` short-circuits to null), so the DB-free description /
 * empty branches are driven here; the identity branch (which reads the vault) is
 * pinned by a Rust self-test and covered end-to-end by the parent's central
 * tier-3 differential. The private continuation / merge / no-user-character
 * builders are NOT exported, so they are not driven here — they are covered by
 * transcription + Rust self-tests + the parent tier-3 (persisted `content`
 * column). The POST functions' row shape (systemSender / systemKind / hostEvent
 * / targeting / opaqueContent) is likewise verified by the parent's tier-3.
 *
 * Pure — runs under `npx tsx` (no DB, no jest). Emits NDJSON to stdout.
 */

import {
  buildAddContent,
  buildAddOpaqueContent,
  buildRemoveContent,
  buildRemoveOpaqueContent,
  buildStatusChangeContent,
  buildStatusChangeOpaqueContent,
  buildScenarioContent,
  buildScenarioOpaqueContent,
  buildUserCharacterContent,
  buildUserCharacterOpaqueContent,
  buildMultiCharacterRosterContent,
  buildMultiCharacterRosterOpaqueContent,
  buildSilentModeEntryContent,
  buildSilentModeEntryOpaqueContent,
  buildSilentModeExitContent,
  buildSilentModeExitOpaqueContent,
  buildJoinScenarioContent,
  buildJoinScenarioOpaqueContent,
  buildTimestampContent,
  buildTimestampOpaqueContent,
  buildTurnPassContent,
  buildUserTurnPassContent,
  buildTurnPassOpaqueContent,
  buildNudgeContent,
  buildNudgeOpaqueContent,
} from '@/lib/services/host-notifications/writer';

// ---- add (description / empty branches; no vault mount → no DB) ----
const addCases: Array<{
  id: string;
  name: string;
  description?: string | null;
  user: string | null;
}> = [
  { id: 'desc-plain', name: 'Ada', description: 'A brilliant analyst.', user: 'Charlie' },
  {
    id: 'desc-template',
    name: 'Ada',
    description: 'Known by {{user}}, called {{char}}.',
    user: 'Charlie',
  },
  { id: 'desc-user-null', name: 'Cid', description: 'A plain description.', user: null },
  { id: 'no-desc', name: 'Bea', description: '', user: null },
  { id: 'desc-only-whitespace', name: 'Dot', description: '   ', user: 'Eve' },
];

// ---- remove ----
const removeCases: Array<{ id: string; name: string }> = [
  { id: 'plain', name: 'Ada' },
  { id: 'unicode', name: 'Zoë' },
];

// ---- status change (known phrases + unknown fallback) ----
const statusCases: Array<{ id: string; name: string; from: string; to: string }> = [
  { id: 'active-to-silent', name: 'Ada', from: 'active', to: 'silent' },
  { id: 'silent-to-active', name: 'Ada', from: 'silent', to: 'active' },
  { id: 'absent-to-removed', name: 'Bea', from: 'absent', to: 'removed' },
  { id: 'unknown-from', name: 'Cid', from: 'lurking', to: 'active' },
  { id: 'unknown-to', name: 'Cid', from: 'active', to: 'lurking' },
];

// ---- scenario (builder trims) ----
const scenarioCases: Array<{ id: string; text: string }> = [
  { id: 'plain', text: 'The parlor at dusk.' },
  { id: 'trailing-ws', text: '  The parlor at dusk.  ' },
];

// ---- user-character (desc present with template / empty) ----
const userCharCases: Array<{ id: string; name: string; description?: string | null }> = [
  { id: 'with-desc-template', name: 'Cid', description: 'The voice of {{user}}.' },
  { id: 'empty-desc', name: 'Cid', description: '' },
  { id: 'null-desc', name: 'Cid', description: null },
];

// ---- multi-character roster ----
interface OtherInfo {
  name: string;
  aliases?: string[];
  pronouns?: { subject: string; object: string; possessive: string };
  description?: string;
  type: 'CHARACTER';
  status?: 'active' | 'silent' | 'absent' | 'removed';
}
const rosterCases: Array<{ id: string; responding: string; others: OtherInfo[] }> = [
  { id: 'alone', responding: 'Ada', others: [] },
  {
    id: 'company',
    responding: 'Ada',
    others: [
      {
        name: 'Bea',
        aliases: ['B'],
        pronouns: { subject: 'she', object: 'her', possessive: 'hers' },
        description: 'A beekeeper.',
        type: 'CHARACTER',
        status: 'silent',
      },
      { name: 'User Cid', type: 'CHARACTER', status: 'active' },
    ],
  },
];

// ---- silent-mode entry / exit ----
const silentCases: Array<{ id: string; name: string }> = [{ id: 'ada', name: 'Ada' }];

// ---- join scenario ----
const joinCases: Array<{ id: string; name: string; scenario: string; user: string | null }> = [
  {
    id: 'template-user',
    name: 'Ada',
    scenario: 'You, {{char}}, arrived to find {{user}} waiting.  ',
    user: 'Charlie',
  },
  { id: 'no-user', name: 'Ada', scenario: 'You simply appeared.', user: null },
];

// ---- timestamp ----
// Turn-pass ("nothing to add", b90cd1f5) builder cases.
const turnPassCases: Array<{ id: string; name: string }> = [
  { id: 'plain', name: 'Ada' },
  { id: 'unicode', name: 'Zoë' },
];

// Nudge ("invited to speak", 6a8a77aa) builder cases.
const nudgeCases: Array<{ id: string; name: string }> = [
  { id: 'plain', name: 'Ada' },
  { id: 'unicode', name: 'Zoë' },
];

const timestampCases: Array<{ id: string; formatted: string }> = [
  { id: 'plain', formatted: 'Monday, the 3rd of never' },
];

async function main(): Promise<void> {
  const out: string[] = [];
  const emit = (kind: string, id: string, value: unknown) =>
    out.push(JSON.stringify({ kind, id, value }));

  for (const c of addCases) {
    const character = { name: c.name, description: c.description } as never;
    emit('add_content', c.id, await buildAddContent(character, c.user));
    emit('add_opaque', c.id, await buildAddOpaqueContent(character, c.user));
  }
  for (const c of removeCases) {
    emit('remove_content', c.id, buildRemoveContent(c.name));
    emit('remove_opaque', c.id, buildRemoveOpaqueContent(c.name));
  }
  for (const c of statusCases) {
    emit('status_content', c.id, buildStatusChangeContent(c.name, c.from as never, c.to as never));
    emit(
      'status_opaque',
      c.id,
      buildStatusChangeOpaqueContent(c.name, c.from as never, c.to as never),
    );
  }
  for (const c of scenarioCases) {
    emit('scenario_content', c.id, buildScenarioContent(c.text));
    emit('scenario_opaque', c.id, buildScenarioOpaqueContent(c.text));
  }
  for (const c of userCharCases) {
    emit('user_char_content', c.id, buildUserCharacterContent(c.name, c.description));
    emit('user_char_opaque', c.id, buildUserCharacterOpaqueContent(c.name, c.description));
  }
  for (const c of rosterCases) {
    emit('roster_content', c.id, buildMultiCharacterRosterContent(c.responding, c.others as never));
    emit(
      'roster_opaque',
      c.id,
      buildMultiCharacterRosterOpaqueContent(c.responding, c.others as never),
    );
  }
  for (const c of silentCases) {
    emit('silent_entry_content', c.id, buildSilentModeEntryContent(c.name));
    emit('silent_entry_opaque', c.id, buildSilentModeEntryOpaqueContent(c.name));
    emit('silent_exit_content', c.id, buildSilentModeExitContent(c.name));
    emit('silent_exit_opaque', c.id, buildSilentModeExitOpaqueContent(c.name));
  }
  for (const c of joinCases) {
    emit('join_content', c.id, buildJoinScenarioContent(c.name, c.scenario, c.user));
    emit('join_opaque', c.id, buildJoinScenarioOpaqueContent(c.name, c.scenario, c.user));
  }
  for (const c of turnPassCases) {
    emit('turn_pass_content', c.id, buildTurnPassContent(c.name));
    emit('user_turn_pass_content', c.id, buildUserTurnPassContent(c.name));
    emit('turn_pass_opaque', c.id, buildTurnPassOpaqueContent(c.name));
  }
  for (const c of nudgeCases) {
    emit('nudge_content', c.id, buildNudgeContent(c.name));
    emit('nudge_opaque', c.id, buildNudgeOpaqueContent(c.name));
  }
  for (const c of timestampCases) {
    emit('timestamp_content', c.id, buildTimestampContent(c.formatted));
    emit('timestamp_opaque', c.id, buildTimestampOpaqueContent(c.formatted));
  }

  for (const line of out) process.stdout.write(line + '\n');
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
