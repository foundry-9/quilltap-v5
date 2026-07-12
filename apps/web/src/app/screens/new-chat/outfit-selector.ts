import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  input,
  output,
  signal,
} from '@angular/core';

import type { ChatCreateOutfitSelectionInput, OutfitSelectionMode } from '../../core/core-contract';

/** One character eligible for a starting-outfit choice (v4 `OutfitSelectorCharacter`). */
export interface OutfitSelectorCharacter {
  id: string;
  name: string;
  isUserControlled?: boolean;
}

interface ModeOption {
  value: OutfitSelectionMode;
  label: string;
  description: string;
  /** Loudly disabled with this title (the wardrobe-composer deferral). */
  disabledTitle?: string;
}

/**
 * The starting-outfit selector (v4 `components/wardrobe/outfit-selector.tsx`,
 * new-chat subset). Per-character mode radios: `default` / `llm_choose` (hidden
 * for the user's persona) / `none`. `manual` (Compose outfit) renders loudly
 * disabled-with-title — the wardrobe-composer family is deferred; `previous_chat`
 * (continuation only) is not rendered this round. Emits `{ characterId, mode }`
 * (no slots for the non-manual modes).
 */
@Component({
  selector: 'qt-outfit-selector',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (characters().length > 0) {
      <div class="space-y-2">
        <label class="mb-2 block text-sm qt-text-primary">Starting Outfit</label>
        @for (char of characters(); track char.id) {
          <div class="rounded-lg border qt-border-default qt-bg-muted/20 p-3">
            @if (showHeaders()) {
              <div class="mb-2 text-sm font-medium qt-text-primary">{{ char.name }}</div>
            }
            <div class="space-y-1.5">
              @for (opt of optionsFor(char); track opt.value) {
                <label
                  class="flex items-start gap-2 rounded px-2 py-1.5 text-sm transition cursor-pointer hover:qt-bg-muted/40"
                  [title]="opt.disabledTitle || ''"
                >
                  <input
                    type="radio"
                    [name]="'outfit-mode-' + char.id"
                    [value]="opt.value"
                    [checked]="modeFor(char.id) === opt.value"
                    (change)="setMode(char.id, opt.value)"
                    [disabled]="disabled() || !!opt.disabledTitle"
                    class="mt-0.5"
                  />
                  <div class="flex-1 min-w-0">
                    <span class="qt-text-primary">{{ opt.label }}</span>
                    <span class="ml-1.5 text-xs qt-text-secondary italic">{{
                      opt.description
                    }}</span>
                  </div>
                </label>
              }
            </div>
            @if (modeFor(char.id) === 'none') {
              <div
                class="mt-2 ml-6 rounded border qt-border-warning/50 qt-bg-warning/10 px-2 py-1.5 text-xs qt-text-warning"
              >
                Character will start undressed
              </div>
            }
          </div>
        }
      </div>
    }
  `,
})
export class OutfitSelector {
  readonly characters = input.required<OutfitSelectorCharacter[]>();
  readonly disabled = input(false);
  readonly selectionsChange = output<ChatCreateOutfitSelectionInput[]>();

  private readonly overrides = signal<Record<string, OutfitSelectionMode>>({});

  protected readonly showHeaders = computed(() => this.characters().length > 1);

  private readonly selections = computed<ChatCreateOutfitSelectionInput[]>(() =>
    this.characters().map((c) => ({
      characterId: c.id,
      mode: this.overrides()[c.id] ?? 'default',
    })),
  );

  constructor() {
    // Mirror v4's "notify parent when selections change" effect.
    effect(() => this.selectionsChange.emit(this.selections()));
  }

  protected modeFor(id: string): OutfitSelectionMode {
    return this.overrides()[id] ?? 'default';
  }

  protected optionsFor(char: OutfitSelectorCharacter): ModeOption[] {
    const opts: ModeOption[] = [
      {
        value: 'default',
        label: 'Use defaults',
        description: 'Items marked default in their wardrobe.',
      },
      {
        value: 'manual',
        label: 'Compose outfit',
        description: 'Pick the starting outfit slot by slot.',
        disabledTitle: 'Composing an outfit here is not yet available in this build.',
      },
    ];
    if (!char.isUserControlled) {
      opts.push({
        value: 'llm_choose',
        label: 'Let character choose',
        description: 'The character picks based on the scenario.',
      });
    }
    opts.push({
      value: 'none',
      label: 'Start undressed',
      description: 'Character will start undressed.',
    });
    return opts;
  }

  protected setMode(id: string, mode: OutfitSelectionMode): void {
    this.overrides.update((o) => ({ ...o, [id]: mode }));
  }
}
