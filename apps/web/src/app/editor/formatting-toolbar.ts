import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import { Icon } from '../ui/icon';

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
  | { kind: 'codeBlock' }
  | { kind: 'outdent' }
  | { kind: 'indent' };

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
 * The source-mode toggle (v4 `FormattingToolbar.tsx:427-451`) is the last
 * section, after a divider. v4 renders it only when its `onToggleSource`
 * callback is passed; an Angular output always exists, so `showSourceToggle`
 * plays that gate — hence its default of `false` here (the host decides; the
 * field defaults it ON, as v4's `showSourceToggle` prop does).
 *
 * DEFERRED (loud): the roleplay-template delimiter buttons — the template
 * plumbing is not client-side yet, and v4's template-aware toolbar is
 * chat-only in practice (no form-field caller passes a `roleplayTemplateId`).
 * That is a future Salon slice, not a toolbar rider.
 */
@Component({
  selector: 'qt-formatting-toolbar',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
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
          class="qt-formatting-button qt-formatting-button-outdent"
          title="Outdent list item (Shift+Tab)"
          aria-label="Outdent list item"
          [disabled]="disabled()"
          (mousedown)="$event.preventDefault()"
          (click)="action.emit({ kind: 'outdent' })"
        >
          ⇤
        </button>
        <button
          type="button"
          class="qt-formatting-button qt-formatting-button-indent"
          title="Indent list item (Tab)"
          aria-label="Indent list item"
          [disabled]="disabled()"
          (mousedown)="$event.preventDefault()"
          (click)="action.emit({ kind: 'indent' })"
        >
          ⇥
        </button>
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

      @if (showSourceToggle()) {
        <div class="qt-formatting-toolbar-divider"></div>
        <div class="qt-formatting-toolbar-section">
          <button
            type="button"
            class="qt-formatting-button qt-formatting-button-source"
            [class.qt-formatting-button-active]="showSource()"
            [title]="sourceTitle()"
            [attr.aria-label]="sourceTitle()"
            [attr.aria-pressed]="showSource()"
            [disabled]="disabled()"
            (mousedown)="$event.preventDefault()"
            (click)="toggleSource.emit()"
          >
            <qt-icon [name]="showSource() ? 'pencil' : 'code'" class="w-4 h-4" />
          </button>
        </div>
      }
    </div>
  `,
})
export class FormattingToolbar {
  readonly disabled = input(false);
  /** Whether the caret is in a code block — flips the code-block button (v4). */
  readonly inCodeBlock = input(false);
  /** Whether the host is showing raw markdown source — flips the toggle (v4). */
  readonly showSource = input(false);
  /** Render the source toggle at all (v4: "only when `onToggleSource` is passed"). */
  readonly showSourceToggle = input(false);

  readonly action = output<FormatAction>();
  readonly toggleSource = output<void>();

  protected readonly buttons = MARKDOWN_BUTTONS;

  /** v4's title/aria-label strings, verbatim (`FormattingToolbar.tsx:437-438`). */
  protected readonly sourceTitle = computed(() =>
    this.showSource() ? 'Switch to rich text editor' : 'Edit markdown source',
  );
}
