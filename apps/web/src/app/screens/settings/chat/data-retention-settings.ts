import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';

import { CoreClient } from '../../../core/core-client';
import { ToastService } from '../../../ui/toast.service';

const DEFAULT_STALE_CHAT_DAYS = 30;
const MIN_DAYS = 1;
const MAX_DAYS = 3650;

/**
 * The instance-wide stale-chat retention window (v4
 * `components/settings/chat-settings/DataRetentionSettings.tsx`,
 * `instance_settings['dataRetention']`). Read daily by the maintenance sweep to
 * decide when a quiet conversation's regenerable working data (compression
 * caches, rendered markdown, model scratch-work, cold-tier chunk embeddings) is
 * tidied away. Global only — there is deliberately no per-chat control.
 *
 * Autosaves — the number input commits on blur (v4's `commit`): an unusable
 * entry (non-finite / out of `[1, 3650]`) is reverted rather than nagged (the
 * bounds live in the copy), and an unchanged value is a no-op. Copy carries over
 * verbatim in v4's steampunk register.
 */
@Component({
  selector: 'qt-data-retention-settings',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (loading()) {
      <p class="qt-text-small qt-text-muted">Loading retention settings…</p>
    } @else {
      <div class="space-y-4">
        <p class="qt-text-small qt-text-muted">
          A conversation left to gather dust accumulates a surprising amount of behind-the-scenes
          paraphernalia — compression caches, pre-rendered pages, the models' own scratch-work. Once
          a chat has gone this many days without anyone actually speaking in it, Quilltap's nightly
          housekeeping quietly tidies that working data away. The conversation itself — every word
          anyone said — remains exactly as you left it, and the tidied bits are rebuilt the moment
          you take up the thread again.
        </p>

        <div>
          <label for="stale-chat-days" class="qt-text-label block mb-2">
            Keep inactive chats' working data for
          </label>
          <div class="flex items-center gap-2">
            <input
              id="stale-chat-days"
              type="number"
              [min]="MIN_DAYS"
              [max]="MAX_DAYS"
              class="qt-input w-28"
              [value]="draft()"
              (input)="draft.set($any($event.target).value)"
              (blur)="commit()"
              (keydown.enter)="$any($event.target).blur()"
              [disabled]="saving()"
            />
            <span class="qt-text-small qt-text-secondary">
              days ({{ MIN_DAYS }}–{{ MAX_DAYS }}; the default is {{ DEFAULT_STALE_CHAT_DAYS }})
            </span>
          </div>
          <p class="qt-text-xs qt-text-secondary mt-1">
            Applies to the whole establishment — there is no per-chat dial. Announcements from the
            Staff don't count as activity; only you and your characters do.
          </p>
        </div>

        @if (error()) {
          <p class="qt-text-small qt-text-error">{{ error() }}</p>
        }
      </div>
    }
  `,
})
export class DataRetentionSettings {
  private readonly core = inject(CoreClient);
  private readonly toasts = inject(ToastService);

  protected readonly MIN_DAYS = MIN_DAYS;
  protected readonly MAX_DAYS = MAX_DAYS;
  protected readonly DEFAULT_STALE_CHAT_DAYS = DEFAULT_STALE_CHAT_DAYS;

  protected readonly loading = signal(true);
  protected readonly saving = signal(false);
  protected readonly error = signal<string | null>(null);
  protected readonly draft = signal<string>(String(DEFAULT_STALE_CHAT_DAYS));

  /** The last persisted value — an unusable draft reverts to it, an unchanged
   *  draft skips the round-trip. */
  private days = DEFAULT_STALE_CHAT_DAYS;

  constructor() {
    void this.load();
  }

  private async load(): Promise<void> {
    try {
      const settings = await this.core.getDataRetentionSettings();
      const loaded =
        typeof settings?.staleChatDays === 'number'
          ? settings.staleChatDays
          : DEFAULT_STALE_CHAT_DAYS;
      this.days = loaded;
      this.draft.set(String(loaded));
    } catch {
      this.error.set('Failed to load data-retention settings');
    } finally {
      this.loading.set(false);
    }
  }

  protected async commit(): Promise<void> {
    const parsed = Math.floor(Number(this.draft()));
    if (!Number.isFinite(parsed) || parsed < MIN_DAYS || parsed > MAX_DAYS) {
      // Revert an unusable entry rather than nag — the bounds live in the copy.
      this.draft.set(String(this.days));
      return;
    }
    if (parsed === this.days) {
      this.draft.set(String(parsed));
      return;
    }

    this.saving.set(true);
    this.error.set(null);
    try {
      const saved = await this.core.updateDataRetentionSettings(parsed);
      this.days = saved.staleChatDays;
      this.draft.set(String(saved.staleChatDays));
      this.toasts.showSuccess('Retention window saved');
    } catch {
      this.error.set('Failed to save data-retention settings');
      this.toasts.showError('Failed to save data-retention settings');
      this.draft.set(String(this.days));
    } finally {
      this.saving.set(false);
    }
  }
}
