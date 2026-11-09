/**
 * The v4-side recorder behind
 * `src/app/document-browser/document-browser.oracle.spec.ts` (P4.D387 item 3).
 *
 * v4 `c34986421` replaced the Scriptorium's file table and the SVAR file
 * manager with ONE document-store browser on the shared folder browser. Its
 * pure half is client-safe and lives in four modules this recorder drives at
 * the pin, REAL code over a fixed corpus:
 *
 * - `lib/folder-browser/preview-kind.ts` — `previewKindOf`, `mimeTypeForName`;
 * - `lib/folder-browser/document-actions.ts` — `capabilitiesOf`,
 *   `canLinkBetween`, `isCustomToolDefinition`, `hasDescription`,
 *   `documentFileActions`, `documentFolderActions`, `documentPreviewKind`;
 * - `lib/mount-index/capabilities.ts` — `deriveMountCapabilities`;
 * - `components/files/FilePreview/types.ts` — `getPreviewType`;
 * - `components/document-browser/useDocumentStoreSource.ts` — `folderTreeOf`
 *   (the module imports `@tanstack/react-query` and React; both load under
 *   tsx from the pin's own `node_modules`, and nothing renders).
 *
 * The seed is every case of v4's own `__tests__/unit/lib/folder-browser/
 * document-actions.test.ts`; the grid around it is P4.D387 item 3's.
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/…` modules
 * and would not compile in the SPA's own tsconfig.
 *
 * Run it from a pinned v4 worktree at `624d0d25c` (Node 24 at
 * `~/.nvm/versions/node/v24.13.1/bin`):
 *
 * ```bash
 * PIN=${PIN:?a detached v4 worktree at 624d0d25c}   # e.g. /tmp/qt-v4-pin-p4.d387-624d0d25c
 * V5W=${V5W:-$HOME/source/quilltap-v5}
 * cp "$V5W/apps/web/oracle/document-browser.recorder.ts" "$PIN/"
 * OUT="$V5W/apps/web/src/testing/fixtures/document-browser.oracle.ndjson"
 * rm -f "$OUT"
 * (cd "$PIN" && PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH TZ=UTC \
 *   npx tsx document-browser.recorder.ts > "$OUT")
 * rm -f "$PIN/document-browser.recorder.ts"
 * wc -l "$OUT"                                  # expect 422
 * grep -c '"fn":"documentFileActions"' "$OUT"   # expect 256
 * ```
 *
 * Expect 422 lines (86 `previewKindOf` + 9 `mimeTypeForName` + 12
 * `getPreviewType` + 4 `documentPreviewKind` + 256 `documentFileActions` + 5
 * `documentFolderActions` + 18 `canLinkBetween` + 5 `canLinkGates` + 7
 * `isCustomToolDefinition` + 6 `hasDescription` + 3 `capabilitiesOf` + 8
 * `deriveMountCapabilities` + 3 `folderTreeOf`); a shorter file means the recorder errored and the redirect
 * already truncated the old one (the empty-file trap).
 */

import { mimeTypeForName, previewKindOf, type PreviewSubject } from '@/lib/folder-browser/preview-kind';
import {
  canLinkBetween,
  capabilitiesOf,
  documentFileActions,
  documentFolderActions,
  documentPreviewKind,
  hasDescription,
  isCustomToolDefinition,
  type BrowserFile,
  type BrowserStore,
  type StoreCapabilities,
} from '@/lib/folder-browser/document-actions';
import { deriveMountCapabilities } from '@/lib/mount-index/capabilities';
import { getPreviewType } from '@/components/files/FilePreview/types';
import { folderTreeOf, type DocumentEntry } from '@/components/document-browser/useDocumentStoreSource';

const emit = (fn: string, id: string, args: unknown[], out: unknown): void => {
  console.log(JSON.stringify({ fn, id, args, out }));
};

// ---- previewKindOf — the seed + the grid -------------------------------------

const KIND_ROWS: Array<[string, PreviewSubject]> = [
  // v4 test `prefers the MIME type, then the stored type, then the extension`
  ['seed mime image beats txt name', { mimeType: 'image/webp', name: 'x.txt' }],
  ['seed fileType docx', { fileType: 'docx', name: 'report' }],
  ['seed blob + PDF ext', { fileType: 'blob', name: 'scan.PDF' }],
  ['seed fileType markdown', { fileType: 'markdown', name: 'notes' }],
  ['seed json ext', { name: 'data.json' }],
  ['seed zip unsupported', { name: 'archive.zip' }],
  // `ignores an octet-stream MIME type and falls back to plain-text detection`
  ['seed octet + png', { mimeType: 'application/octet-stream', name: 'photo.png' }],
  ['seed zip mime + plain', { mimeType: 'application/zip', isPlainText: true }],
];
const MIMES = [
  'image/png',
  'IMAGE/JPEG',
  'image/svg+xml',
  'application/pdf',
  'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  'text/markdown',
  'text/x-markdown',
  'application/markdown',
  'text/plain',
  'text/csv',
  'text/html',
  'application/json',
  'application/javascript',
  'application/typescript',
  'application/xml',
  'application/x-ndjson',
  'application/yaml',
  'application/zip',
  'application/octet-stream',
  'video/mp4',
  '',
];
for (const m of MIMES) KIND_ROWS.push([`mime ${m || '(empty)'}`, { mimeType: m }]);
const FILE_TYPES = ['markdown', 'txt', 'json', 'jsonl', 'pdf', 'docx', 'blob', 'image', 'unknown'];
for (const t of FILE_TYPES) KIND_ROWS.push([`fileType ${t}`, { fileType: t }]);
const NAMES = [
  'a.png',
  'A.PNG',
  'b.jpeg',
  'c.svg',
  'd.avif',
  'e.bmp',
  'f.pdf',
  'g.docx',
  'h.md',
  'i.markdown',
  'j.txt',
  'k.json',
  'l.jsonl',
  'm.yaml',
  'n.yml',
  'o.xml',
  'p.csv',
  'q.html',
  'r.css',
  's.js',
  't.ts',
  'u.zip',
  'noext',
  '.env',
  'a/b/c.MD',
  'dir.d/file',
  'trailing.',
];
for (const n of NAMES) KIND_ROWS.push([`name ${n}`, { name: n }]);
for (const n of ['photo.png', 'scan.pdf', 'notes.md', 'data.json', 'archive.zip', 'noext']) {
  KIND_ROWS.push([`octet + ${n}`, { mimeType: 'application/octet-stream', name: n }]);
}
for (const p of [true, false]) {
  KIND_ROWS.push([`plain ${p} no name`, { isPlainText: p }]);
  KIND_ROWS.push([`plain ${p} zip`, { name: 'x.zip', isPlainText: p }]);
  KIND_ROWS.push([`plain ${p} octet`, { mimeType: 'application/octet-stream', isPlainText: p }]);
}
KIND_ROWS.push(['null mime', { mimeType: null, name: 'z.md' }]);
KIND_ROWS.push(['null name', { name: null }]);
KIND_ROWS.push(['empty', {}]);
KIND_ROWS.push(['mime beats fileType', { mimeType: 'text/plain', fileType: 'pdf', name: 'x.pdf' }]);
KIND_ROWS.push(['fileType beats ext', { fileType: 'txt', name: 'x.png' }]);
KIND_ROWS.push(['unknown mime falls to fileType', { mimeType: 'application/zip', fileType: 'pdf' }]);
KIND_ROWS.push(['unknown mime falls to ext', { mimeType: 'video/mp4', name: 'x.md' }]);
KIND_ROWS.push(['blob + ext', { fileType: 'blob', name: 'x.docx' }]);
KIND_ROWS.push(['mime upper markdown', { mimeType: 'TEXT/MARKDOWN' }]);
for (const [id, subject] of KIND_ROWS) emit('previewKindOf', id, [subject], previewKindOf(subject));

// ---- mimeTypeForName ----------------------------------------------------------

for (const n of ['a/b/c.MD', 'noext', '.env', 'x.JPG', 'x.tar.gz', 'dir.d/file', '', 'x.jsonl', 'trailing.']) {
  emit('mimeTypeForName', n || '(empty)', [n], mimeTypeForName(n));
}

// ---- getPreviewType -----------------------------------------------------------

const PREVIEW_ROWS: Array<[string, string, boolean | undefined]> = [
  ['seed markdown', 'text/markdown', undefined],
  ['seed docx', 'application/vnd.openxmlformats-officedocument.wordprocessingml.document', undefined],
  ['seed pdf', 'application/pdf', undefined],
  ['seed empty plain', '', true],
  ['png', 'image/png', undefined],
  ['json', 'application/json', undefined],
  ['xml', 'application/xml', undefined],
  ['ndjson', 'application/x-ndjson', undefined],
  ['yaml', 'application/yaml', undefined],
  ['x-markdown', 'text/x-markdown', undefined],
  ['octet', 'application/octet-stream', undefined],
  ['zip plain', 'application/zip', true],
];
for (const [id, mime, plain] of PREVIEW_ROWS) {
  const args: unknown[] = plain === undefined ? [mime] : [mime, plain];
  emit('getPreviewType', id, args, getPreviewType(mime, plain));
}

// ---- documentPreviewKind ------------------------------------------------------

const DPK: Array<[string, BrowserFile]> = [
  ['seed blob png mime', { relativePath: 'a.bin', fileName: 'a.bin', fileType: 'blob', originalMimeType: 'image/png' }],
  ['null mime pdf', { relativePath: 'x/y.pdf', fileName: 'y.pdf', fileType: 'pdf', originalMimeType: null }],
  ['markdown', { relativePath: 'notes.md', fileName: 'notes.md', fileType: 'markdown' }],
  ['blob no mime zip', { relativePath: 'a.zip', fileName: 'a.zip', fileType: 'blob' }],
];
for (const [id, f] of DPK) emit('documentPreviewKind', id, [f], documentPreviewKind(f));

// ---- documentFileActions — the grid --------------------------------------------

const ALL: StoreCapabilities = { canWrite: true, canDelete: true, canCreateFolder: true, canMoveIn: true, canMoveOut: true };
const NONE: StoreCapabilities = { canWrite: false, canDelete: false, canCreateFolder: false, canMoveIn: false, canMoveOut: false };
const WRITE_ONLY: StoreCapabilities = { ...NONE, canWrite: true, canDelete: true };
const MOVE_OUT_ONLY: StoreCapabilities = { ...NONE, canMoveOut: true };
const CAPS: Array<[string, StoreCapabilities]> = [
  ['ALL', ALL],
  ['NONE', NONE],
  ['writeOnly', WRITE_ONLY],
  ['moveOutOnly', MOVE_OUT_ONLY],
];
const FILE_KINDS: Array<[string, string, 'database' | 'filesystem']> = [
  ['blob db', 'blob', 'database'],
  ['pdf db', 'pdf', 'database'],
  ['docx fs', 'docx', 'filesystem'],
  ['markdown db', 'markdown', 'database'],
];
const PATHS = ['Tools/x.tool.json', 'tools/X.TOOL.JSON', 'Tools/sub/x.tool.json', 'notes.md'];
for (const [capName, caps] of CAPS) {
  const store: BrowserStore = { id: 's1', mountType: 'database', capabilities: caps };
  for (const linkGroupId of [null, 'g1']) {
    for (const [kindName, fileType, source] of FILE_KINDS) {
      for (const linkTargets of [true, false]) {
        for (const relativePath of PATHS) {
          const file: BrowserFile = {
            relativePath,
            fileName: relativePath.split('/').pop() ?? relativePath,
            fileType,
            source,
            linkGroupId,
            linkCount: linkGroupId ? 2 : 1,
          };
          const opts = { linkTargets };
          emit(
            'documentFileActions',
            `${capName} link=${linkGroupId ?? 'null'} ${kindName} targets=${linkTargets} ${relativePath}`,
            [store, file, opts],
            documentFileActions(store, file, opts),
          );
        }
      }
    }
  }
}

// ---- documentFolderActions ------------------------------------------------------

for (const [capName, caps] of CAPS) {
  const store: BrowserStore = { id: 's1', mountType: 'database', capabilities: caps };
  emit('documentFolderActions', capName, [store], documentFolderActions(store));
}
emit('documentFolderActions', 'no capabilities', [{ id: 's1', mountType: 'database' }], documentFolderActions({ id: 's1', mountType: 'database' }));

// ---- canLinkBetween ---------------------------------------------------------------

const TYPES: Array<BrowserStore['mountType']> = ['database', 'filesystem', 'obsidian'];
for (const a of TYPES) {
  for (const b of TYPES) {
    for (const same of [true, false]) {
      const src: BrowserStore = { id: 'a', mountType: a, capabilities: ALL };
      const dst: BrowserStore = { id: same ? 'a' : 'b', mountType: b, capabilities: ALL };
      emit('canLinkBetween', `${a}->${b} ${same ? 'same' : 'other'}`, [src, dst], canLinkBetween(src, dst));
    }
  }
}
// The capability gates, the seed's `NONE` destination among them.
const GATES: Array<[string, BrowserStore, BrowserStore]> = [
  ['src cannot move out', { id: 'a', mountType: 'database', capabilities: { ...ALL, canMoveOut: false } }, { id: 'b', mountType: 'database', capabilities: ALL }],
  ['dst cannot move in', { id: 'a', mountType: 'database', capabilities: ALL }, { id: 'b', mountType: 'database', capabilities: { ...ALL, canMoveIn: false } }],
  ['dst NONE (seed)', { id: 'a', mountType: 'database', capabilities: ALL }, { id: 'b', mountType: 'database', capabilities: NONE }],
  ['no capabilities', { id: 'a', mountType: 'database' }, { id: 'b', mountType: 'database' }],
  ['fs same store, dst null caps', { id: 'a', mountType: 'filesystem', capabilities: ALL }, { id: 'a', mountType: 'filesystem', capabilities: null }],
];
for (const [id, s, d] of GATES) emit('canLinkGates', id, [s, d], canLinkBetween(s, d));

// ---- isCustomToolDefinition / hasDescription / capabilitiesOf ----------------------

for (const p of ['Tools/dice.tool.json', 'tools/X.TOOL.JSON', 'Tools/sub/x.tool.json', 'xTools/a.tool.json', 'Tools/.tool.json', 'Tools/a.tool.jsonx', 'notes.md']) {
  emit('isCustomToolDefinition', p, [p], isCustomToolDefinition(p));
}
const DESC: Array<[string, BrowserFile]> = [
  ['db blob', { relativePath: 'a', fileName: 'a', fileType: 'blob', source: 'database' }],
  ['db pdf', { relativePath: 'a', fileName: 'a', fileType: 'pdf', source: 'database' }],
  ['db docx', { relativePath: 'a', fileName: 'a', fileType: 'docx', source: 'database' }],
  ['db markdown', { relativePath: 'a', fileName: 'a', fileType: 'markdown', source: 'database' }],
  ['fs pdf', { relativePath: 'a', fileName: 'a', fileType: 'pdf', source: 'filesystem' }],
  ['no source blob', { relativePath: 'a', fileName: 'a', fileType: 'blob' }],
];
for (const [id, f] of DESC) emit('hasDescription', id, [f], hasDescription(f));
emit('capabilitiesOf', 'null', [null], capabilitiesOf(null));
emit('capabilitiesOf', 'no caps', [{ id: 'a', mountType: 'database' }], capabilitiesOf({ id: 'a', mountType: 'database' }));
emit('capabilitiesOf', 'with caps', [{ id: 'a', mountType: 'database', capabilities: WRITE_ONLY }], capabilitiesOf({ id: 'a', mountType: 'database', capabilities: WRITE_ONLY }));

// ---- deriveMountCapabilities --------------------------------------------------------

const MP_ROWS: Array<[string, { enabled: boolean; conversionStatus: string; scanStatus: string }]> = [];
for (const enabled of [true, false]) {
  for (const conversionStatus of ['idle', 'converting', 'deconverting', 'error']) {
    MP_ROWS.push([`enabled=${enabled} ${conversionStatus}`, { enabled, conversionStatus, scanStatus: conversionStatus === 'error' ? 'scanning' : 'idle' }]);
  }
}
for (const [id, mp] of MP_ROWS) {
  // The function reads only enabled / conversionStatus / scanStatus.
  emit('deriveMountCapabilities', id, [mp], deriveMountCapabilities(mp as never));
}

// ---- folderTreeOf ---------------------------------------------------------------------

const entry = (folder: string): DocumentEntry => ({ folder } as DocumentEntry);
const TREES: Array<[string, string[], string[]]> = [
  ['empty', [], []],
  ['case-differing folders', ['Drafts', 'drafts/Old', 'Notes'], ['drafts', 'DRAFTS', 'Drafts', 'drafts/old', '', 'Other']],
  ['folders with none', ['A', 'B/C'], ['']],
];
for (const [id, folders, entryFolders] of TREES) {
  emit('folderTreeOf', id, [folders, entryFolders], folderTreeOf(folders, entryFolders.map(entry)));
}
