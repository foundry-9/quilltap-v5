/**
 * The composer's `@` character typeahead (v4
 * `components/chat/lexical/plugins/MentionTypeaheadPlugin.tsx`, `3376b3dfa`).
 *
 * Type `@` at the start of a word and a list of characters opens beneath the
 * cursor, narrowing as you type. Enter, Tab or a click takes the highlighted
 * name (the first, unless you have arrowed elsewhere); Space does the same once
 * at least one letter follows the `@`, and keeps its space. A bare `@` followed
 * by a space is left alone — `meet me @ 5` is not a summons.
 *
 * What lands is the character's plain name. The `@` goes, with one exception:
 * at the start of a line, where `@Name: question` / `@Name? question` is a
 * Carina query. There the `@` stays provisionally and the next keystrokes
 * decide — a `:` or `?` followed by a space keeps it, anything else drops it.
 * The rule lives in `classifyLineStartMention` (`chat/mentions/`); this file is
 * the adapter, on the char typeahead's shape (`char-insert/char-typeahead-
 * plugin.ts`) and sharing its menu surface and cursor helper.
 *
 * ### How v4's Lexical machinery maps onto ProseMirror
 *
 * - **Line structure.** v4 finds a line start from a text node whose previous
 *   sibling is null or a `LineBreakNode`, in a top-level paragraph, and counts
 *   lines by `LineBreakNode`s. Here a soft line break is a `hard_break` leaf
 *   that `textBeforeCursor` reads as `\n` — so "opens a line" is "the opener is
 *   at offset 0 or after a `\n`", the paragraph must be a direct child of the
 *   doc, and a line's text is the paragraph's text split on `\n`.
 * - **The watched paragraph.** v4 remembers it by Lexical node key; a
 *   ProseMirror node has no identity, so it is remembered by POSITION, mapped
 *   through every transaction (a deleted paragraph maps to "gone" → abandon).
 * - **The verdict watcher.** v4's update listener becomes this plugin's state
 *   field (which judges after each transaction) plus `appendTransaction`
 *   (which applies a `strip`). An appended transaction joins the triggering
 *   keystroke's history event (`prosemirror-history` never opens a new group
 *   for one), which is exactly v4's `HISTORY_MERGE_TAG`: one undo takes back
 *   the keystroke and the dropped `@` together. The strip carries
 *   {@link MENTION_STRIP_META}, which the watcher ignores, as v4 ignores its
 *   own merge-tagged update.
 * - **Composition.** v4 skips an update while the editor is composing; here a
 *   transaction carrying ProseMirror's `composition` meta is skipped.
 * - **The undo re-arm.** v4 re-arms on a `HISTORIC_TAG` update; here on a
 *   transaction `isHistoryTransaction` recognises (undo or redo).
 * - **The completion's own history.** v4's completion update is its own undo
 *   step and the next keystroke starts a fresh one (Lexical merges only runs of
 *   typed characters). `prosemirror-history` would fold a keystroke typed within
 *   500 ms into the completion's event, so the completion is dispatched with
 *   `closeHistory` and followed by an empty `closeHistory` transaction that
 *   ends its event.
 *
 * Registered after the two char typeaheads and above smart typography, text
 * replacement and the keymaps, as v4 mounts it after the two
 * `CharTypeaheadPlugin`s at `COMMAND_PRIORITY_CRITICAL` — Enter must reach
 * this plugin before the composer's submit.
 *
 * @module editor/mentions/mention-typeahead-plugin
 */

import type { Node } from 'prosemirror-model';
import { closeHistory, isHistoryTransaction } from 'prosemirror-history';
import { Plugin, PluginKey, type EditorState, type Transaction } from 'prosemirror-state';
import type { EditorView } from 'prosemirror-view';

import {
  canKeepLineStartAt,
  classifyLineStartMention,
  findMentionTrigger,
  mentionCandidatesFor,
  rankMentionCandidates,
  type LineStartMentionVerdict,
  type MentionCandidate,
} from '../../chat/mentions/mention-typeahead';
import { textBeforeCursor, triggerLeafText } from '../char-insert/trigger-context';
import { TypeaheadMenu, type TypeaheadRow } from '../char-insert/typeahead-menu';
import type { MentionCharacterSource } from './mention-source';

/** Rows visible at once; the menu scrolls with the keyboard beyond this. */
export const MENU_LIMIT = 10;

export const LISTBOX_ID = 'qt-mention-typeahead-listbox';
export const EMPTY_LABEL = 'No such personage in the register';
export const LOADING_LABEL = 'Consulting the register\u2026';
export const ERROR_LABEL = 'The register could not be reached';

/** Transaction meta on the `@`-dropping transaction (v4 `HISTORY_MERGE_TAG`). */
export const MENTION_STRIP_META = 'mention-strip';

export interface MentionTypeaheadOptions {
  /**
   * The character list, or null for a host that does not offer the typeahead
   * (every editor but the Salon composer). Read live.
   */
  source: () => MentionCharacterSource | null;
  /** Characters to list first — normally the current chat's cast. Read live. */
  priorityCharacterIds: () => readonly string[];
}

/** A line-start `@Name` whose `@` is awaiting (or has had) its verdict. */
interface WatchedLine {
  /** Position just before the paragraph; `-1` once the paragraph is gone. */
  paragraphPos: number;
  /** Which `\n`-separated line of the paragraph (soft line breaks count). */
  lineIndex: number;
  name: string;
}

interface WatchState {
  /** The line-start `@Name` awaiting a keep-or-strip verdict, if any. */
  pending: WatchedLine | null;
  /**
   * The last line-start `@Name` to receive a verdict. An undo can put that
   * line back exactly as it stood while undecided; this is what re-arms the
   * watch.
   */
  lastJudged: WatchedLine | null;
  /** A `strip` verdict `appendTransaction` has yet to apply. */
  stripDue: WatchedLine | null;
}

type WatchMeta = { complete: WatchedLine } | { stripDone: true };

const watchKey = new PluginKey<WatchState>('mention-typeahead-watch');

function mapLine(line: WatchedLine | null, tr: Transaction): WatchedLine | null {
  if (!line || !tr.docChanged || line.paragraphPos < 0) return line;
  const mapped = tr.mapping.mapResult(line.paragraphPos, 1);
  return { ...line, paragraphPos: mapped.deleted ? -1 : mapped.pos };
}

/** A soft line break reads as `\n`, as in `textBeforeCursor` (v4 `getTextContent`). */
function paragraphText(paragraph: Node): string {
  return paragraph.textBetween(0, paragraph.content.size, undefined, triggerLeafText);
}

function paragraphAt(doc: Node, pos: number): Node | null {
  if (pos < 0 || pos >= doc.content.size) return null;
  const node = doc.nodeAt(pos);
  return node && node.type.name === 'paragraph' ? node : null;
}

function readVerdict(doc: Node, watched: WatchedLine): LineStartMentionVerdict {
  const paragraph = paragraphAt(doc, watched.paragraphPos);
  if (!paragraph) return 'abandon';
  const line = paragraphText(paragraph).split('\n')[watched.lineIndex] ?? '';
  return classifyLineStartMention(line, watched.name);
}

/**
 * The document range of line `lineIndex`'s first character, when the line
 * starts with `@` (v4 `$firstNodeOfLine` + the `startsWith('@')` check).
 */
function leadingAtRange(doc: Node, watched: WatchedLine): { from: number; to: number } | null {
  const paragraph = paragraphAt(doc, watched.paragraphPos);
  if (!paragraph) return null;
  const text = paragraphText(paragraph);
  let offset = 0;
  for (let line = 0; line < watched.lineIndex; line += 1) {
    const next = text.indexOf('\n', offset);
    if (next < 0) return null;
    offset = next + 1;
  }
  if (text[offset] !== '@') return null;
  // `textBetween` is 1:1 with positions (one char per leaf), and the content of
  // a paragraph starts one position after the position before it.
  const from = watched.paragraphPos + 1 + offset;
  return { from, to: from + 1 };
}

/** The trigger, resolved against the document rather than a bare string. */
interface LiveTrigger {
  query: string;
  /** Document position of the opener. */
  from: number;
  /** Document position one past the last query character. */
  to: number;
  /** The opener starts a line (offset 0, or right after a soft break). */
  opensLine: boolean;
  /** …of a top-level paragraph — the only place a Carina query can live. */
  atLineStart: boolean;
  paragraphPos: number;
  lineIndex: number;
}

function liveTrigger(state: EditorState): LiveTrigger | null {
  const current = textBeforeCursor(state);
  if (!current) return null;
  const match = findMentionTrigger(current.text);
  if (!match) return null;

  const $from = state.selection.$from;
  const opensLine = match.start === 0 || current.text[match.start - 1] === '\n';
  // Lists, quotes, headings and table cells all put markdown before the `@`,
  // so only a paragraph directly under the doc qualifies (v4 `$lineStartOf`).
  const topParagraph = $from.parent.type.name === 'paragraph' && $from.depth === 1;
  const before = current.text.slice(0, match.start);
  return {
    query: match.query,
    from: current.blockStart + match.start,
    to: current.blockStart + match.end,
    opensLine,
    atLineStart: opensLine && topParagraph,
    paragraphPos: $from.before(),
    lineIndex: before.split('\n').length - 1,
  };
}

function watchStateField(): Plugin<WatchState>['spec']['state'] {
  return {
    init: () => ({ pending: null, lastJudged: null, stripDue: null }),
    apply(tr, value, _oldState, newState) {
      let pending = mapLine(value.pending, tr);
      let lastJudged = mapLine(value.lastJudged, tr);
      let stripDue = mapLine(value.stripDue, tr);

      const meta = tr.getMeta(watchKey) as WatchMeta | undefined;
      if (meta && 'complete' in meta) {
        return { pending: meta.complete, lastJudged: null, stripDue: null };
      }
      if (meta && 'stripDone' in meta) stripDue = null;

      // v4 ignores its own merge-tagged strip and every update mid-composition.
      if (tr.getMeta(MENTION_STRIP_META) || tr.getMeta('composition') !== undefined) {
        return { pending, lastJudged, stripDue };
      }

      if (!pending && lastJudged && isHistoryTransaction(tr)) {
        if (readVerdict(newState.doc, lastJudged) === 'pending') pending = lastJudged;
        return { pending, lastJudged, stripDue };
      }

      if (!pending) return { pending, lastJudged, stripDue };

      const verdict = readVerdict(newState.doc, pending);
      if (verdict === 'pending') return { pending, lastJudged, stripDue };
      return { pending: null, lastJudged: pending, stripDue: verdict === 'strip' ? pending : null };
    },
  };
}

/**
 * One `@` typeahead. The menu, the open trigger and the highlight are PER VIEW
 * (see `charTypeaheadPlugin` for why a WeakMap keyed by the view is the honest
 * scope); the pending line-start verdict lives in the editor STATE, because it
 * must follow the document through undo and redo.
 */
export function mentionTypeaheadPlugin(options: MentionTypeaheadOptions): Plugin<WatchState> {
  interface Runtime {
    menu: TypeaheadMenu;
    rows: MentionCandidate[];
    selected: number;
    openQuery: string | null;
    openTrigger: LiveTrigger | null;
    /** Enter / Tab are held: the list is still loading and offers nothing. */
    holdCommitKeys: boolean;
    subscribed: MentionCharacterSource | null;
    unsubscribe: (() => void) | null;
    destroyed: boolean;
  }

  const runtimes = new WeakMap<EditorView, Runtime>();

  function closeMenu(runtime: Runtime): void {
    runtime.openQuery = null;
    runtime.openTrigger = null;
    runtime.rows = [];
    runtime.selected = 0;
    runtime.holdCommitKeys = false;
    runtime.menu.close();
    runtime.subscribed?.setActive(false);
  }

  /** Follow the host's source, re-rendering whenever its snapshot moves. */
  function track(view: EditorView, runtime: Runtime, source: MentionCharacterSource | null): void {
    if (runtime.subscribed === source) return;
    runtime.unsubscribe?.();
    runtime.subscribed?.setActive(false);
    runtime.subscribed = source;
    runtime.unsubscribe = source ? source.subscribe(() => refresh(view)) : null;
  }

  function refresh(view: EditorView): void {
    const runtime = runtimes.get(view);
    if (!runtime || runtime.destroyed) return;

    const source = options.source();
    track(view, runtime, source);
    if (!source || view.composing || !view.editable) {
      closeMenu(runtime);
      return;
    }

    const trigger = liveTrigger(view.state);
    if (!trigger) {
      closeMenu(runtime);
      return;
    }

    // A just-completed line-start `@Name` is still a valid trigger; reopening
    // the menu on it would make the next Enter re-pick instead of send.
    const pending = watchKey.getState(view.state)?.pending ?? null;
    if (pending && trigger.opensLine && trigger.query === pending.name) {
      closeMenu(runtime);
      return;
    }

    source.setActive(true);
    const { characters, isPending, isError } = source.snapshot();
    const prioritySet = new Set(options.priorityCharacterIds());

    if (trigger.query !== runtime.openQuery) runtime.selected = 0;
    runtime.openQuery = trigger.query;
    runtime.openTrigger = trigger;
    runtime.rows = rankMentionCandidates(
      mentionCandidatesFor(characters ?? [], trigger.atLineStart),
      trigger.query,
      prioritySet,
      MENU_LIMIT,
    );
    if (runtime.selected >= runtime.rows.length) runtime.selected = 0;

    // Only a list with nothing in it yet is "loading" — a cached list shows at once.
    const loading = isPending && !characters;
    runtime.holdCommitKeys = loading && runtime.rows.length === 0;
    runtime.menu.setEmptyLabel(
      loading ? LOADING_LABEL : isError && !characters ? ERROR_LABEL : EMPTY_LABEL,
    );

    const caret = view.coordsAtPos(view.state.selection.from);
    runtime.menu.render(
      runtime.rows.map((character) => toRow(character, prioritySet)),
      runtime.rows.length > 0 ? runtime.selected : -1,
      { left: caret.left, top: caret.top, bottom: caret.bottom },
    );
  }

  function commitRow(view: EditorView, index: number, withSpace: boolean): void {
    const runtime = runtimes.get(view);
    if (!runtime) return;
    const character = runtime.rows[index];
    const trigger = runtime.openTrigger;
    if (!character || !trigger) return;
    closeMenu(runtime);

    const name = character.name;
    const tr = view.state.tr;
    // `@Name ` can never become a Carina query, so a Space commit drops the `@`
    // even at the start of a line — as does a name the Carina parser cannot
    // address (`Jean-Luc`, `Zoë`).
    if (trigger.atLineStart && !withSpace && canKeepLineStartAt(name)) {
      tr.insertText(`@${name}`, trigger.from, trigger.to);
      tr.setMeta(watchKey, {
        complete: { paragraphPos: trigger.paragraphPos, lineIndex: trigger.lineIndex, name },
      } satisfies WatchMeta);
    } else {
      tr.insertText(withSpace ? `${name} ` : name, trigger.from, trigger.to);
    }
    closeHistory(tr);
    view.dispatch(tr.scrollIntoView());
    // End the completion's history event, so the next keystroke is its own
    // undo step (see the header).
    view.dispatch(closeHistory(view.state.tr));
    view.focus();
  }

  /**
   * Keys the OPEN menu owns — consumed only while it is open, so with the menu
   * shut Enter still submits and Tab still moves focus. An empty menu owns
   * Escape, and Enter / Tab only while the list is still loading (v4
   * `MenuKeyBindings`' `holdCommitKeys`): `@ari` + Enter typed before the
   * register arrives must not send `@ari` as the message.
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
        commitRow(view, runtime.selected, false);
        return true;
      case ' ': {
        if (event.shiftKey || event.altKey || event.ctrlKey || event.metaKey) return false;
        if (view.composing) return false;
        // A bare `@` then Space is punctuation, not a request.
        if ((runtime.openQuery ?? '').length === 0) return false;
        if (!runtime.rows[runtime.selected]) return false;
        commitRow(view, runtime.selected, true);
        return true;
      }
      case 'Escape':
        closeMenu(runtime);
        return true;
      default:
        return false;
    }
  }

  return new Plugin<WatchState>({
    key: watchKey,
    state: watchStateField(),
    appendTransaction(_transactions, _oldState, newState) {
      const due = watchKey.getState(newState)?.stripDue ?? null;
      if (!due) return null;
      const tr = newState.tr.setMeta(watchKey, { stripDone: true } satisfies WatchMeta);
      tr.setMeta(MENTION_STRIP_META, true);
      const range = leadingAtRange(newState.doc, due);
      // The caret, mapped through the deletion, keeps its place in the text.
      if (range) tr.delete(range.from, range.to);
      return tr;
    },
    view(editorView) {
      const runtime: Runtime = {
        menu: new TypeaheadMenu({
          listboxId: LISTBOX_ID,
          emptyLabel: EMPTY_LABEL,
          activeDescendantTarget: editorView.dom as HTMLElement,
          onSelect: (index) => commitRow(editorView, index, false),
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
        subscribed: null,
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
 * One menu row (v4 `:232-247`). The row key folds in everything the row shows,
 * because the shared menu reuses rows whose keys are unchanged — and a
 * character's name, or whether it is in this chat, can change under a stable id.
 */
function toRow(character: MentionCandidate, prioritySet: ReadonlySet<string>): TypeaheadRow {
  const detail = prioritySet.has(character.id) ? 'in this chat' : character.title || undefined;
  return {
    key: JSON.stringify([character.id, character.name, detail ?? null]),
    glyph: '@',
    label: character.name,
    detail,
  };
}
