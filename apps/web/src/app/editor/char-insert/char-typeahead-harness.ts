/**
 * The spec harness for the two char-typeahead suites — v5's answer to v4's
 * `__tests__/helpers/lexicalPluginHarness`.
 *
 * A REAL ProseMirror editor over the real dialect schema, the REAL committed
 * dataset served through a stubbed `fetch` exactly as the browser would fetch
 * it, and the real `textReplacementPlugin` beneath the typeahead in v4's plugin
 * order — so the suites exercise the whole adapter: lazy load, trigger
 * detection, the menu-free commit, the bail conditions, the undo contract and
 * the Layer 1.5 interaction.
 *
 * @module editor/char-insert/char-typeahead-harness
 */

import { history, undo } from 'prosemirror-history';
import type { Node } from 'prosemirror-model';
import { EditorState, TextSelection } from 'prosemirror-state';
import { EditorView } from 'prosemirror-view';

import '../jsdom-range-shim';
import { dialectSchema, parseMarkdown } from '../markdown-dialect';
import { textReplacementPlugin, type CompiledRules } from '../text-replacement';
import { charTypeaheadPlugin } from './char-typeahead-plugin';
import type { CharProfile } from './types';

export interface HarnessOptions {
  profile: CharProfile;
  /** The profile's settings toggle (v4's `chat_settings` flag). */
  enabled?: boolean;
  /** Compiled Layer 1.5 rules, to exercise the priority interaction. */
  rules?: CompiledRules | null;
}

export interface Harness {
  view: EditorView;
  /**
   * Replace the document with ONE paragraph of literal text and put the caret at
   * the very end (v4 `seedParagraph`).
   *
   * Literal, NOT parsed as markdown: half these cases are about backslashes
   * (`literal \*`, `C:\User`), and a markdown parse would eat the escape before
   * the plugin ever saw it.
   */
  seed(text: string): void;
  /**
   * A paragraph whose trigger text follows a bold run (v4 `seedAfterBoldRun`) —
   * `**bold**:smile` must not open a menu.
   */
  seedAfterBoldRun(bold: string, text: string): void;
  /** A fenced code block (v4 `seedCodeBlock`). */
  seedCodeBlock(text: string): void;
  /** An inline `code` run (v4 `seedInlineCode`). */
  seedInlineCode(text: string): void;
  /** Fire a keydown at the editor, returning whether the plugin consumed it. */
  pressKey(key: string): { handled: boolean };
  /**
   * Put the view into (or out of) an IME composition. `EditorView.composing` is
   * a getter over private state, so the flag is overridden on the instance —
   * the v5 stand-in for v4 seeding Lexical's composition key.
   */
  setComposing(composing: boolean): void;
  /** The document as the dialect serializes it back — here, plain text. */
  text(): string;
  /** Caret offset within its textblock. */
  caretOffset(): number;
  /** The open menu's option labels, or null when the menu is shut. */
  menuRows(): string[] | null;
  undo(): void;
  destroy(): void;
}

/** Fetch counts per URL fragment, so a suite can prove "fetched exactly once". */
export interface FetchStub {
  datasetFetches(): number;
  restore(): void;
}

/**
 * Route the dataset URL to the committed asset and everything else to a 404,
 * the way the browser would. `datasetFails` makes the dataset answer 500, for
 * the silent-failure arm.
 */
export function stubFetch(
  urlFragment: string,
  payload: unknown,
  { datasetFails = false } = {},
): FetchStub {
  const real = globalThis.fetch;
  let count = 0;

  const jsonResponse = (body: unknown, status = 200) =>
    ({
      ok: status >= 200 && status < 300,
      status,
      statusText: status === 200 ? 'OK' : 'Error',
      json: async () => body,
      text: async () => JSON.stringify(body),
    }) as unknown as Response;

  // A plain closure rather than `vi.fn`: this helper is compiled by the app
  // build too (it is not a `.spec.ts`), and importing vitest here would drag a
  // test runner into the production type-check.
  globalThis.fetch = (async (input: RequestInfo | URL) => {
    const url =
      typeof input === 'string' ? input : input instanceof URL ? input.href : (input as Request).url;
    if (url.includes(urlFragment)) {
      count += 1;
      if (datasetFails) return jsonResponse({ error: 'nope' }, 500);
      return jsonResponse(payload);
    }
    return jsonResponse({}, 404);
  }) as unknown as typeof fetch;

  return {
    datasetFetches: () => count,
    restore: () => {
      globalThis.fetch = real;
    },
  };
}

export function mountHarness(options: HarnessOptions): Harness {
  const enabled = options.enabled ?? true;
  const rules = options.rules ?? null;

  const mount = document.createElement('div');
  document.body.appendChild(mount);

  // v4's plugin order: the typeahead sits ABOVE text replacement (NORMAL over
  // LOW), which in ProseMirror is simply "earlier in the plugin array".
  const plugins = [
    charTypeaheadPlugin({ profile: options.profile, enabled: () => enabled }),
    textReplacementPlugin(() => rules),
    history(),
  ];

  const view = new EditorView(mount, {
    state: EditorState.create({ doc: parseMarkdown(''), plugins }),
  });

  /** Install a fresh single-block document and drop the caret at its end. */
  const install = (block: Node) => {
    const doc = dialectSchema.node('doc', null, [block]);
    view.updateState(EditorState.create({ doc, plugins }));
    view.dispatch(
      view.state.tr.setSelection(TextSelection.create(view.state.doc, doc.content.size - 1)),
    );
  };

  const paragraph = (content: Node[]) => dialectSchema.node('paragraph', null, content);

  return {
    view,
    seed(text: string) {
      install(paragraph(text ? [dialectSchema.text(text)] : []));
    },
    seedAfterBoldRun(bold: string, text: string) {
      install(
        paragraph([
          dialectSchema.text(bold, [dialectSchema.marks['strong'].create()]),
          dialectSchema.text(text),
        ]),
      );
    },
    seedCodeBlock(text: string) {
      install(
        dialectSchema.node('code_block', null, text ? [dialectSchema.text(text)] : []),
      );
    },
    seedInlineCode(text: string) {
      install(paragraph([dialectSchema.text(text, [dialectSchema.marks['code'].create()])]));
    },
    setComposing(composing: boolean) {
      Object.defineProperty(view, 'composing', { value: composing, configurable: true });
    },
    pressKey(key: string) {
      const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true });
      const handled = view.someProp('handleKeyDown', (f) => f(view, event) === true) === true;
      return { handled: handled === true };
    },
    text() {
      return view.state.doc.textBetween(0, view.state.doc.content.size, '\n');
    },
    caretOffset() {
      return view.state.selection.$from.parentOffset;
    },
    menuRows() {
      const menu = document.querySelector<HTMLElement>('.qt-typeahead-menu');
      if (!menu) return null;
      return Array.from(menu.querySelectorAll('.qt-typeahead-option-label')).map(
        (row) => row.textContent ?? '',
      );
    },
    undo() {
      undo(view.state, view.dispatch);
    },
    destroy() {
      view.destroy();
      mount.remove();
    },
  };
}

/** The dialect schema, for specs that need to build a document by hand. */
export { dialectSchema };
