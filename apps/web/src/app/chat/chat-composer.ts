import { ChangeDetectionStrategy, Component, computed, input, output, signal } from '@angular/core';

import { Icon } from '../ui/icon';

/**
 * The Salon composer — the sanctioned MVP substitute for v4's Lexical editor: a
 * plain textarea plus Send / Stop / Continue. Enter sends, Shift+Enter inserts a
 * newline (v4 chat-mode behaviour). Send is blocked on blank content (v4's
 * blank-content guard); Continue dispatches a turn continuation with no content.
 * Lexical, the formatting toolbar, and the gutter tools are locked deferrals.
 */
@Component({
  selector: 'qt-chat-composer',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div class="qt-chat-composer">
      <form class="qt-chat-composer-inner" (submit)="onSubmit($event)">
        <textarea
          class="qt-chat-composer-input"
          [value]="text()"
          [disabled]="disabled()"
          [placeholder]="placeholder()"
          rows="1"
          aria-label="Message"
          (input)="onInput($event)"
          (keydown)="onKeydown($event)"
        ></textarea>

        <div class="qt-chat-composer-actions">
          <button
            type="button"
            class="qt-chat-toolbar-button qt-chat-continue-button"
            title="Continue — let the next character respond"
            aria-label="Continue"
            [disabled]="disabled() || busy() || !hasActiveCharacters()"
            (click)="continue.emit()"
          >
            <qt-icon name="arrow-right" class="w-5 h-5" />
          </button>

          @if (busy()) {
            <button
              type="button"
              class="qt-chat-composer-send qt-chat-stop-button"
              title="Stop generating"
              aria-label="Stop generating"
              (click)="stop.emit()"
            >
              <qt-icon name="stop" class="w-5 h-5" />
            </button>
          } @else {
            <button
              type="submit"
              class="qt-chat-composer-send"
              [title]="sendTitle()"
              aria-label="Send message"
              [disabled]="!canSend()"
            >
              <qt-icon name="send" class="w-5 h-5" />
            </button>
          }
        </div>
      </form>
    </div>
  `,
})
export class ChatComposer {
  /** Streaming/awaiting in flight — swaps Send for Stop and blocks input. */
  readonly busy = input(false);
  readonly disabled = input(false);
  readonly hasActiveCharacters = input(true);

  readonly send = output<string>();
  readonly stop = output<void>();
  readonly continue = output<void>();

  protected readonly text = signal('');

  protected readonly canSend = computed(
    () => !this.disabled() && !this.busy() && this.text().trim().length > 0 && this.hasActiveCharacters(),
  );

  protected readonly placeholder = computed(() =>
    this.hasActiveCharacters() ? 'Type a message…' : 'Add a character to start chatting…',
  );

  protected readonly sendTitle = computed(() =>
    this.hasActiveCharacters() ? 'Send message' : 'Add a character to start chatting',
  );

  protected onInput(event: Event): void {
    this.text.set((event.target as HTMLTextAreaElement).value);
  }

  protected onKeydown(event: KeyboardEvent): void {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      this.submit();
    }
  }

  protected onSubmit(event: Event): void {
    event.preventDefault();
    this.submit();
  }

  private submit(): void {
    if (!this.canSend()) {
      return;
    }
    const content = this.text().trim();
    this.send.emit(content);
    this.text.set('');
  }
}
