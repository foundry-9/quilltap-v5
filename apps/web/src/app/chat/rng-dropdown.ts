import { ChangeDetectionStrategy, Component, inject, input, output, signal } from '@angular/core';

import { rollRng, type RngKind } from './chat-cast.api';
import { CoreClient } from '../core/core-client';
import { Icon } from '../ui/icon';

/**
 * A rolled result waiting in the composer (v4 `RngDropdown.tsx:31-40`
 * `RngPendingResult`). The Salon turns it into a `pendingToolResults` entry on
 * the next send, which the spine writes as a TOOL message ahead of the user's.
 */
export interface RngPendingResult {
  tool: 'rng';
  displayName: string;
  icon: string;
  summary: string;
  formattedResult: string;
  requestPrompt: string;
  arguments: Record<string, unknown>;
  success: boolean;
}

/** v4 `OTHER_OPTIONS` (`:19-22`). */
const OTHER_OPTIONS: ReadonlyArray<{ label: string; kind: RngKind }> = [
  { label: 'Flip Coin', kind: 'flip_coin' },
  { label: 'Spin the Bottle', kind: 'spin_the_bottle' },
];

/** v4 `DICE_TYPES` (`:25-28`) — two quick dice with adjustable counts. */
const DICE_TYPES: ReadonlyArray<{ sides: number; label: string }> = [
  { sides: 6, label: 'd6' },
  { sides: 20, label: 'd20' },
];

/**
 * The composer gutter's RNG tool (v4 `components/chat/RngDropdown.tsx`, mounted
 * by `ComposerGutterTools.tsx:132-139` in row 3, column 2 — the slot v5's
 * composer has been carrying an explicit hole for since P4.9E2B).
 *
 * Quick d6 and d20 with ±spinners (1–100), Flip Coin, Spin the Bottle, and a
 * custom `<rolls>d<sides>` form validated client-side against v4's ranges
 * (2–1000 sides, 1–100 rolls) before any request goes out.
 *
 * **Preview mode is the whole point.** v4 rolls with `preview: true` whenever a
 * pending-result handler exists, so the roll is NOT written as a message: it
 * comes back as a chip in the composer, which the operator can discard, and it
 * only becomes a TOOL row when they send. The legacy path — roll straight into
 * the conversation — is what v4 does when nothing is listening for a pending
 * result, and it is kept for the same reason.
 *
 * The request says `kind` where v4 says `type` (the E3A §1 rename: `type` is the
 * v5 request union's own tag). The `arguments` bag that comes BACK is v4's own
 * `{type, rolls}` and rides through untouched — it is what the persisted TOOL row
 * records, and re-keying it would make v5's rows differ from v4's.
 */
@Component({
  selector: 'qt-rng-dropdown',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div class="relative">
      <button
        type="button"
        class="qt-chat-toolbar-button"
        title="Random number generator"
        aria-label="Random number generator"
        [attr.aria-expanded]="open()"
        aria-haspopup="menu"
        [disabled]="disabled() || loading()"
        (click)="toggle()"
      >
        <qt-icon name="dice" class="w-5 h-5" />
      </button>

      @if (open()) {
        <div
          class="absolute bottom-full left-0 mb-1 w-48 qt-card qt-shadow-lg rounded-lg border z-50"
          role="menu"
        >
          <div class="py-1">
            @for (die of diceTypes; track die.sides) {
              <div class="flex items-center px-3 py-1.5 gap-2">
                <button
                  type="button"
                  role="menuitem"
                  class="flex-1 px-2 py-1.5 text-left text-sm hover:qt-bg-muted transition-colors disabled:opacity-50 rounded"
                  [disabled]="loading()"
                  (click)="roll(die.sides, count(die.sides))"
                >
                  Roll {{ count(die.sides) }}{{ die.label }}
                </button>
                <div class="flex flex-col">
                  <button
                    type="button"
                    class="px-1.5 py-0.5 text-xs hover:qt-bg-muted transition-colors disabled:opacity-30 rounded-t border border-b-0"
                    title="Increase dice count"
                    [attr.aria-label]="'Increase ' + die.label + ' count'"
                    [disabled]="loading() || count(die.sides) >= 100"
                    (click)="adjust(die.sides, 1)"
                  >
                    <qt-icon name="chevron-down" class="w-3 h-3 rotate-180" />
                  </button>
                  <button
                    type="button"
                    class="px-1.5 py-0.5 text-xs hover:qt-bg-muted transition-colors disabled:opacity-30 rounded-b border"
                    title="Decrease dice count"
                    [attr.aria-label]="'Decrease ' + die.label + ' count'"
                    [disabled]="loading() || count(die.sides) <= 1"
                    (click)="adjust(die.sides, -1)"
                  >
                    <qt-icon name="chevron-down" class="w-3 h-3" />
                  </button>
                </div>
              </div>
            }

            @for (option of otherOptions; track option.label) {
              <button
                type="button"
                role="menuitem"
                class="w-full px-3 py-2 text-left text-sm hover:qt-bg-muted transition-colors disabled:opacity-50"
                [disabled]="loading()"
                (click)="roll(option.kind, 1)"
              >
                {{ option.label }}
              </button>
            }

            <div class="border-t my-1"></div>

            <button
              type="button"
              role="menuitem"
              class="w-full px-3 py-2 text-left text-sm hover:qt-bg-muted transition-colors disabled:opacity-50 flex items-center justify-between"
              [disabled]="loading()"
              (click)="customOpen.set(!customOpen())"
            >
              <span>Custom Roll</span>
              <qt-icon
                name="chevron-down"
                [class]="'w-3 h-3 transition-transform ' + (customOpen() ? 'rotate-180' : '')"
              />
            </button>

            @if (customOpen()) {
              <div class="px-3 py-2 space-y-2 border-t">
                <div class="flex items-center gap-2">
                  <input
                    type="number"
                    class="w-14 qt-input"
                    placeholder="Rolls"
                    aria-label="Number of rolls"
                    min="1"
                    max="100"
                    [value]="customRolls()"
                    (input)="customRolls.set($any($event.target).value)"
                  />
                  <span class="text-sm">d</span>
                  <input
                    type="number"
                    class="w-16 qt-input"
                    placeholder="Sides"
                    aria-label="Number of sides"
                    min="2"
                    max="1000"
                    [value]="customSides()"
                    (input)="customSides.set($any($event.target).value)"
                  />
                </div>
                <button
                  type="button"
                  class="w-full px-2 py-1 text-sm qt-button qt-button-primary rounded"
                  [disabled]="loading()"
                  (click)="rollCustom()"
                >
                  Roll {{ customRolls() }}d{{ customSides() }}
                </button>
              </div>
            }

            @if (error(); as message) {
              <div class="px-3 py-2 text-xs qt-text-destructive border-t" role="alert">
                {{ message }}
              </div>
            }
          </div>
        </div>
      }
    </div>
  `,
})
export class RngDropdown {
  private readonly core = inject(CoreClient);

  readonly chatId = input.required<string>();
  readonly disabled = input(false);

  /**
   * The rolled-but-unsent result (v4 `onPendingResult`). Its PRESENCE is what
   * selects preview mode in v4; here the Salon always listens, so the roll never
   * writes a message on its own.
   */
  readonly pendingResult = output<RngPendingResult>();

  protected readonly diceTypes = DICE_TYPES;
  protected readonly otherOptions = OTHER_OPTIONS;

  protected readonly open = signal(false);
  protected readonly customOpen = signal(false);
  protected readonly customSides = signal('20');
  protected readonly customRolls = signal('1');
  protected readonly loading = signal(false);
  protected readonly error = signal<string | null>(null);

  /** Per-die counts (v4 `diceRolls`, seeded 1 for each type). */
  private readonly counts = signal<Record<number, number>>({ 6: 1, 20: 1 });

  protected count(sides: number): number {
    return this.counts()[sides] ?? 1;
  }

  /** v4 `adjustDiceCount` — clamped 1..100. */
  protected adjust(sides: number, delta: number): void {
    this.counts.update((prev) => {
      const current = prev[sides] ?? 1;
      return { ...prev, [sides]: Math.max(1, Math.min(100, current + delta)) };
    });
  }

  protected toggle(): void {
    if (this.disabled()) return;
    this.open.set(!this.open());
    this.customOpen.set(false);
    this.error.set(null);
  }

  /** v4 `handleCustomRoll` — the range check happens BEFORE any request. */
  protected rollCustom(): void {
    const sides = Number.parseInt(this.customSides(), 10);
    const rolls = Number.parseInt(this.customRolls(), 10);
    if (Number.isNaN(sides) || sides < 2 || sides > 1000) {
      this.error.set('Sides must be between 2 and 1000');
      return;
    }
    if (Number.isNaN(rolls) || rolls < 1 || rolls > 100) {
      this.error.set('Rolls must be between 1 and 100');
      return;
    }
    void this.roll(sides, rolls);
  }

  /** v4 `executeRng` (`:84-127`). */
  protected async roll(kind: RngKind, rolls: number): Promise<void> {
    if (this.disabled() || this.loading()) return;
    this.loading.set(true);
    this.error.set(null);
    try {
      const result = await rollRng(this.core, this.chatId(), kind, rolls, true);
      if (result) {
        this.pendingResult.emit({
          tool: 'rng',
          displayName: 'Random Number Generator',
          icon: '🎲',
          summary: result.summary,
          formattedResult: result.formattedText,
          requestPrompt: result.requestPrompt,
          arguments: result.arguments,
          success: true,
        });
      }
      this.open.set(false);
      this.customOpen.set(false);
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'Unknown error');
    } finally {
      this.loading.set(false);
    }
  }
}
