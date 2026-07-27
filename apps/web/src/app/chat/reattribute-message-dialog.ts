import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import { CoreClient } from '../core/core-client';
import type { ParticipantDetail } from '../core/core-contract';
import { Avatar } from '../ui/avatar';
import { normalizeAvatarSrc } from '../ui/avatar-stack';
import { Icon } from '../ui/icon';
import { reattributeMessage } from './chat-admin.api';

/**
 * Re-attribute Message (v4 `components/chat/ReattributeMessageDialog.tsx`,
 * 258 LOC) — opened from the MESSAGE ACTION BAR (v4
 * `message-row/MessageActionBar.tsx:138-147`), not from the sidebar. The bulk
 * form of the same operation lives in the Edit Content drawer.
 *
 * ## Ported exactly, and why
 *
 * - **The current participant is excluded from the list** (`:69-71`) — and v4's
 *   action-bar entry is hidden entirely when that leaves nobody (`:139`), so the
 *   "No other participants available in this chat" empty state (`:157`) is
 *   reachable only if the cast changes under an open dialog. It is ported
 *   anyway, being v4's.
 * - **Nothing is preselected** (`:50,58`): the operator must choose, and Submit
 *   stays disabled until they do.
 * - **The warning is stated up front** (`:162-165`): every memory drawn from this
 *   message will be deleted. The count comes back on the reply and is only
 *   mentioned when it is non-zero (`:102-106`).
 * - The card is v4's own overlay markup, not `BaseModal`; the click-outside and
 *   Escape are both ignored while the request is in flight (`:63-66`).
 *
 * The reply's `memoriesDeleted` is the only field read; §1 does not pin response
 * shapes, so a body without it reads as zero and the sentence loses its second
 * half rather than the dialog failing.
 */
@Component({
  selector: 'qt-reattribute-message-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Avatar, Icon],
  template: `
    <div class="qt-dialog-overlay p-4" (click)="onBackdrop($event)">
      <div class="qt-dialog max-w-md" role="dialog" aria-modal="true">
        <div class="qt-dialog-header flex items-center justify-between">
          <h2 class="qt-dialog-title">Re-attribute Message</h2>
          <button
            type="button"
            class="qt-button qt-button-ghost p-2"
            aria-label="Close"
            [disabled]="submitting()"
            (click)="close.emit()"
          >
            <qt-icon name="close" class="w-6 h-6" />
          </button>
        </div>

        <div class="qt-dialog-body">
          @if (available().length === 0) {
            <div class="text-center py-8 qt-text-secondary">
              No other participants available in this chat.
            </div>
          } @else {
            <div class="space-y-4">
              <p class="qt-text-secondary text-sm">
                Select who should be attributed as the sender of this message. Any memories
                associated with this message will be deleted.
              </p>

              <div class="space-y-2 max-h-64 overflow-y-auto">
                @for (p of available(); track p.id) {
                  <button
                    type="button"
                    [class]="
                      'w-full p-3 rounded-lg border text-left transition-all ' +
                      (selected() === p.id
                        ? 'qt-border-primary qt-bg-primary/10 ring-2 ring-primary'
                        : 'qt-border-default hover:qt-border-primary/50 hover:qt-bg-muted/50')
                    "
                    [disabled]="submitting()"
                    (click)="selected.set(p.id)"
                  >
                    <div class="flex items-center gap-3">
                      <qt-avatar [name]="name(p)" [src]="avatar(p)" size="md" />
                      <div class="min-w-0 flex-1">
                        <div class="flex items-center gap-2">
                          <span class="font-semibold text-foreground truncate">{{ name(p) }}</span>
                          <span
                            class="qt-text-xs px-1.5 py-0.5 rounded qt-bg-muted qt-text-secondary"
                            >{{ p.type === 'CHARACTER' ? 'Character' : 'Persona' }}</span
                          >
                        </div>
                        @if (p.character?.title; as title) {
                          <div class="qt-text-xs italic truncate qt-text-secondary">
                            {{ title }}
                          </div>
                        }
                      </div>
                      @if (selected() === p.id) {
                        <qt-icon name="check-circle" class="w-5 h-5 text-primary flex-shrink-0" />
                      }
                    </div>
                  </button>
                }
              </div>

              @if (error(); as message) {
                <p class="text-sm qt-text-danger" role="status">{{ message }}</p>
              }
            </div>
          }
        </div>

        <div class="qt-dialog-footer flex justify-end gap-2">
          <button
            type="button"
            class="qt-button qt-button-secondary"
            [disabled]="submitting()"
            (click)="close.emit()"
          >
            Cancel
          </button>
          <button
            type="button"
            class="qt-button qt-button-primary"
            [disabled]="submitting() || !selected() || available().length === 0"
            (click)="onSubmit()"
          >
            {{ submitting() ? 'Re-attributing...' : 'Re-attribute' }}
          </button>
        </div>
      </div>
    </div>
  `,
})
export class ReattributeMessageDialog {
  private readonly core = inject(CoreClient);

  readonly messageId = input.required<string>();
  readonly currentParticipantId = input<string | null>(null);
  readonly participants = input.required<ParticipantDetail[]>();

  /** v4 `onReattributed()` — the parent refetches and scrolls the message back. */
  readonly reattributed = output<string>();
  readonly close = output<void>();

  protected readonly selected = signal<string | null>(null);
  protected readonly submitting = signal(false);
  protected readonly error = signal<string | null>(null);

  /** v4 `availableParticipants` (:69-71). */
  protected readonly available = computed(() =>
    this.participants().filter((p) => p.id !== this.currentParticipantId()),
  );

  protected name(p: ParticipantDetail): string {
    return p.character?.name || 'Unknown';
  }

  protected avatar(p: ParticipantDetail): string | null {
    return normalizeAvatarSrc(p.character?.avatarUrl);
  }

  protected onBackdrop(event: MouseEvent): void {
    if (event.target !== event.currentTarget || this.submitting()) return;
    this.close.emit();
  }

  /** v4 `handleReattribute` (:73-118). */
  protected async onSubmit(): Promise<void> {
    const target = this.selected();
    if (!target) {
      this.error.set('Please select a participant');
      return;
    }

    this.error.set(null);
    this.submitting.set(true);
    try {
      const result = await reattributeMessage(this.core, this.messageId(), target);
      const name =
        this.participants().find((p) => p.id === target)?.character?.name || 'participant';
      this.reattributed.emit(
        result.memoriesDeleted > 0
          ? `Message re-attributed to ${name}. ${result.memoriesDeleted} ` +
              `${result.memoriesDeleted === 1 ? 'memory' : 'memories'} deleted.`
          : `Message re-attributed to ${name}.`,
      );
      this.close.emit();
    } catch (err) {
      this.error.set(
        err instanceof Error ? err.message : 'Failed to re-attribute message',
      );
    } finally {
      this.submitting.set(false);
    }
  }
}
