import {
  afterNextRender,
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  effect,
  ElementRef,
  inject,
  input,
  output,
  viewChild,
} from '@angular/core';
import { baseKeymap, chainCommands, newlineInCode } from 'prosemirror-commands';
import { history, redo, undo } from 'prosemirror-history';
import { undoInputRule } from 'prosemirror-inputrules';
import { keymap } from 'prosemirror-keymap';
import { splitListItem } from 'prosemirror-schema-list';
import { EditorState, Plugin, type Command } from 'prosemirror-state';
import { Decoration, DecorationSet, EditorView } from 'prosemirror-view';

import { dialectFormattingKeymap, dialectInputRules } from './editing-commands';
import { dialectSchema, parseMarkdown, serializeMarkdown } from './markdown-dialect';

/**
 * `qt-rich-editor` — the bespoke ProseMirror editor over the v4 composer
 * markdown dialect (the D17 GREEN adoption). It hosts a `prosemirror-view`
 * imperatively (framework-agnostic, needs no zone patching), takes markdown in
 * via `value`, and reads markdown out through the {@link markdown-dialect}
 * bridge — so its output is byte-faithful to v4's Lexical composer (proven by
 * `markdown-round-trip.spec.ts`).
 *
 * The imperative handle mirrors v4's `ComposerEditorHandle`
 * (`components/chat/lexical/types.ts`): `focus`, `getMarkdown`, `setMarkdown`,
 * `prependText`. Consumers grab the component via `viewChild` and call these.
 *
 * Two host modes via `submitOnEnter`:
 *  - chat composer: Enter emits `submit`, Shift+Enter inserts a line break
 *    (v4 `KeyboardPlugin` chat mode).
 *  - document pane: Enter splits the block normally (Shift+Enter a line break).
 */
@Component({
  selector: 'qt-rich-editor',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { class: 'qt-rich-editor-host' },
  template: `<div #mount class="qt-rich-editor-mount"></div>`,
})
export class RichEditor {
  /** The markdown content. External changes replace the document (absorb-once). */
  readonly value = input('');
  readonly placeholder = input('');
  readonly disabled = input(false);
  /** Chat mode: Enter submits, Shift+Enter is a line break (v4 KeyboardPlugin). */
  readonly submitOnEnter = input(false);
  /**
   * Composition mode: Cmd+Enter (Mac) / Ctrl+Enter (non-Mac) submits, while
   * plain Enter inserts a paragraph (v4 `KeyboardPlugin` `documentEditingMode`
   * branch, :113-142). `Mod-Enter` in prosemirror-keymap resolves to Cmd on Mac
   * / Ctrl elsewhere — v4's exact `isMac ? metaKey&&!ctrlKey : ctrlKey&&!metaKey`.
   */
  readonly submitOnModEnter = input(false);
  readonly ariaLabel = input('Editor');

  /** Fired with the serialized markdown whenever the document changes. */
  readonly contentChange = output<string>();
  /** Fired when Enter is pressed in `submitOnEnter` mode. */
  readonly submit = output<void>();
  /** Fired with a pasted image file (the composer's paste-image leg). */
  readonly imagePaste = output<File>();

  private readonly mount = viewChild.required<ElementRef<HTMLDivElement>>('mount');
  private readonly destroyRef = inject(DestroyRef);

  private view: EditorView | null = null;
  /** The last markdown we emitted — so an echoed `value` doesn't reset the doc. */
  private lastEmitted = '';

  constructor() {
    afterNextRender(() => this.createView());

    // React to external `value` changes (a fresh document load, a raw-source
    // toggle round-trip, a prependText). Only replaces the doc when the incoming
    // value differs from what we last emitted, so typing never resets selection.
    effect(() => {
      const incoming = this.value();
      if (!this.view || incoming === this.lastEmitted) return;
      this.replaceContent(incoming, /* emit */ true);
    });

    // Keep the view's editability in step with `disabled`.
    effect(() => {
      const dis = this.disabled();
      this.view?.setProps({ editable: () => !dis });
    });

    this.destroyRef.onDestroy(() => this.view?.destroy());
  }

  // --- imperative handle (v4 ComposerEditorHandle) --------------------------

  focus(): void {
    this.view?.focus();
  }

  getMarkdown(): string {
    return this.view ? serializeMarkdown(this.view.state.doc) : this.value();
  }

  setMarkdown(text: string): void {
    this.replaceContent(text, /* emit */ true);
  }

  /** Insert `text` above the current content (v4 prependText — outfit notices). */
  prependText(text: string): void {
    if (!this.view) return;
    const current = serializeMarkdown(this.view.state.doc);
    const combined = current ? `${text}\n\n${current}` : text;
    this.replaceContent(combined, true);
  }

  // --- view lifecycle -------------------------------------------------------

  private createView(): void {
    const doc = parseMarkdown(this.value());
    const norm = serializeMarkdown(doc);
    this.lastEmitted = norm;

    const state = EditorState.create({ doc, plugins: this.buildPlugins() });
    this.view = new EditorView(this.mount().nativeElement, {
      state,
      editable: () => !this.disabled(),
      attributes: () => ({
        class: 'qt-rich-editor-content',
        'aria-label': this.ariaLabel(),
        role: 'textbox',
        'aria-multiline': 'true',
      }),
      handlePaste: (_view, event) => this.handlePaste(event),
      dispatchTransaction: (tr) => {
        if (!this.view) return;
        this.view.updateState(this.view.state.apply(tr));
        if (tr.docChanged) {
          const md = serializeMarkdown(this.view.state.doc);
          this.lastEmitted = md;
          this.contentChange.emit(md);
        }
      },
    });

    // Emit the re-serialized content once after the initial mount — the
    // absorb-once signal the document controller adopts as baseline (v4
    // `computeAbsorbNext`). Without it, a first-load markdown normalization
    // (e.g. `__bold__` → `**bold**`) would surface as the user's first "edit".
    queueMicrotask(() => this.contentChange.emit(norm));
  }

  /**
   * Replace the whole document from a markdown string. Emits the re-serialized
   * (normalized) markdown once so the document controller can absorb a
   * first-load serialization diff as baseline (v4 `computeAbsorbNext`).
   */
  private replaceContent(markdown: string, emit: boolean): void {
    if (!this.view) return;
    const doc = parseMarkdown(markdown);
    const norm = serializeMarkdown(doc);
    this.view.updateState(EditorState.create({ doc, plugins: this.buildPlugins() }));
    this.lastEmitted = norm;
    // Defer the post-load emit off the current change-detection pass. This is
    // the absorb-once signal (v4 debounces its own setInput by ~16ms); emitting
    // synchronously from the `value` effect would re-enter change detection.
    if (emit) queueMicrotask(() => this.contentChange.emit(norm));
  }

  // --- plugins --------------------------------------------------------------

  private buildPlugins(): Plugin[] {
    const submitCmd: Command = () => {
      if (!this.submitOnEnter()) return false;
      this.submit.emit();
      return true;
    };

    // Composition mode: Cmd/Ctrl+Enter submits; otherwise fall through so the
    // combo does nothing special (v4 chat mode lets Lexical handle it).
    const modSubmitCmd: Command = () => {
      if (!this.submitOnModEnter()) return false;
      this.submit.emit();
      return true;
    };

    // Shift+Enter (and Enter inside a code block) inserts a line break — a
    // hard_break node, which serializes to a plain `\n` (the dialect soft break).
    const insertBreak: Command = (state, dispatch) => {
      if (dispatch) {
        const br = dialectSchema.nodes['hard_break'].create();
        dispatch(state.tr.replaceSelectionWith(br).scrollIntoView());
      }
      return true;
    };

    return [
      dialectInputRules(dialectSchema),
      history(),
      keymap({
        'Mod-z': undo,
        'Mod-y': redo,
        'Shift-Mod-z': redo,
        Backspace: undoInputRule,
        // In chat mode Enter submits; else split a list item, break a code
        // block, or fall through to the base paragraph split.
        Enter: chainCommands(
          submitCmd,
          splitListItem(dialectSchema.nodes['list_item']),
          newlineInCode,
        ),
        // Composition mode submit (Cmd/Ctrl+Enter). Bound above the base keymap
        // so it wins when armed, else falls through.
        'Mod-Enter': modSubmitCmd,
        'Shift-Enter': insertBreak,
      }),
      keymap(dialectFormattingKeymap(dialectSchema)),
      keymap(baseKeymap),
      this.placeholderPlugin(),
    ];
  }

  private placeholderPlugin(): Plugin {
    const getText = () => this.placeholder();
    return new Plugin({
      props: {
        decorations: (state) => {
          const { doc } = state;
          const empty =
            doc.childCount === 1 &&
            !!doc.firstChild?.isTextblock &&
            doc.firstChild.content.size === 0;
          if (!empty) return null;
          const text = getText();
          if (!text) return null;
          const deco = Decoration.node(0, doc.firstChild!.nodeSize, {
            class: 'qt-rich-editor-placeholder',
            'data-placeholder': text,
          });
          return DecorationSet.create(doc, [deco]);
        },
      },
    });
  }

  private handlePaste(event: ClipboardEvent): boolean {
    const item = Array.from(event.clipboardData?.items ?? []).find((i) =>
      i.type.startsWith('image/'),
    );
    const file = item?.getAsFile();
    if (file) {
      event.preventDefault();
      this.imagePaste.emit(file);
      return true;
    }
    return false;
  }
}
