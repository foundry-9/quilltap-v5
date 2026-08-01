import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
  viewChild,
} from '@angular/core';

import { CoreClient } from '../core/core-client';
import { Modal } from '../ui/modal';
import { ToastService } from '../ui/toast.service';
import { regenerateChatTitle } from './chat-admin.api';

/**
 * Rename Chat (v4 `components/chat/ChatRenameModal.tsx`, 231 LOC) — reached
 * from the sidebar's Organize section.
 *
 * ## Ported exactly, and why
 *
 * - **The title field and the "Use automatic naming" checkbox are one control
 *   with two halves** (:186-197). Checking the box does not merely record a
 *   preference: it fires `?action=regenerate-title` immediately (:52-54), takes
 *   the title off the reply, reports "Title regenerated", tells the parent, and
 *   CLOSES (:61-67). Unchecking fires nothing at all. The Save button is HIDDEN
 *   while the box is checked (:148) — there is nothing left to save.
 * - **A failed regeneration reverts the checkbox** (:76). The operator is left
 *   exactly where they were, with the reason on screen.
 * - **Save's two early exits** (:84-95): an empty title refuses with "Title
 *   cannot be empty"; an unchanged title AND an unchanged preference closes
 *   without writing. v4 spells the second as `!useAutoRename ===
 *   initialIsManuallyRenamed`, which reads oddly and means "the manual-rename
 *   flag would not move".
 * - **Enter saves** — but only while the field is live (:131), i.e. not while
 *   automatic naming is on.
 * - **The field is focused and its text selected on open** (:38-43), again only
 *   when it is editable.
 *
 * ⚠ **This is the only path in v4 to `regenerate-title`** — v4 has no button of
 * that name anywhere — so until this dialog landed, v5's live `chatRegenerateTitle`
 * verb was unreachable from the UI (dogfood walk 2026-07-27). It spends a real
 * cheap-LLM call per tick.
 */
@Component({
  selector: 'qt-chat-rename-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <!-- v4 hands BaseModal the bare onClose, so the close button and the
         backdrop dismiss even mid-request; only the Cancel BUTTON is
         disabled (:141-146). -->
    <qt-modal title="Rename Chat" maxWidth="md" (close)="close.emit()">
      <div class="mb-4">
        <label for="chat-title" class="qt-label mb-1">Chat Title</label>
        <input
          #titleInput
          id="chat-title"
          type="text"
          class="qt-input"
          placeholder="Enter a title for this chat..."
          [value]="title()"
          [disabled]="loading() || useAutoRename()"
          (input)="title.set($any($event.target).value)"
          (keydown)="onKeyDown($event)"
        />
      </div>

      <div class="mb-2">
        <label class="flex items-center gap-2 cursor-pointer">
          <input
            type="checkbox"
            class="rounded border-input"
            aria-label="Use automatic naming"
            [checked]="useAutoRename()"
            [disabled]="loading()"
            (change)="onAutoRenameToggle($any($event.target))"
          />
          <span class="qt-text-small">Use automatic naming</span>
        </label>
        <p class="qt-text-xs mt-1 ml-6">
          @if (useAutoRename()) {
            The chat title will be updated automatically based on the conversation.
          } @else {
            The chat will keep the title you set and won&apos;t be renamed automatically.
          }
        </p>
      </div>

      @if (regenerating()) {
        <div class="qt-text-small flex items-center gap-2 mt-3" role="status">
          Generating title...
        </div>
      }

      <div qt-modal-footer class="flex justify-end gap-2">
        <button
          type="button"
          class="qt-button qt-button-secondary"
          [disabled]="loading()"
          (click)="close.emit()"
        >
          Cancel
        </button>
        @if (!useAutoRename()) {
          <button
            type="button"
            class="qt-button qt-button-primary"
            [disabled]="loading() || !title().trim()"
            (click)="onSave()"
          >
            {{ saving() ? 'Saving...' : 'Save' }}
          </button>
        }
      </div>
    </qt-modal>
  `,
})
export class ChatRenameModal {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly chatId = input.required<string>();
  readonly currentTitle = input.required<string>();
  readonly isManuallyRenamed = input.required<boolean>();

  /** v4 `onSuccess(newTitle, isManuallyRenamed)` — the parent refetches. */
  readonly renamed = output<{ title: string; isManuallyRenamed: boolean }>();
  readonly close = output<void>();

  protected readonly title = signal('');
  protected readonly useAutoRename = signal(false);
  protected readonly saving = signal(false);
  protected readonly regenerating = signal(false);
  protected readonly loading = computed(() => this.saving() || this.regenerating());

  private readonly titleInput = viewChild<ElementRef<HTMLInputElement>>('titleInput');

  constructor() {
    // v4's re-sync effect (:31-35). v5 mounts this dialog only while open, so it
    // fires once per open — which is also v4's focus/select trigger (:38-43).
    effect(() => {
      this.title.set(this.currentTitle());
      this.useAutoRename.set(!this.isManuallyRenamed());
    });
    effect(() => {
      if (this.useAutoRename()) return;
      const el = this.titleInput()?.nativeElement;
      if (el) {
        el.focus();
        el.select();
      }
    });
  }

  /**
   * v4 `handleAutoRenameToggle` (:45-81). Checking regenerates NOW; unchecking
   * only flips local state — the preference reaches the server on Save.
   *
   * The revert writes the ELEMENT as well as the signal, for the reason
   * `chat-section.ts`'s `write()` records: the binding memo still holds `false`
   * from before the user ticked the box, so putting `false` back leaves it
   * unchanged and Angular never restores the DOM. React reconciles a controlled
   * input regardless, which is why v4 needs no equivalent.
   */
  protected async onAutoRenameToggle(box: HTMLInputElement): Promise<void> {
    const enabled = box.checked;
    this.useAutoRename.set(enabled);
    if (!enabled) return;

    this.regenerating.set(true);
    try {
      const newTitle = await regenerateChatTitle(this.core, this.chatId());
      this.title.set(newTitle);
      this.toasts.showSuccess('Title regenerated');
      this.renamed.emit({ title: newTitle, isManuallyRenamed: false });
      this.close.emit();
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to regenerate title');
      // v4 "Revert toggle on error" (:76) — always to false, not to the prior value.
      this.useAutoRename.set(false);
      box.checked = false;
    } finally {
      this.regenerating.set(false);
    }
  }

  /** v4 `handleSave` (:83-128). */
  protected async onSave(): Promise<void> {
    const trimmed = this.title().trim();
    if (!trimmed) {
      this.toasts.showError('Title cannot be empty');
      return;
    }
    if (trimmed === this.currentTitle() && !this.useAutoRename() === this.isManuallyRenamed()) {
      this.close.emit();
      return;
    }

    this.saving.set(true);
    try {
      const resp = await this.core.dispatch({
        type: 'chatUpdate',
        chatId: this.chatId(),
        chat: { title: trimmed, isManuallyRenamed: !this.useAutoRename() },
      });
      if (resp.type === 'error') {
        throw new Error(resp.data.message || 'Failed to rename chat');
      }
      this.toasts.showSuccess('Chat renamed');
      this.renamed.emit({ title: trimmed, isManuallyRenamed: !this.useAutoRename() });
      this.close.emit();
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to rename chat');
    } finally {
      this.saving.set(false);
    }
  }

  /** v4 `handleKeyDown` (:130-135) — Enter saves while the field is editable. */
  protected onKeyDown(event: KeyboardEvent): void {
    if (event.key === 'Enter' && !event.shiftKey && !this.useAutoRename()) {
      event.preventDefault();
      void this.onSave();
    }
  }
}
