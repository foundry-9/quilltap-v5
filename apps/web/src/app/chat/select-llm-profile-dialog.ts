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
import type { ConnectionProfileDto } from '../core/core-contract';
import { Avatar } from '../ui/avatar';
import { Icon } from '../ui/icon';
import { Modal } from '../ui/modal';
import { ToastService } from '../ui/toast.service';

/**
 * Hand Off Character to AI (v4 `components/chat/SelectLLMProfileDialog.tsx`,
 * 234 LOC).
 *
 * **Not a menu item — an interruption.** It appears when the operator stops
 * speaking as a character who has no connection profile of their own
 * (`useImpersonation.ts:76-89`): somebody has to drive that character now, and
 * nothing on the record says who. `handleStopImpersonation` RETURNS at that
 * point without calling the server; the dialog's confirm is what completes the
 * hand-off, carrying `newConnectionProfileId` (`:115-142`).
 *
 * ## Ported exactly, and why
 *
 * - **The preselection order** (`:74-79`): the character's own default profile
 *   if there is one, otherwise the FIRST profile in the list. Not "nothing".
 * - **Cancel is not a dismissal.** v4 wires `onClose` to `handleCancel`
 *   (`:131`), so the ✕ and the backdrop both take the cancel path — the
 *   impersonation simply stays as it was, and the character keeps being played
 *   by the operator.
 * - **The empty state names the fix** (`:181-187`): with no profiles at all, the
 *   dialog points at Settings → Connection Profiles rather than offering an
 *   inert confirm.
 * - The radio inputs are `sr-only` and the whole card is the label (`:191-208`),
 *   so the click target is the row.
 *
 * v5 substitution: v4 fetches `/api/v1/connection-profiles` itself on open; v5
 * reads the same list through the shared `connectionProfileList` verb, and the
 * dialog is mounted only while open, which is what v4's `enabled: isOpen` means.
 */
@Component({
  selector: 'qt-select-llm-profile-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, Avatar, Icon],
  template: `
    <qt-modal title="Hand Off Character to AI" maxWidth="md" (close)="cancel.emit()">
      <div class="space-y-4">
        <div class="flex items-center gap-3 p-3 qt-card">
          <qt-avatar [name]="characterName()" [src]="characterAvatarUrl()" size="md" />
          <div>
            <div class="font-semibold">{{ characterName() }}</div>
            <div class="qt-text-xs">Will be controlled by AI</div>
          </div>
        </div>

        <div>
          <span class="qt-label mb-2 block">Select an LLM Connection Profile</span>

          @if (loading()) {
            <div class="flex items-center gap-2 qt-text-small py-4">Loading profiles...</div>
          } @else if (profiles().length === 0) {
            <div class="qt-text-small py-4 text-center">
              No connection profiles available.
              <br />
              <span class="qt-text-xs">
                Please create one in Settings &rarr; Connection Profiles.
              </span>
            </div>
          } @else {
            <div class="space-y-2 max-h-64 overflow-y-auto">
              @for (profile of profiles(); track profile.id) {
                <label
                  [class]="
                    'flex items-center gap-3 p-3 rounded-lg cursor-pointer transition-colors ' +
                    (selected() === profile.id ? 'qt-card-selected' : 'qt-card hover:qt-bg-muted/50')
                  "
                >
                  <input
                    type="radio"
                    name="connectionProfile"
                    class="sr-only"
                    [value]="profile.id"
                    [checked]="selected() === profile.id"
                    (change)="selected.set(profile.id)"
                  />
                  <div class="flex-1">
                    <div class="font-medium">{{ profile.name }}</div>
                    @if (profile.modelName) {
                      <div class="qt-text-xs">
                        {{ profile.provider ? profile.provider + ': ' : '' }}{{ profile.modelName }}
                      </div>
                    }
                  </div>
                  @if (selected() === profile.id) {
                    <qt-icon name="check" class="w-5 h-5 text-primary" />
                  }
                </label>
              }
            </div>
          }
        </div>

        <p class="qt-text-xs">
          The selected profile will be used when this character speaks. You can change this later
          in the chat settings.
        </p>

      </div>

      <div qt-modal-footer class="flex justify-end gap-2">
        <button type="button" class="qt-button qt-button-secondary" (click)="cancel.emit()">
          Cancel
        </button>
        <button
          type="button"
          class="qt-button qt-button-primary"
          [disabled]="loading() || !selected()"
          (click)="onConfirm()"
        >
          Assign &amp; Hand Off
        </button>
      </div>
    </qt-modal>
  `,
})
export class SelectLlmProfileDialog {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  readonly characterName = input.required<string>();
  readonly characterAvatarUrl = input<string | null>(null);
  /** The character's own default profile, preselected when there is one. */
  readonly defaultConnectionProfileId = input<string | null>(null);

  /** v4 `onConfirm(participantId, connectionProfileId)` — the parent dispatches. */
  readonly confirm = output<string>();
  /** v4 `onCancel` — reached from Cancel, the close button AND the backdrop. */
  readonly cancel = output<void>();

  protected readonly selected = signal<string | null>(null);

  private readonly profilesQuery = injectQuery(() => ({
    queryKey: ['connection-profiles'],
    queryFn: async (): Promise<ConnectionProfileDto[]> => {
      const data = await this.core.dispatchData({ type: 'connectionProfileList' });
      return (data['profiles'] as ConnectionProfileDto[]) ?? [];
    },
  }));
  protected readonly profiles = computed(() => this.profilesQuery.data() ?? []);
  protected readonly loading = computed(() => this.profilesQuery.isLoading());

  constructor() {
    // v4 answers a failed profile load with a toast (:84) and leaves the dialog
    // open on an empty list.
    effect(() => {
      if (this.profilesQuery.isError()) {
        this.toasts.showError('Failed to load connection profiles');
      }
    });
    // v4's preselection, in v4's order (:74-79).
    effect(() => {
      const list = this.profiles();
      const preferred = this.defaultConnectionProfileId();
      if (preferred) {
        this.selected.set(preferred);
      } else if (list.length > 0) {
        this.selected.set(list[0].id);
      }
    });
  }

  protected onConfirm(): void {
    const profileId = this.selected();
    if (!profileId) {
      this.toasts.showError('Please select a connection profile');
      return;
    }
    this.confirm.emit(profileId);
  }
}
