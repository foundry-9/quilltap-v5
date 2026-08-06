import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import { Avatar } from '../ui/avatar';
import { Modal } from '../ui/modal';

/** The take-over roster the modal renders (v4 `useParticipants.llmParticipants`). */
export interface AllLLMPauseParticipant {
  id: string;
  characterName: string;
  avatarUrl: string | null;
}

/**
 * The all-LLM pause dialog (v4 `components/chat/AllLLMPauseModal.tsx`, 148 LOC).
 *
 * It surfaces when every participant is LLM-controlled and the automatic run hits
 * a pause threshold (3, 6, 12, 24, 48…), so a runaway room stops asking the API
 * on its own. It offers to continue for one more interval, stop the chat, or take
 * control of a character. v4 gave it an opener in `bd419ae9` (bug 37) — before
 * that it was mounted but unreachable, which is why v5 had deferred it.
 *
 * Ported case-for-case: v4's copy and classes verbatim, the three actions each
 * emitting and then requesting a close (v4's `handleContinue`/`handleStop`/
 * `handleTakeOver`), and `closeOnClickOutside={false}` → `[closeOnBackdrop]`.
 */
@Component({
  selector: 'qt-all-llm-pause-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, Avatar],
  template: `
    <qt-modal
      title="All Characters Controlled by AI"
      maxWidth="md"
      [closeOnBackdrop]="false"
      (close)="close.emit()"
    >
      <div class="space-y-4">
        <p class="qt-text">
          This chat has been running automatically for
          <strong>{{ turnCount() }} turns</strong> without user input. All characters are currently
          controlled by AI.
        </p>

        <div class="qt-card p-3">
          <div class="qt-text-small">
            <span class="font-semibold">Pause intervals:</span> 3, 6, 12, 24, 48...
          </div>
          <div class="qt-text-xs mt-1">The next pause will occur at turn {{ nextPauseAt() }}.</div>
        </div>

        <p class="qt-text-small">
          You can continue for more turns, stop the conversation, or take control of one of the
          characters to participate directly.
        </p>
      </div>

      <div qt-modal-footer class="flex flex-col gap-3">
        <div class="flex gap-2">
          <button type="button" class="flex-1 qt-button qt-button-primary" (click)="onContinue()">
            Continue ({{ turnsUntilNext() }} more turns)
          </button>
          <button type="button" class="qt-button qt-button-secondary" (click)="onStop()">Stop</button>
        </div>

        @if (participants().length > 0) {
          <div class="border-t pt-3 mt-1">
            <div class="qt-text-small mb-2">Or take control of a character:</div>
            <div class="flex flex-wrap gap-2">
              @for (p of participants(); track p.id) {
                <button
                  type="button"
                  class="qt-button qt-button-secondary qt-button-sm flex items-center gap-2"
                  (click)="onTakeOver(p.id)"
                >
                  <qt-avatar [name]="p.characterName" [src]="p.avatarUrl" size="xs" />
                  <span>Play as {{ p.characterName }}</span>
                </button>
              }
            </div>
          </div>
        }
      </div>
    </qt-modal>
  `,
})
export class AllLLMPauseModal {
  readonly turnCount = input.required<number>();
  readonly nextPauseAt = input.required<number>();
  readonly participants = input.required<AllLLMPauseParticipant[]>();

  /** v4 `onContinue(turnsToAdd)` — the operator dismisses for another interval. */
  readonly continueRun = output<number>();
  /** v4 `onStop` — pause the chat. */
  readonly stopRun = output<void>();
  /** v4 `onTakeOver(participantId)` — start impersonating that character. */
  readonly takeOver = output<string>();
  readonly close = output<void>();

  protected readonly turnsUntilNext = computed(() => this.nextPauseAt() - this.turnCount());

  protected onContinue(): void {
    this.continueRun.emit(this.turnsUntilNext());
    this.close.emit();
  }

  protected onStop(): void {
    this.stopRun.emit();
    this.close.emit();
  }

  protected onTakeOver(participantId: string): void {
    this.takeOver.emit(participantId);
    this.close.emit();
  }
}
