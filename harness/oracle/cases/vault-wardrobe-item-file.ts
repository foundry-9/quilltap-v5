/**
 * Tier-1 oracle case — the vault `Wardrobe/*.md` parser.
 *
 * Drives v4's REAL parseWardrobeItemFile
 * (lib/database/repositories/vault-overlay/parsers.ts) over synthetic
 * DocMountDocument-like inputs, emitting the parsed WardrobeItem or null. The Rust
 * port (quilltap_core::vault_overlay::parse_wardrobe_item_file) must match exactly:
 * the title fallback chain, the required `types` (parseWardrobeTypesField), the
 * id sanity check (/^[0-9a-f-]{36}$/i else stableUuidFromString), the
 * non-empty-string / boolean-flag / archived field logic, the raw componentItems,
 * the frontmatter-vs-doc timestamp precedence, and the body→description rule.
 *
 * P4.D279 (v4 `4a38222e6`, wardrobe folders): every parsed item carries `folder`
 * — DERIVED from the file's path (`Wardrobe/Formal/Evening/Gown.md` →
 * `Formal/Evening`, a root file → `''`); a frontmatter `folder:` key is ignored
 * (the path wins). `isWardrobeItemDocumentPath` is RECURSIVE (any `.md` at any
 * depth under `Wardrobe/`, except under the first-level reserved `images/` and
 * except `instructions.md` at any depth), and two NEW exports ride the same
 * rows: `isWardrobeItemDocument` (item path + a valid `types` list) and
 * `hasStableWardrobeItemId` (a UUID-shaped frontmatter `id`).
 *
 * Run from the v4 server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-wardrobe-item-file.ts \
 *     > /tmp/oracle-vault-wardrobe-item-file.ndjson
 */

import {
  hasStableWardrobeItemId,
  isWardrobeItemDocument,
  isWardrobeItemDocumentPath,
  parseWardrobeItemFile,
  resolveWardrobeItemId,
  wardrobeItemIdForDocument,
} from '@/lib/database/repositories/vault-overlay/parsers';

type Doc = {
  content: string;
  mountPointId: string;
  relativePath: string;
  fileName: string;
  createdAt: string;
  updatedAt: string;
};
type Row = { id: string; doc: Doc; out: unknown };

const rows: Row[] = [];
const CID = 'char-1';
const T1 = '2024-01-01T00:00:00.000Z';
const T2 = '2024-01-02T00:00:00.000Z';
const UUID = '123e4567-e89b-12d3-a456-426614174000';

function wCase(id: string, content: string, fileName = `${id}.md`, folder = '') {
  const doc: Doc = {
    content,
    mountPointId: 'mp-1',
    relativePath: folder ? `Wardrobe/${folder}/${fileName}` : `Wardrobe/${fileName}`,
    fileName,
    createdAt: T1,
    updatedAt: T2,
  };
  rows.push({ id, doc, out: parseWardrobeItemFile(doc as never, CID) ?? null });
}

// ── full / rich ──────────────────────────────────────────────────────────────
wCase(
  'w-full',
  `---\nid: ${UUID}\ntitle: Greatcoat\ntypes:\n  - top\n  - bottom\nappropriateness: formal\nimagePrompt: a burnished coat\ndefault: true\nreplace: true\ncomponentItems: [slug-a, ${UUID}]\nmigratedFromClothingRecordId: legacy-123\narchivedAt: ${T1}\ncreatedAt: ${T1}\nupdatedAt: ${T2}\n---\nA long woolen coat.`,
);

// ── title fallbacks ──────────────────────────────────────────────────────────
wCase('w-title-from-heading', '---\ntypes: [top]\n---\n# The Heading\n\nbody text');
wCase('w-title-from-filename', '---\ntypes: [top]\n---\njust body', 'leather-boots.md');
wCase('w-fm-title-keeps-heading', '---\ntitle: FM Title\ntypes: [top]\n---\n# Inner\n\nbody');

// ── skip conditions ──────────────────────────────────────────────────────────
wCase('w-no-types', '---\ntitle: Hat\n---\nbody');
wCase('w-no-frontmatter', 'just a markdown body with no frontmatter');
wCase('w-types-bad-enum', '---\ntitle: Hat\ntypes: [hat]\n---\nbody');
wCase('w-empty-title-and-filename', '---\ntypes: [top]\n---\nbody', '.md');

// ── id handling ──────────────────────────────────────────────────────────────
wCase('w-id-from-frontmatter', `---\nid: ${UUID}\ntitle: X\ntypes: [top]\n---\nb`);
wCase('w-id-invalid-stable', '---\nid: not-an-id\ntitle: X\ntypes: [top]\n---\nb');
wCase('w-id-36-nonhex-stable', '---\nid: zzzzzzzz-zzzz-zzzz-zzzz-zzzzzzzzzzzz\ntitle: X\ntypes: [top]\n---\nb');

// ── flags / fields ───────────────────────────────────────────────────────────
wCase('w-archived-true', '---\ntitle: X\ntypes: [top]\narchived: true\n---\nb');
wCase('w-archivedAt-string', `---\ntitle: X\ntypes: [top]\narchivedAt: ${T1}\n---\nb`);
wCase('w-isDefault-flag', '---\ntitle: X\ntypes: [top]\nisDefault: true\n---\nb');
wCase('w-appropriateness-empty', '---\ntitle: X\ntypes: [top]\nappropriateness: ""\n---\nb');
wCase('w-migrated-empty-string', '---\ntitle: X\ntypes: [top]\nmigratedFromClothingRecordId: ""\n---\nb');
wCase('w-component-items', '---\ntitle: X\ntypes: [top]\ncomponentItems: [a, b, c]\n---\nb');
wCase('w-timestamps-from-fm', `---\ntitle: X\ntypes: [top]\ncreatedAt: 2020-05-05T00:00:00.000Z\nupdatedAt: 2021-06-06T00:00:00.000Z\n---\nb`);
wCase('w-empty-body-desc-null', '---\ntitle: X\ntypes: [top]\n---\n');
wCase('w-multibyte', '---\ntitle: Côat\ntypes: [top]\n---\nBödy déscription');

// ── P4.D255 (v4 `7c8572869`): `imageFileId` — a NON-EMPTY string, else null;
// always emitted (parsers.ts:376-379, 415) ─────────────────────────────────
wCase('w-image-file-id', `---\ntitle: X\ntypes: [top]\nimageFileId: ${UUID}\n---\nb`);
wCase('w-image-file-id-empty', '---\ntitle: X\ntypes: [top]\nimageFileId: ""\n---\nb');
wCase('w-image-file-id-number', '---\ntitle: X\ntypes: [top]\nimageFileId: 7\n---\nb');
wCase('w-image-file-id-null', '---\ntitle: X\ntypes: [top]\nimageFileId: null\n---\nb');

// ── P4.D279 (v4 `4a38222e6`): `folder` is the file's PATH under `Wardrobe/`
// (`folderOfWardrobePath`, parsers.ts:428); a frontmatter `folder:` key is
// ignored with a DEBUG line (the path wins); a root file reads `''` ───────────
wCase('w-folder-nested', `---\nid: ${UUID}\ntitle: Gown\ntypes: [top]\n---\nAn evening gown.`, 'Gown.md', 'Formal/Evening');
wCase('w-folder-one-deep-no-id', '---\ntitle: Blazer\ntypes: [top]\n---\nb', 'Blazer.md', 'Formal');
wCase('w-folder-fm-key-ignored', '---\ntitle: Boater\ntypes: [accessories]\nfolder: Hats\n---\nb', 'Boater.md', 'Formal');
wCase('w-folder-fm-key-at-root', '---\ntitle: Boater\ntypes: [accessories]\nfolder: Hats\n---\nb');
wCase('w-folder-fm-key-nonstring', '---\ntitle: Boater\ntypes: [accessories]\nfolder: 7\n---\nb', 'Boater.md', 'Formal/Evening');
wCase('w-folder-root', '---\ntitle: Root Coat\ntypes: [top]\n---\nb');
wCase('w-folder-nested-no-types-null', '---\ntitle: Note\n---\nnot an item', 'todo.md', 'Notes');

for (const row of rows) {
  process.stdout.write(JSON.stringify(row) + '\n');
}

// ── P4.D255 (v4 `3ee3b1342`): the three parser extractions the carriers share,
// through their REAL exports ──────────────────────────────────────────────────
const resolveRows: Array<[string, unknown]> = [
  ['uuid-36', UUID],
  ['upper-hex-36', UUID.toUpperCase()],
  ['dashes-36', '------------------------------------'],
  ['short-35', UUID.slice(0, 35)],
  ['long-37', `${UUID}0`],
  ['number', 12345],
  ['absent', undefined],
  ['null', null],
];
for (const [id, frontmatterId] of resolveRows) {
  process.stdout.write(
    JSON.stringify({
      kind: 'resolve',
      id,
      frontmatterId: typeof frontmatterId === 'string' ? frontmatterId : null,
      out: resolveWardrobeItemId(frontmatterId, 'mp-1', 'Wardrobe/Coat.md'),
    }) + '\n',
  );
}
for (const path of [
  'Wardrobe/Coat.md',
  'wardrobe/coat.MD',
  'WARDROBE/Coat.md',
  'Wardrobe/sub/Coat.md',
  'Wardrobe/instructions.md',
  'Wardrobe/INSTRUCTIONS.md',
  'Wardrobe/Instructions.MD',
  'Wardrobe/',
  'Wardrobe/.md',
  'Wardrobe/Coat.txt',
  'Wardrobe/Coat.md.bak',
  'Other/Coat.md',
  'Wardrobe',
  'Wardrobes/Coat.md',
  'Wardrobe/images/x/pic.webp',
  // P4.D279 (`4a38222e6`): the predicate is RECURSIVE — subfolders hold items;
  // only the FIRST-level `images/` is reserved; `instructions.md` is excluded at
  // any depth; an empty segment (`//`) is never an item.
  'Wardrobe/Formal/Evening/Gown.md',
  'Wardrobe/images/x/note.md',
  'Wardrobe/images/note.md',
  'Wardrobe/IMAGES/x/note.md',
  'Wardrobe/images.md',
  'Wardrobe/A/images/x.md',
  'Wardrobe/A/instructions.md',
  'Wardrobe/A/B/Instructions.MD',
  'Wardrobe//x.md',
  'Wardrobe/A//x.md',
  'Wardrobe/A/',
  'Wardrobe/A/.md',
  'wardrobe/a/B.MD',
]) {
  process.stdout.write(
    JSON.stringify({ kind: 'docpath', id: path, path, out: isWardrobeItemDocumentPath(path) }) + '\n',
  );
}
for (const [id, content] of [
  ['fm-uuid', `---\nid: ${UUID}\ntitle: X\n---\nb`],
  ['fm-bad', '---\nid: not-an-id\n---\nb'],
  ['fm-number', '---\nid: 123\n---\nb'],
  ['no-fm', 'just a body'],
  ['empty', ''],
] as const) {
  process.stdout.write(
    JSON.stringify({
      kind: 'docid',
      id,
      mountPointId: 'mp-2',
      relativePath: 'Wardrobe/Hat.md',
      content,
      out: wardrobeItemIdForDocument({ mountPointId: 'mp-2', relativePath: 'Wardrobe/Hat.md', content }),
    }) + '\n',
  );
}

// ── P4.D279 (v4 `4a38222e6`): the two NEW parser exports the projection's
// `ownsFile` sweep and the folder rename's id stamping call ──────────────────
const isDocRows: Array<[string, string, string | null | undefined]> = [
  ['item-root', 'Wardrobe/Coat.md', `---\ntitle: Coat\ntypes: [top]\n---\nb`],
  ['item-nested', 'Wardrobe/Formal/Evening/Gown.md', `---\ntypes:\n  - top\n  - bottom\n---\n# Gown`],
  ['item-types-only', 'Wardrobe/Coat.md', '---\ntypes: [accessories]\n---\n'],
  ['note-no-types', 'Wardrobe/Notes/todo.md', '---\ntitle: Todo\n---\nbuy buttons'],
  ['note-no-frontmatter', 'Wardrobe/Notes/todo.md', 'just a body'],
  ['types-bad-enum', 'Wardrobe/Coat.md', '---\ntypes: [hat]\n---\nb'],
  ['types-empty', 'Wardrobe/Coat.md', '---\ntypes: []\n---\nb'],
  ['types-mixed-invalid', 'Wardrobe/Coat.md', '---\ntypes: [top, 7]\n---\nb'],
  ['types-scalar', 'Wardrobe/Coat.md', '---\ntypes: top\n---\nb'],
  ['types-dup-ok', 'Wardrobe/Coat.md', '---\ntypes: [top, top]\n---\nb'],
  ['valid-types-under-images', 'Wardrobe/images/x/note.md', '---\ntypes: [top]\n---\nb'],
  ['valid-types-instructions', 'Wardrobe/Formal/instructions.md', '---\ntypes: [top]\n---\nb'],
  ['valid-types-outside', 'Other/Coat.md', '---\ntypes: [top]\n---\nb'],
  ['valid-types-not-md', 'Wardrobe/Coat.txt', '---\ntypes: [top]\n---\nb'],
  ['content-null', 'Wardrobe/Coat.md', null],
  ['content-undefined', 'Wardrobe/Coat.md', undefined],
  ['content-empty', 'Wardrobe/Coat.md', ''],
];
for (const [id, relativePath, content] of isDocRows) {
  process.stdout.write(
    JSON.stringify({
      kind: 'isdoc',
      id,
      relativePath,
      content: content ?? null,
      out: isWardrobeItemDocument(content === undefined ? { relativePath } : { relativePath, content }),
    }) + '\n',
  );
}
const stableRows: Array<[string, string | null | undefined]> = [
  ['uuid', `---\nid: ${UUID}\ntitle: X\n---\nb`],
  ['uuid-upper', `---\nid: ${UUID.toUpperCase()}\n---\nb`],
  ['uuid-quoted', `---\nid: "${UUID}"\n---\nb`],
  ['dashes-36', '---\nid: ------------------------------------\n---\nb'],
  ['nonhex-36', '---\nid: zzzzzzzz-zzzz-zzzz-zzzz-zzzzzzzzzzzz\n---\nb'],
  ['short-35', `---\nid: ${UUID.slice(0, 35)}\n---\nb`],
  ['long-37', `---\nid: ${UUID}0\n---\nb`],
  ['number', '---\nid: 123\n---\nb'],
  ['null-id', '---\nid: null\n---\nb'],
  ['no-id', '---\ntitle: X\ntypes: [top]\n---\nb'],
  ['no-frontmatter', 'just a body'],
  ['empty', ''],
  ['content-null', null],
  ['content-undefined', undefined],
];
for (const [id, content] of stableRows) {
  process.stdout.write(
    JSON.stringify({
      kind: 'stableid',
      id,
      content: content ?? null,
      out: hasStableWardrobeItemId(content),
    }) + '\n',
  );
}
