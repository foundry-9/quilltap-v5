/**
 * Tier-1 oracle — v4's source-mode toolbar text transforms.
 *
 * Records what v4 ACTUALLY does when a formatting button is clicked while the
 * markdown-editor is in source mode. The transforms live in two places in v4:
 *
 *  - `lib/chat/text-transforms.ts` — the pure `toggleWrap` (wrap/unwrap/insert).
 *  - `components/chat/FormattingToolbar.tsx` — `sourceApply` (:123-134),
 *    `sourceToggleWrap` (:137-147), `sourcePrefixLines` (:150-165) and the two
 *    dispatch branches `handleMarkdownClick` (:174-205) /
 *    `handleCodeBlockClick` (:254-275). These are component-internal closures,
 *    NOT exported functions.
 *
 * Because half the behavior is component-internal, a bare `tsx` import oracle
 * cannot reach it. So this oracle RENDERS THE REAL `FormattingToolbar` in jsdom
 * over a real `<textarea>` — wired exactly as v4's `MarkdownLexicalEditor`
 * wires it in source mode (`:216-238`: a controlled textarea whose `value` is
 * the parent state and whose `onChange` is the parent's `onChange`, plus
 * `sourceTextareaRef`/`setInput`/`showSource` into the toolbar) — clicks the
 * real buttons, and records the resulting textarea bytes + cursor. v4's real
 * code produces every recorded value; nothing here reimplements a transform.
 *
 * The stub `editor` is safe: every source-mode branch returns before touching
 * Lexical (`if (showSource && sourceTextareaRef?.current) { … return }`). The
 * only Lexical contact is the mount-time `registerUpdateListener` effect
 * (`:70-82`), which the stub satisfies.
 *
 * OUT OF SCOPE (deliberate): the delimiter branch (`handleDelimiterClick`
 * :317-331) is chat-only — the form toolbar offers no delimiter buttons.
 *
 * Regenerate (from the v4 checkout, Node 24; jest ignores `/.claude/` paths, so
 * the file is mirrored to /tmp first — see the P4.6ax status-log record):
 *
 *   V5=~/source/quilltap-v5
 *   mkdir -p /tmp/qt-oracle-text-transforms
 *   cp $V5/harness/oracle/cases/text-transforms.test.ts /tmp/qt-oracle-text-transforms/
 *   cd ~/source/quilltap-server
 *   PATH=~/.nvm/versions/node/v24.13.1/bin:$PATH \
 *   QT_ORACLE_OUT=/tmp/oracle-text-transforms.ndjson \
 *     npx jest --silent --roots "$PWD" --roots /tmp/qt-oracle-text-transforms \
 *       -- text-transforms
 *
 * @module harness/oracle/cases/text-transforms
 */

import { writeFileSync } from 'fs'
import React, { useRef, useState } from 'react'
import { render, fireEvent, act } from '@testing-library/react'
import type { LexicalEditor } from 'lexical'

import FormattingToolbar from '@/components/chat/FormattingToolbar'

/** One recorded case: the textarea state before, the button, the bytes after. */
interface Case {
  name: string
  /** Initial textarea value. */
  value: string
  /** Selection start/end offsets applied before the click. */
  start: number
  end: number
  /** The `qt-formatting-button-<type>` suffix to click. */
  button: string
}

/**
 * The corpus. Every case is a source-mode button click. Covers, per the order:
 * wrap/unwrap at selection boundaries, empty selection, multi-line
 * prefix/unprefix, and the code-block toggle in/out.
 */
const CASES: Case[] = [
  // --- bold: toggleWrap('**','**') ------------------------------------------
  { name: 'bold wraps a selected word', value: 'hello world', start: 0, end: 5, button: 'bold' },
  { name: 'bold unwraps an already-wrapped selection', value: '**hello** world', start: 0, end: 9, button: 'bold' },
  { name: 'bold on an empty selection inserts the pair and centers the cursor', value: 'ab', start: 1, end: 1, button: 'bold' },
  { name: 'bold on an empty document', value: '', start: 0, end: 0, button: 'bold' },
  { name: 'bold wraps a multi-line selection as one span', value: 'one\ntwo', start: 0, end: 7, button: 'bold' },
  { name: 'bold wraps a selection at the very end', value: 'trailing', start: 4, end: 8, button: 'bold' },
  { name: 'bold unwrap requires BOTH delimiters in the selection', value: '**hello** world', start: 2, end: 7, button: 'bold' },
  { name: 'bold wraps a selection that is only the delimiters', value: '****', start: 0, end: 4, button: 'bold' },

  // --- italic: toggleWrap('_','_') ------------------------------------------
  { name: 'italic wraps a selected word', value: 'hello world', start: 6, end: 11, button: 'italic' },
  { name: 'italic unwraps an already-wrapped selection', value: '_hello_ world', start: 0, end: 7, button: 'italic' },
  { name: 'italic on an empty selection', value: 'ab', start: 2, end: 2, button: 'italic' },

  // --- headings: sourcePrefixLines('# '..'###### ') -------------------------
  { name: 'h1 prefixes the current line', value: 'a title', start: 0, end: 0, button: 'h1' },
  { name: 'h2 prefixes from a mid-line cursor (expands to line start)', value: 'a title', start: 3, end: 3, button: 'h2' },
  { name: 'h3 prefixes only the cursor line of a multi-line value', value: 'one\ntwo\nthree', start: 5, end: 5, button: 'h3' },
  { name: 'h6 prefixes every line the selection touches', value: 'one\ntwo\nthree', start: 1, end: 9, button: 'h6' },
  { name: 'h1 on an already-prefixed line ADDS AGAIN (no toggle-off)', value: '# a title', start: 0, end: 0, button: 'h1' },
  { name: 'h1 on an empty document', value: '', start: 0, end: 0, button: 'h1' },
  { name: 'h4 prefixes a blank line among lines', value: 'one\n\nthree', start: 4, end: 4, button: 'h4' },

  // --- lists / blockquote: sourcePrefixLines('- ' / '1. ' / '> ') -----------
  { name: 'ul prefixes the current line', value: 'item', start: 0, end: 0, button: 'ul' },
  { name: 'ul prefixes each selected line', value: 'one\ntwo', start: 0, end: 7, button: 'ul' },
  { name: 'ol prefixes each selected line with the SAME 1.', value: 'one\ntwo\nthree', start: 0, end: 13, button: 'ol' },
  { name: 'blockquote prefixes each selected line', value: 'one\ntwo', start: 0, end: 7, button: 'blockquote' },
  { name: 'blockquote on an already-quoted line ADDS AGAIN', value: '> quoted', start: 0, end: 0, button: 'blockquote' },
  { name: 'ul with the selection ending exactly at a newline', value: 'one\ntwo', start: 0, end: 4, button: 'ul' },

  // --- code block: toggleWrap('`','`') on a selection, else a fence ---------
  { name: 'code-block wraps a selection in inline backticks', value: 'call render now', start: 5, end: 11, button: 'code-block' },
  { name: 'code-block unwraps an inline-code selection', value: 'call `render` now', start: 5, end: 13, button: 'code-block' },
  { name: 'code-block on an empty selection in an empty doc inserts a fence', value: '', start: 0, end: 0, button: 'code-block' },
  { name: 'code-block fence after text needs a leading newline', value: 'prose', start: 5, end: 5, button: 'code-block' },
  { name: 'code-block fence before text needs a trailing newline', value: 'prose', start: 0, end: 0, button: 'code-block' },
  { name: 'code-block fence mid-value gets newlines on both sides', value: 'ab', start: 1, end: 1, button: 'code-block' },
  { name: 'code-block fence after an existing newline adds none', value: 'prose\n', start: 6, end: 6, button: 'code-block' },
  { name: 'code-block fence before an existing newline adds none', value: '\nprose', start: 0, end: 0, button: 'code-block' },
]

/**
 * v4's source-mode host, transcribed from `MarkdownLexicalEditor.tsx:216-238`:
 * a controlled textarea + the real toolbar with `showSource` on. The `editor`
 * stub is never reached by a source-mode branch (see the module docstring).
 */
function SourceHost({ initial }: { initial: string }) {
  const [value, setValue] = useState(initial)
  const sourceTextareaRef = useRef<HTMLTextAreaElement>(null)

  const editorStub = {
    registerUpdateListener: () => () => {},
  } as unknown as LexicalEditor

  return React.createElement(
    'div',
    null,
    React.createElement(FormattingToolbar, {
      editor: editorStub,
      showSource: true,
      sourceTextareaRef,
      setInput: setValue,
      onToggleSource: () => {},
    }),
    React.createElement('textarea', {
      ref: sourceTextareaRef,
      value,
      onChange: (e: React.ChangeEvent<HTMLTextAreaElement>) => setValue(e.target.value),
      'data-testid': 'source',
    }),
  )
}

describe('oracle: v4 source-mode toolbar text transforms', () => {
  it('records the corpus', async () => {
    const rows: string[] = []

    for (const c of CASES) {
      const { container, unmount } = render(React.createElement(SourceHost, { initial: c.value }))
      const textarea = container.querySelector('textarea') as HTMLTextAreaElement
      textarea.setSelectionRange(c.start, c.end)

      const button = container.querySelector(
        `.qt-formatting-button-${c.button}`,
      ) as HTMLButtonElement
      if (!button) throw new Error(`no button for ${c.button}`)

      fireEvent.click(button)
      // `sourceApply` restores the cursor inside a requestAnimationFrame.
      await act(async () => {
        await new Promise((resolve) => requestAnimationFrame(() => resolve(null)))
      })

      rows.push(
        JSON.stringify({
          name: c.name,
          button: c.button,
          in: { value: c.value, start: c.start, end: c.end },
          out: { value: textarea.value, cursor: textarea.selectionStart },
        }),
      )
      unmount()
    }

    const out = process.env.QT_ORACLE_OUT
    if (!out) throw new Error('QT_ORACLE_OUT is unset')
    writeFileSync(out, rows.join('\n') + '\n')
    expect(rows).toHaveLength(CASES.length)
  })
})
