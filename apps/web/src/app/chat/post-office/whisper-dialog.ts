import { ChangeDetectionStrategy, Component, inject, input, output, signal } from '@angular/core';

import { CoreClient } from '../../core/core-client';

/**
 * The Whisper dialog (v4 `components/chat/WhisperDialog.tsx`): a private line to
 * one character, visible only to them and the operator.
 *
 * **It uses no Post Office verb.** v4 posts the ORDINARY send with
 * `targetParticipantIds: [target]`, which v5's chat-send spine already carries —
 * so this dialog needed no new server work at all.
 *
 * v4's sequencing is the part worth copying carefully (`:49-70`): the dialog
 * closes IMMEDIATELY, before the turn is finished, so the operator is not left
 * staring at a modal while a character composes a reply. Only then does v4 drain
 * the SSE body to completion and call `onSent()`, which refetches the chat. The
 * drain matters — abandoning the stream would abort the server-side turn — and
 * an aborted read is swallowed on purpose ("Stream may be aborted, that's OK").
 *
 * ONE forced divergence, and it is in the error path. v4 splits the round trip
 * into headers (`response.ok` — a failure here throws BEFORE the close) and body
 * (the drain, after). v5's `dispatch` is a single round trip with no header/body
 * seam, so the close cannot be gated on a status that does not exist yet: the
 * dialog closes first and a failure is swallowed to the console, exactly as v4
 * swallows its own. The user-visible property v4 was after — "close immediately
 * so the user isn't waiting" — is preserved; what is lost is v4's ability to
 * keep the dialog open on an immediate rejection.
 */
@Component({
  selector: 'qt-whisper-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="fixed inset-0 z-50 flex items-center justify-center">
      <div class="absolute inset-0 qt-bg-overlay-caption" (click)="close.emit()"></div>
      <div class="qt-dialog relative z-10 w-full max-w-md mx-4" role="dialog" aria-modal="true">
        <div class="qt-dialog-header">
          <h3 class="qt-dialog-title">Whisper to {{ targetName() }}</h3>
        </div>
        <div class="qt-dialog-body">
          <p class="text-sm qt-text-muted mb-3">
            This message will only be visible to you and {{ targetName() }}.
          </p>
          <textarea
            class="qt-input w-full min-h-[100px] resize-y"
            [attr.aria-label]="'Whisper to ' + targetName()"
            [placeholder]="'Write a private message to ' + targetName() + '...'"
            [value]="message()"
            [disabled]="sending()"
            autofocus
            (input)="message.set($any($event.target).value)"
            (keydown)="onKeydown($event)"
          ></textarea>
        </div>
        <div class="qt-dialog-footer">
          <button
            type="button"
            class="qt-button qt-button-secondary"
            [disabled]="sending()"
            (click)="close.emit()"
          >
            Cancel
          </button>
          <button
            type="button"
            class="qt-button qt-button-primary"
            [disabled]="message().trim().length === 0 || sending()"
            (click)="send()"
          >
            {{ sending() ? 'Sending...' : 'Whisper' }}
          </button>
        </div>
      </div>
    </div>
  `,
})
export class WhisperDialog {
  private readonly core = inject(CoreClient);

  readonly chatId = input.required<string>();
  readonly targetName = input.required<string>();
  readonly targetParticipantId = input.required<string>();
  /** The user-controlled participant the human is Speaking As (null = default). */
  readonly speakingAsParticipantId = input<string | null>(null);

  readonly close = output<void>();
  /** The turn finished — the salon refetches and resumes turn order (v4 `onSent`). */
  readonly sent = output<void>();

  protected readonly message = signal('');
  protected readonly sending = signal(false);

  /** v4 `handleKeyDown` (`:78-86`): Enter sends, Shift+Enter newlines, Escape closes. */
  protected onKeydown(event: KeyboardEvent): void {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      void this.send();
    }
    if (event.key === 'Escape') {
      this.close.emit();
    }
  }

  /** v4 `handleSend` (`:34-76`) — see the class comment for the sequencing. */
  protected async send(): Promise<void> {
    const content = this.message().trim();
    if (!content || this.sending()) return;

    this.sending.set(true);
    const request = this.core.dispatch({
      type: 'chatSend',
      chatId: this.chatId(),
      content,
      targetParticipantIds: [this.targetParticipantId()],
      speakingAsParticipantId: this.speakingAsParticipantId() ?? undefined,
    });

    // Close first — the operator should not wait out the reply (v4 `:53-55`).
    this.message.set('');
    this.close.emit();

    try {
      // The v5 analogue of v4's drain: the dispatch settles when the server-side
      // turn completes. Abandoning it would abort the turn, so it is awaited.
      await request;
      this.sent.emit();
    } catch (error) {
      // v4 `:71-73` — a failed whisper is logged, not surfaced.
      console.error('Failed to send whisper:', error);
    } finally {
      this.sending.set(false);
    }
  }
}
