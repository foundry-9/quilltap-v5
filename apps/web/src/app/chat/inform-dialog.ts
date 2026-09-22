import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../core/core-client';
import { MarkdownField } from '../editor/markdown-field';
import { Modal } from '../ui/modal';
import { ToastService } from '../ui/toast.service';
import { chatKeys } from './chat-keys';

/**
 * A current participant of the chat, offered as an inform target (v4
 * `InformDialog.tsx:66-74`'s `InformAudienceCandidate`).
 *
 * Structurally identical to the Post Office's `AudienceCandidate`, and v5's
 * salon feeds BOTH dialogs from the one `audienceCandidates()` computed — but
 * declared here under v4's own name, because the two are separate types in v4
 * and a shared alias would quietly couple two dialogs that only happen to agree.
 */
export interface InformAudienceCandidate {
  /** CHAT PARTICIPANT id — never a character id. */
  participantId: string;
  name: string;
  controlledBy: 'llm' | 'user';
  avatarUrl?: string | null;
  status?: 'active' | 'silent' | 'absent' | 'removed';
}

/**
 * The Inform dialog — a quiet word out of character (v4
 * `components/chat/InformDialog.tsx`, `e7d77bb60`).
 *
 * The operator picks one, several or every LLM-controlled seat and writes a
 * short second-person passage. Each target receives it verbatim as a system
 * block on their next generation, and then it is consumed. Nothing here is ever
 * spoken aloud; the transcript keeps a Host record for the operator alone
 * (labelled `out of character` — `system-message-labels.ts`).
 *
 * Only LLM-controlled seats are offered: a seat the human plays has no
 * generation to slip the passage into. An empty selection means Everyone, and
 * so does a full one — actual COVERAGE decides the record's public/whisper
 * shape, not how the operator clicked, so ticking every seat posts the same
 * `null` the default does.
 *
 * Two divergences from v4's component, both forced and both v5 precedent:
 *
 *  - v4 hangs this off a draggable `FloatingDialog` with persisted geometry
 *    (`storageKey="quilltap:inform-geometry"`, min 720×460, opened 780×620 —
 *    a width derived from the formatting toolbar's own CSS so the toolbar
 *    never wraps). v5 has no such primitive, so this is a centered `qt-modal`
 *    at the `4xl` token (56rem = 896px, comfortably past v4's 780) — the
 *    `insert-announcement-dialog.ts` / `brahma-console-dialog.ts` ruling. The
 *    geometry store has nothing to persist and is NOT ported; recorded rather
 *    than invented.
 *  - v4's body goes through `MarkdownLexicalEditor`; v5's twin of that
 *    component is `qt-markdown-field`, on the same v4 markdown dialect.
 *
 * State resets on each open because the salon renders the dialog conditionally
 * (v4 relies on exactly the same thing — its own comment at `:95-97`), so every
 * open is a fresh mount.
 */
@Component({
  selector: 'qt-inform-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, MarkdownField],
  template: `
    <qt-modal title="Inform the cast" maxWidth="4xl" [closeOnBackdrop]="!isPosting()" (close)="onDialogClose()">
      <!-- Audience — Everyone by default; ticking a seat narrows it (v4 :175-233). -->
      <div class="mb-4">
        <label class="block text-sm qt-text-primary mb-2" id="inform-audience-label">
          Who is told
        </label>
        @if (eligible().length === 0) {
          <div class="qt-text-secondary text-sm">
            No seat in this chat is played by a model, so there is nobody to take the note
            aside.
          </div>
        } @else {
          <div role="group" aria-labelledby="inform-audience-label" class="flex flex-wrap gap-2">
            <button
              type="button"
              [attr.aria-pressed]="everyone()"
              [disabled]="isPosting()"
              [class]="
                'px-3 py-1.5 text-sm rounded-full border qt-border-primary flex items-center gap-2 ' +
                (everyone() ? 'qt-bg-primary/20' : 'hover:qt-bg-primary/10')
              "
              (click)="selected.set([])"
            >
              Everyone
            </button>
            @for (p of eligible(); track p.participantId) {
              <button
                type="button"
                [attr.aria-pressed]="isPicked(p.participantId)"
                [disabled]="isPosting()"
                [class]="
                  'px-3 py-1.5 text-sm rounded-full border qt-border-primary flex items-center gap-2 ' +
                  (isPicked(p.participantId) ? 'qt-bg-primary/20' : 'hover:qt-bg-primary/10')
                "
                (click)="toggleSeat(p.participantId)"
              >
                @if (p.avatarUrl) {
                  <img
                    [src]="p.avatarUrl"
                    alt=""
                    class="w-5 h-5 rounded-full object-cover flex-shrink-0"
                  />
                } @else {
                  <div class="w-5 h-5 rounded-full qt-bg-secondary flex-shrink-0"></div>
                }
                <span class="min-w-0 truncate">{{ p.name }}</span>
                @if (p.status === 'silent' || p.status === 'absent') {
                  <span class="qt-text-xs flex-shrink-0">({{ p.status }})</span>
                }
              </button>
            }
          </div>
        }
      </div>

      <!-- Guidance — the second-person rule, in the house voice (v4 :236-244). -->
      <div class="mb-4 qt-text-xs">
        Write it <em>to</em> them, in the second person, as something they now know or notice —
        <em>You see that Alice slipped the letter into her sleeve.</em>
        <em>You remember that Bob and Carol were at school together.</em> Everyone you tick
        receives the identical words before their next turn, so set down a passage that is true
        from each of their chairs. It is never spoken aloud, and once they have had their turn it
        is gone, like a note fed to the fire.
      </div>

      <!-- The passage itself (v4 :247-262). -->
      <div class="flex flex-col min-h-0">
        <label class="block text-sm qt-text-primary mb-2" id="inform-body-label">
          What they are told
        </label>
        <qt-markdown-field
          [value]="content()"
          [disabled]="isPosting()"
          ariaLabel="What they are told"
          minHeight="12rem"
          (contentChange)="content.set($event)"
        />
      </div>

      <div qt-modal-footer class="flex items-center justify-end gap-3">
        <button
          type="button"
          class="qt-button qt-button-secondary"
          [disabled]="isPosting()"
          (click)="close.emit()"
        >
          Cancel
        </button>
        <button
          type="button"
          class="qt-button qt-button-primary"
          [disabled]="!canSubmit()"
          (click)="onPost()"
        >
          {{ isPosting() ? 'Informing…' : 'Inform' }}
        </button>
      </div>
    </qt-modal>
  `,
})
export class InformDialog {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);
  private readonly queryClient = injectQueryClient();

  readonly chatId = input.required<string>();
  /** Current participants. User-controlled seats are filtered out here (v4 `:81`). */
  readonly audienceCandidates = input<readonly InformAudienceCandidate[]>([]);

  readonly close = output<void>();
  /** A batch landed — the salon refetches the chat (v4 `onPosted`). */
  readonly posted = output<void>();

  protected readonly content = signal('');
  /**
   * Chosen seats. EMPTY MEANS EVERYONE — the default, and what a full selection
   * collapses back to when it is posted (v4 `:101-103`).
   */
  protected readonly selected = signal<string[]>([]);
  protected readonly isPosting = signal(false);

  /**
   * Only LLM-controlled seats can be informed: a seat the human plays has no
   * generation to slip the passage into, so it is not offered at all. v4 tests
   * `!== 'user'` rather than `=== 'llm'`, so a seat whose control this build
   * has never heard of is still offered — carried as written.
   */
  protected readonly eligible = computed(() =>
    this.audienceCandidates().filter((p) => p.controlledBy !== 'user'),
  );

  protected readonly everyone = computed(
    () => this.selected().length === 0 || this.selected().length === this.eligible().length,
  );

  protected readonly canSubmit = computed(
    () => !this.isPosting() && this.content().trim().length > 0 && this.eligible().length > 0,
  );

  private readonly selectedNames = computed(() =>
    this.selected()
      .map((id) => this.eligible().find((p) => p.participantId === id)?.name)
      .filter((name): name is string => Boolean(name)),
  );

  protected isPicked(participantId: string): boolean {
    return this.selected().includes(participantId);
  }

  protected toggleSeat(participantId: string): void {
    this.selected.update((prev) =>
      prev.includes(participantId)
        ? prev.filter((id) => id !== participantId)
        : [...prev, participantId],
    );
  }

  /** v4's `dialogClose`: a post in flight swallows the close (`:163`). */
  protected onDialogClose(): void {
    if (this.isPosting()) return;
    this.close.emit();
  }

  /** v4 `handlePost` (`:126-161`). */
  protected async onPost(): Promise<void> {
    if (!this.canSubmit()) return;
    // Actual coverage decides the record's public/whisper shape, not how the
    // operator clicked: ticking every seat is the same thing as Everyone.
    const everyone = this.everyone();
    const targetParticipantIds = everyone ? null : this.selected();
    const names = this.selectedNames();

    this.isPosting.set(true);
    try {
      await this.core.dispatchData({
        type: 'chatInform',
        chatId: this.chatId(),
        contentMarkdown: this.content().trim(),
        targetParticipantIds,
      });
      this.toasts.showSuccess(
        everyone ? 'The company has been informed' : `Informed ${names.join(', ')}`,
      );
      this.posted.emit();
      void this.queryClient.invalidateQueries({
        queryKey: chatKeys.informs(this.chatId()),
      });
      this.close.emit();
    } catch (err) {
      // v4 splits this in two: a non-ok response toasts the server's own
      // message and RETURNS (leaving the dialog open), and only a thrown
      // network error reaches its catch. v5's dispatch client raises both as
      // errors carrying the server's message, so the two arms are one — and
      // the visible behavior, an error toast with the dialog still open, is
      // the same on both legs.
      this.toasts.showError(
        (err instanceof Error && err.message) || 'Failed to post the inform',
      );
    } finally {
      this.isPosting.set(false);
    }
  }
}
