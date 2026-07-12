import { ChangeDetectionStrategy, Component, effect, inject, input, output, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';

import { CoreClient } from '../../core/core-client';
import type { ImageProfileDto } from '../../core/core-contract';

interface PickerProfile {
  id: string;
  name: string;
  modelName: string;
  isDefault: boolean;
  hasApiKey: boolean;
}

/**
 * The image-generation-profile picker (v4 `ImageProfilePicker`). Self-fetching
 * over `imageProfileList`, bubbling a character's matching profiles to the top
 * (`sortByCharacter` / `sortByUserCharacter`). The list read is lane A's
 * (P4.6p) live variant — code against the pinned `{ profiles, count }` shape;
 * until it lands, an error read simply yields the empty state.
 */
@Component({
  selector: 'qt-image-profile-picker',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [FormsModule],
  template: `
    @if (loading()) {
      <div class="flex items-center gap-2 qt-text-secondary">
        <div class="h-4 w-4 animate-spin rounded-full border-2 border-ring border-r-transparent"></div>
        Loading profiles...
      </div>
    } @else {
      <div class="space-y-2">
        <select
          [ngModel]="value() || ''"
          (ngModelChange)="changed.emit($event || null)"
          [disabled]="disabled()"
          class="w-full px-3 py-2 border qt-border-default qt-bg-card text-foreground rounded-md text-sm focus:outline-none focus:ring-2 focus:ring-ring disabled:opacity-50"
        >
          <option value="">No image generation</option>
          @for (p of profiles(); track p.id) {
            <option [value]="p.id" [selected]="p.id === (value() || '')">
              {{ p.name }} ({{ p.modelName }}){{ p.isDefault ? ' [Default]' : '' }}{{
                !p.hasApiKey ? ' ⚠️ No API Key' : ''
              }}
            </option>
          }
        </select>

        @if (error()) {
          <p class="qt-text-destructive text-sm">{{ error() }}</p>
        }
        @if (profiles().length === 0 && !error()) {
          <p class="text-sm qt-text-secondary">No image profiles available. Create one in Settings first.</p>
        }
      </div>
    }
  `,
})
export class ImageProfilePicker {
  readonly value = input<string | null>(null);
  readonly characterId = input<string | undefined>(undefined);
  readonly userCharacterId = input<string | undefined>(undefined);
  readonly disabled = input(false);
  readonly changed = output<string | null>();

  private readonly core = inject(CoreClient);
  protected readonly profiles = signal<PickerProfile[]>([]);
  protected readonly loading = signal(true);
  protected readonly error = signal<string | null>(null);

  constructor() {
    // Refetch whenever the sort-by character keys change (v4's effect deps).
    effect(() => {
      const characterId = this.characterId();
      const userCharacterId = this.userCharacterId();
      void this.fetch(characterId, userCharacterId);
    });
  }

  private async fetch(characterId?: string, userCharacterId?: string): Promise<void> {
    this.loading.set(true);
    this.error.set(null);
    try {
      const data = await this.core.dispatchData({
        type: 'imageProfileList',
        ...(characterId ? { sortByCharacter: characterId } : {}),
        ...(userCharacterId ? { sortByUserCharacter: userCharacterId } : {}),
      });
      const list = (data['profiles'] as ImageProfileDto[]) ?? [];
      this.profiles.set(
        list.map((p) => ({
          id: p.id,
          name: p.name,
          modelName: p.modelName,
          isDefault: p.isDefault,
          hasApiKey: Boolean(p.apiKey),
        })),
      );
    } catch (err) {
      this.error.set(err instanceof Error ? err.message : 'Failed to fetch profiles');
      this.profiles.set([]);
    } finally {
      this.loading.set(false);
    }
  }
}
