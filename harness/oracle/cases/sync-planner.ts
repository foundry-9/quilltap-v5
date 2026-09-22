/**
 * Tier-1 oracle case — the document-store sync PLANNER (v4
 * `lib/mount-index/sync/planner.ts`, added at `23da0b322`).
 *
 * Drives v4's REAL `planSync` over a fixed corpus and emits one NDJSON row per
 * case carrying BOTH the input and the output, so the Rust port
 * (`quilltap_core::services::mount_index::sync::planner::plan_sync`) replays
 * exactly what v4 was given and its actions + warnings are diffed field by
 * field. The planner is pure — two entry maps, a base, the options, and two
 * booleans — so the whole decision table is expressible as data.
 *
 * The corpus covers every group of v4's own `planner.test.ts` (eleven describe
 * blocks, ~60 cases) plus the shapes that file leaves implicit and a port can
 * silently get wrong: the ORDER of a `create` and the `describe` pushed right
 * after it (same path, same side — a tie the stable sort must not reorder), the
 * `localeCompare` tie-break at one depth, `--direction` on a describe, and a
 * `linkGroupId` whose members sit in a non-alphabetical store order.
 *
 * Run from the v4 server checkout:
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   npx tsx $V5W/harness/oracle/cases/sync-planner.ts > /tmp/oracle-sync-planner.ndjson
 */

import { planSync } from '@/lib/mount-index/sync/planner';
import { descriptionSha256 } from '@/lib/mount-index/sync/sidecar';
import type {
  ManifestEntry,
  SyncEntry,
  SyncEntryMap,
  SyncOptions,
} from '@/lib/mount-index/sync/types';

const T0 = '2026-09-01T10:00:00.000Z';
const T1 = '2026-09-10T10:00:00.000Z';
const T2 = '2026-09-20T10:00:00.000Z';
const BIRTH_OLD = '2024-01-01T00:00:00.000Z';

const SHA_A = 'a'.repeat(64);
const SHA_B = 'b'.repeat(64);
const SHA_C = 'c'.repeat(64);

const GROUP = 'group-0000-1111-2222';

function opts(over: Partial<SyncOptions> = {}): SyncOptions {
  return {
    targetPath: '/tmp/target',
    dryRun: false,
    direction: 'both',
    prefer: 'newer',
    propagateDeletes: true,
    useManifest: true,
    ...over,
  };
}

function file(relativePath: string, over: Partial<SyncEntry> = {}): SyncEntry {
  return {
    relativePath,
    kind: 'file',
    sha256: SHA_A,
    sizeBytes: 100,
    lastModified: T1,
    createdAt: BIRTH_OLD,
    fileType: 'markdown',
    ...over,
  };
}

function folder(relativePath: string, over: Partial<SyncEntry> = {}): SyncEntry {
  return { relativePath, kind: 'folder', lastModified: T1, createdAt: BIRTH_OLD, ...over };
}

const img = (over: Partial<SyncEntry> = {}) =>
  file('lore/harbour.png', { fileType: 'blob', sha256: SHA_A, ...over });

interface Case {
  id: string;
  store: SyncEntry[];
  disk: SyncEntry[];
  base?: Record<string, ManifestEntry>;
  options?: SyncOptions;
  isCharacterVault?: boolean;
  canSetDiskBirthtime?: boolean;
}

const capBase = (text: string): Record<string, ManifestEntry> => ({
  'lore/harbour.png': { kind: 'file', sha256: SHA_A, descriptionSha256: descriptionSha256(text) },
});

const CASES: Case[] = [
  // ---- one side only, no manifest (first run) ----------------------------
  { id: 'first-run/materialises-store-file-on-disk', store: [file('chapters/01.md')], disk: [] },
  { id: 'first-run/materialises-disk-file-in-store', store: [], disk: [file('drafts/new.md')] },
  { id: 'first-run/never-deletes-either-direction', store: [file('a.md')], disk: [file('b.md')] },
  { id: 'first-run/creates-an-empty-folder', store: [folder('lore/maps')], disk: [] },

  // ---- one side only, with a manifest ------------------------------------
  {
    id: 'base/propagates-a-disk-deletion-to-the-store',
    store: [file('notes.md')], disk: [],
    base: { 'notes.md': { kind: 'file', sha256: SHA_A, lastModified: T1 } },
  },
  {
    id: 'base/propagates-a-store-deletion-to-disk',
    store: [], disk: [file('notes.md')],
    base: { 'notes.md': { kind: 'file', sha256: SHA_A, lastModified: T1 } },
  },
  {
    id: 'base/refuses-when-the-survivor-was-edited',
    store: [file('notes.md', { sha256: SHA_B })], disk: [],
    base: { 'notes.md': { kind: 'file', sha256: SHA_A, lastModified: T1 } },
  },
  {
    id: 'base/refuses-when-the-disk-survivor-was-edited',
    store: [], disk: [file('notes.md', { sha256: SHA_B })],
    base: { 'notes.md': { kind: 'file', sha256: SHA_A, lastModified: T1 } },
  },
  {
    id: 'base/no-delete-downgrades-to-a-skip',
    store: [file('notes.md')], disk: [],
    base: { 'notes.md': { kind: 'file', sha256: SHA_A, lastModified: T1 } },
    options: opts({ propagateDeletes: false }),
  },
  {
    id: 'base/no-delete-downgrades-the-disk-side-too',
    store: [], disk: [file('notes.md')],
    base: { 'notes.md': { kind: 'file', sha256: SHA_A, lastModified: T1 } },
    options: opts({ propagateDeletes: false }),
  },
  {
    id: 'base/forgets-an-entry-gone-from-both-sides',
    store: [], disk: [],
    base: { 'notes.md': { kind: 'file', sha256: SHA_A, lastModified: T1 } },
  },
  {
    id: 'base/propagates-an-empty-folder-deletion-as-rmdir',
    store: [folder('drafts')], disk: [],
    base: { drafts: { kind: 'folder' } },
  },

  // ---- both sides, different bytes ---------------------------------------
  {
    id: 'bytes/newer-side-wins-when-only-one-moved',
    store: [file('ch.md', { sha256: SHA_A, lastModified: T1 })],
    disk: [file('ch.md', { sha256: SHA_B, lastModified: T2 })],
    base: { 'ch.md': { kind: 'file', sha256: SHA_A } },
  },
  {
    id: 'bytes/refuses-when-both-moved-since-the-base',
    store: [file('ch.md', { sha256: SHA_B, lastModified: T2 })],
    disk: [file('ch.md', { sha256: SHA_C, lastModified: T1 })],
    base: { 'ch.md': { kind: 'file', sha256: SHA_A } },
  },
  {
    id: 'bytes/prefer-disk-ignores-the-clocks',
    store: [file('ch.md', { sha256: SHA_B, lastModified: T2 })],
    disk: [file('ch.md', { sha256: SHA_C, lastModified: T0 })],
    base: { 'ch.md': { kind: 'file', sha256: SHA_A } },
    options: opts({ prefer: 'disk' }),
  },
  {
    id: 'bytes/prefer-store-resolves-the-other-way',
    store: [file('ch.md', { sha256: SHA_B, lastModified: T0 })],
    disk: [file('ch.md', { sha256: SHA_C, lastModified: T2 })],
    base: { 'ch.md': { kind: 'file', sha256: SHA_A } },
    options: opts({ prefer: 'store' }),
  },
  {
    id: 'bytes/falls-back-to-the-clocks-on-a-first-run',
    store: [file('ch.md', { sha256: SHA_A, lastModified: T2 })],
    disk: [file('ch.md', { sha256: SHA_B, lastModified: T0 })],
  },
  {
    id: 'bytes/refuses-a-first-run-tie',
    store: [file('ch.md', { sha256: SHA_A, lastModified: T1 })],
    disk: [file('ch.md', { sha256: SHA_B, lastModified: T1 })],
  },
  {
    id: 'bytes/carries-the-older-createdAt',
    store: [file('ch.md', { sha256: SHA_A, lastModified: T2, createdAt: '2025-06-01T00:00:00.000Z' })],
    disk: [file('ch.md', { sha256: SHA_B, lastModified: T0, createdAt: BIRTH_OLD })],
  },
  {
    id: 'bytes/a-side-that-cannot-report-createdAt-never-wins',
    store: [file('ch.md', { sha256: SHA_A, lastModified: T2, createdAt: BIRTH_OLD })],
    disk: [file('ch.md', { sha256: SHA_B, lastModified: T0, createdAt: null })],
  },
  {
    id: 'bytes/a-pure-case-rename-follows-the-content-winner',
    store: [file('Notes.md', { sha256: SHA_A, lastModified: T0 })],
    disk: [file('notes.md', { sha256: SHA_B, lastModified: T2 })],
  },
  {
    // The stale-base arm v4's own suite never reaches: neither side matches the
    // base, and the two differ from each other.
    id: 'bytes/a-stale-base-neither-side-matches',
    store: [file('ch.md', { sha256: SHA_B, lastModified: T2 })],
    disk: [file('ch.md', { sha256: SHA_C, lastModified: T0 })],
    base: { 'ch.md': { kind: 'file', sha256: SHA_A } },
  },

  // ---- both sides, same bytes --------------------------------------------
  {
    id: 'clocks/touch-rather-than-copy',
    store: [file('ch.md', { lastModified: T0 })],
    disk: [file('ch.md', { lastModified: T2 })],
  },
  { id: 'clocks/nothing-when-both-agree', store: [file('ch.md')], disk: [file('ch.md')] },
  {
    id: 'clocks/tolerates-a-sub-second-difference',
    store: [file('ch.md', { lastModified: '2026-09-10T10:00:00.000Z' })],
    disk: [file('ch.md', { lastModified: '2026-09-10T10:00:00.900Z' })],
  },
  {
    id: 'clocks/does-not-tolerate-past-the-tolerance',
    store: [file('ch.md', { lastModified: '2026-09-10T10:00:00.000Z' })],
    disk: [file('ch.md', { lastModified: '2026-09-10T10:00:02.000Z' })],
  },
  {
    id: 'clocks/exactly-at-the-tolerance',
    store: [file('ch.md', { lastModified: '2026-09-10T10:00:00.000Z' })],
    disk: [file('ch.md', { lastModified: '2026-09-10T10:00:01.000Z' })],
  },
  {
    id: 'clocks/touches-the-disk-when-the-store-is-newer',
    store: [file('ch.md', { lastModified: T2 })],
    disk: [file('ch.md', { lastModified: T0 })],
  },
  {
    id: 'clocks/no-disk-touch-where-birthtime-is-unsettable',
    store: [file('ch.md', { createdAt: '2025-01-01T00:00:00.000Z' })],
    disk: [file('ch.md', { createdAt: BIRTH_OLD })],
    canSetDiskBirthtime: false,
  },
  {
    id: 'clocks/still-corrects-the-mtime-without-birthtime',
    store: [file('ch.md', { lastModified: T2, createdAt: BIRTH_OLD })],
    disk: [file('ch.md', { lastModified: T0, createdAt: BIRTH_OLD })],
    canSetDiskBirthtime: false,
  },
  {
    id: 'clocks/touches-both-sides-when-only-createdAt-disagrees',
    store: [file('ch.md', { createdAt: '2025-01-01T00:00:00.000Z' })],
    disk: [file('ch.md', { createdAt: BIRTH_OLD })],
  },
  {
    // The disk side reports no birthtime at all: `d.createdAt !== null` is the
    // guard, so the disk touch must NOT fire on the creation date alone.
    id: 'clocks/a-null-disk-birthtime-drives-no-touch',
    store: [file('ch.md', { createdAt: '2025-01-01T00:00:00.000Z' })],
    disk: [file('ch.md', { createdAt: null })],
  },
  {
    id: 'clocks/prefer-store-picks-the-store-clock-for-a-touch',
    store: [file('ch.md', { lastModified: T0 })],
    disk: [file('ch.md', { lastModified: T2 })],
    options: opts({ prefer: 'store' }),
  },

  // ---- kind disagreements -------------------------------------------------
  { id: 'kinds/file-vs-folder', store: [folder('lore')], disk: [file('lore')] },
  { id: 'kinds/folder-vs-file', store: [file('lore')], disk: [folder('lore')] },
  {
    id: 'kinds/a-folder-on-both-sides-is-left-alone',
    store: [folder('lore', { lastModified: T0 })],
    disk: [folder('lore', { lastModified: T2 })],
  },

  // ---- descriptions -------------------------------------------------------
  {
    id: 'caption/pushes-a-store-caption-out-as-a-sidecar',
    store: [img({ description: 'A map of the harbour.', descriptionUpdatedAt: T1 })],
    disk: [img()],
  },
  {
    id: 'caption/pulls-an-edited-sidecar-back-in',
    store: [img({ description: 'old caption' })],
    disk: [img({ description: 'a better caption', descriptionUpdatedAt: T2 })],
    base: capBase('old caption'),
  },
  {
    id: 'caption/refuses-when-both-sides-moved',
    store: [img({ description: 'the store’s new caption' })],
    disk: [img({ description: 'the sidecar’s new caption' })],
    base: capBase('old caption'),
  },
  {
    id: 'caption/a-deleted-sidecar-is-a-clearing',
    store: [img({ description: 'old caption' })],
    disk: [img()],
    base: capBase('old caption'),
  },
  {
    id: 'caption/no-delete-refuses-to-read-it-as-a-clearing',
    store: [img({ description: 'old caption' })],
    disk: [img()],
    base: capBase('old caption'),
    options: opts({ propagateDeletes: false }),
  },
  {
    id: 'caption/says-nothing-when-it-already-matches',
    store: [img({ description: 'same' })],
    disk: [img({ description: 'same' })],
  },
  {
    id: 'caption/ignores-trailing-whitespace',
    store: [img({ description: 'same' })],
    disk: [img({ description: 'same\n' })],
  },
  {
    id: 'caption/a-text-document-caption-is-reported-unsynced',
    store: [file('README.md', { description: 'a note about this file' })],
    disk: [file('README.md')],
  },
  {
    id: 'caption/a-text-document-with-no-caption-says-nothing',
    store: [file('README.md')],
    disk: [file('README.md')],
  },
  {
    // No base, both sides carry a caption and they differ: the sidecar clocks
    // decide, and a tie refuses. (v4's suite never reaches either arm.)
    id: 'caption/no-base-the-sidecar-clock-decides',
    store: [img({ description: 'store text', descriptionUpdatedAt: T0 })],
    disk: [img({ description: 'disk text', descriptionUpdatedAt: T2 })],
  },
  {
    id: 'caption/no-base-a-clock-tie-refuses',
    store: [img({ description: 'store text', descriptionUpdatedAt: T1 })],
    disk: [img({ description: 'disk text', descriptionUpdatedAt: T1 })],
  },
  {
    id: 'caption/no-base-the-store-clock-wins',
    store: [img({ description: 'store text', descriptionUpdatedAt: T2 })],
    disk: [img({ description: 'disk text', descriptionUpdatedAt: T0 })],
  },
  {
    id: 'caption/prefer-store-pushes-the-caption-out',
    store: [img({ description: 'store text', descriptionUpdatedAt: T0 })],
    disk: [img({ description: 'disk text', descriptionUpdatedAt: T2 })],
    options: opts({ prefer: 'store' }),
  },
  {
    id: 'caption/prefer-disk-pulls-the-caption-in',
    store: [img({ description: 'store text', descriptionUpdatedAt: T2 })],
    disk: [img({ description: 'disk text', descriptionUpdatedAt: T0 })],
    options: opts({ prefer: 'disk' }),
  },
  {
    // An empty store caption with no sidecar on disk: nothing to push and
    // nothing to clear.
    id: 'caption/an-empty-store-caption-with-no-sidecar',
    store: [img({ description: '' })],
    disk: [img()],
  },
  {
    // A sidecar beside a file the store does not have yet — the first-run
    // adoption's `describe store`, pushed right after the `create store` at the
    // SAME path and side. The stable sort must keep that order.
    id: 'caption/a-first-run-adoption-carries-its-sidecar-in',
    store: [],
    disk: [img({ description: 'from the sidecar', descriptionUpdatedAt: T2 })],
  },
  {
    // …and its mirror: a store-only blob's caption rides out with the create.
    id: 'caption/a-first-run-materialise-carries-its-caption-out',
    store: [img({ description: 'from the store', descriptionUpdatedAt: T2 })],
    disk: [],
  },
  {
    // A disk-only TEXT file with a sidecar: `isTextPath` suppresses the
    // describe, because a text document cannot carry one.
    id: 'caption/a-disk-only-text-file-carries-no-caption-in',
    store: [],
    disk: [file('notes.md', { description: 'from the sidecar' })],
  },
  {
    id: 'caption/direction-to-store-skips-the-outbound-describe',
    store: [img({ description: 'A map.', descriptionUpdatedAt: T1 })],
    disk: [img()],
    options: opts({ direction: 'to-store' }),
  },

  // ---- --direction --------------------------------------------------------
  {
    id: 'direction/to-disk-turns-store-work-into-skips',
    store: [file('a.md')], disk: [file('b.md')],
    options: opts({ direction: 'to-disk' }),
  },
  {
    id: 'direction/to-store-turns-disk-work-into-skips',
    store: [file('a.md')], disk: [file('b.md')],
    options: opts({ direction: 'to-store' }),
  },
  {
    id: 'direction/conflicts-stay-visible',
    store: [file('ch.md', { sha256: SHA_A, lastModified: T1 })],
    disk: [file('ch.md', { sha256: SHA_B, lastModified: T1 })],
    options: opts({ direction: 'to-disk' }),
  },
  {
    id: 'direction/to-disk-skips-a-store-deletion',
    store: [], disk: [file('notes.md')],
    base: { 'notes.md': { kind: 'file', sha256: SHA_A } },
    options: opts({ direction: 'to-store' }),
  },

  // ---- character vault keystones ------------------------------------------
  {
    id: 'vault/refuses-to-delete-a-keystone',
    store: [file('identity.md')], disk: [],
    base: { 'identity.md': { kind: 'file', sha256: SHA_A } },
    isCharacterVault: true,
  },
  {
    id: 'vault/deletes-an-ordinary-vault-file',
    store: [file('notes/idea.md')], disk: [],
    base: { 'notes/idea.md': { kind: 'file', sha256: SHA_A } },
    isCharacterVault: true,
  },
  {
    id: 'vault/no-keystone-rule-for-an-ordinary-store',
    store: [file('identity.md')], disk: [],
    base: { 'identity.md': { kind: 'file', sha256: SHA_A } },
    isCharacterVault: false,
  },
  {
    id: 'vault/the-keystone-check-is-case-insensitive',
    store: [file('Identity.MD')], disk: [],
    base: { 'identity.md': { kind: 'file', sha256: SHA_A } },
    isCharacterVault: true,
  },
  {
    id: 'vault/a-nested-keystone-path-counts',
    store: [file('wardrobe/instructions.md')], disk: [],
    base: { 'wardrobe/instructions.md': { kind: 'file', sha256: SHA_A } },
    isCharacterVault: true,
  },

  // ---- hard-link groups ---------------------------------------------------
  {
    id: 'group/refreshes-a-siblings-disk-copy',
    store: [
      file('a.md', { sha256: SHA_A, lastModified: T0, linkGroupId: GROUP, linkId: 'link-a' }),
      file('b.md', { sha256: SHA_A, lastModified: T0, linkGroupId: GROUP, linkId: 'link-b' }),
    ],
    disk: [
      file('a.md', { sha256: SHA_B, lastModified: T2 }),
      file('b.md', { sha256: SHA_A, lastModified: T0 }),
    ],
  },
  {
    id: 'group/refuses-two-members-edited-differently',
    store: [
      file('a.md', { sha256: SHA_A, lastModified: T0, linkGroupId: GROUP, linkId: 'link-a' }),
      file('b.md', { sha256: SHA_A, lastModified: T0, linkGroupId: GROUP, linkId: 'link-b' }),
    ],
    disk: [
      file('a.md', { sha256: SHA_B, lastModified: T2 }),
      file('b.md', { sha256: SHA_C, lastModified: T2 }),
    ],
  },
  {
    id: 'group/an-ungrouped-identical-pair-is-left-alone',
    store: [
      file('a.md', { sha256: SHA_A, lastModified: T0, linkId: 'link-a' }),
      file('b.md', { sha256: SHA_A, lastModified: T0, linkId: 'link-b' }),
    ],
    disk: [
      file('a.md', { sha256: SHA_B, lastModified: T2 }),
      file('b.md', { sha256: SHA_A, lastModified: T0 }),
    ],
  },
  {
    // The group's members are walked in a non-alphabetical store order and the
    // EDITED one is second, so the fan-out's `written[0]` is not the first
    // member — the shape a `members[0]` shortcut would get wrong.
    id: 'group/the-written-member-is-not-the-first',
    store: [
      file('z.md', { sha256: SHA_A, lastModified: T0, linkGroupId: GROUP, linkId: 'link-z' }),
      file('m.md', { sha256: SHA_A, lastModified: T0, linkGroupId: GROUP, linkId: 'link-m' }),
    ],
    disk: [
      file('z.md', { sha256: SHA_A, lastModified: T0 }),
      file('m.md', { sha256: SHA_B, lastModified: T2 }),
    ],
  },
  {
    // A third member with no disk copy at all: the fan-out skips it rather than
    // planning a modify against a path that is not there.
    id: 'group/a-sibling-absent-from-disk-is-skipped',
    store: [
      file('a.md', { sha256: SHA_A, lastModified: T0, linkGroupId: GROUP, linkId: 'link-a' }),
      file('b.md', { sha256: SHA_A, lastModified: T0, linkGroupId: GROUP, linkId: 'link-b' }),
      file('c.md', { sha256: SHA_A, lastModified: T0, linkGroupId: GROUP, linkId: 'link-c' }),
    ],
    disk: [
      file('a.md', { sha256: SHA_B, lastModified: T2 }),
      file('b.md', { sha256: SHA_A, lastModified: T0 }),
    ],
  },

  // ---- ordering -----------------------------------------------------------
  {
    id: 'order/parents-before-children-and-store-before-disk',
    store: [file('deep/nested/leaf.md')], disk: [file('other.md')],
  },
  { id: 'order/folders-before-files-at-a-depth', store: [folder('lore'), file('top.md')], disk: [] },
  {
    id: 'order/deletes-children-first-and-last',
    store: [folder('drafts'), file('drafts/old.md'), file('fresh.md')],
    disk: [file('fresh.md')],
    base: {
      drafts: { kind: 'folder' },
      'drafts/old.md': { kind: 'file', sha256: SHA_A },
      'new.md': { kind: 'file', sha256: SHA_A },
    },
  },
  {
    // Three siblings at one depth in a scrambled walk order: only
    // `localeCompare` puts them back, and a byte compare would too — so one of
    // them starts with an accented letter, where the two disagree.
    id: 'order/localeCompare-breaks-the-tie-at-one-depth',
    store: [file('zebra.md'), file('Ábaco.md'), file('apple.md'), file('Banana.md')],
    disk: [],
  },
  {
    // Deletions at mixed depths: children first, and a folder AFTER the files
    // beneath it (the negated sign flips the folder rule too).
    id: 'order/mixed-depth-deletions',
    store: [folder('a'), folder('a/b'), file('a/b/deep.md'), file('a/mid.md'), file('top.md')],
    disk: [],
    base: {
      a: { kind: 'folder' },
      'a/b': { kind: 'folder' },
      'a/b/deep.md': { kind: 'file', sha256: SHA_A },
      'a/mid.md': { kind: 'file', sha256: SHA_A },
      'top.md': { kind: 'file', sha256: SHA_A },
    },
  },
  {
    // A base-only key with no entry on either side, alongside live work: the
    // key-set order is store, then disk, then base.
    id: 'order/a-base-only-key-among-live-work',
    store: [file('s.md')],
    disk: [file('d.md')],
    base: { 'gone.md': { kind: 'file', sha256: SHA_A } },
  },
];

function mapOf(entries: SyncEntry[]): SyncEntryMap {
  return new Map(entries.map(e => [e.relativePath.toLowerCase(), e]));
}

function baseOf(record: Record<string, ManifestEntry>): Map<string, ManifestEntry> {
  return new Map(Object.entries(record).map(([k, v]) => [k.toLowerCase(), v]));
}

for (const c of CASES) {
  const options = c.options ?? opts();
  const isCharacterVault = c.isCharacterVault ?? false;
  const canSetDiskBirthtime = c.canSetDiskBirthtime ?? true;
  const base = c.base ?? {};
  const { actions, warnings } = planSync({
    store: mapOf(c.store),
    disk: mapOf(c.disk),
    base: baseOf(base),
    options,
    isCharacterVault,
    canSetDiskBirthtime,
  });
  process.stdout.write(
    JSON.stringify({
      id: c.id,
      input: { store: c.store, disk: c.disk, base, options, isCharacterVault, canSetDiskBirthtime },
      actions,
      warnings,
    }) + '\n'
  );
}
