import { ChangeDetectionStrategy, Component, computed, inject, input, output, signal } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { CascadePreview } from '../../../core/core-contract';
import { Modal } from '../../../ui/modal';
import { characterKeys, fetchCascadePreview } from '../characters.api';

/** The chosen cascade flags emitted on confirm. */
export interface DeleteChoice {
  cascadeChats: boolean;
  cascadeImages: boolean;
}

/**
 * The character delete dialog (v4 `components/character-delete-dialog.tsx`).
 * Loads the `cascade-preview` impact, lists exclusive chats / images / memories,
 * and offers the `cascadeChats` / `cascadeImages` checkboxes (memories always
 * cascade). Microcopy carries over verbatim.
 */
@Component({
  selector: 'qt-character-delete-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal title="Delete Character" maxWidth="lg" (close)="close.emit()">
      @if (preview.isPending()) {
        <div class="py-8 text-center qt-text-small">Loading...</div>
      } @else if (preview.isError()) {
        <div class="py-4 qt-text-destructive">{{ errorMessage() }}</div>
      } @else {
        <p class="qt-text-small mb-4">
          Are you sure you want to delete
          <strong class="text-foreground">{{ characterName() }}</strong
          >?
        </p>

        @if (hasExclusiveData()) {
          <div class="qt-alert-warning mb-4">
            <p class="font-medium mb-3">This character has associated data:</p>

            @if (data()!.exclusiveChats.length > 0) {
              <div class="mb-3">
                <p class="text-sm mb-2">
                  <strong>{{ data()!.exclusiveChats.length }}</strong> exclusive chat{{
                    data()!.exclusiveChats.length !== 1 ? 's' : ''
                  }}
                  (only with this character):
                </p>
                <ul class="text-sm opacity-80 ml-4 list-disc max-h-32 overflow-y-auto">
                  @for (chat of data()!.exclusiveChats; track chat.id) {
                    <li>
                      {{ chat.title }} ({{ chat.messageCount }} message{{
                        chat.messageCount !== 1 ? 's' : ''
                      }})
                    </li>
                  }
                </ul>
              </div>
            }

            @if (data()!.totalExclusiveImageCount > 0) {
              <p class="text-sm">
                <strong>{{ data()!.totalExclusiveImageCount }}</strong> exclusive image{{
                  data()!.totalExclusiveImageCount !== 1 ? 's' : ''
                }}
                (not used elsewhere)
              </p>
            }

            @if (data()!.memoryCount > 0) {
              <p class="text-sm">
                <strong>{{ data()!.memoryCount }}</strong> memor{{
                  data()!.memoryCount !== 1 ? 'ies' : 'y'
                }}
                (will always be deleted)
              </p>
            }
          </div>

          <div class="space-y-3 mb-2">
            @if (data()!.exclusiveChats.length > 0) {
              <label class="flex items-center gap-3 cursor-pointer">
                <input
                  type="checkbox"
                  class="w-4 h-4 rounded qt-border-default"
                  [checked]="deleteChats()"
                  (change)="deleteChats.set($any($event.target).checked)"
                />
                <span class="qt-text-secondary">Delete exclusive chats</span>
              </label>
            }
            @if (data()!.totalExclusiveImageCount > 0) {
              <label class="flex items-center gap-3 cursor-pointer">
                <input
                  type="checkbox"
                  class="w-4 h-4 rounded qt-border-default"
                  [checked]="deleteImages()"
                  (change)="deleteImages.set($any($event.target).checked)"
                />
                <span class="qt-text-secondary">Delete exclusive images</span>
              </label>
            }
          </div>
        } @else {
          <p class="qt-text-small mb-4">
            This character has no exclusive chats or images that would be deleted.
          </p>
        }
      }

      <div qt-modal-footer class="flex gap-3 justify-end">
        <button type="button" class="qt-button qt-button-secondary" (click)="close.emit()">
          Cancel
        </button>
        <button
          type="button"
          class="qt-button qt-button-destructive"
          [disabled]="preview.isPending()"
          (click)="confirm()"
        >
          Delete Character
        </button>
      </div>
    </qt-modal>
  `,
})
export class CharacterDeleteDialog {
  private readonly core = inject(CoreClient);

  readonly characterId = input.required<string>();
  readonly characterName = input.required<string>();

  readonly close = output<void>();
  readonly confirmed = output<DeleteChoice>();

  protected readonly deleteChats = signal(true);
  protected readonly deleteImages = signal(true);

  protected readonly preview = injectQuery(() => ({
    queryKey: characterKeys.detail(this.characterId()).concat('cascade-preview'),
    queryFn: (): Promise<CascadePreview> => fetchCascadePreview(this.core, this.characterId()),
  }));

  protected readonly data = computed(() => this.preview.data());
  protected readonly hasExclusiveData = computed(() => {
    const p = this.data();
    return (
      !!p && (p.exclusiveChats.length > 0 || p.totalExclusiveImageCount > 0 || p.memoryCount > 0)
    );
  });

  protected errorMessage(): string {
    const err = this.preview.error();
    return err instanceof Error ? err.message : 'Failed to load preview';
  }

  protected confirm(): void {
    this.confirmed.emit({
      cascadeChats: this.deleteChats(),
      cascadeImages: this.deleteImages(),
    });
  }
}
