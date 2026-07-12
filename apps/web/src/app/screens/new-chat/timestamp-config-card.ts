import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';
import { FormsModule } from '@angular/forms';

import type { TimestampConfig } from '../../core/core-contract';

type TimestampMode = 'NONE' | 'START_ONLY' | 'EVERY_MESSAGE' | 'EVERY_N_MINUTES';
type TimestampFormat = 'ISO8601' | 'FRIENDLY' | 'DATE_ONLY' | 'TIME_ONLY' | 'CUSTOM';

interface Ts {
  mode: TimestampMode;
  format: TimestampFormat;
  intervalMinutes?: number;
  customFormat?: string | null;
  timezone?: string | null;
  autoPrepend?: boolean;
  useFictionalTime?: boolean;
  fictionalBaseTimestamp?: string | null;
}

const DEFAULT_TS: Ts = {
  mode: 'NONE',
  format: 'FRIENDLY',
  useFictionalTime: false,
  autoPrepend: true,
  intervalMinutes: 15,
};

const MODES: { value: TimestampMode; label: string; description: string }[] = [
  { value: 'NONE', label: 'Disabled', description: 'No timestamp injection' },
  { value: 'START_ONLY', label: 'Conversation Start', description: 'Include timestamp only in the initial system prompt' },
  { value: 'EVERY_MESSAGE', label: 'Every Message', description: 'Update timestamp with each message sent' },
  {
    value: 'EVERY_N_MINUTES',
    label: 'Every X Minutes',
    description:
      'Have the Host announce the time only when at least this many minutes have passed since the last announcement',
  },
];

const FORMATS: { value: TimestampFormat; label: string; description: string; example: string }[] = [
  { value: 'FRIENDLY', label: 'Friendly', description: 'Human-readable format (e.g., "March 15, 2024 at 2:30 PM")', example: 'March 15, 2024 at 2:30 PM' },
  { value: 'ISO8601', label: 'ISO 8601', description: 'Standard machine-readable format', example: '2024-03-15T14:30:00Z' },
  { value: 'DATE_ONLY', label: 'Date Only', description: 'Just the date, no time', example: 'March 15, 2024' },
  { value: 'TIME_ONLY', label: 'Time Only', description: 'Just the time, no date', example: '2:30 PM' },
  { value: 'CUSTOM', label: 'Custom', description: 'Use a custom format string (date-fns tokens)', example: '' },
];

/**
 * The compact "Reality Injection Mode" timestamp card (v4
 * `TimestampConfigCard` with `compact`). Fully controlled: reads
 * {@link TimestampConfig} (loose on the SPA side) and emits the whole config on
 * every change. Selecting `EVERY_N_MINUTES` the first time seeds a 15-minute
 * interval (v4's `handleModeChange`).
 */
@Component({
  selector: 'qt-timestamp-config-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [FormsModule],
  template: `
    <div class="space-y-4 qt-text-small" [class.opacity-60]="disabled()" [class.pointer-events-none]="disabled()">
      <!-- Mode -->
      <div>
        <label class="block qt-text-label mb-2">Timestamp Injection Mode</label>
        <div class="space-y-2">
          @for (m of modes; track m.value) {
            <label class="flex items-start gap-3 p-3 border qt-border-default rounded qt-hover-accent cursor-pointer transition-colors">
              <input
                type="radio"
                name="qt-ts-mode"
                [value]="m.value"
                [checked]="cfg().mode === m.value"
                (change)="setMode(m.value)"
                [disabled]="disabled()"
                class="mt-1"
              />
              <div class="flex-1">
                <div class="font-medium qt-text-primary">{{ m.label }}</div>
                <div class="qt-text-secondary">{{ m.description }}</div>
              </div>
            </label>
          }
        </div>
      </div>

      @if (cfg().mode === 'EVERY_N_MINUTES') {
        <div>
          <label class="block qt-text-label mb-2">Interval (minutes)</label>
          <input
            type="number"
            min="1"
            step="1"
            [ngModel]="cfg().intervalMinutes ?? 15"
            (ngModelChange)="setInterval($event)"
            [disabled]="disabled()"
            class="w-32 px-3 py-2 border qt-border-default rounded bg-background qt-text-primary focus:outline-none focus:ring-2 focus:ring-primary"
          />
        </div>
      }

      @if (cfg().mode !== 'NONE') {
        <div>
          <label class="block qt-text-label mb-2">Timestamp Format</label>
          <select
            [ngModel]="cfg().format"
            (ngModelChange)="setFormat($event)"
            [disabled]="disabled()"
            class="w-full qt-select"
          >
            @for (f of formats; track f.value) {
              <option [value]="f.value" [selected]="f.value === cfg().format">
                {{ f.label }}{{ f.example ? ' - ' + f.example : '' }}
              </option>
            }
          </select>
        </div>

        @if (cfg().format === 'CUSTOM') {
          <div>
            <label class="block qt-text-label mb-2">Custom Format String</label>
            <input
              type="text"
              [ngModel]="cfg().customFormat || ''"
              (ngModelChange)="patch({ customFormat: $event || null })"
              placeholder="e.g., 'PPpp' or 'yyyy-MM-dd HH:mm:ss'"
              [disabled]="disabled()"
              class="w-full px-3 py-2 border qt-border-default rounded bg-background qt-text-primary focus:outline-none focus:ring-2 focus:ring-primary"
            />
          </div>
        }

        <div>
          <label class="block qt-text-label mb-2">Timezone</label>
          <div class="flex gap-2">
            <input
              type="text"
              [ngModel]="cfg().timezone || ''"
              (ngModelChange)="patch({ timezone: $event || null })"
              [placeholder]="detectedTz"
              [disabled]="disabled()"
              class="flex-1 px-3 py-2 border qt-border-default rounded bg-background qt-text-primary focus:outline-none focus:ring-2 focus:ring-primary"
            />
            <button
              type="button"
              (click)="detectTz()"
              [disabled]="disabled()"
              class="px-3 py-2 text-xs border qt-border-default rounded qt-hover-accent transition-colors qt-text-secondary whitespace-nowrap"
              title="Set to your browser's detected timezone"
            >
              Detect
            </button>
          </div>
        </div>

        <div>
          <label class="block qt-text-label mb-2">Injection Method</label>
          <label class="flex items-start gap-3 p-3 border qt-border-default rounded qt-hover-accent cursor-pointer transition-colors">
            <input
              type="checkbox"
              [ngModel]="cfg().autoPrepend ?? true"
              (ngModelChange)="patch({ autoPrepend: $event })"
              [disabled]="disabled()"
              class="mt-1"
            />
            <div class="flex-1">
              <div class="font-medium qt-text-primary">Auto-prepend to system prompt</div>
              <div class="qt-text-secondary text-xs">
                {{
                  (cfg().autoPrepend ?? true)
                    ? 'Timestamp will be automatically prepended to system prompt'
                    : 'Use {{timestamp}} template variable in system prompt instead'
                }}
              </div>
            </div>
          </label>
        </div>

        <div>
          <label class="block qt-text-label mb-2">Fictional Time</label>
          <label class="flex items-start gap-3 p-3 border qt-border-default rounded qt-hover-accent cursor-pointer transition-colors">
            <input
              type="checkbox"
              [ngModel]="cfg().useFictionalTime ?? false"
              (ngModelChange)="patch({ useFictionalTime: $event })"
              [disabled]="disabled()"
              class="mt-1"
            />
            <div class="flex-1">
              <div class="font-medium qt-text-primary">Use fictional time</div>
              <div class="qt-text-secondary text-xs">
                Instead of real-time, use a fictional base timestamp that advances with each message
              </div>
            </div>
          </label>
        </div>

        @if (cfg().useFictionalTime) {
          <div>
            <label class="block qt-text-label mb-2">Fictional Base Timestamp</label>
            <input
              type="datetime-local"
              [ngModel]="cfg().fictionalBaseTimestamp || ''"
              (ngModelChange)="patch({ fictionalBaseTimestamp: $event || null })"
              [disabled]="disabled()"
              class="w-full px-3 py-2 border qt-border-default rounded bg-background qt-text-primary focus:outline-none focus:ring-2 focus:ring-primary"
            />
          </div>
        }
      }

      <div class="mt-4 p-2 qt-bg-muted rounded text-xs qt-text-secondary">{{ infoLine() }}</div>
    </div>
  `,
})
export class TimestampConfigCard {
  readonly value = input<TimestampConfig | null>(null);
  readonly disabled = input(false);
  readonly changed = output<TimestampConfig>();

  protected readonly modes = MODES;
  protected readonly formats = FORMATS;
  protected readonly detectedTz = (() => {
    try {
      return Intl.DateTimeFormat().resolvedOptions().timeZone;
    } catch {
      return 'e.g., America/New_York';
    }
  })();

  protected readonly cfg = computed<Ts>(() => ({ ...DEFAULT_TS, ...((this.value() as Ts | null) ?? {}) }));

  protected setMode(mode: TimestampMode): void {
    const next: Ts = { ...this.cfg(), mode };
    if (mode === 'EVERY_N_MINUTES' && !next.intervalMinutes) next.intervalMinutes = 15;
    this.changed.emit(next as unknown as TimestampConfig);
  }

  protected setInterval(raw: string | number): void {
    const parsed = Number.parseInt(String(raw), 10);
    const next = Number.isFinite(parsed) && parsed >= 1 ? parsed : 1;
    this.patch({ intervalMinutes: next });
  }

  protected setFormat(format: TimestampFormat): void {
    this.patch({ format });
  }

  protected detectTz(): void {
    try {
      this.patch({ timezone: Intl.DateTimeFormat().resolvedOptions().timeZone });
    } catch {
      // Intl unavailable.
    }
  }

  protected patch(p: Partial<Ts>): void {
    this.changed.emit({ ...this.cfg(), ...p } as unknown as TimestampConfig);
  }

  protected infoLine(): string {
    const c = this.cfg();
    if (c.mode === 'NONE') return 'Timestamp injection is disabled';
    const when =
      c.mode === 'START_ONLY'
        ? 'at conversation start'
        : c.mode === 'EVERY_N_MINUTES'
          ? `every ${c.intervalMinutes ?? 15} minute${(c.intervalMinutes ?? 15) === 1 ? '' : 's'}`
          : 'with every message';
    return `Timestamps will be injected ${when} in ${c.format} format${c.useFictionalTime ? ' (fictional time)' : ''}`;
  }
}
