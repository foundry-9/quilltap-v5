import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

/**
 * A formatting action emitted by {@link FormattingToolbar}, consumed by
 * {@link markdown-field} which dispatches the matching ProseMirror command.
 */
export type FormatAction =
  | { kind: 'bold' }
  | { kind: 'italic' }
  | { kind: 'heading'; level: number }
  | { kind: 'ul' }
  | { kind: 'ol' }
  | { kind: 'blockquote' }
  | { kind: 'codeBlock' };

interface ButtonConfig {
  label: string;
  tooltip: string;
  type: string;
  action: FormatAction;
}

/** v4 `lib/chat/annotations.ts` `MARKDOWN_FORMATS` + the code-block button. */
const MARKDOWN_BUTTONS: ButtonConfig[] = [
  { label: 'B', tooltip: 'Bold', type: 'bold', action: { kind: 'bold' } },
  { label: 'I', tooltip: 'Italic', type: 'italic', action: { kind: 'italic' } },
  { label: 'H1', tooltip: 'Heading 1', type: 'h1', action: { kind: 'heading', level: 1 } },
  { label: 'H2', tooltip: 'Heading 2', type: 'h2', action: { kind: 'heading', level: 2 } },
  { label: 'H3', tooltip: 'Heading 3', type: 'h3', action: { kind: 'heading', level: 3 } },
  { label: 'H4', tooltip: 'Heading 4', type: 'h4', action: { kind: 'heading', level: 4 } },
  { label: 'H5', tooltip: 'Heading 5', type: 'h5', action: { kind: 'heading', level: 5 } },
  { label: 'H6', tooltip: 'Heading 6', type: 'h6', action: { kind: 'heading', level: 6 } },
  { label: '• …', tooltip: 'Unordered List', type: 'ul', action: { kind: 'ul' } },
  { label: '1. …', tooltip: 'Ordered List', type: 'ol', action: { kind: 'ol' } },
  { label: '“', tooltip: 'Blockquote', type: 'blockquote', action: { kind: 'blockquote' } },
];

/**
 * The rich-editor formatting toolbar (v4 `components/chat/FormattingToolbar.tsx`
 * markdown section). Bold / italic / H1–H6 / lists / blockquote plus a code-block
 * toggle — the same button inventory v4 shows (strikethrough/highlight are dialect
 * marks reached by typing, not toolbar buttons in v4). Buttons emit a
 * {@link FormatAction}; the host {@link markdown-field} dispatches the command so
 * the toolbar stays presentation-only. `mousedown` is prevented to keep the
 * selection in the editor (v4 `preventFocusLoss`).
 *
 * DEFERRED (loud): the roleplay-template delimiter buttons and the source-mode
 * toggle — the template plumbing is not client-side yet (item 3's
 * `roleplayTemplateId` deferral) and the field has no raw-source view.
 */
@Component({
  selector: 'qt-formatting-toolbar',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="qt-formatting-toolbar">
      <div class="qt-formatting-toolbar-section">
        @for (btn of buttons; track btn.type) {
          <button
            type="button"
            class="qt-formatting-button qt-formatting-button-{{ btn.type }}"
            [title]="btn.tooltip"
            [disabled]="disabled()"
            (mousedown)="$event.preventDefault()"
            (click)="action.emit(btn.action)"
          >
            {{ btn.label }}
          </button>
        }
        <button
          type="button"
          class="qt-formatting-button qt-formatting-button-code-block"
          [class.qt-formatting-button-active]="inCodeBlock()"
          [title]="inCodeBlock() ? 'End code block' : 'Insert code block'"
          [disabled]="disabled()"
          (mousedown)="$event.preventDefault()"
          (click)="action.emit({ kind: 'codeBlock' })"
        >
          {{ inCodeBlock() ? '/CODE' : 'CODE' }}
        </button>
      </div>
    </div>
  `,
})
export class FormattingToolbar {
  readonly disabled = input(false);
  /** Whether the caret is in a code block — flips the code-block button (v4). */
  readonly inCodeBlock = input(false);

  readonly action = output<FormatAction>();

  protected readonly buttons = MARKDOWN_BUTTONS;
}
