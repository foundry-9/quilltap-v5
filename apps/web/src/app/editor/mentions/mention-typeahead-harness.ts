/**
 * The spec harness for the `@` typeahead — v5's answer to v4's
 * `__tests__/helpers/lexicalPluginHarness` as `MentionTypeaheadPlugin.test.tsx`
 * uses it, and a sibling of `char-insert/char-typeahead-harness.ts`.
 *
 * A REAL ProseMirror editor over the real dialect schema, the REAL plugin, the
 * REAL query-backed source over a real `QueryClient` (the list answer is the
 * test's to script, as v4 scripts `fetch`), `prosemirror-history`, and a base
 * keymap whose Enter splits the block — so "Enter goes through" is observable
 * the way v4's is (a new paragraph; in the composer, a send).
 *
 * @module editor/mentions/mention-typeahead-harness
 */

import { QueryClient } from '@tanstack/angular-query-experimental';
import { baseKeymap } from 'prosemirror-commands';
import { history, undo } from 'prosemirror-history';
import { keymap } from 'prosemirror-keymap';
import { EditorState, TextSelection } from 'prosemirror-state';
import { EditorView } from 'prosemirror-view';

import type { MentionCandidate } from '../../chat/mentions/mention-typeahead';
import { softBrokenLines } from '../char-insert/char-typeahead-harness';
import '../jsdom-range-shim';
import { dialectSchema, parseMarkdown } from '../markdown-dialect';
import { createQueryMentionSource } from './mention-source';
import { mentionTypeaheadPlugin } from './mention-typeahead-plugin';

export interface MentionHarnessOptions {
  /** What the character list request answers with (v4's `respond`). */
  respond: () => Promise<readonly MentionCandidate[]>;
  /** The composer's `mentionPriorityCharacterIds`. */
  priorityCharacterIds?: readonly string[];
}

export interface MentionHarness {
  view: EditorView;
  /** One paragraph of `lines` joined by soft line breaks; caret at the end. */
  seed(...lines: string[]): void;
  /** Type at the caret, the way an input event would land. */
  type(text: string): void;
  /**
   * Fire a keydown through every plugin's `handleKeyDown`, in order — the
   * typeahead, then the keymaps — returning whether one consumed it.
   */
  pressKey(key: string, modifiers?: { shiftKey?: boolean }): { handled: boolean };
  /** Let the query settle (v4 `settle()` — four short ticks). */
  settle(): Promise<void>;
  /** The document as text: soft breaks `\n`, blocks `\n\n` (v4 `readText`). */
  text(): string;
  /** Caret offset within its textblock. */
  caretOffset(): number;
  /** The open menu's option labels, or `[]` when there are none. */
  optionLabels(): string[];
  /** The open menu's text (empty-state label included), or null when shut. */
  menuText(): string | null;
  undo(): void;
  destroy(): void;
}

export function mountMentionHarness(options: MentionHarnessOptions): MentionHarness {
  // No retries: v4's harness client fails a request at once, which is what
  // lets its error case assert within one settle.
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const source = createQueryMentionSource(queryClient, ['characters', 'list'], options.respond);
  const priority = options.priorityCharacterIds ?? [];

  const mount = document.createElement('div');
  document.body.appendChild(mount);

  const plugins = [
    mentionTypeaheadPlugin({ source: () => source, priorityCharacterIds: () => priority }),
    history(),
    keymap(baseKeymap),
  ];

  const view = new EditorView(mount, {
    state: EditorState.create({ doc: parseMarkdown(''), plugins }),
  });

  const tick = () => new Promise((resolve) => setTimeout(resolve, 5));

  return {
    view,
    seed(...lines: string[]) {
      const doc = dialectSchema.node('doc', null, [
        dialectSchema.node('paragraph', null, softBrokenLines(lines)),
      ]);
      view.updateState(EditorState.create({ doc, plugins }));
      view.dispatch(
        view.state.tr.setSelection(TextSelection.create(view.state.doc, doc.content.size - 1)),
      );
    },
    type(text: string) {
      view.dispatch(view.state.tr.insertText(text));
    },
    pressKey(key: string, modifiers = {}) {
      const event = new KeyboardEvent('keydown', {
        key,
        bubbles: true,
        cancelable: true,
        shiftKey: modifiers.shiftKey ?? false,
      });
      const handled = view.someProp('handleKeyDown', (f) => f(view, event) === true) === true;
      return { handled };
    },
    async settle() {
      for (let i = 0; i < 4; i += 1) await tick();
    },
    text() {
      return view.state.doc.textBetween(0, view.state.doc.content.size, '\n\n', (leaf) =>
        leaf.type.name === 'hard_break' ? '\n' : '',
      );
    },
    caretOffset() {
      return view.state.selection.$from.parentOffset;
    },
    optionLabels() {
      return Array.from(
        document.querySelectorAll('.qt-typeahead-menu [role="option"] .qt-typeahead-option-label'),
      ).map((row) => row.textContent ?? '');
    },
    menuText() {
      const menu = document.querySelector<HTMLElement>('.qt-typeahead-menu');
      return menu ? (menu.textContent ?? '') : null;
    },
    undo() {
      undo(view.state, view.dispatch);
    },
    destroy() {
      view.destroy();
      mount.remove();
      queryClient.clear();
    },
  };
}
