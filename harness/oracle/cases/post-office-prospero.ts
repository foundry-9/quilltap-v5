/**
 * Tier-1 differential oracle for the W4.6b Prospero notifications PURE content
 * builders — driving v4's REAL exports so the byte-exact visible + opaque bodies
 * are proven against the Rust port (`services::prospero_notifications`).
 *
 * Pure-only: no DB. Every function here is a pure v4 export. The `post*`
 * functions' `chat_messages` rows are covered by the central combined tier-3 the
 * parent owns. The DB loaders (`loadProspero*`) are stateful and out of scope for
 * this differential.
 *
 * Runs under `npx tsx` (no DB, no jest). Emits NDJSON to stdout.
 *
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5/harness/oracle/cases/post-office-prospero.ts \
 *       > /tmp/oracle-po-prospero.ndjson
 */

import {
  buildConnectionProfileChangeContent,
  buildConnectionProfileChangeOpaqueContent,
  buildGeneralContextContent,
  buildGeneralContextOpaqueContent,
  buildCombinedContextContent,
  buildCombinedContextOpaqueContent,
  buildGroupAndVaultWhisperContent,
  buildGroupAndVaultWhisperOpaqueContent,
  buildCarinaErrorContent,
  buildCarinaErrorOpaqueContent,
  type ProsperoProjectContext,
  type ProsperoGeneralContext,
  type ProsperoDocumentStoreInfo,
  type CarinaErrorKind,
} from '@/lib/services/prospero-notifications/writer';

// ---- shared fixtures (mirrored on the Rust side by (kind,id)) ----

const GENERAL: ProsperoGeneralContext = {
  mountPointId: 'gen-mp-1',
  name: 'Quilltap General',
  mountType: 'database',
};
const GENERAL_BACKTICK: ProsperoGeneralContext = {
  mountPointId: 'gen-mp-2',
  name: 'My `Shelf`',
  mountType: 'database',
};

function store(
  id: string,
  name: string,
  mountType: ProsperoDocumentStoreInfo['mountType'],
  storeType: ProsperoDocumentStoreInfo['storeType'],
  isOfficial: boolean,
  enabled: boolean,
): ProsperoDocumentStoreInfo {
  return { id, name, mountType, storeType, isOfficial, enabled };
}

// ---- connection-profile change ----
const connProfileCases: Array<{
  id: string;
  characterName: string;
  old: string | null;
  next: string | null;
}> = [
  { id: 'both', characterName: 'Ada', old: 'Claude', next: 'GPT' },
  { id: 'old-null', characterName: 'Ada', old: null, next: 'GPT' },
  { id: 'new-null', characterName: 'Bea', old: 'Claude', next: null },
  { id: 'both-null', characterName: 'Cy', old: null, next: null },
  { id: 'empty-strings', characterName: 'Dot', old: '', next: '' },
];

// ---- general context ----
const generalCases: Array<{ id: string; general: ProsperoGeneralContext }> = [
  { id: 'plain', general: GENERAL },
  { id: 'backtick-name', general: GENERAL_BACKTICK },
];

// ---- combined context ----
const projFull: ProsperoProjectContext = {
  name: 'Novel',
  description: '  A grand tale.  ',
  documentStores: [
    store('mp-off', 'Official Shelf', 'database', 'documents', true, true),
    store('mp-a', 'Alpha', 'filesystem', 'documents', false, true),
    store('mp-b', 'Beta', 'obsidian', 'character', false, false),
  ],
};
const projDescOnly: ProsperoProjectContext = {
  name: 'DescOnly',
  description: 'Only a description.',
  documentStores: [],
};
// P4.D103 (v4 `8f868109`): `instructions` left `ProsperoProjectContext`
// entirely — the standing-instructions section carries it every turn now, so
// re-whispering it here would duplicate it. This case KEEPS its name as the
// tripwire: a project whose only former content was instructions now has NO
// content at all, so the whisper must come back EMPTY.
const projInstrOnly: ProsperoProjectContext = {
  name: 'InstrOnly',
  description: '   ',
  documentStores: [],
};
const projStoresOnly: ProsperoProjectContext = {
  name: 'StoresOnly',
  description: null,
  documentStores: [store('mp-x', 'Xeno', 'database', 'documents', false, true)],
};
const projAmbiguous: ProsperoProjectContext = {
  name: 'Ambiguous',
  description: 'x',
  documentStores: [
    store('mp-d1', 'Dup', 'database', 'documents', false, true),
    store('mp-d2', 'Dup', 'filesystem', 'documents', false, true),
  ],
};
const projEmpty: ProsperoProjectContext = {
  name: 'Empty',
  description: '   ',
  documentStores: [],
};

const combinedCases: Array<{
  id: string;
  project: ProsperoProjectContext | null;
  general: ProsperoGeneralContext | null;
}> = [
  { id: 'null-project-with-general', project: null, general: GENERAL },
  { id: 'full-with-general', project: projFull, general: GENERAL },
  { id: 'full-no-general', project: projFull, general: null },
  { id: 'desc-only-general', project: projDescOnly, general: GENERAL },
  { id: 'instr-only-no-general', project: projInstrOnly, general: null },
  { id: 'stores-only-general', project: projStoresOnly, general: GENERAL },
  { id: 'ambiguous-names', project: projAmbiguous, general: null },
  { id: 'empty-project-with-general', project: projEmpty, general: GENERAL },
  { id: 'empty-project-no-general', project: projEmpty, general: null },
];

// ---- group + vault whisper ----
const g1 = store('grp-1', 'Team Shelf', 'database', 'documents', false, true);
const g2 = store('grp-2', 'Beta Group', 'filesystem', 'documents', false, false);
const gChar = store('grp-3', 'Char Store', 'obsidian', 'character', false, true);
const vault = store('vlt-1', 'Ada Vault', 'database', 'character', false, true);
const vaultDisabled = store('vlt-2', 'Bea `Vault`', 'filesystem', 'character', false, false);

const groupVaultCases: Array<{
  id: string;
  groups: ProsperoDocumentStoreInfo[];
  vault: ProsperoDocumentStoreInfo | null;
}> = [
  { id: 'neither', groups: [], vault: null },
  { id: 'groups-only', groups: [g1, g2], vault: null },
  { id: 'vault-only', groups: [], vault },
  { id: 'both', groups: [g1, gChar], vault },
  { id: 'both-disabled-vault', groups: [g2], vault: vaultDisabled },
];

// ---- carina error ----
const carinaCases: Array<{
  id: string;
  kind: CarinaErrorKind;
  name: string | null;
  detail: string | null | undefined;
}> = [
  { id: 'not-found', kind: 'not-found', name: null, detail: undefined },
  { id: 'no-profile-named', kind: 'no-profile', name: 'Ada', detail: undefined },
  { id: 'no-profile-unnamed', kind: 'no-profile', name: null, detail: undefined },
  { id: 'no-profile-empty-name', kind: 'no-profile', name: '', detail: undefined },
  { id: 'llm-failed-full', kind: 'llm-failed', name: 'Bea', detail: 'rate limit' },
  { id: 'llm-failed-no-detail', kind: 'llm-failed', name: 'Bea', detail: null },
  { id: 'llm-failed-no-name', kind: 'llm-failed', name: null, detail: 'timeout' },
  { id: 'llm-failed-empty-detail', kind: 'llm-failed', name: 'Cy', detail: '' },
];

function main(): void {
  const out: string[] = [];
  const emit = (kind: string, id: string, value: unknown) =>
    out.push(JSON.stringify({ kind, id, value }));

  for (const c of connProfileCases) {
    emit('conn_content', c.id, buildConnectionProfileChangeContent(c.characterName, c.old, c.next));
    emit(
      'conn_opaque',
      c.id,
      buildConnectionProfileChangeOpaqueContent(c.characterName, c.old, c.next),
    );
  }
  for (const c of generalCases) {
    emit('general_content', c.id, buildGeneralContextContent(c.general));
    emit('general_opaque', c.id, buildGeneralContextOpaqueContent(c.general));
  }
  for (const c of combinedCases) {
    emit('combined_content', c.id, buildCombinedContextContent(c.project, c.general));
    emit('combined_opaque', c.id, buildCombinedContextOpaqueContent(c.project, c.general));
  }
  for (const c of groupVaultCases) {
    emit('group_content', c.id, buildGroupAndVaultWhisperContent(c.groups, c.vault));
    emit('group_opaque', c.id, buildGroupAndVaultWhisperOpaqueContent(c.groups, c.vault));
  }
  for (const c of carinaCases) {
    emit('carina_content', c.id, buildCarinaErrorContent(c.kind, c.name, c.detail));
    emit('carina_opaque', c.id, buildCarinaErrorOpaqueContent(c.kind, c.name, c.detail));
  }

  for (const line of out) process.stdout.write(line + '\n');
}

main();
