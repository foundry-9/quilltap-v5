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
import { Modal } from '../../ui/modal';
import { listTools } from '../chat-admin.api';
import { ToolSettingsContent } from './tool-settings-content';

/**
 * LLM Tool Settings (v4 `components/chat/ChatToolSettingsModal.tsx`, 171 LOC) —
 * the Chat section's "Tools…" entry, which until now refused BY NAME because the
 * inventory behind it was unported.
 *
 * ## Ported exactly, and why
 *
 * - **Both sets are edited locally and written whole** (`:36-37,78-85`): the
 *   verb replaces `disabledTools` and `disabledToolGroups`, it does not patch
 *   them, so Cancel really does discard.
 * - **Save is disabled until something moved** (`:104-124`), compared set
 *   against set rather than by reference.
 * - **`showAvailability` is TRUE here** (`:141`) — this modal knows which chat it
 *   is for, so it greys out and discounts tools that chat cannot reach. (v4's
 *   project modal passes false; the Prospero sibling is not this lane's.)
 * - The footer note is v4's, word for word.
 *
 * ## The `allowToolUse` warning box — REDUCED, and why
 *
 * v4 shows a warning when any LLM participant's connection profile has
 * `allowToolUse === false` (`ChatModals.tsx:392`), because then nothing below
 * matters. v5's chat read does not project `allowToolUse` on a participant's
 * connection profile (`api/salon.rs` enriches only id/name/provider/modelName),
 * so the condition cannot be computed in the browser. The box is therefore not
 * rendered rather than rendered wrong — and the input is kept, so the day the
 * projection grows the field, one binding turns it on. Recorded as a cross-lane
 * escalation in the lane record; this lane may not change the server.
 */
@Component({
  selector: 'qt-chat-tool-settings-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, ToolSettingsContent],
  template: `
    <qt-modal title="LLM Tool Settings" (close)="close.emit()">
      @if (profileToolsDisabled()) {
        <div class="qt-warning-box mb-4">
          <p class="qt-label">Tools disabled by connection profile</p>
          <p class="text-xs mt-1">
            The current connection profile has &ldquo;Allow tool use&rdquo; turned off. No tools
            will be sent to the LLM regardless of the settings below. To re-enable tools, edit
            the connection profile in The Forge.
          </p>
        </div>
      }

      <qt-tool-settings-content
        [availableTools]="tools()"
        [disabledTools]="localTools()"
        [disabledGroups]="localGroups()"
        [showAvailability]="true"
        [loading]="toolsQuery.isLoading()"
        footerNote="Disabled tools will not be available to the AI for this chat. Changes take effect on the next message."
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
export class ChatToolSettingsModal {
  private readonly core = inject(CoreClient);

  readonly chatId = input.required<string>();
  readonly disabledTools = input.required<string[]>();
  readonly disabledToolGroups = input.required<string[]>();
  /** See the class note: v5's chat read cannot supply this yet. */
  readonly profileToolsDisabled = input(false);

  /** v4 `onSuccess(newDisabledTools, newDisabledToolGroups)`. */
  readonly saved = output<{ disabledTools: string[]; disabledToolGroups: string[] }>();
  readonly close = output<void>();

  protected readonly localTools = signal<ReadonlySet<string>>(new Set());
  protected readonly localGroups = signal<ReadonlySet<string>>(new Set());
  protected readonly saving = signal(false);
  protected readonly error = signal<string | null>(null);

  protected readonly toolsQuery = injectQuery(() => ({
    queryKey: ['tools', this.chatId()],
    queryFn: () => listTools(this.core, { chatId: this.chatId() }),
  }));
  protected readonly tools = computed(() => this.toolsQuery.data() ?? []);

  constructor() {
    // v4's re-sync effect (`:68-72`).
    effect(() => {
      this.localTools.set(new Set(this.disabledTools()));
      this.localGroups.set(new Set(this.disabledToolGroups()));
    });
  }

  /** v4 `hasChanges` (`:104-124`) — set against set, not reference against reference. */
  protected readonly hasChanges = computed(() => {
    const originalTools = new Set(this.disabledTools());
    const originalGroups = new Set(this.disabledToolGroups());
    if (originalTools.size !== this.localTools().size) return true;
    for (const id of this.localTools()) if (!originalTools.has(id)) return true;
    if (originalGroups.size !== this.localGroups().size) return true;
    for (const id of this.localGroups()) if (!originalGroups.has(id)) return true;
    return false;
  });

  /** v4 `handleSave` (`:75-102`). */
  protected async onSave(): Promise<void> {
    this.error.set(null);
    this.saving.set(true);
    const disabledTools = [...this.localTools()];
    const disabledToolGroups = [...this.localGroups()];
    try {
      await this.core.dispatchData({
        type: 'chatUpdateToolSettings',
        chatId: this.chatId(),
        disabledTools,
        disabledToolGroups,
      });
      this.saved.emit({ disabledTools, disabledToolGroups });
      this.close.emit();
    } catch (err) {
      this.error.set(
        err instanceof Error ? err.message : 'Failed to save tool settings',
      );
    } finally {
      this.saving.set(false);
    }
  }
}
