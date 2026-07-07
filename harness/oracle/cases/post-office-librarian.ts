/**
 * Tier-1 differential oracle for the W4.6b Librarian-notifications PURE builders —
 * driving v4's REAL exported functions from
 * `lib/services/librarian-notifications/writer.ts` so every byte-exact
 * `build*Content` / `build*OpaqueContent` + `contentHiddenFromCharacters` is
 * proven against the Rust port ([`services::librarian_notifications`]).
 *
 * Pure-only: no DB. The `post*` chat_messages rows are covered by a central
 * combined tier-3 the parent owns. Runs under `npx tsx`; emits NDJSON to stdout.
 *
 * Regenerate:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/post-office-librarian.ts \
 *     > /tmp/oracle-po-librarian.ndjson
 */

import {
  contentHiddenFromCharacters,
  buildOpenContent,
  buildOpenOpaqueContent,
  buildRenameContent,
  buildRenameOpaqueContent,
  buildSaveContent,
  buildSaveOpaqueContent,
  buildDeleteContent,
  buildDeleteOpaqueContent,
  buildFolderCreatedContent,
  buildFolderCreatedOpaqueContent,
  buildFolderDeletedContent,
  buildFolderDeletedOpaqueContent,
  buildAttachContent,
  buildAttachOpaqueContent,
  buildWriteContent,
  buildWriteOpaqueContent,
  buildMoveContent,
  buildMoveOpaqueContent,
  buildCopyContent,
  buildCopyOpaqueContent,
  buildBlobWriteContent,
  buildBlobWriteOpaqueContent,
  buildUploadContent,
  buildUploadOpaqueContent,
  buildSummaryContent,
  buildSummaryOpaqueContent,
  SUMMARY_CONTENT_PREFIX,
  SUMMARY_OPAQUE_CONTENT_PREFIX,
} from '@/lib/services/librarian-notifications/writer';

type Scope = 'project' | 'document_store' | 'general';
const CHAT = 'chat-1';

// ---- contentHiddenFromCharacters ----
const hiddenCases: Array<{ id: string; content: string }> = [
  { id: 'no-frontmatter', content: 'Just a body, no frontmatter.' },
  { id: 'hidden-true', content: '---\ncharacter_read: false\n---\nsecret' },
  { id: 'hidden-false', content: '---\ncharacter_read: true\n---\nvisible' },
  { id: 'default-visible', content: '---\ntitle: Note\n---\nvisible' },
];

// ---- open ----
const openCases: Array<{ id: string; scope: Scope; mountPoint: string | null; isNew: boolean; origin: any }> = [
  { id: 'ds-existing-user', scope: 'document_store', mountPoint: 'Household', isNew: false, origin: { kind: 'opened-by-user' } },
  { id: 'ds-new-char', scope: 'document_store', mountPoint: 'Household', isNew: true, origin: { kind: 'opened-by-character', characterName: 'Ada' } },
  { id: 'project-existing', scope: 'project', mountPoint: null, isNew: false, origin: { kind: 'opened-by-user' } },
  { id: 'general-new', scope: 'general', mountPoint: null, isNew: true, origin: { kind: 'opened-by-character', characterName: 'Bea' } },
  { id: 'ds-self', scope: 'document_store', mountPoint: 'self', isNew: false, origin: { kind: 'opened-by-user' } },
  { id: 'ds-nomount', scope: 'document_store', mountPoint: null, isNew: false, origin: { kind: 'opened-by-user' } },
  { id: 'ds-reserved-name', scope: 'document_store', mountPoint: 'Project', isNew: false, origin: { kind: 'opened-by-user' } },
];
const openTitle = 'The Grand Ledger';
const openPath = 'Notes/a b.md';

// ---- rename ----
const renameCases: Array<{ id: string; scope: Scope; mountPoint: string | null }> = [
  { id: 'ds', scope: 'document_store', mountPoint: 'Household' },
  { id: 'project', scope: 'project', mountPoint: null },
  { id: 'ds-self', scope: 'document_store', mountPoint: 'self' },
];

// ---- save ----
const saveCases: Array<{ id: string; diff: string }> = [
  { id: 'matching-prefix', diff: 'I\'ve made changes to "notes.md":\n```diff\n- old\n+ new\n```' },
  { id: 'no-match-fallback', diff: 'Some diff without the expected prefix.' },
  { id: 'matching-multiline-title', diff: 'I\'ve made changes to "a report":\ndetails here' },
];

// ---- delete / folder ----
const actorCases: Array<{ id: string; scope: Scope; mountPoint: string | null; origin: any }> = [
  { id: 'ds-user', scope: 'document_store', mountPoint: 'Household', origin: { kind: 'by-user' } },
  { id: 'ds-char', scope: 'document_store', mountPoint: 'Household', origin: { kind: 'by-character', characterName: 'Ada' } },
  { id: 'project-char', scope: 'project', mountPoint: null, origin: { kind: 'by-character', characterName: 'Zed' } },
  { id: 'general-user', scope: 'general', mountPoint: null, origin: { kind: 'by-user' } },
];

// ---- attach ----
const attachCases: Array<{ id: string; mountPoint: string | null; mimeType: string; description?: string }> = [
  { id: 'image-desc', mountPoint: 'Album', mimeType: 'image/png', description: '  A sunset over the bay.  ' },
  { id: 'image-nodesc', mountPoint: 'Album', mimeType: 'IMAGE/JPEG' },
  { id: 'doc-nomount', mountPoint: null, mimeType: 'text/markdown', description: 'A memo.' },
  { id: 'image-emptydesc', mountPoint: 'Album', mimeType: 'image/webp', description: '   ' },
];

// ---- write ----
const bigBody = Array.from({ length: 200 }, (_, i) => `line ${i}`).join('\n');
const bigChars = 'x'.repeat(6200);
const writeCases: Array<{ id: string; scope: Scope; mountPoint: string | null; origin: any; change: any }> = [
  { id: 'created-body-user', scope: 'document_store', mountPoint: 'Household', origin: { kind: 'by-user' }, change: { kind: 'created', body: 'Hello world.' } },
  { id: 'created-empty-char', scope: 'project', mountPoint: null, origin: { kind: 'by-character', characterName: 'Ada' }, change: { kind: 'created', body: '   ' } },
  { id: 'edited-char', scope: 'document_store', mountPoint: 'self', origin: { kind: 'by-character', characterName: 'Bea' }, change: { kind: 'edited', diff: '- a\n+ b' } },
  { id: 'created-truncate-lines', scope: 'general', mountPoint: null, origin: { kind: 'by-user' }, change: { kind: 'created', body: bigBody } },
  { id: 'edited-truncate-chars', scope: 'document_store', mountPoint: 'Household', origin: { kind: 'by-user' }, change: { kind: 'edited', diff: bigChars } },
];
const writeUri = 'qtap://self/Notes/a.md';

// ---- move ----
const moveCases: Array<{ id: string; scope: Scope; mountPoint: string | null; origin: any; isFolder: boolean }> = [
  { id: 'file-user', scope: 'document_store', mountPoint: 'Household', origin: { kind: 'by-user' }, isFolder: false },
  { id: 'folder-char', scope: 'project', mountPoint: null, origin: { kind: 'by-character', characterName: 'Ada' }, isFolder: true },
];

// ---- copy ----
const copyCases: Array<{ id: string; origin: any }> = [
  { id: 'user', origin: { kind: 'by-user' } },
  { id: 'char', origin: { kind: 'by-character', characterName: 'Ada' } },
];

// ---- blob ----
const blobCases: Array<{ id: string; mountPoint: string | null; mimeType: string; sizeBytes: number; description?: string; origin: any }> = [
  { id: 'image-desc', mountPoint: 'Album', mimeType: 'image/png', sizeBytes: 12345, description: '  A red door.  ', origin: { kind: 'by-user' } },
  { id: 'asset-nodesc', mountPoint: null, mimeType: 'application/pdf', sizeBytes: 999, origin: { kind: 'by-character', characterName: 'Ada' } },
];

// ---- upload ----
const uploadCases: Array<{ id: string; uploads: Array<{ fileId: string; filename: string }> }> = [
  { id: 'empty', uploads: [] },
  { id: 'single', uploads: [{ fileId: 'f-1', filename: 'sunset.png' }] },
  { id: 'plural', uploads: [{ fileId: 'f-1', filename: 'a.png' }, { fileId: 'f-2', filename: 'b.jpg' }] },
];

// ---- summary ----
const summaryCases: Array<{ id: string; summary: string }> = [
  { id: 'plain', summary: 'They agreed to meet at dawn.' },
  { id: 'whitespace', summary: '   Trimmed body.   \n' },
];

function main(): void {
  const out: string[] = [];
  const emit = (kind: string, id: string, value: unknown) => out.push(JSON.stringify({ kind, id, value }));

  for (const c of hiddenCases) emit('content_hidden', c.id, contentHiddenFromCharacters(c.content));

  for (const c of openCases) {
    const p: any = { chatId: CHAT, displayTitle: openTitle, filePath: openPath, scope: c.scope, mountPoint: c.mountPoint, isNew: c.isNew, origin: c.origin };
    emit('open_content', c.id, buildOpenContent(p));
    emit('open_opaque', c.id, buildOpenOpaqueContent(p));
  }

  for (const c of renameCases) {
    const p: any = { chatId: CHAT, oldDisplayTitle: 'Old Name', newDisplayTitle: 'New Name', oldFilePath: 'Notes/old.md', newFilePath: 'Notes/new.md', scope: c.scope, mountPoint: c.mountPoint };
    emit('rename_content', c.id, buildRenameContent(p));
    emit('rename_opaque', c.id, buildRenameOpaqueContent(p));
  }

  for (const c of saveCases) {
    emit('save_content', c.id, buildSaveContent(c.diff));
    emit('save_opaque', c.id, buildSaveOpaqueContent(c.diff));
  }

  for (const c of actorCases) {
    const del: any = { chatId: CHAT, displayTitle: 'The Ledger', filePath: 'Notes/x.md', scope: c.scope, mountPoint: c.mountPoint, origin: c.origin };
    emit('delete_content', c.id, buildDeleteContent(del));
    emit('delete_opaque', c.id, buildDeleteOpaqueContent(del));
    const fc: any = { chatId: CHAT, folderPath: 'Archive/2026', scope: c.scope, mountPoint: c.mountPoint, origin: c.origin };
    emit('folder_created_content', c.id, buildFolderCreatedContent(fc));
    emit('folder_created_opaque', c.id, buildFolderCreatedOpaqueContent(fc));
    emit('folder_deleted_content', c.id, buildFolderDeletedContent(fc));
    emit('folder_deleted_opaque', c.id, buildFolderDeletedOpaqueContent(fc));
  }

  for (const c of attachCases) {
    const p: any = { chatId: CHAT, displayTitle: 'Portrait', filePath: 'photos/p.png', mountPoint: c.mountPoint, mountFileId: 'link-1', mimeType: c.mimeType, description: c.description };
    emit('attach_content', c.id, buildAttachContent(p));
    emit('attach_opaque', c.id, buildAttachOpaqueContent(p));
  }

  for (const c of writeCases) {
    const p: any = { chatId: CHAT, displayTitle: 'Report', uri: writeUri, scope: c.scope, mountPoint: c.mountPoint, origin: c.origin, change: c.change };
    emit('write_content', c.id, buildWriteContent(p));
    emit('write_opaque', c.id, buildWriteOpaqueContent(p));
  }

  for (const c of moveCases) {
    const p: any = { chatId: CHAT, oldDisplayTitle: 'Before', newDisplayTitle: 'After', oldUri: 'qtap://self/a.md', newUri: 'qtap://self/b.md', scope: c.scope, mountPoint: c.mountPoint, origin: c.origin, isFolder: c.isFolder };
    emit('move_content', c.id, buildMoveContent(p));
    emit('move_opaque', c.id, buildMoveOpaqueContent(p));
  }

  for (const c of copyCases) {
    const p: any = { chatId: CHAT, sourceDisplayTitle: 'Original', destDisplayTitle: 'Duplicate', sourceMountPoint: 'Household', destMountPoint: 'Archive', sourceUri: 'qtap://Household/a.md', destUri: 'qtap://Archive/a.md', origin: c.origin };
    emit('copy_content', c.id, buildCopyContent(p));
    emit('copy_opaque', c.id, buildCopyOpaqueContent(p));
  }

  for (const c of blobCases) {
    const p: any = { chatId: CHAT, displayTitle: 'Asset', uri: 'qtap://Album/img.png', mountPoint: c.mountPoint, mimeType: c.mimeType, sizeBytes: c.sizeBytes, description: c.description, origin: c.origin };
    emit('blob_content', c.id, buildBlobWriteContent(p));
    emit('blob_opaque', c.id, buildBlobWriteOpaqueContent(p));
  }

  for (const c of uploadCases) {
    const p: any = { chatId: CHAT, uploads: c.uploads };
    emit('upload_content', c.id, buildUploadContent(p));
    emit('upload_opaque', c.id, buildUploadOpaqueContent(p));
  }

  for (const c of summaryCases) {
    emit('summary_content', c.id, buildSummaryContent(c.summary));
    emit('summary_opaque', c.id, buildSummaryOpaqueContent(c.summary));
  }

  emit('const', 'summary_content_prefix', SUMMARY_CONTENT_PREFIX);
  emit('const', 'summary_opaque_content_prefix', SUMMARY_OPAQUE_CONTENT_PREFIX);

  for (const line of out) process.stdout.write(line + '\n');
}

main();
