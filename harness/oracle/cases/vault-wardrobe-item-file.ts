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
 * Run from the v4 server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/vault-wardrobe-item-file.ts \
 *     > /tmp/oracle-vault-wardrobe-item-file.ndjson
 */

import { parseWardrobeItemFile } from '@/lib/database/repositories/vault-overlay/parsers';

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

function wCase(id: string, content: string, fileName = `${id}.md`) {
  const doc: Doc = {
    content,
    mountPointId: 'mp-1',
    relativePath: `Wardrobe/${fileName}`,
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

for (const row of rows) {
  process.stdout.write(JSON.stringify(row) + '\n');
}
