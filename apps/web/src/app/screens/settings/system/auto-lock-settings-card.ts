import { ChangeDetectionStrategy, Component, computed, effect, inject, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { UnlockStateDto } from '../../../core/core-contract';
import { ChatSettingsCard } from '../chat/chat-settings.api';
import { SystemSettingsSignals } from './system-settings-signals.service';

/** v4 `AutoLockConfig` (`AutoLockSettingsCard.tsx:11-14`); default `{false, 15}`. */
interface AutoLockConfig {
  enabled: boolean;
  idleMinutes: number;
}

const DEFAULT_AUTO_LOCK: AutoLockConfig = { enabled: false, idleMinutes: 15 };

/** The unlock-state query key — a card-local read of the passphrase gate. */
const unlockStateKey = ['unlockState'] as const;

/**
 * The Auto-Lock card (v4 `components/settings/AutoLockSettingsCard.tsx`):
 * configure the idle timer that locks Quilltap. It is only functional when a
 * user passphrase is set — the whole card gates on `hasUserPassphrase` (v4
 * `:126-136`), read from the unlock-state query alongside the chat-settings
 * `autoLockSettings` bag.
 *
 * On save v4 fires a `quilltap-autolock-settings-changed` window event so the
 * app-wide provider re-fetches (v4 `:92`); v5 nudges {@link SystemSettingsSignals}.
 * It also re-reads the passphrase gate whenever the Encryption Passphrase card
 * signals a change (v4 `:56-62` listens for `quilltap-passphrase-changed`).
 *
 * The idle-minutes input carries a local DRAFT so a mid-edit value never
 * round-trips until blur/Enter (v4 keeps `config.idleMinutes` in state and only
 * saves on blur when `>= 1`, `:105-116`).
 */
@Component({
  selector: 'qt-auto-lock-settings-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (hasPassphrase() === null) {
      <div class="flex items-center justify-center py-4">
        <div class="qt-text-muted text-sm">Loading...</div>
      </div>
    } @else if (!hasPassphrase()) {
      <div class="space-y-3">
        <p class="qt-text-small qt-text-muted">
          Auto-lock requires a passphrase to be set. Without one, there is nothing to lock behind,
          rather like installing a deadbolt on a door that has no frame. Set a passphrase under
          &ldquo;Encryption Passphrase&rdquo; above, then return here.
        </p>
      </div>
    } @else {
      <div class="space-y-4">
        <label class="flex items-center gap-3 cursor-pointer">
          <input
            type="checkbox"
            class="qt-checkbox"
            [checked]="config().enabled"
            [disabled]="saving()"
            (change)="onToggle($any($event.target).checked)"
          />
          <span class="qt-text-small">Automatically lock after idle period</span>
        </label>

        @if (config().enabled) {
          <div class="flex items-center gap-2">
            <label class="qt-text-small">Lock after</label>
            <input
              type="number"
              min="1"
              class="qt-input w-20 p-1 text-center"
              [value]="idleDraft()"
              [disabled]="saving()"
              (input)="onMinutesInput($event)"
              (blur)="onMinutesBlur()"
              (keydown.enter)="onMinutesBlur()"
            />
            <span class="qt-text-small">minutes of inactivity</span>
          </div>
        }

        @if (saveError(); as msg) {
          <p class="qt-alert qt-alert-error text-sm">{{ msg }}</p>
        }
        @if (saved()) {
          <p class="qt-text-xs qt-text-success">Settings saved</p>
        }
      </div>
    }
  `,
})
export class AutoLockSettingsCard extends ChatSettingsCard {
  private readonly coreClient = inject(CoreClient);
  private readonly qc = injectQueryClient();
  private readonly signals = inject(SystemSettingsSignals);

  private readonly unlockQuery = injectQuery(() => ({
    queryKey: unlockStateKey,
    queryFn: () => this.coreClient.dispatchExpect({ type: 'unlockState' }, 'unlockState'),
  }));

  /** v4 `hasPassphrase` — null while the unlock state loads, then the boolean. */
  protected readonly hasPassphrase = computed<boolean | null>(() => {
    const resp = this.unlockQuery.data() as { data: UnlockStateDto } | undefined;
    if (!resp) return null;
    return resp.data.hasUserPassphrase ?? false;
  });

  /** v4 `config` — the persisted `autoLockSettings` bag with defaults. */
  protected readonly config = computed<AutoLockConfig>(() => {
    const bag = this.settings()?.['autoLockSettings'] as Partial<AutoLockConfig> | undefined;
    return {
      enabled: bag?.enabled ?? DEFAULT_AUTO_LOCK.enabled,
      idleMinutes: bag?.idleMinutes ?? DEFAULT_AUTO_LOCK.idleMinutes,
    };
  });

  /** The in-flight minutes draft; `null` means "track the persisted config". */
  private readonly minutesDraft = signal<number | null>(null);
  protected readonly idleDraft = computed(() => this.minutesDraft() ?? this.config().idleMinutes);

  protected readonly saved = signal(false);

  constructor() {
    super();
    // v4 `:55-62` — re-read the passphrase gate when the passphrase changes.
    effect(() => {
      const n = this.signals.passphraseChanged();
      if (n > 0) {
        void this.qc.invalidateQueries({ queryKey: unlockStateKey });
      }
    });
  }

  protected onToggle(enabled: boolean): void {
    void this.persist({ ...this.config(), enabled });
  }

  /** v4 `handleMinutesChange` (`:105-110`) — only accept an integer `>= 1`. */
  protected onMinutesInput(event: Event): void {
    const minutes = parseInt((event.target as HTMLInputElement).value, 10);
    if (!isNaN(minutes) && minutes >= 1) {
      this.minutesDraft.set(minutes);
    }
  }

  /** v4 `handleMinutesBlur` (`:112-116`) — persist the draft when valid. */
  protected onMinutesBlur(): void {
    const draft = this.minutesDraft();
    if (draft !== null && draft >= 1) {
      void this.persist({ ...this.config(), idleMinutes: draft });
    }
    this.minutesDraft.set(null);
  }

  /**
   * v4 `handleSave` (`:64-99`): PUT the whole `autoLockSettings` bag, flash
   * "Settings saved" for 2 s, and notify the app-wide provider. On failure the
   * base card surfaces `saveError` (v4 sets its own `error`).
   */
  private async persist(newConfig: AutoLockConfig): Promise<void> {
    this.saved.set(false);
    await this.save({ autoLockSettings: newConfig }, 'Failed to save auto-lock settings');
    if (!this.saveError()) {
      this.saved.set(true);
      setTimeout(() => this.saved.set(false), 2000);
      // v4 `:92` — the provider re-fetches its idle config.
      this.signals.notifyAutoLockSettingsChanged();
    }
  }
}
