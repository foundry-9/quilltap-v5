import {
  ChangeDetectionStrategy,
  Component,
  effect,
  type ElementRef,
  input,
  output,
  signal,
  untracked,
  viewChild,
} from '@angular/core';
import { setBlockType, toggleMark, wrapIn } from 'prosemirror-commands';
import type { Command } from 'prosemirror-state';
import { liftListItem, sinkListItem, wrapInList } from 'prosemirror-schema-list';

import { dialectSchema } from './markdown-dialect';
import { recordRecent } from './char-insert/recents-storage';
import { FormattingToolbar, type CharInsert, type FormatAction } from './formatting-toolbar';
import { RichEditor } from './rich-editor';
import { applySourceFormat } from './source-transforms';

/**
 * `qt-markdown-field` — the shared form-field markdown editor (v4's
 * `components/markdown-editor/MarkdownLexicalEditor.tsx`, "Designed for forms").
 * A {@link FormattingToolbar} over a {@link RichEditor} on the v4 composer
 * dialect, so a form field's stored bytes stay byte-faithful to v4. The host
 * binds `value` and listens to `contentChange`, exactly like the textarea it
 * replaces — the emitted strings are identical in shape (the absorb-once seam
 * normalizes a first-load markdown diff, v4 `computeAbsorbNext`).
 *
 * `recordKey` is v4's `remountKey`: change it to force the editor to re-init from
 * the current `value` when the underlying record swaps in place (the RichEditor's
 * own `value` effect already re-inits on a value change; this covers the
 * identical-value-different-record edge).
 *
 * `minHeight` is v4's input of the same name: a CSS min-height for the editor
 * body (v4 puts it on the Lexical contenteditable,
 * `MarkdownLexicalEditor.tsx:90`). Here it binds to the `qt-rich-editor`
 * element, whose content area stretches to fill it — the toolbar is a sibling
 * on both sides, so a site's overall field height matches v4's.
 *
 * DIVERGENCE (deliberate): v4 DEFAULTS `minHeight` to `12rem` (`:151`); this
 * defaults to unset, i.e. the content-height rendering v5 has had since the
 * editor's D17 adoption. The P4.6al-adopted sites (memory editor, the character
 * new/edit prose fields) pre-date this input and pass nothing, so a v4-matching
 * default would silently resize them — and 12rem is the wrong value for every
 * one of them anyway (v4 passes 6/8/10/12rem there explicitly). Every site
 * ported since passes v4's effective value explicitly, INCLUDING the sites where
 * v4 itself falls through to its default. See the P4.6aq status-log record: the
 * P4.6al-adopted sites still lack their v4 minHeight — a named residual gap.
 *
 * `showSourceToggle` is v4's input of the same name and, as in v4
 * (`MarkdownLexicalEditor.tsx:59-61,150`), it DEFAULTS ON: every form caller
 * gets the "Edit markdown source" button. The toggle is self-contained here —
 * no consumer passes anything to get it.
 *
 * The source-mode shape follows v4's end state, not its mechanics. v4 force-
 * remounts its whole Lexical editor on leaving source mode (`sourceLeaveCount`
 * → `composerKey`, `:190-207`) because its bridge reads `initialMarkdown` only
 * once. v5 instead keeps `sourceText` as the single source of truth across the
 * swap, exactly as the Document-Mode pane does with its own `editorValue`
 * (`documents/document-pane.ts`): the textarea and the editor both read it, so
 * the editor re-inits from the edited bytes without depending on the consumer
 * echoing `value` back.
 *
 * The absorb-once rule extends to the swap: v4 tags its remount conversion
 * `external-sync` and its update listener SKIPS that tag
 * (`MarkdownBridgePlugin.tsx:158-180`), so leaving source mode never pushes the
 * re-parsed/normalized text to the parent — the parent keeps the raw bytes the
 * textarea gave it. v5 arms `absorbNext` on the re-mount to reproduce that
 * exactly.
 *
 * DEFERRED (loud): `roleplayTemplateId`-aware toolbar delimiters — the template
 * plumbing is not client-side yet, and v4's template-aware toolbar is chat-only
 * in practice (none of its form-field callers pass one). A future Salon slice.
 */
@Component({
  selector: 'qt-markdown-field',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RichEditor, FormattingToolbar],
  host: { class: 'qt-markdown-field' },
  template: `
    <qt-formatting-toolbar
      [disabled]="disabled()"
      [inCodeBlock]="inCodeBlock()"
      [showSource]="showSource()"
      [showSourceToggle]="showSourceToggle()"
      (action)="onAction($event)"
      (insertChar)="onInsertChar($event)"
      (toggleSource)="onToggleSource()"
    />
    @if (showSource()) {
      <textarea
        #sourceArea
        class="qt-markdown-field-source w-full px-3 py-2 font-mono text-sm qt-text-primary resize-y outline-none"
        [style.min-height]="minHeight()"
        [value]="sourceText()"
        [disabled]="disabled()"
        [attr.aria-label]="sourceAriaLabel()"
        spellcheck="false"
        style="line-height: 1.6; background-color: var(--color-background)"
        (input)="onSourceInput($any($event.target).value)"
      ></textarea>
    } @else {
      <qt-rich-editor
        #editor
        [style.min-height]="minHeight()"
        [value]="sourceText()"
        [placeholder]="placeholder()"
        [disabled]="disabled()"
        [ariaLabel]="ariaLabel()"
        (contentChange)="onEditorContentChange($event)"
        (imagePaste)="imagePaste.emit($event)"
      />
    }
  `,
})
export class MarkdownField {
  readonly value = input('');
  readonly placeholder = input('');
  readonly disabled = input(false);
  readonly ariaLabel = input('Editor');
  /** v4 `remountKey` — re-init the editor when the bound record changes. */
  readonly recordKey = input<string | number | null>(null);
  /**
   * Minimum height of the editor body — any CSS height value (v4 `minHeight`).
   * Empty leaves the height content-driven (see the class doc: v4's own default
   * is `12rem`; ours is unset, and every ported site passes v4's value).
   */
  readonly minHeight = input('');
  /** Show the "Edit markdown source" toggle. v4 defaults this ON (`:150`). */
  readonly showSourceToggle = input(true);

  readonly contentChange = output<string>();
  readonly imagePaste = output<File>();

  /** Absent while the raw-source textarea is showing (v4 unmounts Lexical too). */
  protected readonly editor = viewChild(RichEditor);
  private readonly sourceArea = viewChild<ElementRef<HTMLTextAreaElement>>('sourceArea');
  /** Mirrors the editor's code-block flag — read safely once the view exists. */
  protected readonly inCodeBlock = signal(false);

  /** Raw-source view active (v4 `showSource`, `MarkdownLexicalEditor.tsx:153`). */
  protected readonly showSource = signal(false);
  /**
   * The bytes both views render — the single source of truth across the swap.
   * Seeded from `value`, re-seeded from the editor when entering source mode and
   * from the textarea when leaving it.
   */
  protected readonly sourceText = signal('');

  constructor() {
    // Track the editor's code-block state for the toolbar. In source mode there
    // is no editor; the flag holds at false (v4's toolbar tracks Lexical's
    // selection, which likewise has no code block while the textarea shows).
    effect(() => this.inCodeBlock.set(this.editor()?.inCodeBlock() ?? false));

    // The parent's `value` feeds the views — but never mid-source-edit, which
    // would fight the user's typing (v4's textarea IS `value`-controlled, but
    // v4's parent echoes every keystroke straight back, so the two agree).
    effect(() => {
      const v = this.value();
      untracked(() => {
        if (!this.showSource()) this.sourceText.set(v);
      });
    });

    // Re-key: on a record swap force a re-init from the current value. Read
    // `value` untracked so this fires only on recordKey changes (a plain value
    // change is already handled by the editor's own absorb-once effect).
    let first = true;
    effect(() => {
      this.recordKey();
      if (first) {
        first = false;
        return;
      }
      const v = untracked(() => this.value());
      // A re-init is not a user edit — absorb its emit too.
      this.absorbNext = true;
      untracked(() => this.sourceText.set(v));
      queueMicrotask(() => this.editor()?.setMarkdown(v));
    });
  }

  /** Swallow the editor's initial absorb-once emit (v4 `computeAbsorbNext`), so
   *  a form field emits only on genuine edits — exactly the textarea contract it
   *  replaces (no dirty-on-load, no multi-field mount cascade). A record re-key
   *  arms it again below. */
  private absorbNext = true;

  protected onEditorContentChange(markdown: string): void {
    if (this.absorbNext) {
      this.absorbNext = false;
      return;
    }
    // Keep the swap's source of truth current, so entering source mode shows the
    // editor's own bytes. Re-entrancy is not a risk: the editor skips a `value`
    // that matches what it last emitted.
    this.sourceText.set(markdown);
    this.contentChange.emit(markdown);
  }

  /** v4's `aria-label` for the source textarea (`MarkdownLexicalEditor.tsx:223`). */
  protected sourceAriaLabel(): string {
    const label = this.ariaLabel();
    return label ? `${label} (markdown source)` : 'Markdown source';
  }

  /**
   * Enter/leave raw-source mode (v4 `toggleSourceMode`, `:188-193`). Entering
   * seeds the textarea from the editor's own serialization; leaving hands the
   * edited bytes back. The re-mounted editor emits its re-serialization once —
   * absorbed, because v4's remount likewise never pushes it to the parent (its
   * conversion is tagged `external-sync`, which its update listener skips).
   */
  protected onToggleSource(): void {
    if (this.showSource()) {
      this.showSource.set(false);
      this.absorbNext = true;
      return;
    }
    // The editor still exists at click time — take its bytes before the swap.
    const current = this.editor()?.getMarkdown();
    if (current !== undefined) this.sourceText.set(current);
    this.showSource.set(true);
  }

  /** A textarea keystroke: v4's controlled `onChange` → the parent, every time. */
  protected onSourceInput(value: string): void {
    this.sourceText.set(value);
    this.contentChange.emit(value);
  }

  /**
   * A character picked from a toolbar picker (v4 `insertCharAtSelection`, whose
   * `editor` prop is the Lexical instance the toolbar was given).
   *
   * DELIBERATE DIVERGENCE in source mode: v4 hands the picker its Lexical editor
   * unconditionally, and in raw-source mode that editor is not the view the
   * writer is looking at — the pick lands in a document the source textarea is
   * about to replace, i.e. it is silently lost. v5 inserts at the textarea's
   * caret instead, the same way the formatting buttons already take a source
   * branch here (`applyToSource`). A button that visibly does nothing is not
   * worth reproducing.
   */
  protected onInsertChar({ profile, char }: CharInsert): void {
    if (this.showSource()) {
      const textarea = this.sourceArea()?.nativeElement;
      if (!textarea) return;
      const start = textarea.selectionStart;
      const value = textarea.value.slice(0, start) + char + textarea.value.slice(textarea.selectionEnd);
      this.sourceText.set(value);
      this.contentChange.emit(value);
      recordRecent(profile, char);
      requestAnimationFrame(() => {
        textarea.selectionStart = textarea.selectionEnd = start + char.length;
        textarea.focus();
      });
      return;
    }
    this.editor()?.insertChar(profile, char);
  }

  protected onAction(action: FormatAction): void {
    if (this.showSource()) {
      this.applyToSource(action);
      return;
    }
    this.editor()?.runCommand(this.commandFor(action));
  }

  /**
   * A toolbar button in source mode: transform the raw text (v4's source
   * branches, ported in {@link applySourceFormat}) and restore the cursor after
   * the re-render — v4 does the same in a `requestAnimationFrame`
   * (`sourceApply`, `FormattingToolbar.tsx:123-134`).
   */
  private applyToSource(action: FormatAction): void {
    const textarea = this.sourceArea()?.nativeElement;
    if (!textarea) return;

    const result = applySourceFormat(
      { value: textarea.value, start: textarea.selectionStart, end: textarea.selectionEnd },
      action,
    );
    this.sourceText.set(result.value);
    this.contentChange.emit(result.value);
    requestAnimationFrame(() => {
      textarea.selectionStart = textarea.selectionEnd = result.cursor;
      textarea.focus();
    });
  }

  private commandFor(action: FormatAction): Command {
    const schema = dialectSchema;
    switch (action.kind) {
      case 'bold':
        return toggleMark(schema.marks['strong']);
      case 'italic':
        return toggleMark(schema.marks['em']);
      case 'heading':
        return setBlockType(schema.nodes['heading'], { level: action.level });
      case 'ul':
        return wrapInList(schema.nodes['bullet_list']);
      case 'ol':
        return wrapInList(schema.nodes['ordered_list']);
      case 'blockquote':
        return wrapIn(schema.nodes['blockquote']);
      case 'codeBlock':
        // Toggle: leave a code block back to a paragraph, else enter one (v4).
        return this.inCodeBlock()
          ? setBlockType(schema.nodes['paragraph'])
          : setBlockType(schema.nodes['code_block']);
      case 'outdent':
        return liftListItem(schema.nodes['list_item']);
      case 'indent':
        return sinkListItem(schema.nodes['list_item']);
    }
  }
}
