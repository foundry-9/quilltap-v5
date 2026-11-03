/**
 * Oracle case: formatting prompt-hint generator (chat-orchestration wave 1.2, Part B).
 *
 * Drives the REAL exported function from v4's lib/chat/template-prompt-hint.ts:
 *   generateFormattingPromptHint(delimiters, narrationDelimiters, options).
 * Pure — no clock, no injection.
 *
 * P4.D351 (v4 `778b97b2b`, the Novel template): `narrationDelimiters` is a
 * THREE-state — `undefined` (no narration bullet), `null` (the plain-prose
 * bullet), or marks. `JSON.stringify` OMITS an `undefined` narration and KEEPS
 * a `null`, so the row's `narration` key carries all three states on the wire.
 * A wrap's `matchCurlyQuotes` / `closeOptional` add the capitalised notes; an
 * `options.markdownAsterisks === false` adds the double-asterisk bullet.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/template-prompt-hint.ts \
 *     > /tmp/oracle-template-prompt-hint.ndjson
 */

import { generateFormattingPromptHint } from '@/lib/chat/template-prompt-hint'
import type { TemplateDelimiter, NarrationDelimiters } from '@/lib/schemas/template.types'
import { NOVEL_TEMPLATE } from '@/lib/chat/novel-template'

// Delimiter builders (only the fields the generator reads matter; the rest are
// filled minimally to satisfy the runtime shape — the generator never validates).
function wrap(
  name: string,
  delimiters: string | [string, string],
  flags: { matchCurlyQuotes?: boolean; closeOptional?: boolean } = {},
): TemplateDelimiter {
  return { kind: 'wrap', name, buttonName: 'x', style: 'qt-x', delimiters, ...flags } as TemplateDelimiter
}
function linePrefix(name: string, marker: string): TemplateDelimiter {
  return { kind: 'linePrefix', name, buttonName: 'x', style: 'qt-x', marker } as TemplateDelimiter
}
function tagPrefix(name: string, open: string, close: string): TemplateDelimiter {
  return { kind: 'tagPrefix', name, buttonName: 'x', style: 'qt-x', open, close } as TemplateDelimiter
}

type HintOptions = { markdownAsterisks?: boolean }

type Row = {
  id: string
  delimiters: TemplateDelimiter[]
  narration: NarrationDelimiters | null | undefined
  options?: HintOptions
  out: string
}

const rows: Row[] = []
function push(
  id: string,
  delimiters: TemplateDelimiter[],
  narration: NarrationDelimiters | null | undefined,
  options?: HintOptions,
) {
  rows.push({ id, delimiters, narration, options, out: generateFormattingPromptHint(delimiters, narration, options ?? {}) })
}

// Empty / narration-only.
push('empty-no-narration', [], undefined)
push('narration-only-string', [], '*')
push('narration-only-tuple', [], ['[', ']'])

// Each kind alone (no narration).
push('wrap-same-only', [wrap('Thoughts', '_')], undefined)
push('wrap-tuple-only', [wrap('Aside', ['{', '}'])], undefined)
push('lineprefix-only', [linePrefix('OOC', '// ')], undefined)
push('tagprefix-only', [tagPrefix('Rank', '[', ']')], undefined)

// All kinds together, with narration.
push(
  'all-kinds-with-narration',
  [wrap('Thoughts', '_'), linePrefix('OOC', '// '), tagPrefix('Rank', '[', ']'), wrap('Aside', ['{', '}'])],
  '*',
)

// narration + a wrap that RESTATES the narration default → the wrap is skipped.
push('wrap-restates-narration-string', [wrap('Redundant', '*')], '*')
push('wrap-restates-narration-tuple', [wrap('Redundant', ['<', '>'])], ['<', '>'])
// narration + a wrap that only partially matches (prefix same, suffix differs) → kept.
push('wrap-partial-match-kept', [wrap('Half', ['*', '+'])], '*')

// narration tuple + several delimiters.
push(
  'narration-tuple-plus-delims',
  [wrap('Speech', '"'), linePrefix('Note', '> ')],
  ['<<', '>>'],
)

// Multiple wraps, one restating narration among them.
push(
  'mixed-with-one-restatement',
  [wrap('Keep1', '~'), wrap('Drop', '*'), wrap('Keep2', ['(', ')'])],
  '*',
)

// Wrap with empty-string delimiters, no narration → kept (empty prefix/suffix,
// but "" !== undefined so not skipped).
push('wrap-empty-no-narration', [wrap('Blank', '')], undefined)

// Duplicate names across kinds (generator doesn't dedupe by name).
push(
  'duplicate-names',
  [wrap('Same', '_'), linePrefix('Same', '// '), tagPrefix('Same', '<', '>')],
  '*',
)

// Special characters in markers (verify literal embedding, no escaping).
push('special-chars-marker', [linePrefix('Emoji', '🔒 '), tagPrefix('Angle', '⟨', '⟩')], '»')

// --- P4.D351: the three-state narration, the wrap notes, markdownAsterisks ---
push('narration-null-only', [], null)
push('narration-null-with-wraps', [wrap('Dialogue', '"'), wrap('OOC', ['((', '))'])], null)
// A `*` wrap under `null` narration is NOT skipped (the narration prefixes stay undefined).
push('narration-null-wrap-restating-star-not-skipped', [wrap('Action', '*')], null)
push('wrap-curly-note', [wrap('Speech', '"', { matchCurlyQuotes: true })], undefined)
push('wrap-close-optional-note-straight', [wrap('Speech', '"', { closeOptional: true })], undefined)
push('wrap-close-optional-note-pair', [wrap('Quote', ['«', '»'], { closeOptional: true })], undefined)
push('wrap-both-notes', [wrap('Dialogue', '"', { matchCurlyQuotes: true, closeOptional: true })], '*')
// The restates-narration skip fires BEFORE the notes.
push('wrap-notes-skipped-when-restating', [wrap('Dialogue', '"', { matchCurlyQuotes: true, closeOptional: true })], '"')
push('wrap-flags-false-no-note', [wrap('Speech', '"', { matchCurlyQuotes: false, closeOptional: false })], undefined)
push('markdown-false-alone', [], undefined, { markdownAsterisks: false })
push('markdown-false-with-delims', [wrap('Thoughts', '_'), linePrefix('OOC', '// ')], '*', { markdownAsterisks: false })
push('markdown-true-explicit', [wrap('Thoughts', '_')], '*', { markdownAsterisks: true })
// The Novel authoring whole — v4's REAL `NOVEL_TEMPLATE` (lib/chat/novel-template.ts):
// the starter the SPA's "Draft formatting instructions" button would show.
push('markdown-false-narration-null-novel', NOVEL_TEMPLATE.delimiters, NOVEL_TEMPLATE.narrationDelimiters, {
  markdownAsterisks: NOVEL_TEMPLATE.markdownAsterisks,
})
// `''` narration is falsy → no narration bullet; the asterisk bullet alone.
push('markdown-false-empty-narration-string', [], '', { markdownAsterisks: false })
push('narration-null-markdown-undefined-options-empty', [], null, {})

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n')
