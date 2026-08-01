import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../core/core-client';
import { fetchProjects, projectKeys } from '../screens/prospero/projects.api';
import { Modal } from '../ui/modal';
import { ToastService } from '../ui/toast.service';

/**
 * Assign to Project (v4 `components/chat/ChatProjectModal.tsx`, 180 LOC) —
 * reached from the sidebar's Chat section.
 *
 * ## Ported exactly, and why
 *
 * - **"No project" is a real choice, not an empty state** (:162). Selecting it
 *   sends `projectId: null`, which detaches the chat; v4's success copy for that
 *   branch is "Chat removed from project" (:94).
 * - **Save with the current project closes without writing** (:60-63) — the
 *   comparison is against the PROP, not the local selection.
 * - **The dialog is sealed while saving**: v4 passes `closeOnClickOutside={!saving}`
 *   and `closeOnEscape={!saving}` (:141-142), which is the opposite of the rename
 *   dialog's arrangement and is reproduced here.
 * - **"Currently in: <name>"** appears only when the chat already has one (:145).
 * - The project list is fetched only while the dialog is open (v4 `enabled: isOpen`,
 *   :54); v5 mounts the dialog only while open, so the query is unconditional here
 *   and means the same thing.
 *
 * This **retires the REDUCED Project entry**: until now the sidebar showed a
 * link to the Prospero screen, and only when the chat already had a project, so
 * a chat could never be filed or unfiled from the Salon at all.
 */
@Component({
  selector: 'qt-chat-project-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal title="Assign to Project" [closeOnBackdrop]="!saving()" (close)="onClose()">
      <div class="space-y-4">
        @if (projectName()) {
          <p class="qt-text-small">
            Currently in: <span class="font-medium">{{ projectName() }}</span>
          </p>
        }

        <div>
          <label for="chat-project" class="qt-label mb-2">Select Project</label>
          <select
            id="chat-project"
            class="qt-select w-full"
            [value]="selected() ?? ''"
            [disabled]="saving() || loading()"
            (change)="selected.set($any($event.target).value || null)"
          >
            <option value="">No project</option>
            @for (project of projects(); track project.id) {
              <!-- The per-option [selected] is not redundant with the select's
                   own [value]: the value binding runs before the options exist,
                   and the browser snaps an unmatched value back to the first
                   option (the sibling selects in chat-section.ts carry the
                   same pair for the same reason). -->
              <option [value]="project.id" [selected]="project.id === selected()">
                {{ project.name }}
              </option>
            }
          </select>
          @if (loading()) {
            <p class="qt-text-xs mt-2">Loading projects...</p>
          }
        </div>

        <p class="qt-text-xs qt-text-secondary">
          Organize this chat by assigning it to a project, or remove it from its current
          project.
        </p>

      </div>

      <div qt-modal-footer class="flex justify-end gap-2">
        <button
          type="button"
          class="qt-button qt-button-secondary"
          [disabled]="saving()"
          (click)="onClose()"
        >
          Cancel
        </button>
        <button
          type="button"
          class="qt-button qt-button-primary"
          [disabled]="saving() || loading()"
          (click)="onSave()"
        >
          {{ saving() ? 'Saving...' : 'Save' }}
        </button>
      </div>
    </qt-modal>
  `,
})
export class ChatProjectModal {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly chatId = input.required<string>();
  readonly projectId = input<string | null>(null);
  readonly projectName = input<string | null>(null);

  /** v4 `onSuccess` → `fetchChat`. */
  readonly assigned = output<void>();
  readonly close = output<void>();

  protected readonly selected = signal<string | null>(null);
  protected readonly saving = signal(false);

  private readonly projectsQuery = injectQuery(() => ({
    queryKey: projectKeys.list(),
    queryFn: () => fetchProjects(this.core),
  }));
  protected readonly projects = computed(() => this.projectsQuery.data() ?? []);
  protected readonly loading = computed(() => this.projectsQuery.isLoading());

  constructor() {
    // v4's re-sync effect (:46-49).
    effect(() => this.selected.set(this.projectId() ?? null));
  }

  /** v4 `handleSubmit` → `handleProjectChange` (:58-114). */
  protected async onSave(): Promise<void> {
    const next = this.selected();
    if (next === (this.projectId() ?? null)) {
      this.close.emit();
      return;
    }

    this.saving.set(true);
    try {
      const resp = await this.core.dispatch({
        type: 'chatUpdate',
        chatId: this.chatId(),
        chat: { projectId: next },
      });
      if (resp.type === 'error') {
        throw new Error(resp.data.message || 'Failed to update project');
      }
      const name = next ? this.projects().find((p) => p.id === next)?.name : null;
      this.toasts.showSuccess(next ? `Chat moved to "${name}"` : 'Chat removed from project');
      this.assigned.emit();
      this.close.emit();
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to update project');
    } finally {
      this.saving.set(false);
    }
  }

  /** v4 seals the dialog while saving (`closeOnEscape={!saving}`, :142). */
  protected onClose(): void {
    if (this.saving()) return;
    this.close.emit();
  }
}
