/**
 * Oracle case: the `ZodError.message` bytes v4's repositories log and throw
 * when a row fails its schema (P4.130) — `base.repository.ts` `validate` logs
 * ERROR `Data validation failed {collection, error: extractErrorMessage(e)}`,
 * `validateSafe` WARN `Safe validation failed {collection, error:
 * e.message}`, and a restore/import catch carries the same string in its
 * warning. Zod 4's `ZodError.message` is `JSON.stringify(issues, null, 2)`.
 *
 * Drives v4's REAL schemas with `safeParse` — the three the port validates
 * rows against:
 *
 * - `GroupSchema` (`lib/schemas/group.types.ts`; `groups.repository.ts`
 *   constructs its repository with it — NOT `GroupRowSchema`, though on a
 *   v4-written row the five store-resident extension keys read `undefined`, so
 *   outcome and issue order are the row schema's);
 * - `GroupDocMountLinkSchema` (`lib/schemas/mount-index.types.ts`);
 * - `ChatMetadataBaseSchema` (`lib/schemas/chat.types.ts`; `chats.repository.ts`
 *   — NOT `ChatMetadataSchema`), over a minimal valid chat plus a patch of the
 *   three Concierge enum columns.
 *
 * Per row it emits `{id, schema, row, ok: true}` or `{id, schema, row, message,
 * issues}`. A BLOB cell is only expressible here: better-sqlite3 hands it
 * back as a `Buffer`, but v4's SQLite collection hydrates a `Buffer` in a
 * non-BLOB, non-JSON column with `blobToEmbedding` (`backend.ts`), so Zod
 * meets a `Float32Array` — `parsedType` answers `obj.constructor.name`, and
 * zod still runs a string's length checks over the array's `.length`
 * (measured, P4.130; the order had predicted `received Buffer`). The corpus
 * writes `{"$float32": n}`, this side turns it into `new Float32Array(n)`, the
 * Rust side into its marker. An ABSENT key
 * is JS `undefined` (v4 reads a NULL cell as `undefined`); an explicit `null`
 * is `null`.
 *
 * v5's twins: `quilltap_core::api::zod_issues::{zod_group_issues,
 * zod_group_doc_mount_link_issues}` and `quilltap_core::services::
 * dangerous_content::chat_override::concierge_columns_zod_error`, diffed
 * byte-exact through `zod_error_message` by
 * `crates/quilltap-harness/tests/repository_zod_messages_equivalence.rs`.
 *
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   rm -f /tmp/oracle-repository-zod-messages.ndjson
 *   cd ~/source/quilltap-server
 *   $N/npx tsx $V5W/harness/oracle/cases/repository-zod-messages.ts \
 *     > /tmp/oracle-repository-zod-messages.ndjson
 */

import { GroupSchema } from '@/lib/schemas/group.types';
import { GroupDocMountLinkSchema } from '@/lib/schemas/mount-index.types';
import { ChatMetadataBaseSchema } from '@/lib/schemas/chat.types';

type Row = Record<string, unknown>;

const TS = '2026-01-02T03:04:05.000Z';
const GROUP: Row = {
  id: 'd2310000-0000-4000-8000-0000000000c1',
  name: 'Loners',
  officialMountPointId: 'e2000000-0000-4000-8000-0000000000f3',
  createdAt: TS,
  updatedAt: TS,
};
const LINK: Row = {
  id: 'f2310000-0000-4000-8000-0000000000d1',
  groupId: 'd2310000-0000-4000-8000-0000000000c1',
  mountPointId: 'e2000000-0000-4000-8000-0000000000f3',
  createdAt: TS,
  updatedAt: TS,
};
const CHAT: Row = {
  id: 'c1000000-0000-4000-8000-0000000000e7',
  userId: 'a1000000-0000-4000-8000-000000000001',
  title: 'The Refusal Room',
  createdAt: TS,
  updatedAt: TS,
};

const ABSENT = '<absent>';
const patch = (base: Row, p: Row): Row => {
  const r: Row = { ...base };
  for (const [k, v] of Object.entries(p)) {
    if (v === ABSENT) delete r[k];
    else r[k] = v;
  }
  return r;
};
const f32 = (n: number) => ({ $float32: n });

const rows: Array<[string, 'group' | 'groupDocMountLink' | 'chatMetadataBase', Row]> = [];
const group = (id: string, p: Row) => rows.push([id, 'group', patch(GROUP, p)]);
const link = (id: string, p: Row) => rows.push([id, 'groupDocMountLink', patch(LINK, p)]);
const chat = (id: string, p: Row) => rows.push([id, 'chatMetadataBase', patch(CHAT, p)]);

// --- GroupSchema -------------------------------------------------------------
group('group-valid', {});
group('group-official-absent', { officialMountPointId: ABSENT });
group('group-official-null', { officialMountPointId: null });
group('group-name-100', { name: 'x'.repeat(100) });
group('group-name-101', { name: 'x'.repeat(101) });
group('group-name-99x-astral', { name: 'x'.repeat(99) + '\u{1F600}' });
group('group-name-100x-astral', { name: 'x'.repeat(100) + '\u{1F600}' });
group('group-name-50-astral', { name: '\u{1F600}'.repeat(50) });
group('group-name-empty', { name: '' });
group('group-name-float32-1', { name: f32(1) });
group('group-name-float32-0', { name: f32(0) });
group('group-name-float32-100', { name: f32(100) });
group('group-name-float32-101', { name: f32(101) });
group('group-name-absent', { name: ABSENT });
group('group-name-null', { name: null });
group('group-name-number', { name: 7 });
group('group-id-v0', { id: 'd2310000-0000-0000-8000-0000000000c5' });
group('group-id-absent', { id: ABSENT });
group('group-id-float32', { id: f32(16) });
group('group-official-v0', { officialMountPointId: 'e2000000-0000-0000-8000-0000000000f3' });
group('group-official-number', { officialMountPointId: 7 });
group('group-official-float32', { officialMountPointId: f32(4) });
group('group-created-date-only', { createdAt: '2026-01-02' });
group('group-created-no-seconds', { createdAt: '2026-01-02T03:04Z' });
group('group-created-offset', { createdAt: '2026-01-02T03:04:05+01:00' });
group('group-created-absent', { createdAt: ABSENT });
group('group-created-null', { createdAt: null });
group('group-created-number', { createdAt: 1767323045000 });
group('group-created-float32', { createdAt: f32(2) });
group('group-updated-absent', { updatedAt: ABSENT });
group('group-both-stamps-absent', { createdAt: ABSENT, updatedAt: ABSENT });
group('group-multi', { id: 'nope', name: '', officialMountPointId: 7, updatedAt: '2026' });

// --- GroupDocMountLinkSchema ---------------------------------------------------
link('link-valid', {});
link('link-v0-mp', { mountPointId: 'e2000000-0000-0000-8000-0000000000f3' });
link('link-bad-ts', { createdAt: '2026-01-02' });
link('link-missing-ts', { updatedAt: ABSENT });
link('link-mp-absent', { mountPointId: ABSENT });
link('link-mp-null', { mountPointId: null });
link('link-mp-float32', { mountPointId: f32(16) });
link('link-group-absent', { groupId: ABSENT });
link('link-id-number', { id: 42 });
link('link-multi', { id: 'x', mountPointId: null, createdAt: '2026', updatedAt: ABSENT });

// --- ChatMetadataBaseSchema — the five messages P4.124 pinned by hand --------
chat('chat-valid', {});
chat('chat-concierge-null', { conciergeMode: null, conciergeModeSetBy: null });
chat('chat-concierge-valid', {
  conciergeMode: 'locked',
  conciergeModeSetBy: 'operator',
  conciergeModeReason: 'migration',
});
chat('chat-mode-bogus', { conciergeMode: 'bogus' });
chat('chat-mode-number', { conciergeMode: 5 });
chat('chat-setby-nobody', { conciergeModeSetBy: 'nobody' });
chat('chat-reason-whim', { conciergeModeReason: 'whim' });
chat('chat-all-three', {
  conciergeMode: 'bogus',
  conciergeModeSetBy: 'nobody',
  conciergeModeReason: 'whim',
});
chat('chat-reason-number', { conciergeModeReason: 5 });

const SCHEMAS = {
  group: GroupSchema,
  groupDocMountLink: GroupDocMountLinkSchema,
  chatMetadataBase: ChatMetadataBaseSchema,
} as const;

const materialize = (r: Row): Row => {
  const out: Row = {};
  for (const [k, v] of Object.entries(r)) {
    const marker = v as { $float32?: unknown } | null;
    out[k] =
      marker && typeof marker === 'object' && typeof marker.$float32 === 'number'
        ? new Float32Array(marker.$float32)
        : v;
  }
  return out;
};

for (const [id, schema, row] of rows) {
  const r = SCHEMAS[schema].safeParse(materialize(row));
  if (r.success) {
    process.stdout.write(JSON.stringify({ id, schema, row, ok: true }) + '\n');
  } else {
    process.stdout.write(
      JSON.stringify({ id, schema, row, message: r.error.message, issues: r.error.issues }) + '\n',
    );
  }
}
