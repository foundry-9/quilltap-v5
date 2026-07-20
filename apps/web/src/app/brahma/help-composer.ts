/**
 * HelpChatComposer analogue (port of v4
 * `components/help-chat/HelpChatComposer.tsx`).
 *
 * A bare textarea + send button on the `qt-help-composer*` classes: Enter sends,
 * Shift+Enter inserts a newline, the field auto-grows to 120px. This lane
 * carries the composer even though the Help Chat family itself stays deferred
 * (p4.9i2) — the Brahma Console is its only consumer for now.
 *
 * @module brahma/help-composer
 */

import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  input,
  output,
  signal,
  viewChild,
} from '@angular/core';

import { Icon } from '../ui/icon';

@Component({
  selector: 'qt-help-composer',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div class="qt-help-composer">
      <textarea
        #input
        class="qt-help-composer-input"
        rows="1"
        style="min-height: 38px; max-height: 120px"
        [value]="content()"
        [placeholder]="placeholder()"
        [disabled]="disabled()"
        (input)="onInput($event)"
        (keydown)="onKeyDown($event)"
      ></textarea>
      <button
        type="button"
        class="qt-help-composer-send"
        title="Send"
        [disabled]="disabled() || !content().trim()"
        (click)="handleSend()"
      >
        <qt-icon name="send" class="w-4 h-4" />
      </button>
    </div>
  `,
})
export class HelpComposer {
  readonly disabled = input(false);
  readonly placeholder = input('Ask a question...');
  readonly send = output<string>();

  protected readonly content = signal('');
  private readonly inputEl = viewChild.required<ElementRef<HTMLTextAreaElement>>('input');

  protected onInput(event: Event): void {
    const el = event.target as HTMLTextAreaElement;
    this.content.set(el.value);
    // Auto-grow to a 120px cap (v4 `handleInput`).
    el.style.height = 'auto';
    el.style.height = Math.min(el.scrollHeight, 120) + 'px';
  }

  protected onKeyDown(event: KeyboardEvent): void {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      this.handleSend();
    }
  }

  protected handleSend(): void {
    const trimmed = this.content().trim();
    if (!trimmed || this.disabled()) return;
    this.send.emit(trimmed);
    this.content.set('');
    // Reset textarea height (v4).
    const el = this.inputEl().nativeElement;
    el.style.height = 'auto';
  }

  /** Focus the input (v4 exposes `inputRef` so the console can re-focus it). */
  focus(): void {
    this.inputEl().nativeElement.focus({ preventScroll: true });
  }
}
