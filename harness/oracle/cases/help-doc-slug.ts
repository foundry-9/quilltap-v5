/**
 * P4.d6 oracle case — the help-document slug (v4 `lib/help/help-doc-slug.ts`
 * `helpDocSlug`, promoted out of `help-doc-sync.ts` by `d6e74145`).
 *
 * Tier 1: a pure function over a fixed corpus. Imports v4's REAL module (never
 * reimplements the chain) and emits one NDJSON row per input:
 *   { kind: 'slug', input, output }
 *
 * The corpus deliberately probes the parts a port gets wrong:
 *   - the two anchored, NON-global strips (`^help/`, `\.md$`) vs substring hits;
 *   - the GLOBAL `[^a-zA-Z0-9]` map (slashes, dots, spaces, underscores);
 *   - `toLowerCase()` AFTER the map;
 *   - ⚠ the non-/u regex's UTF-16 CODE-UNIT semantics: a non-BMP char is a
 *     surrogate pair and must yield TWO dashes, not one. A `chars()`-based port
 *     passes every ASCII case and fails only here.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<this worktree>
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx $V5/harness/oracle/cases/help-doc-slug.ts > /tmp/oracle-help-doc-slug.ndjson
 */

const CORPUS: string[] = [
  // The documented example + the shipped help docs the drift was found through.
  'help/character-creation.md',
  'help/answer-confirmation.md',
  'help/brahma-console.md',
  'help/custom-tools.md',
  'help/post-office.md',
  'help/aurora.md',
  // Nested: only the FIRST `help/` is stripped; the inner slash maps to a dash.
  'help/nested/deep-doc.md',
  'help/a/b/c.md',
  'help/help/x.md',
  // Anchors are not substring matches.
  'docs/help/x.md',
  'help/a.mdx',
  'help/a.md.md',
  'help/a.mdx.md',
  'not-help/x.md',
  'helpful/x.md',
  // Case folding happens after the character map.
  'help/weird-CASE-Name.md',
  'help/Brahma Console.md',
  'help/ALLCAPS.md',
  // Assorted non-alphanumerics, each one dash.
  'help/a_b.c.md',
  'help/a+b=c.md',
  'help/spaced   out.md',
  'help/trailing-.md',
  'help/-leading.md',
  'help/dots...dots.md',
  'help/tab\there.md',
  'help/hash#frag.md',
  'help/paren(s).md',
  // Digits survive.
  'help/v2-migration-3.md',
  'help/123.md',
  // Degenerate.
  '',
  'help/',
  'help/.md',
  '.md',
  'help',
  '/',
  // ⚠ UTF-16 code-unit semantics — the surrogate-pair cases.
  'help/a\u{1F600}b.md', // one non-BMP char -> TWO dashes
  'help/\u{1F600}.md',
  'help/a\u{00E9}b.md', // BMP non-ASCII -> ONE dash
  'help/\u{4E2D}\u{6587}.md', // two BMP CJK units -> two dashes
  'help/mixed-\u{1F600}-\u{00E9}.md',
]

async function main(): Promise<void> {
  const { helpDocSlug } = await import('@/lib/help/help-doc-slug')

  for (const input of CORPUS) {
    process.stdout.write(
      JSON.stringify({ kind: 'slug', input, output: helpDocSlug(input) }) + '\n',
    )
  }
  process.exit(0)
}

main().catch((err) => {
  process.stderr.write(`help-doc-slug oracle failed: ${err?.stack ?? err}\n`)
  process.exit(1)
})
