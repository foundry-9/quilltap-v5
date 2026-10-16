/**
 * The spec harness for the `/` command menu — v5's answer to v4's
 * `__tests__/helpers/lexicalPluginHarness` as `SlashCommandPlugin.test.tsx`
 * uses it, and a sibling of `mentions/mention-typeahead-harness.ts`.
 *
 * A REAL ProseMirror editor over the real dialect schema, the REAL plugin, the
 * REAL query-backed sources over a real `QueryClient` (the two list answers are
 * the test's to script, as v4 scripts `fetch`), `prosemirror-history`, and a
 * base keymap whose Enter splits the block — so "Enter reaches the editor" is
 * observable the way v4's is. The host's handlers are a STUB that records every
 * call.
 *
 * @module editor/slash/slash-command-harness
 */

import { QueryClient } from '@tanstack/angular-query-experimental';
import { baseKeymap } from 'prosemirror-commands';
import { history, undo } from 'prosemirror-history';
import { keymap } from 'prosemirror-keymap';
import { EditorState, TextSelection } from 'prosemirror-state';
import { EditorView } from 'prosemirror-view';

import type {
  ComposerAction,
  SlashBuiltInToolSource,
  SlashCustomToolSource,
} from '../../chat/slash-commands';
import { softBrokenLines } from '../char-insert/char-typeahead-harness';
import '../jsdom-range-shim';
import { dialectSchema, parseMarkdown } from '../markdown-dialect';
import {
  createQuerySlashToolSource,
  type SlashCommandHost,
  type SlashCustomToolRef,
} from './slash-command-host';
import { slashCommandPlugin } from './slash-command-plugin';

export interface SlashHarnessOptions {
  /** What Prospero's tool list answers with. */
  builtIns: () => Promise<readonly SlashBuiltInToolSource[]>;
  /** What Pascal's roster answers with. */
  customs: () => Promise<readonly SlashCustomToolSource[]>;
  /** The composer actions the host wires (v4's context `actions` keys). */
  actions?: readonly ComposerAction[];
  /** v4's `customToolsAvailable`. Default true. */
  customToolsAvailable?: boolean;
  /** False binds a `null` host — the setting off (v4 `composerSlashCommands: false`). */
  enabled?: boolean;
}

/** One recorded host call: the handler's name and its arguments. */
export type HostCall = [string, ...unknown[]];

export interface SlashHarness {
  view: EditorView;
  host: SlashCommandHost;
  /** Every host handler call, in order. */
  calls: HostCall[];
  /** Calls to one handler (`'attach'`, `'openRunTool'`, …), argument lists only. */
  callsTo(name: string): unknown[][];
  /** How many times each list was fetched. */
  fetches: { builtIns: number; customs: number };
  /** One paragraph of `lines` joined by soft line breaks; caret at the end. */
  seed(...lines: string[]): void;
  /** Type at the caret, the way an input event would land. */
  type(text: string): void;
  /** Fire a keydown through every plugin's `handleKeyDown`, in order. */
  pressKey(key: string): { handled: boolean };
  /** Let the queries settle (four short ticks). */
  settle(): Promise<void>;
  /** The document as text: soft breaks `\n`, blocks `\n\n`. */
  text(): string;
  /** The open menu's option labels, or `[]` when there are none. */
  optionLabels(): string[];
  /** The open menu's option glyphs. */
  optionGlyphs(): string[];
  /** The open menu's option details. */
  optionDetails(): string[];
  /** The open menu's text (empty-state label included), or null when shut. */
  menuText(): string | null;
  undo(): void;
  destroy(): void;
}

export function mountSlashHarness(options: SlashHarnessOptions): SlashHarness {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const calls: HostCall[] = [];
  const fetches = { builtIns: 0, customs: 0 };
  const record = (name: string, ...args: unknown[]) => calls.push([name, ...args]);

  const actions: Partial<Record<ComposerAction, () => void>> = {};
  for (const action of options.actions ?? []) actions[action] = () => record(action);

  const host: SlashCommandHost = {
    chatId: 'chat-1',
    actions,
    openRunTool: (toolId?: string) => record('openRunTool', toolId),
    openCustomTool: (initial?: SlashCustomToolRef) => record('openCustomTool', initial),
    runBuiltInNow: async (toolId: string) => {
      record('runBuiltInNow', toolId);
    },
    runCustomNow: async (tool: SlashCustomToolRef) => {
      record('runCustomNow', tool);
    },
    customToolsAvailable: () => options.customToolsAvailable ?? true,
    builtInTools: createQuerySlashToolSource(queryClient, {
      queryKey: ['tools', 'chat-1', 'schemas'],
      queryFn: () => {
        fetches.builtIns += 1;
        return options.builtIns();
      },
      select: (tools) => tools,
      staleTime: 60_000,
    }),
    customTools: createQuerySlashToolSource(queryClient, {
      queryKey: ['customTools', 'chat-1'],
      queryFn: () => {
        fetches.customs += 1;
        return options.customs();
      },
      select: (tools) => tools,
      staleTime: 0,
    }),
  };
  const enabled = options.enabled ?? true;

  const mount = document.createElement('div');
  document.body.appendChild(mount);

  const plugins = [
    slashCommandPlugin({ host: () => (enabled ? host : null) }),
    history(),
    keymap(baseKeymap),
  ];

  const view = new EditorView(mount, {
    state: EditorState.create({ doc: parseMarkdown(''), plugins }),
  });

  const tick = () => new Promise((resolve) => setTimeout(resolve, 5));
  const rows = (selector: string) =>
    Array.from(document.querySelectorAll(`.qt-typeahead-menu [role="option"] ${selector}`)).map(
      (el) => el.textContent ?? '',
    );

  return {
    view,
    host,
    calls,
    callsTo(name: string) {
      return calls.filter(([n]) => n === name).map(([, ...args]) => args);
    },
    fetches,
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
    pressKey(key: string) {
      const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true });
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
    optionLabels: () => rows('.qt-typeahead-option-label'),
    optionGlyphs: () => rows('.qt-typeahead-option-glyph'),
    optionDetails: () => rows('.qt-typeahead-option-detail'),
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
