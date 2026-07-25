import { ChangeDetectionStrategy, Component, input, output, signal } from '@angular/core';

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
 * **The turn itself belongs to the Salon, not to this dialog**, and that split is
 * forced by the framework rather than chosen. v4's dialog owns its `fetch` and
 * calls `onSent` from a closure that outlives its own unmount — React lets it.
 * An Angular `output()` is torn down when the component is destroyed, so a
 * dialog that closed first and emitted afterwards would emit into nothing: the
 * chat would never refetch and the whisper would sit invisible until something
 * else refetched. So the dialog is a form — it reports the line and closes — and
 * `SalonConversation` runs the send, keeping v4's close-then-await order and its
 * swallowed failure at the call site.
 *
 * The other divergence is in the error path. v4 splits the round trip into
 * headers (`response.ok` — a failure there throws BEFORE the close) and body (the
 * drain, after). v5's dispatch has no header/body seam, so the close cannot be
 * gated on a status that does not exist yet. The property v4 wanted — "close
 * immediately so the user isn't waiting" — is preserved; what is lost is v4's
 * ability to keep the dialog open on an immediate rejection.
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
            (click)="commit()"
          >
            {{ sending() ? 'Sending...' : 'Whisper' }}
          </button>
        </div>
      </div>
    </div>
  `,
})
export class WhisperDialog {
  readonly targetName = input.required<string>();
  /** Carried on `send` so the Salon needs no lookup of its own. */
  readonly targetParticipantId = input.required<string>();

  readonly close = output<void>();
  /** The operator committed a line — the Salon runs the turn (v4's own `fetch`). */
  readonly send = output<{ targetParticipantId: string; content: string }>();

  protected readonly message = signal('');
  /**
   * Kept for the `disabled` bindings v4 carries. In practice it flips for one
   * tick, because the dialog closes the instant the line is committed — the
   * documented consequence of the divergence above.
   */
  protected readonly sending = signal(false);

  /** v4 `handleKeyDown` (`:78-86`): Enter sends, Shift+Enter newlines, Escape closes. */
  protected onKeydown(event: KeyboardEvent): void {
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault();
      this.commit();
    }
    if (event.key === 'Escape') {
      this.close.emit();
    }
  }

  /** v4 `handleSend` (`:34-76`) — hand the line over, clear, and close at once. */
  protected commit(): void {
    const content = this.message().trim();
    if (!content || this.sending()) return;
    this.sending.set(true);
    this.send.emit({ targetParticipantId: this.targetParticipantId(), content });
    // Close first — the operator should not wait out the reply (v4 `:53-55`).
    this.message.set('');
    this.close.emit();
  }
}
