import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
} from '@angular/core';

import { CoreClient } from '../core/core-client';
import type { ImageProfileDto } from '../core/core-contract';
import { ProviderIcon } from './provider-icon';

/** One option row (v4 `ImageProfilePicker` `ImageProfile`, `:13-21`). */
interface PickerProfile {
  id: string;
  name: string;
  provider: string;
  modelName: string;
  isDefault: boolean;
  hasApiKey: boolean;
}

/**
 * The shared image-generation-profile picker — a port of v4
 * `components/image-profiles/ImageProfilePicker.tsx`. Self-fetching over
 * `imageProfileList`, it renders v4's option labels verbatim
 * (`{name} ({modelName})` + ` [Default]` + ` ⚠️ No API Key`, `:92-96`) behind a
 * leading "No image generation" empty option (`:88`), then the selected
 * profile's detail card with its provider glyph (`:110-129`).
 *
 * **There is no auto-select of the default profile.** v4 starts `value` null and
 * `[Default]` is a label only — the user must pick. Every consumer carries that.
 *
 * This is the ONE picker implementation (P4.9b consolidated the earlier
 * `screens/new-chat/image-profile-picker.ts` into it; New Chat, the
 * `/generate-image` screen, and the standalone dialog all mount this).
 *
 * **Deferred (loud, named): `sortByUserCharacter`.** v4 also bubbles the user
 * character's matching profiles (`ImageProfilePicker.tsx:52-54` →
 * `?sortByUserCharacter=`), but v5's `imageProfileList` variant carries
 * `sortByCharacter` ONLY (`crates/quilltap-core/src/api/types.rs:787-790`);
 * widening it is a Rust change and out of this pure-SPA lane. The input is
 * therefore absent rather than accepted-and-ignored — New Chat previously bound
 * it and the field was silently dropped on the wire. Named for the next
 * image-profiles server rider.
 */
@Component({
  selector: 'qt-image-profile-picker',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ProviderIcon],
  template: `
    @if (loading()) {
      <div class="flex items-center gap-2 qt-text-secondary">
        <div
          class="h-4 w-4 animate-spin rounded-full border-2 border-ring border-r-transparent"
        ></div>
        Loading profiles...
      </div>
    } @else {
      <div class="space-y-2">
        <!-- The options load asynchronously, so the selection rides
             [selected] per option, never [value]/[ngModel] on the select
             (the standing dogfood-#6 rule). -->
        <select
          [disabled]="disabled()"
          (change)="onSelect($any($event.target).value)"
          class="w-full px-3 py-2 border qt-border-default qt-bg-card text-foreground rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-ring disabled:opacity-50"
        >
          <option value="" [selected]="!value()">No image generation</option>
          @for (p of profiles(); track p.id) {
            <option [value]="p.id" [selected]="p.id === value()">
              {{ optionLabel(p) }}
            </option>
          }
        </select>

        @if (error(); as e) {
          <p class="qt-text-destructive text-sm">{{ e }}</p>
        }

        @if (profiles().length === 0 && !error()) {
          <p class="text-sm qt-text-secondary">
            No image profiles available. Create one in Settings first.
          </p>
        }

        <!-- Selected-profile detail card (v4 :110-129) -->
        @if (selectedProfile(); as selected) {
          <div class="mt-3 p-3 rounded-md border qt-bg-info/10 qt-border-info">
            <div class="space-y-2">
              <div class="flex items-center gap-2">
                <qt-provider-icon [provider]="selected.provider" />
                <div>
                  <p class="font-medium text-sm text-foreground">{{ selected.name }}</p>
                  <p class="text-xs qt-text-secondary">{{ selected.modelName }}</p>
                </div>
              </div>
            </div>
          </div>
        }
      </div>
    }
  `,
})
export class ImageProfilePicker {
  readonly value = input<string | null>(null);
  readonly characterId = input<string | undefined>(undefined);
  readonly disabled = input(false);
  readonly changed = output<string | null>();

  private readonly core = inject(CoreClient);
  protected readonly profiles = signal<PickerProfile[]>([]);
  protected readonly loading = signal(true);
  protected readonly error = signal<string | null>(null);

  /**
   * v4 `:110-114`: the detail card renders only when a value is selected AND
   * the list is non-empty AND the value resolves to a loaded profile.
   */
  protected readonly selectedProfile = computed<PickerProfile | null>(() => {
    const id = this.value();
    if (!id) return null;
    return this.profiles().find((p) => p.id === id) ?? null;
  });

  constructor() {
    // Refetch whenever the sort-by character changes (v4's effect deps, `:69`).
    effect(() => {
      const characterId = this.characterId();
      void this.fetch(characterId);
    });
  }

  /** v4 `:92-96` — the option label, concatenated in v4's exact order. */
  protected optionLabel(p: PickerProfile): string {
    return (
      `${p.name} (${p.modelName})` +
      (p.isDefault ? ' [Default]' : '') +
      (!p.hasApiKey ? ' ⚠️ No API Key' : '')
    );
  }

  /** v4 `:84`: an empty selection reports null, not `''`. */
  protected onSelect(raw: string): void {
    this.changed.emit(raw || null);
  }

  private async fetch(characterId?: string): Promise<void> {
    this.loading.set(true);
    this.error.set(null);
    try {
      const data = await this.core.dispatchData({
        type: 'imageProfileList',
        ...(characterId ? { sortByCharacter: characterId } : {}),
      });
      const list = (data['profiles'] as ImageProfileDto[]) ?? [];
      this.profiles.set(
        list.map((p) => ({
          id: p.id,
          name: p.name,
          provider: p.provider,
          modelName: p.modelName,
          isDefault: p.isDefault,
          hasApiKey: Boolean(p.apiKey),
        })),
      );
    } catch (err) {
      // v4 `:61-63` — the failure message replaces the list.
      this.error.set(err instanceof Error ? err.message : 'Failed to fetch profiles');
      this.profiles.set([]);
    } finally {
      this.loading.set(false);
    }
  }
}
