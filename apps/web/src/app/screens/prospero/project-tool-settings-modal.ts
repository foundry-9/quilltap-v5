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

import { CoreClient } from '../../core/core-client';
import { listTools } from '../../chat/chat-admin.api';
import { ToolSettingsContent } from '../../chat/tools/tool-settings-content';
import { Modal } from '../../ui/modal';

/**
 * Default Tool Settings (v4 `components/tools/tool-settings/
 * ProjectToolSettingsModal.tsx`, 145 LOC) — the Model Behavior card's Configure
 * button, which until now was a disabled affordance.
 *
 * The v4 differences from its chat-scoped sibling are the whole point of it
 * being a separate dialog, and both are carried:
 *
 * - **`showAvailability` is FALSE** (`:112`). A project has no chat to check a
 *   tool against, so nothing is greyed out and the shared content's arithmetic
 *   counts every tool as available.
 * - **The footer note is the project's own** (`:115`), word for word: these are
 *   defaults for NEW chats, and existing chats are not affected.
 *
 * Everything else matches the chat modal, because v4's two dialogs share
 * `ToolSettingsContent`: both sets are edited locally and written whole (so
 * Cancel really discards), and Save stays disabled until something moved,
 * compared set against set (`:82-101`).
 *
 * v4 fetches `/api/v1/tools` gated on `isOpen` (`:41`) and re-syncs its local
 * sets from the props (`:47-53`); v5 mounts the dialog only while open, so the
 * query is unconditional and means the same thing, and the re-sync is an effect.
 */
@Component({
  selector: 'qt-project-tool-settings-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, ToolSettingsContent],
  template: `
    <qt-modal title="Default Tool Settings" (close)="close.emit()">
      <qt-tool-settings-content
        [availableTools]="tools()"
        [disabledTools]="localTools()"
        [disabledGroups]="localGroups()"
        [showAvailability]="false"
        [loading]="toolsQuery.isLoading()"
        footerNote="Configure which tools are enabled by default for new chats in this project. Existing chats are not affected."
        (disabledToolsChange)="localTools.set($event)"
        (disabledGroupsChange)="localGroups.set($event)"
      />

      @if (error(); as message) {
        <p class="text-xs qt-text-danger mt-3" role="status">{{ message }}</p>
      }

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
          [disabled]="saving() || !hasChanges()"
          (click)="onSave()"
        >
          {{ saving() ? 'Saving...' : 'Save' }}
        </button>
      </div>
    </qt-modal>
  `,
})
export class ProjectToolSettingsModal {
  private readonly core = inject(CoreClient);

  readonly projectId = input.required<string>();
  readonly disabledTools = input.required<string[]>();
  readonly disabledToolGroups = input.required<string[]>();

  /** v4 `onSuccess(newDisabledTools, newDisabledToolGroups)`. */
  readonly saved = output<{ disabledTools: string[]; disabledToolGroups: string[] }>();
  readonly close = output<void>();

  protected readonly localTools = signal<ReadonlySet<string>>(new Set());
  protected readonly localGroups = signal<ReadonlySet<string>>(new Set());
  protected readonly saving = signal(false);
  protected readonly error = signal<string | null>(null);

  /**
   * The inventory with NO `chatId` — v4 asks for the bare `/api/v1/tools` here
   * (`:43`), because availability is a per-chat notion and this dialog has no
   * chat. The key is separate from the chat modal's for the same reason.
   */
  protected readonly toolsQuery = injectQuery(() => ({
    queryKey: ['tools', 'project-defaults'],
    queryFn: () => listTools(this.core),
  }));
  protected readonly tools = computed(() => this.toolsQuery.data() ?? []);

  constructor() {
    // v4's re-sync effect (`:47-53`).
    effect(() => {
      this.localTools.set(new Set(this.disabledTools()));
      this.localGroups.set(new Set(this.disabledToolGroups()));
    });
  }

  /** v4 `hasChanges` (`:82-101`) — set against set, not reference against reference. */
  protected readonly hasChanges = computed(() => {
    const originalTools = new Set(this.disabledTools());
    const originalGroups = new Set(this.disabledToolGroups());
    if (originalTools.size !== this.localTools().size) return true;
    for (const id of this.localTools()) if (!originalTools.has(id)) return true;
    if (originalGroups.size !== this.localGroups().size) return true;
    for (const id of this.localGroups()) if (!originalGroups.has(id)) return true;
    return false;
  });

  /** v4 `handleSave` (`:56-81`). */
  protected async onSave(): Promise<void> {
    this.error.set(null);
    this.saving.set(true);
    const defaultDisabledTools = [...this.localTools()];
    const defaultDisabledToolGroups = [...this.localGroups()];
    try {
      await this.core.dispatchData({
        type: 'projectToolSettingsUpdate',
        projectId: this.projectId(),
        defaultDisabledTools,
        defaultDisabledToolGroups,
      });
      this.saved.emit({
        disabledTools: defaultDisabledTools,
        disabledToolGroups: defaultDisabledToolGroups,
      });
      this.close.emit();
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'Failed to save tool settings');
    } finally {
      this.saving.set(false);
    }
  }
}
