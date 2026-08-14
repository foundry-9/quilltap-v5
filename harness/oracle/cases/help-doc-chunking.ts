/**
 * P4.D77 oracle case — help-document chunking (v4 `lib/help/help-doc-chunking.ts`
 * `buildHelpDocChunks` + `helpChunkEmbeddingText`, new at `24633026`).
 *
 * Tier 1: pure functions over a fixed corpus. Imports v4's REAL module (which
 * in turn pulls in the REAL Scriptorium chunker, `lib/mount-index/chunker.ts`)
 * and emits one NDJSON row per input:
 *   { kind: 'chunks', name, input, output: [{ chunkIndex, heading, content }] }
 *   { kind: 'embedText', name, docTitle, heading, content, output }
 *
 * The corpus deliberately probes what a port gets wrong:
 *   - the SIZE TARGETS. 400/700/100 is not the chunker's 800/1200/200 default,
 *     and a port that forgot to pass the options produces a single chunk where
 *     v4 produces five. The multi-section documents below are sized to sit
 *     between the two regimes, so a defaulted port diverges immediately.
 *   - the OVERLAP prefix carried into each chunk after the first, and the
 *     word-boundary trim inside it.
 *   - heading tracking across sections, including a document whose first
 *     paragraphs precede any heading (leading chunks carry `null`).
 *   - the oversized single paragraph (no blank lines at all) that the chunker
 *     must break on sentence/hard boundaries rather than paragraph ones.
 *   - the whitespace-only and empty arms, which yield NOTHING (not one empty
 *     chunk).
 *   - CRLF and a `#`-through-`######` heading ladder, since the heading regex
 *     is line-anchored.
 *   - ⚠ `helpChunkEmbeddingText`'s guard is `heading ? … : docTitle` — JS
 *     TRUTHINESS, so an EMPTY-STRING heading takes the title-only branch. A
 *     port matching on `Some(_)` passes every other case and fails only here.
 *   - the U+203A separator itself (a single ›, not '>' and not ' > ').
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5=<this worktree>
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx $V5/harness/oracle/cases/help-doc-chunking.ts \
 *     > /tmp/oracle-help-doc-chunking.ndjson
 */

/** A filler paragraph of a known length, repeated to reach a target size. */
const FILLER = 'Words about the subject at hand, repeated for length. '

/** v4's own test helper, verbatim — sections of `## Section N` + filler. */
function longDocument(sections: number): string {
  const filler = FILLER.repeat(60)
  return Array.from({ length: sections }, (_, i) => `## Section ${i}\n\n${filler}`).join('\n\n')
}

const CHUNK_CORPUS: Array<[string, string]> = [
  // Degenerate: nothing at all comes back (not one empty chunk).
  ['empty', ''],
  ['whitespace-only', '   \n\n  '],
  ['newlines-only', '\n\n\n'],

  // A short document stays whole, and carries its H1 as the heading.
  ['short-with-h1', '# Title\n\nA brief paragraph.'],
  ['short-no-heading', 'Just a sentence, with no heading above it.'],

  // ⚠ THE SIZE-TARGET PROBE. ~2,600 chars: over the help 700-token (2,800-char)
  // ceiling is what matters — this one sits just under it so it stays whole,
  // and its bigger sibling below crosses it while staying UNDER the
  // Scriptorium's 1,200-token (4,800-char) default. A port that forgot to pass
  // HELP_CHUNK_OPTIONS keeps the sibling whole and diverges there.
  ['just-under-help-ceiling', `## Heading\n\n${FILLER.repeat(45)}`],
  ['over-help-under-scriptorium', `## Heading\n\n${FILLER.repeat(75)}`],

  // Multi-section documents: sequential indices, per-section headings, overlap.
  ['two-sections', longDocument(2)],
  ['six-sections', longDocument(6)],
  ['eight-sections', longDocument(8)],

  // Prose BEFORE the first heading — the leading chunks must carry null.
  [
    'preamble-then-headings',
    `${FILLER.repeat(40)}\n\n## First\n\n${FILLER.repeat(40)}\n\n## Second\n\n${FILLER.repeat(40)}`,
  ],

  // One oversized paragraph, no blank lines: the hard-split path.
  ['single-huge-paragraph', FILLER.repeat(120).trim()],

  // The heading ladder (# … ######) and a non-heading '#x'.
  [
    'heading-ladder',
    ['# One', 'body', '## Two', 'body', '### Three', 'body', '###### Six', 'body', '#NotAHeading', 'body'].join(
      '\n\n',
    ),
  ],

  // CRLF line endings across a section boundary.
  ['crlf-sections', `## Alpha\r\n\r\n${FILLER.repeat(40)}\r\n\r\n## Beta\r\n\r\n${FILLER.repeat(40)}`],

  // Non-BMP content, so any UTF-16-vs-scalar slicing difference shows up.
  ['emoji-sections', `## Emoji \u{1F600}\n\n${FILLER.repeat(40)}\u{1F600}\n\n## Plain\n\n${FILLER.repeat(40)}`],

  // A real help document's shape: frontmatter already stripped by the sync.
  [
    'realistic-help-page',
    [
      '# Chat Settings',
      'Everything about how a conversation is run.',
      '## Image Description Settings',
      FILLER.repeat(30),
      '## Uncensored fallback profile',
      FILLER.repeat(30),
      '## Timeouts',
      FILLER.repeat(30),
    ].join('\n\n'),
  ],
]

const EMBED_CORPUS: Array<[string, string, string | null | undefined, string]> = [
  ['title-and-heading', 'Chat Settings', 'Image Description Settings', 'Body text.'],
  ['null-heading', 'Chat Settings', null, 'Body text.'],
  ['undefined-heading', 'Chat Settings', undefined, 'Body text.'],
  // ⚠ THE TRUTHINESS EDGE: '' is falsy, so title-only.
  ['empty-heading', 'Chat Settings', '', 'Body text.'],
  ['whitespace-heading', 'Chat Settings', ' ', 'Body text.'],
  ['empty-title', '', 'Some Heading', 'Body text.'],
  ['empty-content', 'Chat Settings', 'Some Heading', ''],
  ['multiline-content', 'Chat Settings', 'Some Heading', 'Line one.\n\nLine two.'],
  ['separator-lookalikes', 'A > B', '>', 'Body.'],
  ['non-bmp', 'Guide \u{1F600}', 'Section \u{1F600}', 'Body \u{1F600}.'],
]

async function main(): Promise<void> {
  const { buildHelpDocChunks, helpChunkEmbeddingText } = await import('@/lib/help/help-doc-chunking')

  for (const [name, input] of CHUNK_CORPUS) {
    const output = buildHelpDocChunks(input)
    process.stdout.write(JSON.stringify({ kind: 'chunks', name, input, output }) + '\n')
  }

  for (const [name, docTitle, heading, content] of EMBED_CORPUS) {
    process.stdout.write(
      JSON.stringify({
        kind: 'embedText',
        name,
        docTitle,
        // `undefined` would vanish from the JSON; tag it so the Rust side can
        // tell the two falsy-but-different inputs apart.
        heading: heading === undefined ? null : heading,
        headingUndefined: heading === undefined,
        content,
        output: helpChunkEmbeddingText(docTitle, heading, content),
      }) + '\n',
    )
  }

  process.exit(0)
}

main().catch((err) => {
  process.stderr.write(`help-doc-chunking oracle failed: ${err?.stack ?? err}\n`)
  process.exit(1)
})
