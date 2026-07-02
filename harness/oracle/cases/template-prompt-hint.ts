/**
 * Oracle case: formatting prompt-hint generator (chat-orchestration wave 1.2, Part B).
 *
 * Drives the REAL exported function from v4's lib/chat/template-prompt-hint.ts:
 *   generateFormattingPromptHint(delimiters, narrationDelimiters).
 * Pure — no clock, no injection.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/template-prompt-hint.ts \
 *     > /tmp/oracle-template-prompt-hint.ndjson
 */

import { generateFormattingPromptHint } from '@/lib/chat/template-prompt-hint'
import type { TemplateDelimiter, NarrationDelimiters } from '@/lib/schemas/template.types'

// Delimiter builders (only the fields the generator reads matter; the rest are
// filled minimally to satisfy the runtime shape — the generator never validates).
function wrap(name: string, delimiters: string | [string, string]): TemplateDelimiter {
  return { kind: 'wrap', name, buttonName: 'x', style: 'qt-x', delimiters } as TemplateDelimiter
}
function linePrefix(name: string, marker: string): TemplateDelimiter {
  return { kind: 'linePrefix', name, buttonName: 'x', style: 'qt-x', marker } as TemplateDelimiter
}
function tagPrefix(name: string, open: string, close: string): TemplateDelimiter {
  return { kind: 'tagPrefix', name, buttonName: 'x', style: 'qt-x', open, close } as TemplateDelimiter
}

type Row = {
  id: string
  delimiters: TemplateDelimiter[]
  narration: NarrationDelimiters | undefined
  out: string
}

const rows: Row[] = []
function push(id: string, delimiters: TemplateDelimiter[], narration: NarrationDelimiters | undefined) {
  rows.push({ id, delimiters, narration, out: generateFormattingPromptHint(delimiters, narration) })
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

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n')
