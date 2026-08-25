import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

import type { FileAssociations } from '../../core/core-contract';
import { Modal } from '../../ui/modal';

/**
 * The associations-aware delete confirmation (v4
 * `components/files/FileDeleteConfirmation.tsx`). Shown after a bare delete
 * returned the `FILE_HAS_ASSOCIATIONS` envelope: it lists the characters and
 * chats the file is linked to, and offers "Delete Anyway" (the dissociate arm).
 * Pure presentational — the parent owns the dissociate dispatch.
 */
@Component({
  selector: 'qt-file-delete-confirmation',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal title="This file is in use" maxWidth="lg" (close)="cancel.emit()">
      <div class="space-y-4">
        <p class="text-base font-semibold">
          "{{ filename() }}" is associated with characters and messages
        </p>

        @if (hasCharacters()) {
          <div>
            <p class="qt-text-small font-medium mb-2">Used by characters:</p>
            <ul class="space-y-1 ml-4">
              @for (char of associations().characters; track char.id) {
                <li class="qt-text-small">
                  <span class="font-medium">{{ char.name }}</span>
                  @if (char.usage) {
                    <span class="qt-text-secondary"> — {{ char.usage }}</span>
                  }
                </li>
              }
            </ul>
          </div>
        }

        @if (hasMessages()) {
          <div>
            <p class="qt-text-small font-medium mb-2">Attached to messages in:</p>
            <ul class="space-y-1 ml-4">
              @for (msg of associations().messages; track msg.chatId + '-' + msg.messageId) {
                <li class="qt-text-small">{{ msg.chatName }}</li>
              }
            </ul>
          </div>
        }

        <p class="qt-text-small qt-text-secondary pt-2">
          Deleting will remove these associations. Messages will show a note indicating the
          attachment was deleted.
        </p>
      </div>

      <div qt-modal-footer class="flex justify-end gap-2">
        <button
          type="button"
          class="qt-button qt-button-secondary"
          [disabled]="isDeleting()"
          (click)="cancel.emit()"
        >
          Cancel
        </button>
        <button
          type="button"
          class="qt-button bg-destructive qt-text-on-destructive disabled:opacity-50"
          [disabled]="isDeleting()"
          (click)="confirm.emit()"
        >
          {{ isDeleting() ? 'Deleting...' : 'Delete Anyway' }}
        </button>
      </div>
    </qt-modal>
  `,
})
export class FileDeleteConfirmation {
  readonly filename = input.required<string>();
  readonly associations = input.required<FileAssociations>();
  readonly isDeleting = input(false);

  readonly confirm = output<void>();
  readonly cancel = output<void>();

  protected readonly hasCharacters = computed(() => this.associations().characters.length > 0);
  protected readonly hasMessages = computed(() => this.associations().messages.length > 0);
}
