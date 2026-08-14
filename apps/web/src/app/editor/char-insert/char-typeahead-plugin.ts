/**
 * The composer's inline character typeahead — one implementation, two dataset
 * profiles (v4 `components/chat/lexical/plugins/CharTypeaheadPlugin.tsx`).
 *
 * - **emoji** — type `:` plus at least two characters and pick an emoji by
 *   name. A closing `:` on an exact shortcode (`:smile:`) commits without the
 *   menu ever mattering.
 * - **Unicode** — type `\` plus a LaTeX name (`\to`, `\phi`) or a code point
 *   (`→`). A trailing space on an exact alias (`\to `) commits without the
 *   menu ever mattering, and KEEPS the space the writer typed.
 *
 * Two commit paths per profile, one engine. What it inserts is the literal
 * Unicode character — not a shortcode, not a node, not an image. That single
 * decision is why markdown round-trip, export, search, embeddings and the LLM
 * wire are all untouched by both features.
 *
 * Every DECISION (which characters match, in what order, where the trigger
 * starts and ends, whether the cursor is inside math) lives in the Tier B
 * engine beside this file, which is v4's byte-for-byte. This file is the
 * ADAPTER that asks the questions and applies the answers. Keep it that way.
 *
 * ### Where the adapter necessarily differs from v4's
 *
 * v4 reads the text of the anchor Lexical TEXT NODE and then asks separately
 * whether that node is glued to a preceding inline run (`**bold**:smi` must not
 * open). ProseMirror has no such node boundary at the caret, so this reads the
 * whole textblock's text up to the caret instead — which answers the same
 * question by construction: `bold:smi` fails `findTrigger`'s opener-context
 * rule with no second check, and the math bail sees the same prefix a reader
 * would. Inline leaf nodes collapse to one placeholder character so the string
 * stays 1:1 with document positions (the {@link textReplacementPlugin}
 * precedent).
 *
 * Registered BEFORE `textReplacementPlugin` in `buildPlugins()`, which is v5's
 * equivalent of v4 sitting at `COMMAND_PRIORITY_NORMAL` above Layer 1.5's
 * `LOW`: both `:` and ` ` are also text-replacement word-boundary triggers, and
 * this handler swallows the keystroke ONLY when it commits a character.
 * Otherwise it returns false, so a `:` typed after a replaceable word still
 * fires the replacement.
 *
 * @module editor/char-insert/char-typeahead-plugin
 */

import { Plugin, type EditorState } from 'prosemirror-state';
import type { EditorView } from 'prosemirror-view';

import { isInCodeContext } from '../code-context';
import { commitCharOverTrigger } from './insert-char';
import { isInsideMathSpan } from './math-span';
import { findByAlias, findByCodePoint, searchChars } from './search';
import { findTrigger } from './trigger';
import { TypeaheadMenu, type TypeaheadRow } from './typeahead-menu';
import type { CharEntry, CharIndex, CharProfile, TriggerMatch } from './types';

/** Rows visible at once; the menu scrolls with the keyboard beyond this. */
const MENU_LIMIT = 10;

/**
 * Inline leaf nodes (an image) collapse to ONE placeholder so the extracted
 * string stays 1:1 with document positions — the same device
 * `text-replacement.ts` uses. U+FFFC is in no query alphabet and is not
 * word-opening context, so a leaf immediately before an opener correctly
 * refuses to trigger.
 */
const LEAF = '￼';

export interface CharTypeaheadOptions {
  /**
   * Which dataset this instance serves. Both profiles are mounted, side by side.
   * FIXED for the life of the plugin instance, as in v4.
   */
  profile: CharProfile;
  /**
   * The profile's `chat_settings` toggle, read LIVE on every keystroke so a
   * settings change takes effect without rebuilding the editor (the
   * `textReplacementRules` closure-read precedent). Gates the automatic trigger
   * ONLY — the toolbar pickers are never gated.
   */
  enabled: () => boolean;
}

/** The trigger, resolved against the document rather than a bare string. */
interface LiveTrigger {
  match: TriggerMatch;
  /** Document position of the opener. */
  from: number;
  /** Document position one past the last query character. */
  to: number;
}

function toRow(profile: CharProfile, entry: CharEntry): TypeaheadRow {
  return {
    key: entry.char,
    glyph: entry.char,
    label: entry.name,
    detail: entry.aliases[0] ? profile.ui.formatAlias(entry.aliases[0]) : undefined,
  };
}

/**
 * The text of the current textblock up to the caret, with its document offset.
 *
 * Returns null in every position a typing aid must stay out of: a non-collapsed
 * selection, a fenced code block, an inline `code` run (both via the shared
 * {@link isInCodeContext}).
 */
function textBeforeCursor(state: EditorState): { text: string; blockStart: number } | null {
  const { selection } = state;
  if (!selection.empty) return null;
  if (isInCodeContext(state)) return null;

  const $from = selection.$from;
  const parent = $from.parent;
  if (!parent.isTextblock) return null;

  const offset = $from.parentOffset;
  return {
    text: parent.textBetween(0, offset, undefined, LEAF),
    blockStart: $from.start(),
  };
}

/**
 * Resolve a query to the single character the menu-free commit path should
 * insert, or null to leave the literal text alone rather than guess.
 */
function resolveExact(profile: CharProfile, index: CharIndex, query: string): CharEntry | null {
  const byAlias = findByAlias(index, query, { caseSensitive: profile.caseSensitiveAliases });
  if (byAlias) return byAlias;
  if (profile.codePointQueries) return findByCodePoint(index, query);
  return null;
}

/**
 * The menu's option list for a query. A code-point query resolves
 * arithmetically, so it can name a character the curation dropped — put it
 * first, because someone who typed `→` asked for exactly one thing.
 */
function optionsFor(profile: CharProfile, index: CharIndex, query: string): CharEntry[] {
  const results = searchChars(index, query, MENU_LIMIT);
  if (!profile.codePointQueries) return results;

  const direct = findByCodePoint(index, query);
  if (!direct) return results;
  return [direct, ...results.filter((entry) => entry.char !== direct.char)].slice(0, MENU_LIMIT);
}

/**
 * One typeahead, bound to one profile.
 *
 * The menu, the open trigger and the highlight are PER VIEW, not per plugin
 * instance: `EditorState.create` builds a fresh `plugins` array every time, so
 * ProseMirror tears down and rebuilds the plugin views on every
 * `view.updateState` with a new state — while the plugin object itself lives
 * on. State kept in this factory's closure would therefore be shared between a
 * destroyed view and its replacement (and, in principle, between two editors
 * given the same plugin). A WeakMap keyed by the view is the honest scope.
 */
export function charTypeaheadPlugin(options: CharTypeaheadOptions): Plugin {
  const { profile, enabled } = options;

  /** Per-view runtime. Absent until the plugin's view is created. */
  interface Runtime {
    menu: TypeaheadMenu;
    rows: CharEntry[];
    selected: number;
    openQuery: string | null;
    openTrigger: LiveTrigger | null;
    /** The view is gone; a dataset promise resolving afterwards must not touch it. */
    destroyed: boolean;
  }

  const runtimes = new WeakMap<EditorView, Runtime>();

  /**
   * Kick off the lazy fetch, once an opener is actually in play — an instance
   * whose writer never types one never pays for the dataset. Failure is silent
   * (the loader logs one warning per page load); typing is never impeded.
   */
  function ensureIndex(view: EditorView): void {
    if (profile.loader.getLoaded()) return;
    void profile.loader.load().then((loaded) => {
      // The dataset arriving is not a document change, so nothing else will
      // re-run the trigger scan — do it here so the menu opens on the query the
      // writer has already typed.
      if (loaded) refresh(view);
    });
  }

  /** The live trigger at the caret, or null when there is nothing to act on. */
  function liveTrigger(state: EditorState): LiveTrigger | null {
    const current = textBeforeCursor(state);
    if (!current) return null;

    const match = findTrigger(current.text, profile.trigger);
    if (!match) return null;

    // `$$\ph` is a formula being typed, not a request for φ. Checked against the
    // text BEFORE the trigger, so the trigger's own backslash can never be
    // mistaken for a delimiter.
    if (profile.bailInsideMath && isInsideMathSpan(current.text.slice(0, match.start))) return null;

    return {
      match,
      from: current.blockStart + match.start,
      to: current.blockStart + match.end,
    };
  }

  function closeMenu(runtime: Runtime): void {
    runtime.openQuery = null;
    runtime.openTrigger = null;
    runtime.rows = [];
    runtime.selected = 0;
    runtime.menu.close();
  }

  /**
   * Recompute the menu from the current state. The bail conditions MIRROR Layer
   * 1.5's and must not drift from them:
   *
   *   1. feature disabled       4. inside a fenced code block
   *   2. IME composition        5. inside an inline `code` run
   *   3. no collapsed selection 6. inside a math span (Unicode only)
   *                             7. dataset not loaded yet
   */
  function refresh(view: EditorView): void {
    const runtime = runtimes.get(view);
    if (!runtime || runtime.destroyed) return;

    // `editable` closes over the host's `disabled` input: a composer that goes
    // read-only mid-stream must not leave a live menu hanging over it. (The key
    // handlers need no such guard — ProseMirror runs edit handlers only on an
    // editable view.)
    if (!enabled() || view.composing || !view.editable) {
      closeMenu(runtime);
      return;
    }

    const trigger = liveTrigger(view.state);
    if (!trigger) {
      closeMenu(runtime);
      return;
    }

    // A closed trigger (`:smile:`) is the keydown handler's business; if it
    // reached here the alias did not resolve, and re-opening a menu on the text
    // the user just closed would be a jack-in-the-box.
    if (trigger.match.closed) {
      closeMenu(runtime);
      return;
    }

    const index = profile.loader.getLoaded();
    if (!index) {
      ensureIndex(view);
      closeMenu(runtime);
      return;
    }

    const query = trigger.match.query;
    if (query !== runtime.openQuery) runtime.selected = 0;
    runtime.openQuery = query;
    runtime.openTrigger = trigger;
    runtime.rows = optionsFor(profile, index, query);

    const caret = view.coordsAtPos(view.state.selection.from);
    runtime.menu.render(
      runtime.rows.map((entry) => toRow(profile, entry)),
      runtime.rows.length > 0 ? runtime.selected : -1,
      { left: caret.left, top: caret.top, bottom: caret.bottom },
    );
  }

  function commitRow(view: EditorView, index: number): void {
    const runtime = runtimes.get(view);
    if (!runtime) return;
    const entry = runtime.rows[index];
    const trigger = runtime.openTrigger;
    if (!entry || !trigger) return;
    closeMenu(runtime);
    // No suffix on the menu path: nothing was swallowed, and an inserted
    // character is punctuation-adjacent (v4 `$insertWithoutTrailingSpace`).
    commitCharOverTrigger(view, profile, entry.char, trigger.from, trigger.to, '');
    view.focus();
  }

  /**
   * The menu-free commit keystroke: emoji's closing `:` and Unicode's trailing
   * space, which are the same idea with different punctuation. Returns true only
   * when it actually commits, so a non-committing keystroke still reaches the
   * text-replacement plugin below it.
   */
  function handleCommitKey(view: EditorView, key: string): boolean {
    const commitKey = profile.commitKey;
    if (!commitKey || key !== commitKey.key) return false;
    if (!enabled() || view.composing) return false;

    const index = profile.loader.getLoaded();
    if (!index) {
      // Only the emoji profile fetches from here — see `mayTriggerLoad`. The
      // space bar must never pull down a quarter of a megabyte.
      if (commitKey.mayTriggerLoad) ensureIndex(view);
      return false;
    }

    const current = textBeforeCursor(view.state);
    if (!current) return false;

    // The commit key has not landed yet — returning true stops it ever doing so
    // — so ask Tier B about the text as it WOULD read.
    const match = findTrigger(`${current.text}${commitKey.probeSuffix}`, profile.trigger);
    if (!match) return false;
    if (commitKey.requireClosed && !match.closed) return false;
    if (profile.bailInsideMath && isInsideMathSpan(current.text.slice(0, match.start))) return false;

    const entry = resolveExact(profile, index, match.query);
    if (!entry) return false;

    const runtime = runtimes.get(view);
    if (runtime) closeMenu(runtime);
    commitCharOverTrigger(
      view,
      profile,
      entry.char,
      current.blockStart + match.start,
      // `end` excludes the closer, and the probe suffix is text the writer has
      // not typed yet, so the range never runs past what is actually there.
      current.blockStart + match.end,
      commitKey.insertSuffix,
    );
    return true;
  }

  /**
   * Keys the OPEN menu owns. Consumed only while it is open, so with the menu
   * shut Enter still submits the message and Tab still moves focus — the same
   * contract v4 gets by mounting its menu's handlers only once a trigger has
   * resolved.
   */
  function handleMenuKey(view: EditorView, key: string): boolean {
    const runtime = runtimes.get(view);
    if (!runtime || runtime.openTrigger === null) return false;

    switch (key) {
      case 'ArrowDown':
        if (runtime.rows.length === 0) return true;
        runtime.selected = (runtime.selected + 1) % runtime.rows.length;
        refresh(view);
        return true;
      case 'ArrowUp':
        if (runtime.rows.length === 0) return true;
        runtime.selected =
          (runtime.selected - 1 + runtime.rows.length) % runtime.rows.length;
        refresh(view);
        return true;
      case 'Enter':
      case 'Tab':
        if (runtime.rows.length === 0) return true;
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
          listboxId: profile.ui.listboxId,
          emptyLabel: profile.ui.emptyLabel,
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
        destroyed: false,
      };
      runtimes.set(editorView, runtime);

      return {
        update: (view) => refresh(view),
        destroy: () => {
          runtime.destroyed = true;
          closeMenu(runtime);
          runtime.menu.destroy();
          runtimes.delete(editorView);
        },
      };
    },
    props: {
      handleKeyDown(view, event) {
        if (handleMenuKey(view, event.key)) {
          event.preventDefault();
          return true;
        }
        return handleCommitKey(view, event.key);
      },
      handleDOMEvents: {
        /**
         * A caret-anchored menu has no host to hide it: v4's unmounts with the
         * Lexical menu, so a blurred editor must take this one down too. Never
         * consumes the event — the Document-Mode flush-on-blur save rides it.
         */
        blur: (view) => {
          const runtime = runtimes.get(view);
          if (runtime) closeMenu(runtime);
          return false;
        },
      },
    },
  });
}
