import {
  ChangeDetectionStrategy,
  Component,
  OnInit,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import { CoreClient } from '../core/core-client';
import type { ChatSettingsDto } from '../core/core-contract';
import { Modal } from '../ui/modal';
import { AutonomousRoomCard } from './autonomous-room-card';
import {
  INITIAL_AUTONOMOUS_STATE,
  MS_PER_HOUR,
  formStateToUpdatePatch,
  statusToFormState,
  type AutonomousSettingsHint,
  type NewChatAutonomousState,
} from './autonomous.logic';

/**
 * The Edit-Enclave modal (v4 `components/new-chat/EditEnclaveModal.tsx`): reuses
 * the shared {@link AutonomousRoomCard} to edit an existing autonomous room in
 * place. On open it loads the room's status snapshot AND the user-level hints in
 * parallel, converts milliseconds → human units, and on save converts back and
 * calls `autonomousRoomUpdateSettings`. Edits land on the live chat row and take
 * effect on the running run's next turn as well as future runs. If the user's
 * `always_refuse` policy clamped the destructive flag off, that is surfaced.
 *
 * The in-chat (Salon Organize) entry point is a TIER-3 deferral (lane B's
 * `screens/salon/**`); this modal is reached from the Scheduled-Rooms settings card.
 */
@Component({
  selector: 'qt-edit-enclave-modal',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Modal, AutonomousRoomCard],
  template: `
    <qt-modal title="Edit Enclave" maxWidth="2xl" (close)="close.emit()">
      @if (loadError()) {
        <div class="qt-alert-error">{{ loadError() }}</div>
      } @else if (loading()) {
        <div class="qt-text-secondary py-8 text-center">Loading enclave settings…</div>
      } @else {
        <div class="space-y-5">
          <div>
            <label for="edit-enclave-title" class="qt-label mb-1">Room Title</label>
            <input
              id="edit-enclave-title"
              type="text"
              [value]="title()"
              (input)="title.set($any($event.target).value)"
              [disabled]="saving()"
              placeholder="Name this enclave…"
              class="qt-input"
            />
            <p class="mt-1 qt-text-xs qt-text-muted">
              Setting a title here pins it — the automatic titler will leave it be.
            </p>
          </div>

          <qt-autonomous-room-card
            [value]="auto()"
            [settingsHint]="settingsHint()"
            [disabled]="saving()"
            (changed)="patchAuto($event)"
          />

          @if (clampedNote()) {
            <div class="qt-alert-warning qt-text-small">
              Your user-level policy is set to <em>always refuse</em> destructive tools, so the
              pre-authorization was left off for this room.
            </div>
          }
        </div>
      }

      <div qt-modal-footer class="flex justify-end gap-2">
        <button
          type="button"
          (click)="close.emit()"
          [disabled]="saving()"
          class="qt-button qt-button-secondary"
        >
          Cancel
        </button>
        <button
          type="button"
          (click)="handleSave()"
          [disabled]="saving() || loading() || !!loadError() || !title().trim()"
          class="qt-button qt-button-primary"
        >
          {{ saving() ? 'Saving…' : 'Save Changes' }}
        </button>
      </div>
    </qt-modal>
  `,
})
export class EditEnclaveModal implements OnInit {
  private readonly core = inject(CoreClient);

  readonly chatId = input.required<string>();
  /** Current room title, shown pre-filled and editable. */
  readonly currentTitle = input('');
  readonly close = output<void>();
  /** Fired after a successful save with the (trimmed) new title. */
  readonly saved = output<string>();

  protected readonly title = signal('');
  protected readonly auto = signal<NewChatAutonomousState>(INITIAL_AUTONOMOUS_STATE);
  protected readonly settingsHint = signal<AutonomousSettingsHint | undefined>(undefined);
  protected readonly loading = signal(false);
  protected readonly loadError = signal<string | null>(null);
  protected readonly saving = signal(false);
  protected readonly clampedNote = signal(false);

  ngOnInit(): void {
    void this.load();
  }

  private async load(): Promise<void> {
    this.loading.set(true);
    this.loadError.set(null);
    this.clampedNote.set(false);
    this.title.set(this.currentTitle());
    try {
      const [status, settings] = await Promise.all([
        this.core.autonomousRoomStatus(this.chatId()),
        this.loadSettingsHint(),
      ]);
      this.auto.set(statusToFormState(status));
      this.settingsHint.set(settings);
    } catch (err) {
      this.loadError.set(
        err instanceof Error ? err.message : 'Could not load this enclave’s settings.',
      );
    } finally {
      this.loading.set(false);
    }
  }

  private async loadSettingsHint(): Promise<AutonomousSettingsHint | undefined> {
    try {
      const resp = await this.core.dispatchExpect({ type: 'chatSettings' }, 'chatSettings');
      const ar = ((resp.data as ChatSettingsDto)['autonomousRoomSettings'] ?? {}) as {
        visibilityDefault?: AutonomousSettingsHint['visibilityDefault'];
        destructiveToolPolicy?: AutonomousSettingsHint['destructiveToolPolicy'];
        defaultFreshnessWindowMs?: number;
      };
      return {
        visibilityDefault: ar.visibilityDefault,
        destructiveToolPolicy: ar.destructiveToolPolicy,
        defaultFreshnessHours:
          typeof ar.defaultFreshnessWindowMs === 'number' && ar.defaultFreshnessWindowMs > 0
            ? Math.round(ar.defaultFreshnessWindowMs / MS_PER_HOUR)
            : undefined,
      };
    } catch {
      return undefined;
    }
  }

  protected patchAuto(patch: Partial<NewChatAutonomousState>): void {
    this.auto.update((prev) => ({ ...prev, ...patch }));
  }

  protected async handleSave(): Promise<void> {
    const trimmedTitle = this.title().trim();
    if (!trimmedTitle) return;
    this.saving.set(true);
    this.clampedNote.set(false);
    try {
      const patch = formStateToUpdatePatch(trimmedTitle, this.auto());
      const result = await this.core.autonomousRoomUpdateSettings(this.chatId(), patch);
      if (result.clampedDestructive) {
        // v4 still saves; the flag was clamped off by the user policy.
        this.clampedNote.set(true);
      }
      this.saved.emit(trimmedTitle);
      this.close.emit();
    } catch (err) {
      this.loadError.set(err instanceof Error ? err.message : 'Failed to update enclave');
    } finally {
      this.saving.set(false);
    }
  }
}
