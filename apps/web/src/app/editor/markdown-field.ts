import {
  ChangeDetectionStrategy,
  Component,
  effect,
  input,
  output,
  signal,
  untracked,
  viewChild,
} from '@angular/core';
import { setBlockType, toggleMark, wrapIn } from 'prosemirror-commands';
import type { Command } from 'prosemirror-state';
import { wrapInList } from 'prosemirror-schema-list';

import { dialectSchema } from './markdown-dialect';
import { FormattingToolbar, type FormatAction } from './formatting-toolbar';
import { RichEditor } from './rich-editor';

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
 * DEFERRED (loud): `roleplayTemplateId`-aware toolbar delimiters — the template
 * plumbing is not client-side yet.
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
      (action)="onAction($event)"
    />
    <qt-rich-editor
      #editor
      [style.min-height]="minHeight()"
      [value]="value()"
      [placeholder]="placeholder()"
      [disabled]="disabled()"
      [ariaLabel]="ariaLabel()"
      (contentChange)="onEditorContentChange($event)"
      (imagePaste)="imagePaste.emit($event)"
    />
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

  readonly contentChange = output<string>();
  readonly imagePaste = output<File>();

  protected readonly editor = viewChild.required(RichEditor);
  /** Mirrors the editor's code-block flag — read safely once the view exists. */
  protected readonly inCodeBlock = signal(false);

  constructor() {
    // Track the editor's code-block state for the toolbar (effect runs after the
    // view query resolves, so `editor()` is safe here).
    effect(() => this.inCodeBlock.set(this.editor().inCodeBlock()));

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
      queueMicrotask(() => this.editor().setMarkdown(v));
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
    this.contentChange.emit(markdown);
  }

  protected onAction(action: FormatAction): void {
    this.editor().runCommand(this.commandFor(action));
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
        return this.editor().inCodeBlock()
          ? setBlockType(schema.nodes['paragraph'])
          : setBlockType(schema.nodes['code_block']);
    }
  }
}
