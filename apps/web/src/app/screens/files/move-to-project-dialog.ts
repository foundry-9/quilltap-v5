import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../core/core-client';
import type { ProjectSummary } from '../../core/core-contract';
import { Modal } from '../../ui/modal';
import { ToastService } from '../../ui/toast.service';

const GENERAL_FILES_VALUE = '__general__';

/**
 * Move a general file into a project (or back to General Files) (v4
 * `components/files/MoveToProjectModal.tsx`). Lists the user's projects (minus
 * the current one), lets the operator pick a destination + a target folder path,
 * and promotes via `filePromote`. The rich `FolderPicker` is deferred (P4.6af
 * tier 3) — a plain folder-path field stands in.
 */
@Component({
  selector: 'qt-move-to-project-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal],
  template: `
    <qt-modal [title]="modalTitle()" maxWidth="lg" (close)="close.emit()">
      <div class="space-y-4">
        <p class="qt-text-small">
          Move <span class="font-medium">"{{ fileName() }}"</span> to a different location.
        </p>

        <div>
          <label for="move-to-project" class="qt-label mb-2">Select Destination</label>
          <select
            id="move-to-project"
            class="qt-select w-full"
            [disabled]="saving() || projectsQuery.isPending()"
            (change)="onSelect($any($event.target).value)"
          >
            <option value="" [selected]="selectedValue() === ''">Select a destination...</option>
            @if (currentProjectId()) {
              <option [value]="generalValue" [selected]="selectedValue() === generalValue">
                General Files (No Project)
              </option>
            }
            @for (project of availableProjects(); track project.id) {
              <option [value]="project.id" [selected]="selectedValue() === project.id">
                {{ project.name }}
              </option>
            }
          </select>
          @if (projectsQuery.isPending()) {
            <p class="qt-text-xs mt-1 qt-text-secondary">Loading projects...</p>
          } @else if (availableProjects().length === 0 && !currentProjectId()) {
            <p class="qt-text-xs mt-1 qt-text-secondary">
              No projects found. Create a project first.
            </p>
          }
        </div>

        @if (selectedProjectId() && !isGeneralFilesSelected()) {
          <div>
            <label for="move-folder" class="qt-label mb-2">Folder</label>
            <input
              id="move-folder"
              type="text"
              class="qt-input w-full font-mono"
              placeholder="/"
              [value]="folderPath()"
              [disabled]="saving()"
              (input)="folderPath.set($any($event.target).value)"
            />
          </div>
        }

        <p class="qt-text-xs qt-text-secondary">
          @if (isGeneralFilesSelected()) {
            The file will be moved to General Files. Any existing links to chats or characters will
            be preserved.
          } @else {
            The file will be moved to the selected project. Any existing links to chats or
            characters will be preserved.
          }
        </p>
      </div>

      <div qt-modal-footer class="flex justify-end gap-2">
        <button
          type="button"
          class="qt-button qt-button-secondary"
          [disabled]="saving()"
          (click)="close.emit()"
        >
          Cancel
        </button>
        <button
          type="button"
          class="qt-button qt-button-primary"
          [disabled]="saving() || projectsQuery.isPending() || !selectedValue()"
          (click)="onMove()"
        >
          {{ saving() ? 'Moving...' : buttonText() }}
        </button>
      </div>
    </qt-modal>
  `,
})
export class MoveToProjectDialog {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly fileId = input.required<string>();
  readonly fileName = input.required<string>();
  readonly currentProjectId = input<string | null>(null);

  readonly close = output<void>();
  /** The move landed — the parent removes the row from the list. */
  readonly success = output<{ projectId: string | null; projectName: string }>();

  protected readonly generalValue = GENERAL_FILES_VALUE;
  protected readonly selectedValue = signal('');
  protected readonly folderPath = signal('/');
  protected readonly saving = signal(false);

  protected readonly projectsQuery = injectQuery(() => ({
    queryKey: ['projects'],
    queryFn: async (): Promise<ProjectSummary[]> => {
      const data = await this.core.dispatchData({ type: 'projectList' });
      return (data['projects'] as ProjectSummary[] | undefined) ?? [];
    },
  }));

  protected readonly availableProjects = computed(() =>
    (this.projectsQuery.data() ?? []).filter((p) => p.id !== this.currentProjectId()),
  );

  protected readonly isGeneralFilesSelected = computed(
    () => this.selectedValue() === GENERAL_FILES_VALUE,
  );
  protected readonly selectedProjectId = computed(() =>
    this.isGeneralFilesSelected() ? null : this.selectedValue() || null,
  );

  protected readonly modalTitle = computed(() =>
    this.currentProjectId() ? 'Move File' : 'Move to Project',
  );
  protected readonly buttonText = computed(() =>
    this.isGeneralFilesSelected() ? 'Move to General Files' : 'Move to Project',
  );

  protected onSelect(value: string): void {
    this.selectedValue.set(value);
    this.folderPath.set('/');
  }

  /** v4 `MoveToProjectModal.tsx:73-113` — toast only, no inline surface. */
  protected async onMove(): Promise<void> {
    if (!this.selectedValue()) return;
    this.saving.set(true);
    const targetProjectId = this.isGeneralFilesSelected() ? null : this.selectedProjectId();
    try {
      await this.core.dispatchData({
        type: 'filePromote',
        fileId: this.fileId(),
        targetProjectId,
        folderPath: this.isGeneralFilesSelected() ? '/' : this.folderPath(),
      });
      const projectName = this.isGeneralFilesSelected()
        ? 'General Files'
        : (this.availableProjects().find((p) => p.id === targetProjectId)?.name ?? 'project');
      this.success.emit({ projectId: targetProjectId, projectName });
      this.close.emit();
      this.toasts.showSuccess(`"${this.fileName()}" moved to ${projectName}`);
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to move file');
    } finally {
      this.saving.set(false);
    }
  }
}
