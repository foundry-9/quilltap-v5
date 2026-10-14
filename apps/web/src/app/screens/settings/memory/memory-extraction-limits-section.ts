import { ChangeDetectionStrategy, Component, signal } from '@angular/core';

import type { MemoryExtractionLimitsConfig } from '../../../core/core-contract';
import { fetchExtractionLimits, memoryKeys, setExtractionLimits } from '../../../memory/memory.api';
import { MEMORY_SETTING_DEFAULTS } from './memory-setting-defaults';
import { injectMemorySettingsDocument } from './memory-settings-document';

type NumericKey = 'maxPerHour' | 'softStartFraction' | 'softFloor';

interface FieldSpec {
  key: NumericKey;
  label: string;
  help: string;
  min: number;
  max: number;
  step: number;
  integer: boolean;
}

/** v4 `FIELDS` (`memory-extraction-limits-section.tsx:22-26`). */
const FIELDS: ReadonlyArray<FieldSpec> = [
  { key: 'maxPerHour', label: 'Notes per hour', help: 'The most new notes one character may take in an hour.', min: 1, max: 10_000, step: 1, integer: true },
  { key: 'softStartFraction', label: 'Begin choosing at', help: 'The share of the hourly limit (0 to 1) past which only weightier notes are kept.', min: 0, max: 1, step: 0.05, integer: false },
  { key: 'softFloor', label: 'Importance floor', help: 'Past that point, the least importance a new note must carry (0 to 1).', min: 0, max: 1, step: 0.05, integer: false },
];

/**
 * The extraction rate limiter (`instance_settings['memoryExtractionLimits']`),
 * on the Extraction Grain card — v4 `components/tools/memory-extraction-
 * limits-section.tsx` (`b045dd986`, at `83334b9a5`). When a character's new
 * notes in the past hour approach `maxPerHour`, only the weightier candidates
 * are kept; at the limit, extraction pauses for that character until the
 * hour rolls on.
 *
 * Its own settings document (L-1), its own error line. The checkbox snaps
 * back on a failed save (L-3); a number field clears its draft FIRST, then
 * judges it (v4 `:39-52`).
 */
@Component({
  selector: 'qt-memory-extraction-limits-section',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (doc.loading()) {
      <p class="qt-text-small qt-text-muted">Loading extraction limits…</p>
    } @else {
      <div class="space-y-3">
        <h3 class="qt-text-label">Rate limits</h3>
        <label class="flex items-start gap-3 qt-body">
          <input
            type="checkbox"
            class="qt-checkbox mt-1"
            [checked]="doc.config().enabled"
            [disabled]="doc.saving()"
            (change)="toggleEnabled($any($event.target))"
          />
          <span>
            <span>Limit how quickly a character takes notes</span>
            <span class="block qt-text-xs qt-text-muted">A guard against a long, busy evening filling the shelves. Off by default.</span>
          </span>
        </label>
        <div class="grid gap-3 sm:grid-cols-3">
          @for (field of fields; track field.key) {
            <div class="space-y-1">
              <label [for]="'extraction-limits-' + field.key" class="block qt-text-label">{{ field.label }}</label>
              <input
                [id]="'extraction-limits-' + field.key"
                type="number"
                class="qt-input"
                [attr.min]="field.min"
                [attr.max]="field.max"
                [attr.step]="field.step"
                [disabled]="doc.saving() || !doc.config().enabled"
                [value]="fieldValue(field)"
                (input)="setDraft(field.key, $any($event.target).value)"
                (blur)="commit(field)"
              />
              <p class="qt-text-xs qt-text-muted">{{ field.help }}</p>
            </div>
          }
        </div>
        @if (doc.error(); as msg) {
          <p class="qt-text-small qt-text-destructive">{{ msg }}</p>
        }
      </div>
    }
  `,
})
export class MemoryExtractionLimitsSection {
  protected readonly doc = injectMemorySettingsDocument<MemoryExtractionLimitsConfig>({
    queryKey: memoryKeys.extractionLimitsConfig(),
    read: fetchExtractionLimits,
    write: setExtractionLimits,
    defaults: MEMORY_SETTING_DEFAULTS.extractionLimits,
    noun: 'extraction limits',
  });

  protected readonly fields = FIELDS;
  private readonly drafts = signal<Partial<Record<NumericKey, string>>>({});

  protected fieldValue(field: FieldSpec): string {
    return this.drafts()[field.key] ?? String(this.doc.config()[field.key]);
  }

  protected setDraft(key: NumericKey, value: string): void {
    this.drafts.update((d) => ({ ...d, [key]: value }));
  }

  /** v4 `commit` (`:39-52`). */
  protected commit(field: FieldSpec): void {
    const raw = this.drafts()[field.key];
    if (raw === undefined) return;
    this.drafts.update((d) => {
      const next = { ...d };
      delete next[field.key];
      return next;
    });
    const value = Number(raw.trim());
    const bad =
      raw.trim() === '' ||
      !Number.isFinite(value) ||
      value < field.min ||
      value > field.max ||
      (field.integer && !Number.isInteger(value));
    if (bad) {
      this.doc.setError(
        `${field.label} must be ${field.integer ? 'a whole number' : 'a number'} between ${field.min} and ${field.max}`,
      );
      return;
    }
    if (value !== this.doc.config()[field.key]) void this.doc.save({ [field.key]: value });
  }

  /** v4 `:62` — `{ enabled: !config.enabled }`; the box re-syncs once the save settles (L-3). */
  protected async toggleEnabled(el: HTMLInputElement): Promise<void> {
    const enabled = this.doc.config().enabled;
    await this.doc.save({ enabled: !enabled });
    el.checked = this.doc.config().enabled;
  }
}
