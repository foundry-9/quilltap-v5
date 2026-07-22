/**
 * Tier-1 oracle case — the Markdown frontmatter parser + its YAML reader.
 *
 * Drives v4's REAL parseFrontmatter (lib/doc-edit/markdown-parser.ts, which calls
 * eemeli/yaml's YAML.parse) over a corpus of file contents, emitting
 * { data, bodyStartLine, bodyStartOffset }. The Rust port
 * (quilltap_core::markdown::parse_frontmatter) hand-rolls the YAML reader for the
 * constrained subset (read-side companion to locked Decision A) and must match
 * exactly on this corpus: the structural delimiter/offset logic AND the YAML 1.2
 * core-schema scalar resolution, quoting, comments, and flow/block sequences.
 *
 * Run from the v4 server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/markdown-frontmatter.ts \
 *     > /tmp/oracle-markdown-frontmatter.ndjson
 */

import { parseFrontmatter } from '@/lib/doc-edit/markdown-parser';

type Row = {
  id: string;
  content: string;
  data: unknown;
  bodyStartLine: number;
  bodyStartOffset: number;
};

const rows: Row[] = [];
function fm(id: string, content: string) {
  const r = parseFrontmatter(content);
  rows.push({
    id,
    content,
    data: r.data,
    bodyStartLine: r.bodyStartLine,
    bodyStartOffset: r.bodyStartOffset,
  });
}

// ── structural: presence, delimiters, offsets ────────────────────────────────
fm('no-fm', 'Just body text');
fm('no-fm-dashes-mid', 'hello\n---\nworld');
fm('crlf-not-recognized', '---\r\nname: Hero\r\n---\r\nbody');
fm('no-close', '---\nname: Hero\nbody continues');
fm('empty-fm', '---\n---\nbody');
fm('empty-fm-blankline', '---\n\n---\nbody');
fm('comments-only-fm', '---\n# just a note\n---\nbody');
fm('body-empty', '---\nname: Hero\n---\n');
fm('body-none-after-close', '---\nname: Hero\n---');
fm('multibyte-in-fm', '---\nname: café\n---\nbody'); // offset is UTF-16 code units
fm('fm-is-array', '---\n- a\n- b\n---\nbody');
fm('fm-is-scalar', '---\njust a string\n---\nbody');
fm('dup-key', '---\nname: A\nname: B\n---\nx');
// (nested maps, flow maps, block scalars, anchors, exotic numbers are the
//  documented out-of-subset seam — deliberately NOT in this equivalence corpus.)

// ── scalars: core-schema resolution ──────────────────────────────────────────
fm('plain-str', '---\nname: Hero\n---\nx');
fm('multiword-str', '---\nname: the quick brown\n---\nx');
fm('bool-true', '---\nisDefault: true\n---\nx');
fm('bool-True', '---\nisDefault: True\n---\nx');
fm('bool-false', '---\nflag: false\n---\nx');
fm('yes-is-string', '---\nflag: yes\n---\nx');
fm('null-tilde', '---\nx: ~\n---\nb');
fm('null-empty', '---\nx:\n---\nb');
fm('null-word', '---\nx: null\n---\nb');
fm('int', '---\nn: 42\n---\nb');
fm('neg-int', '---\nn: -5\n---\nb');
fm('float', '---\nn: 1.5\n---\nb');
fm('iso-stays-string', '---\ncreatedAt: 2024-01-01T00:00:00.000Z\n---\nb');
fm('url-stays-string', '---\nurl: http://x.com\n---\nb');
fm('str-true-quoted', '---\nb: "true"\n---\nx');
fm('empty-quoted', '---\nname: ""\n---\nx');

// ── quotes + comments ────────────────────────────────────────────────────────
fm('dquote-colon', '---\nname: "Hero: bold"\n---\nx');
fm('dquote-escapes', '---\nname: "a\\"b\\tc"\n---\nx');
fm('dquote-unicode', '---\nname: "caf\\u00e9"\n---\nx');
fm('squote', "---\nname: 'hi there'\n---\nx");
fm('squote-escape', "---\nname: 'it''s'\n---\nx");
fm('val-space-hash-comment', '---\nname: Hero # a hero\n---\nx');
fm('val-nospace-hash', '---\nname: Hero#c\n---\nx');
fm('val-is-comment', '---\nname: # c\n---\nx');
fm('quoted-then-comment', '---\nname: "Hero" # c\n---\nx');
fm('trailing-space-val', '---\nname: Hero   \n---\nx');
fm('plain-with-bracket', '---\nname: a[b]\n---\nx');

// ── sequences ────────────────────────────────────────────────────────────────
fm('flow-seq', '---\ntypes: [top, bottom]\n---\nx');
fm('flow-seq-quoted', '---\ntypes: ["top", "bottom"]\n---\nx');
fm('flow-empty', '---\ntypes: []\n---\nx');
fm('flow-trailing-comma', '---\ntypes: [top, bottom,]\n---\nx');
fm('flow-then-comment', '---\ntypes: [a, b] # c\n---\nx');
fm('block-seq-2indent', '---\ntypes:\n  - top\n  - bottom\n---\nx');
fm('block-seq-0indent', '---\ntypes:\n- top\n- bottom\n---\nx');
fm('block-seq-quoted', '---\ntypes:\n  - "top"\n  - bottom\n---\nx');
fm('block-seq-null-item', '---\ntypes:\n  -\n  - top\n---\nx');
fm('empty-then-block-comment', '---\ntypes: # c\n  - x\n---\nx');

// ── folded plain scalars (dogfood finding #18) ───────────────────────────────
// v4's emitter (eemeli/yaml stringify) wraps long plain scalars at ~80 columns
// onto indented continuation lines; YAML folds them back with single spaces.
// Real v4-written vaults carry these (imagePrompt/appropriateness), so they are
// IN-subset for the Rust reader.
fm('folded-2-lines', '---\nimagePrompt: navy dress, long sleeves,\n  appropriate to the mid 1920s\n---\nx');
fm('folded-3-lines', '---\nnote: one two\n  three four\n  five six\n---\nx');
fm('folded-then-key', '---\na: first part\n  second part\nb: after\n---\nx');
fm('folded-before-seq', '---\na: long value\n  wrapped tail\ntypes:\n  - top\n---\nx');
// (folds interrupted by comments, tab-indented continuations, and folds inside
//  block sequences stay out-of-subset — conservative null, not in the corpus.)
fm(
  'folded-wardrobe-real',
  '---\nid: d0eeb775-bba4-4aed-98f4-bafba35d32f0\ntitle: Navy Dress 1925\ntypes:\n  - top\n  - bottom\nappropriateness: casual, 1925, Paris\nimagePrompt: navy crêpe dress, long sleeves, a modest neckline, dropped waist\n  appropriate to the mid 1920s\ncreatedAt: 2026-06-20T18:35:25.430Z\nupdatedAt: 2026-06-20T18:35:25.430Z\n---\n\n_navy crêpe._',
);
fm(
  'folded-appropriateness-real',
  '---\nid: 08366b4f-629c-4488-ab07-c434125737b8\ntitle: Naked\ntypes:\n  - top\n  - bottom\n  - footwear\n  - accessories\ncomponentItems:\n  - white-gold-wedding-ring\nappropriateness: Intimate, clothing-optional, bed, bedroom, sleep, lodge,\n  sunroom, pool, swimming\nreplace: true\ncreatedAt: 2026-05-06T21:32:12.336Z\nupdatedAt: 2026-05-29T17:50:56.890Z\n---\n',
);

// ── multi-key + realistic prompt/wardrobe frontmatter ────────────────────────
fm('multi-key', '---\nname: Hero\nisDefault: true\ntypes: [top]\n---\nThe prompt body');
fm(
  'wardrobe-like',
  '---\nid: 123e4567-e89b-12d3-a456-426614174000\ntitle: "Greatcoat"\ntypes:\n  - top\n  - bottom\ndefault: true\ncomponentItems: [slug-a, slug-b]\ncreatedAt: 2024-01-01T00:00:00.000Z\n---\nA long coat.',
);

for (const row of rows) {
  process.stdout.write(JSON.stringify(row) + '\n');
}
