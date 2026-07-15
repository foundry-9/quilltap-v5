import { ChangeDetectionStrategy, Component, computed, effect, signal } from '@angular/core';

import { ErrorAlert } from '../../../ui/error-alert';
import { ChatSettingsCard } from './chat-settings.api';
import {
  DEFAULT_CONTEXT_COMPRESSION_SETTINGS,
  type ContextCompressionSettings as CompressionBag,
} from './chat-settings.types';
import { SettingsCard } from './settings-card';

type Slider = 'window' | 'compression' | 'projectContext';

const WINDOW_MIN = 3;
const WINDOW_MAX = 10;
const TARGET_MIN = 300;
const TARGET_MAX = 2000;
const INTERVAL_MAX = 20;
const INTERVAL_FALLBACK = 5;

/**
 * The Context Compression card (v4 `components/settings/chat-settings/
 * ContextCompressionSettings.tsx`): the sliding-window compression that trims
 * token costs in long conversations by summarizing older messages with a cheap
 * LLM.
 *
 * **v4's drag/commit protocol, ported exactly** — a slider must not PUT on every
 * pixel of a drag. Each slider tracks a LOCAL value while dragging and commits on
 * release; the displayed value is the local one while that slider is being
 * dragged and the persisted one otherwise, so an external update still lands. A
 * commit that would write an unchanged value is skipped entirely.
 *
 * **The window/interval cross-validation** (v4
 * `handleWindowSizeCommitWithValidation`): the project-context re-injection
 * interval may not be shorter than the window (there is no point re-sending more
 * often than the window slides). Raising the window past the stored interval
 * therefore pushes the interval up WITH IT — a single PUT carrying both keys.
 * Interval `0` means "never after the initial message" and is exempt.
 *
 * v4's system-prompt compression is gone (only message history is compressed);
 * `systemPromptTargetTokens` survives in the bag and rides the merge untouched.
 */
@Component({
  selector: 'qt-context-compression-settings',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ErrorAlert, SettingsCard],
  template: `
    @if (loading()) {
      <p class="qt-text-small qt-text-muted">Loading context-compression settings…</p>
    } @else {
      <qt-settings-card
        title="Context Compression"
        subtitle="Reduce token costs in long conversations by compressing older messages using a cheap LLM. Recent messages are kept in full context while older messages are summarized."
      >
        @if (saveError(); as msg) {
          <qt-error-alert [message]="msg" class="mb-3" />
        }

        <div class="space-y-6">
          <!-- Enable / disable -->
          <div class="flex items-center justify-between">
            <div>
              <span class="qt-text-label block">Enable Context Compression</span>
              <p class="qt-text-xs qt-text-secondary mt-1">
                When enabled, older messages beyond the sliding window are compressed to save
                tokens.
              </p>
            </div>
            <button
              type="button"
              role="switch"
              aria-label="Enable Context Compression"
              [attr.aria-checked]="current().enabled"
              [disabled]="saving()"
              (click)="onEnabledChange(!current().enabled)"
              class="relative inline-flex h-6 w-11 items-center rounded-full transition-colors"
              [class.bg-primary]="current().enabled"
              [class.qt-bg-muted]="!current().enabled"
              [class.opacity-50]="saving()"
              [class.cursor-not-allowed]="saving()"
            >
              <span
                class="inline-block h-4 w-4 transform rounded-full bg-background transition-transform"
                [class.translate-x-6]="current().enabled"
                [class.translate-x-1]="!current().enabled"
              ></span>
            </button>
          </div>

          <!-- Sliding window size -->
          <div [class.opacity-50]="!current().enabled" [class.pointer-events-none]="!current().enabled">
            <label for="compression-window" class="qt-text-label block mb-2">
              Sliding Window Size
              <span class="qt-text-xs qt-text-secondary ml-2">
                ({{ displayWindowSize() }} messages)
              </span>
            </label>
            <input
              id="compression-window"
              type="range"
              class="w-full cursor-pointer"
              [min]="WINDOW_MIN"
              [max]="WINDOW_MAX"
              [value]="displayWindowSize()"
              [disabled]="!current().enabled"
              (mousedown)="startWindow()"
              (touchstart)="startWindow()"
              (input)="changeWindow($any($event.target).value)"
              (mouseup)="commitWindow()"
              (touchend)="commitWindow()"
            />
            <div class="flex justify-between qt-text-xs qt-text-secondary mt-1">
              <span>3 (more aggressive)</span>
              <span>10 (more context)</span>
            </div>
            <p class="qt-text-xs qt-text-secondary mt-2">
              Number of recent messages to keep in full context. Messages older than this are
              compressed.
            </p>
          </div>

          <!-- History compression target -->
          <div [class.opacity-50]="!current().enabled" [class.pointer-events-none]="!current().enabled">
            <label for="compression-target" class="qt-text-label block mb-2">
              History Compression Target
              <span class="qt-text-xs qt-text-secondary ml-2">
                (~{{ displayCompressionTarget() }} tokens)
              </span>
            </label>
            <input
              id="compression-target"
              type="range"
              class="w-full cursor-pointer"
              [min]="TARGET_MIN"
              [max]="TARGET_MAX"
              step="100"
              [value]="displayCompressionTarget()"
              [disabled]="!current().enabled"
              (mousedown)="startTarget()"
              (touchstart)="startTarget()"
              (input)="changeTarget($any($event.target).value)"
              (mouseup)="commitTarget()"
              (touchend)="commitTarget()"
            />
            <div class="flex justify-between qt-text-xs qt-text-secondary mt-1">
              <span>300 (minimal)</span>
              <span>2000 (detailed)</span>
            </div>
            <p class="qt-text-xs qt-text-secondary mt-2">
              Target token count for compressed conversation history.
            </p>
          </div>

          <!-- Project context re-injection -->
          <div [class.opacity-50]="!current().enabled" [class.pointer-events-none]="!current().enabled">
            <label for="compression-interval" class="qt-text-label block mb-2">
              Project Context Re-injection
              <span class="qt-text-xs qt-text-secondary ml-2">
                (every
                {{
                  displayProjectContextInterval() === 0
                    ? 'never'
                    : displayProjectContextInterval() + ' messages'
                }})
              </span>
            </label>
            <input
              id="compression-interval"
              type="range"
              class="w-full cursor-pointer"
              [min]="displayWindowSize()"
              [max]="INTERVAL_MAX"
              [value]="
                displayProjectContextInterval() === 0
                  ? displayWindowSize()
                  : displayProjectContextInterval()
              "
              [disabled]="!current().enabled"
              (mousedown)="startInterval()"
              (touchstart)="startInterval()"
              (input)="changeInterval($any($event.target).value)"
              (mouseup)="commitInterval()"
              (touchend)="commitInterval()"
            />
            <div class="flex justify-between qt-text-xs qt-text-secondary mt-1">
              <span>{{ displayWindowSize() }} (minimum)</span>
              <span>20 (less frequent)</span>
            </div>
            <p class="qt-text-xs qt-text-secondary mt-2">
              How often to re-inject project instructions into the system prompt. Set to the window
              size to ensure project context is always present in recent messages.
            </p>
          </div>

          <!-- Info box -->
          <div class="qt-bg-muted rounded-lg p-4 qt-text-small">
            <p class="font-medium mb-2">How it works:</p>
            <ul class="list-disc list-inside space-y-1 qt-text-secondary">
              <li>
                The AI keeps full access to your most recent {{ displayWindowSize() }} messages
              </li>
              <li>Older messages are summarized together by a fast, inexpensive LLM</li>
              <li>Tool definitions are never compressed</li>
              <li>The AI can use request_full_context to reload the full conversation if needed</li>
            </ul>
          </div>
        </div>
      </qt-settings-card>
    }
  `,
})
export class ContextCompressionSettings extends ChatSettingsCard {
  protected readonly WINDOW_MIN = WINDOW_MIN;
  protected readonly WINDOW_MAX = WINDOW_MAX;
  protected readonly TARGET_MIN = TARGET_MIN;
  protected readonly TARGET_MAX = TARGET_MAX;
  protected readonly INTERVAL_MAX = INTERVAL_MAX;

  /** v4 `settings.contextCompressionSettings || DEFAULT_CONTEXT_COMPRESSION_SETTINGS`. */
  protected readonly current = computed<CompressionBag>(
    () =>
      (this.settings()?.['contextCompressionSettings'] as CompressionBag | undefined) ??
      DEFAULT_CONTEXT_COMPRESSION_SETTINGS,
  );

  /** Which slider is mid-drag (v4's `dragging` state); null when none. */
  private readonly dragging = signal<Slider | null>(null);

  private readonly localWindowSize = signal(DEFAULT_CONTEXT_COMPRESSION_SETTINGS.windowSize);
  private readonly localCompressionTarget = signal(
    DEFAULT_CONTEXT_COMPRESSION_SETTINGS.compressionTargetTokens,
  );
  private readonly localProjectContextInterval = signal(
    DEFAULT_CONTEXT_COMPRESSION_SETTINGS.projectContextReinjectInterval,
  );

  constructor() {
    super();
    // v4 seeds the locals from `useState(compressionSettings.…)` on first render;
    // here the settings arrive async, so re-seed whenever the persisted value
    // moves — but NEVER mid-drag, which would fight the user's thumb.
    effect(() => {
      const bag = this.current();
      if (this.dragging() !== null) return;
      this.localWindowSize.set(bag.windowSize);
      this.localCompressionTarget.set(bag.compressionTargetTokens);
      this.localProjectContextInterval.set(this.storedInterval(bag));
    });
  }

  /** v4 `compressionSettings.projectContextReinjectInterval ?? 5`. */
  private storedInterval(bag: CompressionBag): number {
    return bag.projectContextReinjectInterval ?? INTERVAL_FALLBACK;
  }

  // v4: use the local value while dragging, else the persisted one (so an
  // external update still lands).
  protected readonly displayWindowSize = computed(() =>
    this.dragging() === 'window' ? this.localWindowSize() : this.current().windowSize,
  );
  protected readonly displayCompressionTarget = computed(() =>
    this.dragging() === 'compression'
      ? this.localCompressionTarget()
      : this.current().compressionTargetTokens,
  );
  protected readonly displayProjectContextInterval = computed(() =>
    this.dragging() === 'projectContext'
      ? this.localProjectContextInterval()
      : this.storedInterval(this.current()),
  );

  protected async onEnabledChange(enabled: boolean): Promise<void> {
    await this.update({ enabled });
  }

  // --- Sliding window ------------------------------------------------------

  protected startWindow(): void {
    this.dragging.set('window');
    this.localWindowSize.set(this.current().windowSize);
  }

  /** v4 `handleWindowSizeChangeWithValidation` — clamp, then keep the interval legal. */
  protected changeWindow(raw: string): void {
    const clamped = Math.min(WINDOW_MAX, Math.max(WINDOW_MIN, Number.parseInt(raw, 10)));
    this.localWindowSize.set(clamped);
    if (this.localProjectContextInterval() !== 0 && this.localProjectContextInterval() < clamped) {
      this.localProjectContextInterval.set(clamped);
    }
  }

  /** v4 `handleWindowSizeCommitWithValidation` — one PUT, up to two keys. */
  protected async commitWindow(): Promise<void> {
    this.dragging.set(null);
    const bag = this.current();
    const next = this.localWindowSize();
    const updates: Partial<CompressionBag> = {};
    if (next !== bag.windowSize) {
      updates.windowSize = next;
    }
    const storedInterval = this.storedInterval(bag);
    if (storedInterval !== 0 && storedInterval < next) {
      updates.projectContextReinjectInterval = next;
    }
    if (Object.keys(updates).length > 0) {
      await this.update(updates);
    }
  }

  // --- History compression target ------------------------------------------

  protected startTarget(): void {
    this.dragging.set('compression');
    this.localCompressionTarget.set(this.current().compressionTargetTokens);
  }

  protected changeTarget(raw: string): void {
    this.localCompressionTarget.set(
      Math.min(TARGET_MAX, Math.max(TARGET_MIN, Number.parseInt(raw, 10))),
    );
  }

  protected async commitTarget(): Promise<void> {
    this.dragging.set(null);
    const next = this.localCompressionTarget();
    if (next !== this.current().compressionTargetTokens) {
      await this.update({ compressionTargetTokens: next });
    }
  }

  // --- Project-context re-injection interval --------------------------------

  protected startInterval(): void {
    this.dragging.set('projectContext');
    this.localProjectContextInterval.set(this.storedInterval(this.current()));
  }

  /** v4 `handleProjectContextIntervalChange` — 0 (never) is exempt from the floor. */
  protected changeInterval(raw: string): void {
    const value = Number.parseInt(raw, 10);
    const min = this.displayWindowSize();
    this.localProjectContextInterval.set(
      value === 0 ? 0 : Math.min(INTERVAL_MAX, Math.max(min, value)),
    );
  }

  protected async commitInterval(): Promise<void> {
    this.dragging.set(null);
    const next = this.localProjectContextInterval();
    if (next !== this.storedInterval(this.current())) {
      await this.update({ projectContextReinjectInterval: next });
    }
  }

  /** v4 `{ contextCompressionSettings: { ...currentSettings, ...updates } }`. */
  private async update(updates: Partial<CompressionBag>): Promise<void> {
    await this.save(
      { contextCompressionSettings: { ...this.current(), ...updates } },
      'Failed to update context compression settings',
    );
  }
}
