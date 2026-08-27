import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  input,
  output,
  signal,
} from '@angular/core';

import type {
  ChatCreateOutfitSelectionInput,
  OutfitSelectionMode,
  WardrobeSlotType,
} from '../../core/core-contract';
import { WARDROBE_SLOT_META, WARDROBE_SLOT_TYPES } from '../../wardrobe/slot-meta';

/** One character eligible for a starting-outfit choice (v4 `OutfitSelectorCharacter`). */
export interface OutfitSelectorCharacter {
  id: string;
  name: string;
  isUserControlled?: boolean;
  /**
   * The character's `canChooseOutfit` vault flag (v4 `8bf3cb5f`). When true —
   * and the character is LLM-controlled — a fresh chat defaults this character's
   * Starting Outfit to `Let character choose` rather than `Use defaults`
   * ({@link computeSyncInitialMode}).
   */
  canChooseOutfit?: boolean;
}

/**
 * The Starting Outfit mode a character opens with, computed from what we can
 * know synchronously (v4 `8bf3cb5f` `computeSyncInitialMode`). An LLM-controlled
 * character flagged `canChooseOutfit` defaults to letting the character choose;
 * everyone else opens on `default`.
 *
 * v4 additionally seeds continuation chats to `previous_chat` and then refines a
 * provisional `default` to `default`-vs-`manual` once each character's wardrobe
 * loads (no usable default outfit → `manual`, expanded), with collapsed-header
 * mode badges. Those pieces ride v5's DEFERRED wardrobe-composer family — this
 * new-chat selector has no continuation source, does not load wardrobes, and
 * renders `manual` (Compose) loudly disabled — so they are not ported here; the
 * disabled Compose radio (with its "not yet available" title) is that
 * deferral's visible surface. See the work-order status header.
 *
 * ⚠ 2026-08-27, P4.D130: v4 `aec86a613` put a `Wear an outfit…` pull-down above
 * the slot rows in `OutfitComposer`, which the chat-start Manual mode hosts as
 * well as the wardrobe dialog. v5 ported the pull-down into the composer, so it
 * arrives here FOR FREE the moment Compose is enabled — but the mode is
 * disabled, so nothing renders it today. Whoever lands the wardrobe-composer
 * family owns proving it in this host: the pull-down is part of that scope, not
 * a separate follow-up.
 */
export function computeSyncInitialMode(
  char: OutfitSelectorCharacter,
  sourceChatId?: string | null,
): OutfitSelectionMode {
  if (sourceChatId) return 'previous_chat';
  if (char.canChooseOutfit && !char.isUserControlled) return 'llm_choose';
  return 'default';
}

/**
 * v4's per-character equipped-outfit summary from a source chat
 * (`outfit-selector.tsx:102`): each slot holds the items resolved at the end of
 * that conversation.
 */
export type PreviousOutfitSummary = Record<
  string,
  Partial<Record<WardrobeSlotType, { itemId: string; title: string }[]>>
>;

/** v4 `previousChatPreview` (`outfit-selector.tsx:372-390`). v4's own
 *  `SLOT_LABELS` map went with `4423ad10` — labels come from the registry. */
export function previousChatPreview(
  slots: PreviousOutfitSummary[string] | null | undefined,
): string | null {
  if (!slots) return null;
  const equipped = WARDROBE_SLOT_TYPES.map((slot) => ({
    slot,
    titles: (slots[slot] ?? []).map((i) => i.title),
  })).filter((entry) => entry.titles.length > 0);
  if (equipped.length === 0) {
    return 'Nothing equipped at the end of the source chat — defaults will be used.';
  }
  return equipped
    .map((e) => `${WARDROBE_SLOT_META[e.slot].label}: ${e.titles.join(', ')}`)
    .join(' · ');
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
 * for the user's persona) / `none`. Each character's initial mode is seeded
 * synchronously by {@link computeSyncInitialMode} — an LLM character flagged
 * `canChooseOutfit` (v4 `8bf3cb5f`) opens on `llm_choose`, everyone else on
 * `default`. `manual` (Compose outfit) renders loudly disabled-with-title — the
 * wardrobe-composer family is deferred; `previous_chat` (continuation only) is
 * not rendered this round. Emits `{ characterId, mode }` (no slots for the
 * non-manual modes).
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
            @if (previewFor(char.id); as preview) {
              <div
                class="mt-2 ml-6 rounded border qt-border-default qt-bg-muted/40 px-2 py-1.5 text-xs qt-text-secondary"
              >
                {{ preview }}
              </div>
            }
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
  /**
   * Continuation mode (v4 `sourceChatId` + `previousOutfitSummary`): when set,
   * a "Same as last conversation" option leads the list and every character
   * opens on it. Used by the Merge In… flow (P4.9E3C); the new-chat form leaves
   * both unset and behaves exactly as before.
   */
  readonly sourceChatId = input<string | null>(null);
  readonly previousOutfitSummary = input<PreviousOutfitSummary | null>(null);
  readonly selectionsChange = output<ChatCreateOutfitSelectionInput[]>();

  private readonly overrides = signal<Record<string, OutfitSelectionMode>>({});

  protected readonly showHeaders = computed(() => this.characters().length > 1);

  private readonly selections = computed<ChatCreateOutfitSelectionInput[]>(() =>
    this.characters().map((c) => ({
      characterId: c.id,
      mode: this.overrides()[c.id] ?? computeSyncInitialMode(c, this.sourceChatId()),
    })),
  );

  constructor() {
    // Mirror v4's "notify parent when selections change" effect.
    effect(() => this.selectionsChange.emit(this.selections()));
  }

  protected modeFor(id: string): OutfitSelectionMode {
    const override = this.overrides()[id];
    if (override) return override;
    const char = this.characters().find((c) => c.id === id);
    return char ? computeSyncInitialMode(char, this.sourceChatId()) : 'default';
  }

  /** The continuation preview line, shown while `previous_chat` is chosen. */
  protected previewFor(id: string): string | null {
    if (this.modeFor(id) !== 'previous_chat') return null;
    return previousChatPreview(this.previousOutfitSummary()?.[id] ?? null);
  }

  protected optionsFor(char: OutfitSelectorCharacter): ModeOption[] {
    const opts: ModeOption[] = [];
    if (this.sourceChatId()) {
      opts.push({
        value: 'previous_chat',
        label: 'Same as last conversation',
        description:
          'Carry forward whatever they were wearing at the end of the source chat',
      });
    }
    opts.push(
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
    );
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
