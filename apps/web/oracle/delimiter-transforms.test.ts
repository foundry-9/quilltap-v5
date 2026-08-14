/**
 * Tier-1 oracle — v4's roleplay-DELIMITER toolbar surface (the P4.9L lane).
 *
 * The sibling recorder `harness/oracle/cases/text-transforms.test.ts` records
 * the MARKDOWN buttons and says of the delimiter branch: "OUT OF SCOPE
 * (deliberate): the delimiter branch (`handleDelimiterClick`) is chat-only —
 * the form toolbar offers no delimiter buttons." P4.9L gives the CHAT composer
 * its toolbar, so that branch is now in scope; this file is its recorder. The
 * older one is left untouched (its sentence is still true of the form toolbar).
 *
 * Four sections, each driving v4's REAL code — nothing here reimplements a
 * transform, a tooltip, or the narration-button synthesis:
 *
 *  A. `getDelimiterTooltip` (`lib/chat/annotations.ts:61-75`) over a delimiter
 *     corpus — the byte-contractual `title=` on every delimiter button.
 *  B. The SOURCE-mode delimiter branch (`FormattingToolbar.handleDelimiterClick`
 *     :317-331 → `lib/chat/text-transforms.ts` `toggleWrap` /
 *     `toggleLinePrefix` / `insertTagPrefix`), recorded by rendering the REAL
 *     toolbar in jsdom over a real `<textarea>` and clicking the real buttons —
 *     the same host shape the sibling recorder uses.
 *  C. The RICH-TEXT delimiter branch: v4's real `FormattingCommandPlugin`
 *     `APPLY_DELIMITER_COMMAND` handler dispatched into a real headless Lexical
 *     editor, recording the EXPORTED COMPOSER MARKDOWN (the wire bytes) and the
 *     resulting caret offset. This is the load-bearing section: the applied
 *     delimiters are literal text, so what reaches the provider depends on the
 *     bridge's escape post-pass (`DEFAULT_PRESERVED_MARKDOWN_CHARS`), and
 *     reading the two escape regexes is exactly the mistake dogfood #84 was.
 *  D. The rendered delimiter BUTTON LIST — v4's narration-button synthesis and
 *     the "drop a template delimiter that matches narration" filter
 *     (`FormattingToolbar.tsx:382-413`), recorded as the buttons' order,
 *     labels, titles and classes straight off the DOM.
 *
 * The section-B/D stub `editor` is safe for the same reason the sibling
 * recorder's is: every source-mode branch returns before touching Lexical, and
 * the only mount-time contact is `registerUpdateListener`.
 *
 * The consumer is `apps/web/src/app/editor/delimiter-transforms.spec.ts` +
 * `formatting-toolbar.delimiters.spec.ts`, which byte-diff v5 against
 * `apps/web/src/app/editor/__fixtures__/delimiter-oracle.json`. Fix the port,
 * not the vectors.
 *
 * Regenerate (from the v4 checkout, Node 24; jest ignores paths under
 * `/.claude/`, so the file is mirrored to /tmp first — the standing recipe):
 *
 *   V5=~/source/quilltap-v5
 *   mkdir -p /tmp/qt-oracle-delimiters
 *   cp $V5/apps/web/oracle/delimiter-transforms.test.ts /tmp/qt-oracle-delimiters/
 *   cd ~/source/quilltap-server
 *   PATH=~/.nvm/versions/node/v24.13.1/bin:$PATH \
 *   QT_ORACLE_OUT=$V5/apps/web/src/app/editor/__fixtures__/delimiter-oracle.json \
 *     npx jest --silent --roots "$PWD" --roots /tmp/qt-oracle-delimiters \
 *       -- delimiter-transforms
 *
 * (When the lane runs from a worktree, point `V5` at the worktree path — the
 * `--roots` mirror is what lets jest see the file at all.)
 *
 * @module apps/web/oracle/delimiter-transforms
 */

import { writeFileSync } from 'fs'
import React, { useRef, useState } from 'react'
import { render, fireEvent, act } from '@testing-library/react'
import {
  $createParagraphNode,
  $createTextNode,
  $getRoot,
  $getSelection,
  $isRangeSelection,
  $setSelection,
  $createRangeSelection,
  createEditor,
  type LexicalEditor,
} from 'lexical'
import { HeadingNode, QuoteNode, registerRichText } from '@lexical/rich-text'
import { ListNode, ListItemNode } from '@lexical/list'
import { LinkNode } from '@lexical/link'
import { $createCodeNode, CodeNode, CodeHighlightNode } from '@lexical/code'
import { TableNode, TableCellNode, TableRowNode } from '@lexical/table'

import FormattingToolbar from '@/components/chat/FormattingToolbar'
import { getDelimiterTooltip } from '@/lib/chat/annotations'
import {
  APPLY_DELIMITER_COMMAND,
  FormattingCommandPlugin,
} from '@/components/chat/lexical/plugins/FormattingCommandPlugin'
import { $exportComposerMarkdown } from '@/components/chat/lexical/plugins/MarkdownBridgePlugin'
import type { NarrationDelimiters, TemplateDelimiter } from '@/lib/schemas/template.types'

/** `DEFAULT_PRESERVED_MARKDOWN_CHARS` — the bridge's real export escape set. */
const PRESERVED = ['*', '_', '`', '~']

// ---------------------------------------------------------------------------
// The delimiter corpus — one of every kind, plus the shapes that exercise the
// narration synthesis and its dedupe.
// ---------------------------------------------------------------------------

const D_THOUGHTS: TemplateDelimiter = {
  kind: 'wrap',
  name: 'Thoughts',
  buttonName: 'Th',
  delimiters: '~',
  style: 'qt-rp-thoughts',
}
const D_ASIDE: TemplateDelimiter = {
  kind: 'wrap',
  name: 'Aside',
  buttonName: 'As',
  delimiters: ['((', '))'],
  style: 'qt-rp-aside',
}
const D_OOC: TemplateDelimiter = {
  kind: 'linePrefix',
  name: 'Out of character',
  buttonName: 'OOC',
  marker: '// ',
  style: 'qt-rp-ooc',
}
const D_RANK: TemplateDelimiter = {
  kind: 'tagPrefix',
  name: 'Rank',
  buttonName: 'Rank',
  open: '[',
  close: ']',
  style: 'qt-rp-rank',
}
/** Deliberately the SAME delimiters as the `'*'` narration — the dedupe target. */
const D_STAR_DUPE: TemplateDelimiter = {
  kind: 'wrap',
  name: 'Action',
  buttonName: 'Act',
  delimiters: '*',
  style: 'qt-chat-narration',
}
/** The same delimiters as the `['[' , ']']` narration, in tuple form. */
const D_BRACKET_DUPE: TemplateDelimiter = {
  kind: 'wrap',
  name: 'Bracketed',
  buttonName: 'Br',
  delimiters: ['[', ']'],
  style: 'qt-rp-bracket',
}
/**
 * Shares the `['[' , ']']` narration's OPEN bracket but not its close — the
 * shape that discriminates v4's two-clause dedupe (`prefix !== narPrefix ||
 * suffix !== narSuffix`) from a prefix-only one. It must SURVIVE.
 */
const D_HALF_MATCH: TemplateDelimiter = {
  kind: 'wrap',
  name: 'Half match',
  buttonName: 'Half',
  delimiters: ['[', '>'],
  style: 'qt-rp-half',
}
/** The mirror: the narration's CLOSE bracket with a different open. Survives too. */
const D_HALF_MATCH_2: TemplateDelimiter = {
  kind: 'wrap',
  name: 'Other half',
  buttonName: 'Oth',
  delimiters: ['<', ']'],
  style: 'qt-rp-half2',
}
/**
 * A legacy-shaped tuple whose close is empty. Zod rejects it on a WRITE, but
 * `getDelimiterTooltip`'s `suffix || 'EOL'` branch is only reachable through
 * such a row, so it is recorded rather than reasoned about.
 */
const D_EOL_TUPLE = {
  kind: 'wrap',
  name: 'Shout',
  buttonName: 'Sh',
  delimiters: ['>>', ''],
  style: 'qt-rp-shout',
} as unknown as TemplateDelimiter

/** The narration button v4 SYNTHESIZES (`FormattingToolbar.tsx:398-404`). */
function narrationButton(narration: NarrationDelimiters): TemplateDelimiter {
  return {
    kind: 'wrap',
    name: 'Narration',
    buttonName: 'Nar',
    delimiters: narration,
    style: 'qt-chat-narration',
  }
}

// ---------------------------------------------------------------------------
// Section B/D host — v4's source-mode wiring with a template that HAS delimiters
// ---------------------------------------------------------------------------

const TEMPLATE_ID = '11111111-2222-3333-4444-555555555555'

/** Stub `fetch` so the toolbar's template effect resolves to `delimiters`. */
function stubTemplateFetch(delimiters: TemplateDelimiter[]) {
  const original = globalThis.fetch
  globalThis.fetch = (async () => ({
    ok: true,
    json: async () => ({
      id: TEMPLATE_ID,
      name: 'Probe template',
      description: null,
      isBuiltIn: false,
      delimiters,
    }),
  })) as unknown as typeof fetch
  return () => {
    globalThis.fetch = original
  }
}

function SourceHost({
  initial,
  narration,
}: {
  initial: string
  narration?: NarrationDelimiters
}) {
  const [value, setValue] = useState(initial)
  const sourceTextareaRef = useRef<HTMLTextAreaElement>(null)

  const editorStub = {
    registerUpdateListener: () => () => {},
  } as unknown as LexicalEditor

  return React.createElement(
    'div',
    null,
    React.createElement(FormattingToolbar, {
      roleplayTemplateId: TEMPLATE_ID,
      editor: editorStub,
      showSource: true,
      sourceTextareaRef,
      setInput: setValue,
      onToggleSource: () => {},
      narrationDelimiters: narration,
    }),
    React.createElement('textarea', {
      ref: sourceTextareaRef,
      value,
      onChange: (e: React.ChangeEvent<HTMLTextAreaElement>) => setValue(e.target.value),
      'data-testid': 'source',
    }),
  )
}

// ---------------------------------------------------------------------------
// Section C host — a headless Lexical editor with the REAL command plugin
// ---------------------------------------------------------------------------

/**
 * Build a headless editor with v4's composer node set and register the REAL
 * `FormattingCommandPlugin` commands against it.
 *
 * The plugin is a React component reading `useLexicalComposerContext()`, so it
 * is rendered inside `LexicalComposerContext`'s provider seeded with an editor
 * we already hold. That runs v4's own `registerCommand` calls verbatim — no
 * handler is transcribed.
 */
function makeEditorWithPlugin(): { editor: LexicalEditor; unmount: () => void } {
  const editor = createEditor({
    namespace: 'delimiter-oracle',
    nodes: [
      HeadingNode,
      QuoteNode,
      ListNode,
      ListItemNode,
      LinkNode,
      CodeNode,
      CodeHighlightNode,
      TableNode,
      TableCellNode,
      TableRowNode,
    ],
    onError: (error) => {
      throw error
    },
  })
  editor.setRootElement(document.createElement('div'))
  registerRichText(editor)

  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const {
    LexicalComposerContext,
    createLexicalComposerContext,
  } = require('@lexical/react/LexicalComposerContext')
  const { unmount } = render(
    React.createElement(
      LexicalComposerContext.Provider,
      { value: [editor, createLexicalComposerContext(null, undefined)] },
      React.createElement(FormattingCommandPlugin, null),
    ),
  )
  return { editor, unmount }
}

/** Seed one paragraph of plain TYPED text and select `[start, end)` in it. */
function seedParagraph(editor: LexicalEditor, text: string, start: number, end: number) {
  editor.update(
    () => {
      const root = $getRoot()
      root.clear()
      const p = $createParagraphNode()
      const node = $createTextNode(text)
      p.append(node)
      root.append(p)

      const selection = $createRangeSelection()
      selection.anchor.set(node.getKey(), start, 'text')
      selection.focus.set(node.getKey(), end, 'text')
      $setSelection(selection)
    },
    { discrete: true },
  )
}

/** Seed TWO paragraphs and select across them (the multi-block wrap edge). */
function seedTwoParagraphs(
  editor: LexicalEditor,
  first: string,
  second: string,
  start: number,
  end: number,
) {
  editor.update(
    () => {
      const root = $getRoot()
      root.clear()
      const p1 = $createParagraphNode()
      const a = $createTextNode(first)
      p1.append(a)
      const p2 = $createParagraphNode()
      const b = $createTextNode(second)
      p2.append(b)
      root.append(p1, p2)

      const selection = $createRangeSelection()
      selection.anchor.set(a.getKey(), start, 'text')
      selection.focus.set(b.getKey(), end, 'text')
      $setSelection(selection)
    },
    { discrete: true },
  )
}

/** Seed a fenced CODE block — v4's delimiter handler refuses to touch one. */
function seedCodeBlock(editor: LexicalEditor, text: string, offset: number) {
  editor.update(
    () => {
      const root = $getRoot()
      root.clear()
      const code = $createCodeNode()
      const node = $createTextNode(text)
      code.append(node)
      root.append(code)

      const selection = $createRangeSelection()
      selection.anchor.set(node.getKey(), offset, 'text')
      selection.focus.set(node.getKey(), offset, 'text')
      $setSelection(selection)
    },
    { discrete: true },
  )
}

/** Seed `bold` + plain in one paragraph and select the BOLD run. */
function seedBoldRun(editor: LexicalEditor, bold: string, rest: string) {
  editor.update(
    () => {
      const root = $getRoot()
      root.clear()
      const p = $createParagraphNode()
      const b = $createTextNode(bold)
      b.toggleFormat('bold')
      const r = $createTextNode(rest)
      p.append(b, r)
      root.append(p)

      const selection = $createRangeSelection()
      selection.anchor.set(b.getKey(), 0, 'text')
      selection.focus.set(b.getKey(), bold.length, 'text')
      $setSelection(selection)
    },
    { discrete: true },
  )
}

// ---------------------------------------------------------------------------
// Corpora
// ---------------------------------------------------------------------------

interface TooltipCase {
  name: string
  delimiter: TemplateDelimiter
}

const TOOLTIP_CASES: TooltipCase[] = [
  { name: 'wrap with a single-char string delimiter', delimiter: D_THOUGHTS },
  { name: 'wrap with an [open, close] tuple', delimiter: D_ASIDE },
  { name: 'linePrefix names its marker and EOL', delimiter: D_OOC },
  { name: 'tagPrefix spells the bracketed TOKEN form', delimiter: D_RANK },
  { name: 'the synthesized narration button (string)', delimiter: narrationButton('*') },
  { name: 'the synthesized narration button (tuple)', delimiter: narrationButton(['[', ']']) },
  { name: 'a wrap tuple with an empty close falls back to EOL', delimiter: D_EOL_TUPLE },
]

interface SourceCase {
  name: string
  /** The delimiter whose BUTTON is clicked (matched on its rendered title). */
  delimiter: TemplateDelimiter
  /** The template roster the stubbed fetch answers with. */
  template: TemplateDelimiter[]
  narration?: NarrationDelimiters
  value: string
  start: number
  end: number
}

const TEMPLATE_ALL = [D_THOUGHTS, D_ASIDE, D_OOC, D_RANK]

const SOURCE_CASES: SourceCase[] = [
  // --- wrap ----------------------------------------------------------------
  {
    name: 'wrap: a selected word gets the string delimiter on both sides',
    delimiter: D_THOUGHTS,
    template: TEMPLATE_ALL,
    value: 'hello world',
    start: 0,
    end: 5,
  },
  {
    name: 'wrap: an already-wrapped selection unwraps',
    delimiter: D_THOUGHTS,
    template: TEMPLATE_ALL,
    value: '~hello~ world',
    start: 0,
    end: 7,
  },
  {
    name: 'wrap: an empty selection inserts the pair and centers the caret',
    delimiter: D_THOUGHTS,
    template: TEMPLATE_ALL,
    value: 'ab',
    start: 1,
    end: 1,
  },
  {
    name: 'wrap: a tuple delimiter uses open and close',
    delimiter: D_ASIDE,
    template: TEMPLATE_ALL,
    value: 'hello world',
    start: 6,
    end: 11,
  },
  {
    name: 'wrap: a multi-line selection is wrapped as one span',
    delimiter: D_ASIDE,
    template: TEMPLATE_ALL,
    value: 'one\ntwo',
    start: 0,
    end: 7,
  },
  // --- linePrefix ----------------------------------------------------------
  {
    name: 'linePrefix: a caret on an unprefixed line adds the marker',
    delimiter: D_OOC,
    template: TEMPLATE_ALL,
    value: 'a note',
    start: 3,
    end: 3,
  },
  {
    name: 'linePrefix: a caret on an already-prefixed line removes it',
    delimiter: D_OOC,
    template: TEMPLATE_ALL,
    value: '// a note',
    start: 5,
    end: 5,
  },
  {
    name: 'linePrefix: a mixed multi-line selection ADDS to every line',
    delimiter: D_OOC,
    template: TEMPLATE_ALL,
    value: '// one\ntwo\nthree',
    start: 0,
    end: 16,
  },
  {
    name: 'linePrefix: an all-prefixed multi-line selection removes from every line',
    delimiter: D_OOC,
    template: TEMPLATE_ALL,
    value: '// one\n// two',
    start: 0,
    end: 13,
  },
  {
    name: 'linePrefix: a partial-line selection expands to whole lines first',
    delimiter: D_OOC,
    template: TEMPLATE_ALL,
    value: 'alpha\nbeta\ngamma',
    start: 7,
    end: 8,
  },
  // --- tagPrefix -----------------------------------------------------------
  {
    name: 'tagPrefix: inserts the bracket pair at the line start, caret between',
    delimiter: D_RANK,
    template: TEMPLATE_ALL,
    value: 'reporting for duty',
    start: 9,
    end: 9,
  },
  {
    name: 'tagPrefix: the SECOND line gets its own pair',
    delimiter: D_RANK,
    template: TEMPLATE_ALL,
    value: 'one\ntwo',
    start: 5,
    end: 5,
  },
  {
    name: 'tagPrefix: an existing pair is NOT toggled off — a second click stacks',
    delimiter: D_RANK,
    template: TEMPLATE_ALL,
    value: '[]hello',
    start: 1,
    end: 1,
  },
  // --- the synthesized narration button ------------------------------------
  {
    name: 'narration: the synthesized button wraps with the template narration char',
    delimiter: narrationButton('*'),
    template: TEMPLATE_ALL,
    narration: '*',
    value: 'she smiled',
    start: 0,
    end: 10,
  },
  {
    name: 'narration: a tuple narration wraps with open and close',
    delimiter: narrationButton(['[', ']']),
    template: TEMPLATE_ALL,
    narration: ['[', ']'],
    value: 'she smiled',
    start: 4,
    end: 10,
  },
]

interface ButtonListCase {
  name: string
  template: TemplateDelimiter[]
  narration?: NarrationDelimiters
}

const BUTTON_LIST_CASES: ButtonListCase[] = [
  {
    name: 'no narration: only the template delimiters, in template order',
    template: TEMPLATE_ALL,
  },
  {
    name: 'string narration leads the section, template delimiters follow',
    template: TEMPLATE_ALL,
    narration: '*',
  },
  {
    name: 'a template delimiter matching the narration is dropped',
    template: [D_STAR_DUPE, ...TEMPLATE_ALL],
    narration: '*',
  },
  {
    name: 'the dedupe compares prefix AND suffix, so a tuple narration drops its tuple twin',
    template: [D_BRACKET_DUPE, ...TEMPLATE_ALL],
    narration: ['[', ']'],
  },
  {
    name: 'a tuple narration does NOT drop a same-prefix string delimiter',
    template: [D_STAR_DUPE, ...TEMPLATE_ALL],
    narration: ['[', ']'],
  },
  {
    name: 'the dedupe needs BOTH ends: a shared open with a different close survives',
    template: [D_HALF_MATCH, D_HALF_MATCH_2, D_BRACKET_DUPE],
    narration: ['[', ']'],
  },
  {
    name: 'an empty template with narration still shows the narration button',
    template: [],
    narration: '*',
  },
  {
    name: 'an empty template and no narration shows no delimiter section at all',
    template: [],
  },
]

interface RichCase {
  name: string
  delimiter: TemplateDelimiter
  /**
   * How the document is seeded before the command is dispatched. `paragraph`
   * (the default) is one plain TYPED paragraph — not imported markdown, see the
   * module note. The other three exist to record the STRUCTURAL edges a flat
   * `{value,start,end}` model cannot express.
   */
  shape?: 'paragraph' | 'twoParagraphs' | 'codeBlock' | 'boldRun'
  /** Plain TYPED paragraph text. For `twoParagraphs`, the FIRST paragraph. */
  text: string
  /** The second paragraph (`twoParagraphs`) or the trailing plain run (`boldRun`). */
  second?: string
  start: number
  end: number
}

const RICH_CASES: RichCase[] = [
  {
    name: 'wrap: a selected word, exported',
    delimiter: D_THOUGHTS,
    text: 'hello world',
    start: 0,
    end: 5,
  },
  {
    name: 'wrap: unwrapping an already-wrapped selection',
    delimiter: D_THOUGHTS,
    text: '~hello~ world',
    start: 0,
    end: 7,
  },
  {
    name: 'wrap: an empty selection inserts the pair',
    delimiter: D_THOUGHTS,
    text: 'ab',
    start: 1,
    end: 1,
  },
  {
    name: 'wrap: a tuple delimiter',
    delimiter: D_ASIDE,
    text: 'hello world',
    start: 6,
    end: 11,
  },
  {
    name: 'narration: the asterisk pair reaches the wire UNESCAPED',
    delimiter: narrationButton('*'),
    text: 'she smiled',
    start: 0,
    end: 10,
  },
  {
    name: 'narration: a tuple narration',
    delimiter: narrationButton(['[', ']']),
    text: 'she smiled',
    start: 4,
    end: 10,
  },
  {
    name: 'linePrefix: the whole block takes the marker',
    delimiter: D_OOC,
    text: 'a note',
    start: 3,
    end: 3,
  },
  {
    name: 'linePrefix: an already-prefixed block loses it',
    delimiter: D_OOC,
    text: '// a note',
    start: 5,
    end: 5,
  },
  {
    name: 'tagPrefix: the pair lands at the block start',
    delimiter: D_RANK,
    text: 'reporting for duty',
    start: 9,
    end: 9,
  },
  {
    name: 'wrap: a selection spanning TWO paragraphs',
    delimiter: D_ASIDE,
    shape: 'twoParagraphs',
    text: 'one',
    second: 'two',
    start: 0,
    end: 3,
  },
  {
    name: 'linePrefix: a caret inside a CODE BLOCK is refused',
    delimiter: D_OOC,
    shape: 'codeBlock',
    text: 'x = 1',
    start: 3,
    end: 3,
  },
  {
    name: 'wrap: a selected BOLD run — the raw-text insert drops the mark',
    delimiter: D_THOUGHTS,
    shape: 'boldRun',
    text: 'hello',
    second: ' world',
    start: 0,
    end: 5,
  },
]

// ---------------------------------------------------------------------------

describe('oracle: v4 roleplay-delimiter toolbar surface', () => {
  it('records the corpus', async () => {
    const tooltips = TOOLTIP_CASES.map((c) => ({
      name: c.name,
      delimiter: c.delimiter,
      tooltip: getDelimiterTooltip(c.delimiter),
    }))

    // --- B: source-mode clicks on the real toolbar --------------------------
    const source: unknown[] = []
    for (const c of SOURCE_CASES) {
      const restore = stubTemplateFetch(c.template)
      const { container, unmount } = await act(async () =>
        render(React.createElement(SourceHost, { initial: c.value, narration: c.narration })),
      )
      // Let the toolbar's template fetch settle so the buttons exist.
      await act(async () => {
        await Promise.resolve()
      })

      const textarea = container.querySelector('textarea') as HTMLTextAreaElement
      textarea.setSelectionRange(c.start, c.end)

      const title = getDelimiterTooltip(c.delimiter)
      const button = container.querySelector(
        `.qt-rp-annotation-button[title="${title.replace(/"/g, '\\"')}"]`,
      ) as HTMLButtonElement | null
      if (!button) throw new Error(`no delimiter button titled ${JSON.stringify(title)}`)

      fireEvent.click(button)
      // `sourceApply` restores the cursor inside a requestAnimationFrame.
      await act(async () => {
        await new Promise((resolve) => requestAnimationFrame(() => resolve(null)))
      })

      source.push({
        name: c.name,
        delimiter: c.delimiter,
        narration: c.narration ?? null,
        in: { value: c.value, start: c.start, end: c.end },
        out: { value: textarea.value, cursor: textarea.selectionStart },
      })
      unmount()
      restore()
    }

    // --- D: the rendered delimiter button list ------------------------------
    const buttonLists: unknown[] = []
    for (const c of BUTTON_LIST_CASES) {
      const restore = stubTemplateFetch(c.template)
      const { container, unmount } = await act(async () =>
        render(React.createElement(SourceHost, { initial: '', narration: c.narration })),
      )
      await act(async () => {
        await Promise.resolve()
      })

      const buttons = Array.from(
        container.querySelectorAll<HTMLButtonElement>('.qt-rp-annotation-button'),
      ).map((b) => ({
        label: b.textContent,
        title: b.getAttribute('title'),
        className: b.className,
      }))
      // How many dividers precede the picker section tells us whether the
      // delimiter section rendered at all (v4 gates it on `hasDelimiters`).
      const dividers = container.querySelectorAll('.qt-formatting-toolbar-divider').length

      buttonLists.push({
        name: c.name,
        template: c.template,
        narration: c.narration ?? null,
        buttons,
        dividers,
      })
      unmount()
      restore()
    }

    // --- C: rich-text APPLY_DELIMITER_COMMAND → exported markdown -----------
    const rich: unknown[] = []
    for (const c of RICH_CASES) {
      const { editor, unmount } = makeEditorWithPlugin()
      switch (c.shape ?? 'paragraph') {
        case 'twoParagraphs':
          seedTwoParagraphs(editor, c.text, c.second ?? '', c.start, c.end)
          break
        case 'codeBlock':
          seedCodeBlock(editor, c.text, c.start)
          break
        case 'boldRun':
          seedBoldRun(editor, c.text, c.second ?? '')
          break
        default:
          seedParagraph(editor, c.text, c.start, c.end)
      }

      const applied = editor.dispatchCommand(APPLY_DELIMITER_COMMAND, c.delimiter)
      // `dispatchCommand` runs the handler in a NON-discrete update, so the
      // editor state visible to a read right after it is still the pre-command
      // one. A discrete no-op forces the commit before the export.
      editor.update(() => {}, { discrete: true })

      let markdown = ''
      let anchorOffset: number | null = null
      editor.getEditorState().read(() => {
        markdown = $exportComposerMarkdown(editor, PRESERVED)
        const selection = $getSelection()
        anchorOffset = $isRangeSelection(selection) ? selection.anchor.offset : null
      })

      rich.push({
        name: c.name,
        delimiter: c.delimiter,
        in: {
          shape: c.shape ?? 'paragraph',
          text: c.text,
          second: c.second ?? null,
          start: c.start,
          end: c.end,
        },
        out: { markdown, anchorOffset, applied },
      })
      unmount()
    }

    const out = process.env.QT_ORACLE_OUT
    if (!out) throw new Error('QT_ORACLE_OUT is unset')
    writeFileSync(out, JSON.stringify({ tooltips, source, buttonLists, rich }, null, 2) + '\n')

    expect(tooltips).toHaveLength(TOOLTIP_CASES.length)
    expect(source).toHaveLength(SOURCE_CASES.length)
    expect(buttonLists).toHaveLength(BUTTON_LIST_CASES.length)
    expect(rich).toHaveLength(RICH_CASES.length)
  })
})
