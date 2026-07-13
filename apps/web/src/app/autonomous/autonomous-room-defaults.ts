import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  signal,
} from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../core/core-client';
import type { ChatSettingsDto } from '../core/core-contract';

type VisibilityDefault = 'owner_only' | 'household' | 'open';
type DestructiveToolPolicy = 'always_refuse' | 'opt_in_per_room';

/** The user-level autonomous-room defaults (v4 `AutonomousRoomSettings` `DEFAULTS`). */
interface AutonomousRoomUserSettings {
  dailyTokenBudget?: number | null;
  defaultFreshnessWindowMs?: number;
  visibilityDefault?: VisibilityDefault;
  destructiveToolPolicy?: DestructiveToolPolicy;
}

const MS_PER_HOUR = 60 * 60 * 1000;
const DEFAULT_FRESHNESS_MS = 12 * MS_PER_HOUR;

/**
 * The user-level autonomous-room defaults (v4
 * `components/settings/chat-settings/AutonomousRoomSettings.tsx`): daily token
 * budget, default freshness window (hours), default visibility, destructive-tool
 * policy. Autosaves — the numeric inputs commit on blur (typing 5 then 0 must not
 * race the save), the selects on change. Each save merges the partial into
 * `chat_settings.autonomousRoomSettings`. Copy carries over verbatim.
 */
@Component({
  selector: 'qt-autonomous-room-defaults',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="space-y-4">
      <!-- Daily token budget -->
      <div>
        <label class="block">
          <span class="font-medium text-foreground">Daily token budget</span>
          <span class="block qt-text-small mt-1 mb-2">
            Caps the cumulative input + output tokens spent across every autonomous room you own,
            rolled over at instance-local midnight. Leave blank for no cap. Pilot value: 1,000,000.
          </span>
          <div class="flex items-center gap-2">
            <input
              type="text"
              inputmode="numeric"
              pattern="[0-9]*"
              placeholder="(no cap)"
              class="qt-input w-48"
              [value]="dailyTokenText()"
              (input)="dailyTokenText.set($any($event.target).value)"
              (blur)="commitDailyToken()"
              (keydown.enter)="$any($event.target).blur()"
              [disabled]="saving()"
            />
            <span class="qt-text-small">tokens / day</span>
          </div>
        </label>
      </div>

      <!-- Default freshness window -->
      <div>
        <label class="block">
          <span class="font-medium text-foreground">Default freshness window</span>
          <span class="block qt-text-small mt-1 mb-2">
            How long after a missed scheduled run the scheduler should still consider catching up,
            in hours. Per-room overrides are honored. Default: 12 hours.
          </span>
          <div class="flex items-center gap-2">
            <input
              type="text"
              inputmode="numeric"
              pattern="[0-9]*"
              class="qt-input w-24"
              [value]="freshnessText()"
              (input)="freshnessText.set($any($event.target).value)"
              (blur)="commitFreshness()"
              (keydown.enter)="$any($event.target).blur()"
              [disabled]="saving()"
            />
            <span class="qt-text-small">hours</span>
          </div>
        </label>
      </div>

      <!-- Default visibility -->
      <div>
        <label class="block">
          <span class="font-medium text-foreground">Default visibility</span>
          <span class="block qt-text-small mt-1 mb-2">
            Where new autonomous rooms appear by default. Per-room overrides are honored at
            creation.
          </span>
          <select
            class="qt-input"
            [value]="current().visibilityDefault"
            (change)="update({ visibilityDefault: $any($event.target).value })"
            [disabled]="saving()"
          >
            <option value="owner_only">Owner only — hidden from the main Salon list</option>
            <option value="household">Household — visible per chat-sharing rules</option>
            <option value="open">Open — visible in the main Salon list</option>
          </select>
        </label>
      </div>

      <!-- Destructive-tool policy -->
      <div>
        <label class="block">
          <span class="font-medium text-foreground">Destructive-tool policy</span>
          <span class="block qt-text-small mt-1 mb-2">
            Acts as a ceiling: <em>Always refuse</em> blocks destructive tools across every
            autonomous room regardless of per-room settings; <em>Opt in per room</em> honors a
            room's explicit pre-authorization.
          </span>
          <select
            class="qt-input"
            [value]="current().destructiveToolPolicy"
            (change)="update({ destructiveToolPolicy: $any($event.target).value })"
            [disabled]="saving()"
          >
            <option value="always_refuse">Always refuse — destructive tools never available</option>
            <option value="opt_in_per_room">Opt in per room — honor per-room authorization</option>
          </select>
        </label>
      </div>
    </div>
  `,
})
export class AutonomousRoomDefaults {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();

  protected readonly saving = signal(false);

  private readonly settingsQuery = injectQuery(() => ({
    queryKey: ['chatSettings'],
    queryFn: async (): Promise<ChatSettingsDto> => {
      const resp = await this.core.dispatchExpect({ type: 'chatSettings' }, 'chatSettings');
      return resp.data;
    },
  }));

  /** The saved autonomous-room settings merged over the v4 defaults. */
  protected readonly current = computed<Required<AutonomousRoomUserSettings>>(() => {
    const ar = (this.settingsQuery.data()?.['autonomousRoomSettings'] ??
      {}) as AutonomousRoomUserSettings;
    return {
      dailyTokenBudget: ar.dailyTokenBudget ?? null,
      defaultFreshnessWindowMs: ar.defaultFreshnessWindowMs ?? DEFAULT_FRESHNESS_MS,
      visibilityDefault: ar.visibilityDefault ?? 'owner_only',
      destructiveToolPolicy: ar.destructiveToolPolicy ?? 'opt_in_per_room',
    };
  });

  // Local text for the two numeric inputs (commit on blur; reset on invalid). Seeded
  // from the saved value and re-synced whenever the persisted value genuinely changes.
  protected readonly dailyTokenText = signal('');
  protected readonly freshnessText = signal('');

  constructor() {
    effect(() => {
      const c = this.current();
      this.dailyTokenText.set(c.dailyTokenBudget != null ? String(c.dailyTokenBudget) : '');
      this.freshnessText.set(String(Math.round(c.defaultFreshnessWindowMs / MS_PER_HOUR)));
    });
  }

  protected async commitDailyToken(): Promise<void> {
    const trimmed = this.dailyTokenText().trim();
    if (trimmed === '') {
      await this.update({ dailyTokenBudget: null });
      return;
    }
    const parsed = Number.parseInt(trimmed, 10);
    if (!Number.isFinite(parsed) || parsed <= 0) {
      // Reset to the saved value.
      const saved = this.current().dailyTokenBudget;
      this.dailyTokenText.set(saved != null ? String(saved) : '');
      return;
    }
    await this.update({ dailyTokenBudget: parsed });
  }

  protected async commitFreshness(): Promise<void> {
    const parsed = Number.parseInt(this.freshnessText().trim(), 10);
    if (!Number.isFinite(parsed) || parsed <= 0) {
      this.freshnessText.set(
        String(Math.round(this.current().defaultFreshnessWindowMs / MS_PER_HOUR)),
      );
      return;
    }
    await this.update({ defaultFreshnessWindowMs: parsed * MS_PER_HOUR });
  }

  /** Merge the partial into the saved autonomous-room settings and persist. */
  protected async update(patch: Partial<AutonomousRoomUserSettings>): Promise<void> {
    this.saving.set(true);
    try {
      const next = { ...this.current(), ...patch };
      await this.core.dispatchExpect(
        { type: 'chatSettingsUpdate', settings: { autonomousRoomSettings: next } },
        'chatSettings',
      );
      await this.queryClient.invalidateQueries({ queryKey: ['chatSettings'] });
    } catch {
      // Leave current; the next refetch restores server truth.
    } finally {
      this.saving.set(false);
    }
  }
}
