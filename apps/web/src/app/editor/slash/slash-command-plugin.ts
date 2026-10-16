/**
 * The composer's `/` command menu (v4
 * `components/chat/lexical/plugins/SlashCommandPlugin.tsx`, `4877fc2da`).
 *
 * Type `/` at the start of a word and a menu of commands opens beneath the
 * cursor: the composer's own actions (attach, library, image, announce, mail,
 * inform, the dice), Prospero's user-invocable tools, and Pascal's custom tools
 * for this chat. Enter, Tab or a click takes the highlighted command; the
 * `/query` leaves the composer in one undoable step, and then the command runs —
 * at once when a tool has nothing to complete, otherwise in its existing dialog,
 * opened straight onto that tool's form.
 *
 * Which commands exist, where a trigger starts and how matches rank are all
 * decided in `chat/slash-commands/` (v4 `lib/slash-commands/`); this file asks
 * and applies. Space is not a commit key — commands take no arguments, so a
 * space falls out of the query alphabet, the trigger dissolves on the next
 * refresh and the text stays as prose. Escape closes the menu and leaves the
 * text.
 *
 * ### How v4's Lexical machinery maps onto ProseMirror
 *
 * - **Mounting.** v4 returns `null` from the plugin while `composerSlashCommands`
 *   is off, so Enter reaches the editor. Here the host is read live and a
 *   `null` host is inert — the composer binds `null` while the setting is off
 *   (and every other editor host never binds one).
 * - **`$isGluedToPreviousRun`.** No twin needed: `textBeforeCursor` reads the
 *   whole textblock up to the caret (`trigger-context.ts`).
 * - **`HoldCommitKeys`.** While a list is still loading and nothing matches,
 *   Enter and Tab are swallowed — "so `/self_inv` + Enter typed before the
 *   tools arrive does not send the draft".
 * - **The commit's undo shape.** v4's `SLASH_COMMAND_TAG` is "deliberately NOT
 *   `HISTORY_MERGE_TAG`: merging would fold the removal into the typing that
 *   produced `/query`, so an undo would take the whole word away instead of
 *   bringing it back". Here the deletion is dispatched with `closeHistory` and
 *   followed by an empty `closeHistory` transaction (the `@` typeahead's commit
 *   precedent), so it is its own history event and ONE undo restores `/atta`.
 * - **Dispatch after the commit.** v4 dispatches in the update's `onUpdate`, "so
 *   a dialog opening never races the editor's own selection restore".
 *   ProseMirror's `dispatch` is synchronous, so calling
 *   {@link dispatchSlashCommand} after it returns gives the same order by
 *   construction.
 * - **The glyph.** v4 renders an `<Icon>` per row (`:75-92`): wrench for a
 *   Prospero tool, wand for Pascal's, the action's own for the composer's, dice
 *   otherwise. The shared menu is plain DOM, so it emits the `qt-icon` span
 *   itself (`TypeaheadRow.iconName`) — the same names, the theme's icon set.
 * - **The listbox id.** v4's `slash-command-listbox` takes v5's `qt-` prefix
 *   (`SLASH_LISTBOX_ID_PREFIX`, the `qt-mention-typeahead-listbox` convention).
 *
 * Registered after the `@` typeahead and above smart typography, text
 * replacement and the keymaps — v4's `LexicalComposerWrapper` mounts it after
 * `MentionTypeaheadPlugin` at `COMMAND_PRIORITY_CRITICAL`.
 *
 * @module editor/slash/slash-command-plugin
 */

import { closeHistory } from 'prosemirror-history';
import { Plugin, type EditorState } from 'prosemirror-state';
import type { EditorView } from 'prosemirror-view';

import {
  buildSlashCommands,
  findSlashTrigger,
  rankSlashCommands,
  type ComposerAction,
  type SlashCommand,
} from '../../chat/slash-commands';
import type { IconName } from '../../ui/icon';
import { textBeforeCursor } from '../char-insert/trigger-context';
import { TypeaheadMenu, type TypeaheadRow } from '../char-insert/typeahead-menu';
import type { SlashCommandHost } from './slash-command-host';

/** Rows for a typed query; a bare `/` lists everything and the menu scrolls. */
export const MENU_LIMIT = 12;

export const LISTBOX_ID = 'qt-slash-command-listbox';
export const EMPTY_LABEL = 'No such command';
export const LOADING_LABEL = 'Consulting the tool room…';

/** Actions derived from the dialog openers rather than the composer's handler map. */
export const DIALOG_ACTIONS: ReadonlySet<ComposerAction> = new Set(['tool', 'roll', 'pascal']);

/** v4 `ACTION_ICONS` (`:75-84`). */
export const ACTION_ICONS: Readonly<Partial<Record<ComposerAction, IconName>>> = {
  attach: 'paperclip',
  library: 'file-plus',
  image: 'camera',
  announce: 'megaphone',
  mail: 'mail',
  inform: 'info',
  tool: 'wrench',
  pascal: 'wand',
};

/** v4 `glyphFor` (`:86-92`). */
export function slashIconFor(command: SlashCommand): IconName {
  if (command.payload.kind === 'builtin-tool') return 'wrench';
  if (command.payload.kind === 'custom-tool') return 'wand';
  return ACTION_ICONS[command.payload.action] ?? 'dice';
}

export interface SlashCommandPluginOptions {
  /**
   * The composer's handlers and lists, or null for a host that does not offer
   * the menu (every editor but the Salon composer, and the composer itself
   * while `composerSlashCommands` is off). Read live.
   */
  host: () => SlashCommandHost | null;
}

/** Run a picked command against the composer's handlers (v4 `:103-123`, verbatim). */
export function dispatchSlashCommand(command: SlashCommand, host: SlashCommandHost): void {
  const { payload } = command;
  switch (payload.kind) {
    case 'action':
      if (payload.action === 'tool') host.openRunTool();
      else if (payload.action === 'roll') host.openRunTool('rng');
      else if (payload.action === 'pascal') host.openCustomTool();
      else host.actions[payload.action]?.();
      return;
    case 'builtin-tool':
      if (command.needsDialog) host.openRunTool(payload.toolId);
      else void host.runBuiltInNow(payload.toolId);
      return;
    case 'custom-tool': {
      const tool = { name: payload.name, asCharacterId: payload.asCharacterId };
      if (command.needsDialog) host.openCustomTool(tool);
      else void host.runCustomNow(tool);
      return;
    }
  }
}

/** The trigger, resolved against the document. */
interface LiveTrigger {
  query: string;
  /** Document position of the `/`. */
  from: number;
  /** Document position one past the last query character. */
  to: number;
}

function liveTrigger(state: EditorState): LiveTrigger | null {
  const current = textBeforeCursor(state);
  if (!current) return null;
  const match = findSlashTrigger(current.text);
  if (!match) return null;
  return {
    query: match.query,
    from: current.blockStart + match.start,
    to: current.blockStart + match.end,
  };
}

/**
 * The commands this host offers right now (v4 `:192-204`): the wired actions
 * minus the dialog ones, then `tool` and `roll` always and `pascal` only while
 * Pascal has a roster; built-ins from the list; customs only while available.
 */
function commandsFor(host: SlashCommandHost, available: boolean): SlashCommand[] {
  const wired = Object.keys(host.actions) as ComposerAction[];
  const actions = new Set<ComposerAction>(wired.filter((action) => !DIALOG_ACTIONS.has(action)));
  actions.add('tool');
  actions.add('roll');
  if (available) actions.add('pascal');
  return buildSlashCommands({
    actions,
    builtInTools: host.builtInTools.snapshot().tools ?? [],
    customTools: available ? (host.customTools.snapshot().tools ?? []) : [],
  });
}

/**
 * One `/` menu. The menu, the open trigger and the highlight are PER VIEW (the
 * char typeahead's reasoning: a WeakMap keyed by the view is the honest scope).
 */
export function slashCommandPlugin(options: SlashCommandPluginOptions): Plugin {
  interface Runtime {
    menu: TypeaheadMenu;
    rows: SlashCommand[];
    selected: number;
    openQuery: string | null;
    openTrigger: LiveTrigger | null;
    /** Enter / Tab are held: a list is still loading and nothing matches. */
    holdCommitKeys: boolean;
    /** The host whose two sources this view follows. */
    tracked: SlashCommandHost | null;
    unsubscribe: (() => void) | null;
    destroyed: boolean;
  }

  const runtimes = new WeakMap<EditorView, Runtime>();

  function deactivate(host: SlashCommandHost | null): void {
    host?.builtInTools.setActive(false);
    host?.customTools.setActive(false);
  }

  function closeMenu(runtime: Runtime): void {
    runtime.openQuery = null;
    runtime.openTrigger = null;
    runtime.rows = [];
    runtime.selected = 0;
    runtime.holdCommitKeys = false;
    runtime.menu.close();
    deactivate(runtime.tracked);
  }

  /**
   * Follow the host's two sources, re-rendering an OPEN menu whenever either
   * snapshot moves. A list landing never opens a shut menu: v4's menu opens
   * only from an editor update, so Escape (whose own `setActive(false)`
   * notifies) leaves it shut until the writer next types or moves the caret.
   */
  function track(view: EditorView, runtime: Runtime, host: SlashCommandHost | null): void {
    if (runtime.tracked === host) return;
    runtime.unsubscribe?.();
    deactivate(runtime.tracked);
    runtime.tracked = host;
    if (!host) {
      runtime.unsubscribe = null;
      return;
    }
    const onChange = () => {
      if (runtime.openTrigger !== null) refresh(view);
    };
    const offBuiltIns = host.builtInTools.subscribe(onChange);
    const offCustoms = host.customTools.subscribe(onChange);
    runtime.unsubscribe = () => {
      offBuiltIns();
      offCustoms();
    };
  }

  function refresh(view: EditorView): void {
    const runtime = runtimes.get(view);
    if (!runtime || runtime.destroyed) return;

    const host = options.host();
    track(view, runtime, host);
    if (!host || view.composing || !view.editable) {
      closeMenu(runtime);
      return;
    }

    const trigger = liveTrigger(view.state);
    if (!trigger) {
      closeMenu(runtime);
      return;
    }

    const available = host.customToolsAvailable();
    host.builtInTools.setActive(true);
    host.customTools.setActive(available);

    if (trigger.query !== runtime.openQuery) runtime.selected = 0;
    runtime.openQuery = trigger.query;
    runtime.openTrigger = trigger;
    runtime.rows = rankSlashCommands(commandsFor(host, available), trigger.query, MENU_LIMIT);
    if (runtime.selected >= runtime.rows.length) runtime.selected = 0;

    // Only a list with nothing in it yet is "loading" — cached tools show at once.
    const builtIns = host.builtInTools.snapshot();
    const customs = host.customTools.snapshot();
    const loading =
      (builtIns.isPending && !builtIns.tools) || (available && customs.isPending && !customs.tools);
    runtime.holdCommitKeys = loading && runtime.rows.length === 0;
    runtime.menu.setEmptyLabel(loading ? LOADING_LABEL : EMPTY_LABEL);

    const caret = view.coordsAtPos(view.state.selection.from);
    runtime.menu.render(
      runtime.rows.map(toRow),
      runtime.rows.length > 0 ? runtime.selected : -1,
      { left: caret.left, top: caret.top, bottom: caret.bottom },
    );
  }

  function commitRow(view: EditorView, index: number): void {
    const runtime = runtimes.get(view);
    if (!runtime) return;
    // Read everything the dispatch needs BEFORE the commit moves the state on.
    const command = runtime.rows[index];
    const trigger = runtime.openTrigger;
    const host = options.host();
    if (!command || !trigger || !host) return;
    closeMenu(runtime);

    const tr = view.state.tr.delete(trigger.from, trigger.to);
    closeHistory(tr);
    view.dispatch(tr.scrollIntoView());
    // End the removal's history event, so the next keystroke is its own undo
    // step and one undo brings `/query` back (see the header).
    view.dispatch(closeHistory(view.state.tr));
    view.focus();
    dispatchSlashCommand(command, host);
  }

  /**
   * Keys the OPEN menu owns — consumed only while it is open, so with the menu
   * shut Enter still submits and Tab still moves focus. An empty menu owns
   * Escape, and Enter / Tab only while a list is still loading.
   */
  function handleMenuKey(view: EditorView, event: KeyboardEvent): boolean {
    const runtime = runtimes.get(view);
    if (!runtime || runtime.openTrigger === null) return false;

    switch (event.key) {
      case 'ArrowDown':
        if (runtime.rows.length === 0) return false;
        runtime.selected = (runtime.selected + 1) % runtime.rows.length;
        refresh(view);
        return true;
      case 'ArrowUp':
        if (runtime.rows.length === 0) return false;
        runtime.selected = (runtime.selected - 1 + runtime.rows.length) % runtime.rows.length;
        refresh(view);
        return true;
      case 'Enter':
      case 'Tab':
        if (runtime.rows.length === 0) return runtime.holdCommitKeys;
        commitRow(view, runtime.selected);
        return true;
      case 'Escape':
        closeMenu(runtime);
        return true;
      default:
        return false;
    }
  }

  return new Plugin({
    view(editorView) {
      const runtime: Runtime = {
        menu: new TypeaheadMenu({
          listboxId: LISTBOX_ID,
          emptyLabel: EMPTY_LABEL,
          activeDescendantTarget: editorView.dom as HTMLElement,
          onSelect: (index) => commitRow(editorView, index),
          onHighlight: (index) => {
            const current = runtimes.get(editorView);
            if (!current) return;
            current.selected = index;
            refresh(editorView);
          },
        }),
        rows: [],
        selected: 0,
        openQuery: null,
        openTrigger: null,
        holdCommitKeys: false,
        tracked: null,
        unsubscribe: null,
        destroyed: false,
      };
      runtimes.set(editorView, runtime);

      return {
        update: (view) => refresh(view),
        destroy: () => {
          runtime.destroyed = true;
          closeMenu(runtime);
          runtime.unsubscribe?.();
          runtime.menu.destroy();
          runtimes.delete(editorView);
        },
      };
    },
    props: {
      handleKeyDown(view, event) {
        if (!handleMenuKey(view, event)) return false;
        event.preventDefault();
        return true;
      },
      handleDOMEvents: {
        /** A caret-anchored menu has no host to hide it (as the char typeahead). */
        blur: (view) => {
          const runtime = runtimes.get(view);
          if (runtime) closeMenu(runtime);
          return false;
        },
      },
    },
  });
}

/**
 * One menu row (v4 `:206-220`): `/${command} — ${label}` with the source as the
 * muted detail. The row key folds in everything the row shows, because the
 * shared menu reuses rows whose keys are unchanged — and a tool's title can
 * change under a stable key.
 */
function toRow(command: SlashCommand): TypeaheadRow {
  const label = `/${command.command} — ${command.label}`;
  return {
    key: JSON.stringify([command.key, label, command.source]),
    iconName: slashIconFor(command),
    label,
    detail: command.source,
  };
}
