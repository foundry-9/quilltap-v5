import { ChangeDetectionStrategy, Component, inject, input, signal } from '@angular/core';

import type { SubpromptRecord } from '../core/core-contract';
import { Icon } from '../ui/icon';
import { ToastService } from '../ui/toast.service';
import { SubpromptEditorModal } from './subprompt-editor-modal';
import { injectCharacterSubprompts, subpromptErrorMessage } from './subprompts.api';

/**
 * `qt-subprompts-section` — the list of a character's subprompts on the Aurora
 * "System Prompts" tab, under the primary prompts (v4
 * `components/characters/system-prompts-editor/SubpromptsSection.tsx` at
 * `2f4254b42`).
 *
 * Create, edit and delete all go through the shared editor dialog and the
 * shared query, so this is the same editor the New Chat and Salon dropdowns
 * summon and the same cache they read.
 *
 * The delete confirmation is an INLINE popover anchored to the trash button —
 * v4's shape, not a modal — so exactly one row can be mid-confirmation at a
 * time and dismissing it costs one click.
 */
@Component({
  selector: 'qt-subprompts-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, SubpromptEditorModal],
  // v4's outer element is a `div` in a stack of siblings; an unstyled Angular
  // custom element is `display: inline`, which would drop the `pt-6 border-t`
  // rule below out of the flow (dogfood #97 / #107's class).
  host: { class: 'block' },
  template: `
    <div class="space-y-4 pt-6 border-t qt-border-default">
      <div class="flex justify-between items-center">
        <div>
          <h3 class="qt-heading-4 text-foreground">Subprompts</h3>
          <p class="qt-text-small">
            Smaller instructions {{ characterName() }} may carry into a particular chat. Each lives
            as a Markdown file in the vault’s <code>Subprompts/</code> folder, and is switched on or
            off per conversation from the New Chat dialog or the Participants drawer. Write them to
            the character in the second person, as you would a system prompt.
          </p>
        </div>
        <button type="button" class="qt-button-primary flex-shrink-0" (click)="openCreate()">
          + Add Subprompt
        </button>
      </div>

      @if (isLoading()) {
        <div class="text-center py-4 qt-text-secondary">Loading subprompts...</div>
      } @else if (subprompts().length === 0) {
        <div class="qt-card text-center">
          <p class="qt-text-small mb-4">
            No subprompts yet. Add one to have it on offer when a chat begins.
          </p>
          <button type="button" class="qt-button-primary" (click)="openCreate()">
            Create First Subprompt
          </button>
        </div>
      } @else {
        <div class="space-y-3">
          @for (s of subprompts(); track s.id) {
            <div class="qt-card hover:bg-accent/50 transition">
              <div class="flex justify-between items-start">
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2 mb-1">
                    <h4 class="qt-text-primary truncate">{{ s.title }}</h4>
                    <span class="text-xs qt-text-secondary truncate" [title]="s.path">{{
                      s.path
                    }}</span>
                  </div>
                  <p class="qt-text-small line-clamp-2">{{ preview(s) }}</p>
                </div>
                <div class="flex items-center gap-1 ml-4">
                  <button
                    type="button"
                    class="qt-button-icon qt-button-ghost"
                    title="Edit"
                    [attr.aria-label]="'Edit subprompt ' + s.title"
                    (click)="openEdit(s)"
                  >
                    <qt-icon name="pencil" class="w-4 h-4" />
                  </button>
                  <div class="relative">
                    <button
                      type="button"
                      class="qt-button-icon qt-button-ghost hover:qt-text-destructive"
                      title="Delete"
                      [attr.aria-label]="'Delete subprompt ' + s.title"
                      (click)="toggleConfirm(s.id)"
                    >
                      <qt-icon name="trash" class="w-4 h-4" />
                    </button>
                    @if (deleteConfirm() === s.id) {
                      <div
                        class="absolute right-0 top-full mt-1 p-3 qt-bg-card border qt-border-default rounded-lg qt-shadow-lg z-10 min-w-[220px]"
                      >
                        <p class="text-sm text-foreground mb-2">
                          Delete this subprompt? Any chat with it in play drops it.
                        </p>
                        <div class="flex gap-2">
                          <button
                            type="button"
                            class="qt-button-destructive qt-button-sm flex-1"
                            [disabled]="removePending()"
                            (click)="remove(s.id)"
                          >
                            Delete
                          </button>
                          <button
                            type="button"
                            class="qt-button-secondary qt-button-sm flex-1"
                            (click)="deleteConfirm.set(null)"
                          >
                            Cancel
                          </button>
                        </div>
                      </div>
                    }
                  </div>
                </div>
              </div>
            </div>
          }
        </div>
      }
    </div>

    <qt-subprompt-editor-modal
      [open]="creating() || editing() !== null"
      [characterId]="characterId()"
      [characterName]="characterName()"
      [editing]="editing()"
      (close)="closeEditor()"
    />
  `,
})
export class SubpromptsSection {
  private readonly toasts = inject(ToastService);

  readonly characterId = input.required<string>();
  readonly characterName = input.required<string>();

  private readonly api = injectCharacterSubprompts(() => this.characterId());

  protected readonly subprompts = this.api.subprompts;
  protected readonly isLoading = this.api.isLoading;
  protected readonly removePending = this.api.removePending;

  protected readonly editing = signal<SubpromptRecord | null>(null);
  protected readonly creating = signal(false);
  protected readonly deleteConfirm = signal<string | null>(null);

  /** v4 `:76` — cut at 150 characters, ellipsis only when there was more. */
  protected preview(s: SubpromptRecord): string {
    return s.content.length > 150 ? `${s.content.slice(0, 150)}...` : s.content;
  }

  protected openCreate(): void {
    this.creating.set(true);
  }

  protected openEdit(s: SubpromptRecord): void {
    this.editing.set(s);
  }

  protected closeEditor(): void {
    this.creating.set(false);
    this.editing.set(null);
  }

  /** v4 `:92` — clicking the trash toggles this row's confirm, closing any other. */
  protected toggleConfirm(id: string): void {
    this.deleteConfirm.set(this.deleteConfirm() === id ? null : id);
  }

  /** v4 `handleDelete` (`:31-39`) — the confirm clears in `finally`, either way. */
  protected async remove(id: string): Promise<void> {
    try {
      await this.api.remove(id);
      this.toasts.showSuccess('Subprompt deleted');
    } catch (err) {
      this.toasts.showError(subpromptErrorMessage(err, 'Failed to delete subprompt'));
    } finally {
      this.deleteConfirm.set(null);
    }
  }
}
