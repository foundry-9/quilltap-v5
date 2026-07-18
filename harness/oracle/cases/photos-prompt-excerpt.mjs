/**
 * Tier-1 leaf oracle for v4 `extractPromptExcerpt`
 * (`lib/photos/user-gallery-service.ts:441-447`).
 *
 * The v5 port hand-rolls that function's regex (the core has no JS-regex
 * engine), so the transcription is pinned against the REAL JS engine rather
 * than against a reading of the pattern. This script prints, for each vector,
 * the verbatim v4 output; those strings are transcribed into the
 * `prompt_excerpt_matches_the_js_regex_vectors` table test in
 * `crates/quilltap-core/src/photos/user_gallery_service.rs`.
 *
 * The regex and the surrounding truncation are copied verbatim from v4 — this
 * file is a harness, so it reproduces the source rather than importing it (the
 * function is not exported).
 *
 * Run (from anywhere; Node 24):
 *   ~/.nvm/versions/node/v24.13.1/bin/node harness/oracle/cases/photos-prompt-excerpt.mjs
 */

// v4 `lib/photos/user-gallery-service.ts:443`, verbatim.
const RE = /##\s+Original prompt\s*\n+([^\n][^\n]*(?:\n[^\n#][^\n]*)*)/;

function extractPromptExcerpt(extractedText) {
  if (!extractedText) return '';
  const match = extractedText.match(RE);
  if (!match) return '';
  const para = match[1].trim();
  return para.length > 200 ? `${para.slice(0, 200).trimEnd()}…` : para;
}

const VECTORS = [
  '---\ntags: []\n---\n\n## Original prompt\n\nA brass owl at dusk.\nStill the same paragraph.\n\n## Scene\n\nelsewhere\n',
  '## Original prompt\n\nfirst line\n## Scene\n',
  '## Scene\n\nx',
  '## Original prompt\n\n',
  // `###` still matches: the regex finds `##` at offset 1.
  '### Original prompt\n\nheading three\n',
  '## Original prompt   \n   \nindented start\n',
  '##\tOriginal prompt\nno blank line\nsecond\n',
  '## Original prompt\n\n   trailing spaces   \n',
  '## Original promptX\n\nnope\n',
  // A `#` may START the captured paragraph; it only terminates a continuation
  // line (`[^\n][^\n]*` vs `\n[^\n#][^\n]*`).
  '## Original prompt\n\n#hash starts line\n',
  '## Original prompt\n\nline one\n\nline two\n',
  'intro\n## Original prompt\n\nafter intro\n',
  '## Original prompt\n \n \n after mixed ws\n',
  // The two truncation vectors.
  `## Original prompt\n\n${'x'.repeat(250)}\n`,
  `## Original prompt\n\n${'y'.repeat(198)}   \nmore\n`,
];

for (const input of VECTORS) {
  console.log(JSON.stringify({ input, expected: extractPromptExcerpt(input) }));
}
