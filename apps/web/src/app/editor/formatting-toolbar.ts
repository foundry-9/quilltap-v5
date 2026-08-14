import {
  ChangeDetectionStrategy,
  Component,
  computed,
  input,
  output,
  signal,
} from '@angular/core';

import { EmojiPickerPopover } from './char-insert/emoji-picker-popover';
import { EMOJI_PROFILE } from './char-insert/profiles/emoji';
import { UNICODE_PROFILE } from './char-insert/profiles/unicode';
import type { CharProfile } from './char-insert/types';
import { UnicodePickerPopover } from './char-insert/unicode-picker-popover';
import { getDelimiterTooltip, visibleDelimiters } from './delimiter-transforms';
import { Icon } from '../ui/icon';
import type { NarrationDelimiters, TemplateDelimiter } from '../core/core-contract';

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

/** A picker's chosen character, with the profile whose recents it belongs to. */
export interface CharInsert {
  profile: CharProfile;
  char: string;
}

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
 * The roleplay-template DELIMITER buttons (`FormattingToolbar.tsx:459-480`)
 * landed in P4.9L, when the Salon composer got this toolbar. They are their own
 * section after a divider, exactly as in v4, and render only when the resolved
 * list is non-empty — v4's `hasDelimiters` gate, which is also why a template
 * still being fetched shows nothing rather than an empty rail. The list itself
 * (the synthesized narration button, then the template's own minus any whose
 * marks match narration) is {@link visibleDelimiters}; the toolbar stays
 * presentation-only and emits the chosen delimiter for the host to apply.
 *
 * Form-field hosts pass neither input, so they render no delimiter section —
 * which matches v4, where no form-field caller passes a `roleplayTemplateId`.
 * v4 FETCHES the template inside the toolbar; v5 takes the resolved delimiters
 * as inputs instead, because the Salon already fetches that template for its
 * rendering patterns (`salon-conversation.ts`) and a second fetch of the same
 * row would be a second source of truth.
 */
@Component({
  selector: 'qt-formatting-toolbar',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, EmojiPickerPopover, UnicodePickerPopover],
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

      <!-- Roleplay-template delimiters — their own section, shown only when the
           active template resolves at least one button (v4 \`hasDelimiters\`).
           The narration button leads; see \`visibleDelimiters\`. -->
      @if (shownDelimiters().length > 0) {
        <div class="qt-formatting-toolbar-divider"></div>
        <div class="qt-formatting-toolbar-section">
          @for (delimiter of shownDelimiters(); track $index) {
            <button
              type="button"
              class="qt-rp-annotation-button"
              [title]="tooltipFor(delimiter)"
              [disabled]="disabled()"
              (mousedown)="$event.preventDefault()"
              (click)="applyDelimiter.emit(delimiter)"
            >
              {{ delimiter.buttonName }}
            </button>
          }
        </div>
      }

      <!-- Emoji and symbol pickers — their own sections, after the markdown
           buttons. NEITHER is gated by its composer setting: those flags govern
           the automatic \`:\` / \`\\\` triggers, which are the part that can
           surprise. An explicit button press never can. -->
      <div class="qt-formatting-toolbar-divider"></div>
      <div class="qt-formatting-toolbar-section relative">
        <button
          type="button"
          class="qt-formatting-button qt-formatting-button-emoji"
          [class.qt-formatting-button-active]="emojiPickerOpen()"
          title="Insert emoji (or type \`:\` and a name)"
          aria-label="Insert emoji"
          aria-haspopup="dialog"
          [attr.aria-expanded]="emojiPickerOpen()"
          [disabled]="disabled()"
          (mousedown)="$event.preventDefault()"
          (click)="toggleEmojiPicker()"
        >
          ☺
        </button>
        @if (emojiPickerOpen()) {
          <qt-emoji-picker-popover
            (pick)="insertChar.emit({ profile: emojiProfile, char: $event })"
            (close)="emojiPickerOpen.set(false)"
          />
        }
      </div>
      <!-- Its own section so the popover anchors under Ω rather than under ☺.
           Shares \`qt-formatting-button-emoji\`: that class does nothing
           emoji-specific — it sizes a GLYPH label the way the arrow-glyph
           buttons are sized, which is exactly what Ω needs (v4's note). -->
      <div class="qt-formatting-toolbar-section relative">
        <button
          type="button"
          class="qt-formatting-button qt-formatting-button-emoji"
          [class.qt-formatting-button-active]="unicodePickerOpen()"
          title="Insert a symbol (or type \`\\\` and a name)"
          aria-label="Insert a symbol"
          aria-haspopup="dialog"
          [attr.aria-expanded]="unicodePickerOpen()"
          [disabled]="disabled()"
          (mousedown)="$event.preventDefault()"
          (click)="toggleUnicodePicker()"
        >
          Ω
        </button>
        @if (unicodePickerOpen()) {
          <qt-unicode-picker-popover
            (pick)="insertChar.emit({ profile: unicodeProfile, char: $event })"
            (close)="unicodePickerOpen.set(false)"
          />
        }
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
  /**
   * The active roleplay template's own delimiter entries (v4 fetches these
   * inside the toolbar; v5's Salon already has them — see the class doc). Empty
   * while a template is still being fetched, which is v4's `loadingTemplate`
   * gate arriving by a different road.
   */
  readonly delimiters = input<readonly TemplateDelimiter[]>([]);
  /** The template's narration characters — the synthesized "Nar" button (v4). */
  readonly narrationDelimiters = input<NarrationDelimiters | null>(null);

  readonly action = output<FormatAction>();
  readonly toggleSource = output<void>();
  /** A delimiter button was pressed; the host applies it to whichever view it shows. */
  readonly applyDelimiter = output<TemplateDelimiter>();
  /**
   * A character picked from one of the two pickers. The toolbar stays
   * presentation-only, so the host does the inserting — and it is the host that
   * knows whether it is showing the editor or the raw-source textarea.
   */
  readonly insertChar = output<CharInsert>();

  protected readonly emojiPickerOpen = signal(false);
  protected readonly unicodePickerOpen = signal(false);
  protected readonly emojiProfile = EMOJI_PROFILE;
  protected readonly unicodeProfile = UNICODE_PROFILE;

  /** One picker at a time — v4's two popovers are independent, but showing both
   *  at once would stack two panels over the same corner of the toolbar. */
  protected toggleEmojiPicker(): void {
    this.unicodePickerOpen.set(false);
    this.emojiPickerOpen.update((open) => !open);
  }

  protected toggleUnicodePicker(): void {
    this.emojiPickerOpen.set(false);
    this.unicodePickerOpen.update((open) => !open);
  }

  protected readonly buttons = MARKDOWN_BUTTONS;

  /** v4's `allDelimiters` (`FormattingToolbar.tsx:415-417`). */
  protected readonly shownDelimiters = computed(() =>
    visibleDelimiters(this.delimiters(), this.narrationDelimiters()),
  );

  protected tooltipFor(delimiter: TemplateDelimiter): string {
    return getDelimiterTooltip(delimiter);
  }

  /** v4's title/aria-label strings, verbatim (`FormattingToolbar.tsx:437-438`). */
  protected readonly sourceTitle = computed(() =>
    this.showSource() ? 'Switch to rich text editor' : 'Edit markdown source',
  );
}
