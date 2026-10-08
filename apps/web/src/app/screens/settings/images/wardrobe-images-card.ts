import { ChangeDetectionStrategy, Component, computed } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import type { ImageProfileDto, WardrobeImageSettingsDto } from '../../../core/core-contract';
import { ErrorAlert } from '../../../ui/error-alert';
import { ChatSettingsCard } from '../chat/chat-settings.api';
import { SettingsCard } from '../chat/settings-card';
import { fetchImageProfiles, imageProfileKeys } from './image-profiles.api';

/**
 * v4 `components/settings/chat-settings/types.ts:545`
 * `DEFAULT_WARDROBE_IMAGE_SETTINGS` (`b3f937076` added `generateFromTools`).
 * Also what a pre-round server's row reads as — it carries no bag (C2 §7).
 */
export const DEFAULT_WARDROBE_IMAGE_SETTINGS: WardrobeImageSettingsDto = {
  imageProfileId: null,
  generateFromTools: false,
};

/** One profile's line in the picker, with the uncensored desks marked
 *  (v4 `WardrobeImageSettings.tsx:28-32`). */
export function wardrobeImageProfileLabel(profile: ImageProfileDto): string {
  const base = `${profile.name} (${profile.provider} - ${profile.modelName})`;
  return profile.isDangerousCompatible ? `${base} (uncensored)` : base;
}

/**
 * Wardrobe Images (v4 `7c8572869` + `b3f937076`
 * `components/settings/chat-settings/WardrobeImageSettings.tsx`, read at the
 * pin `f5e953a3f`) — the designated image profile that draws pictures of
 * garments and outfits (`chatSettings.wardrobeImageSettings.imageProfileId`),
 * and whether the wardrobe tools may queue a picture of an item they create or
 * change (`generateFromTools`, off by default).
 *
 * Null means "the default image profile"; the server resolves it through
 * `resolveWardrobeImageProfile`, which deliberately ignores the Story
 * Backgrounds profile.
 *
 * The `story-backgrounds-card.ts` pattern: `extends ChatSettingsCard`, the
 * WHOLE bag PUT with one key replaced (the server replaces the JSON column
 * wholesale), `[selected]` per option (the dogfood-#6 rule), and a failed save
 * surfaced as v4's `failureMessage` (`useChatSettings.ts:660-684` — both
 * handlers pass `Failed to update wardrobe image settings`).
 */
@Component({
  selector: 'qt-wardrobe-images-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ErrorAlert, SettingsCard],
  template: `
    @if (loading()) {
      <p class="qt-text-small qt-text-muted">Loading settings…</p>
    } @else {
      <qt-settings-card title="Wardrobe Images" subtitle="Pictures of every garment and outfit">
        @if (saveError(); as msg) {
          <qt-error-alert [message]="msg" class="mb-3" />
        }

        <div class="space-y-6">
          <div>
            <label
              class="flex items-start gap-3 p-4 border qt-border-default rounded qt-hover-accent cursor-pointer"
            >
              <input
                type="checkbox"
                class="qt-checkbox mt-1"
                [checked]="current().generateFromTools ?? false"
                [disabled]="saving()"
                (change)="onGenerateFromToolsChange($any($event.target).checked)"
              />
              <div class="flex-1">
                <div class="font-medium text-foreground">Portraits from the Wardrobe Tools</div>
                <div class="qt-text-small mt-1">
                  When a character runs up a new garment with the wardrobe tools, or alters
                  one&apos;s look, send it round to the artist for a portrait. Each sitting is a
                  paid commission, which is why the door stays shut until you open it.
                </div>
              </div>
            </label>
          </div>

          <div class="space-y-2">
            <label for="wardrobe-image-profile" class="block font-medium text-foreground">
              Wardrobe Artist
            </label>
            <p class="qt-text-small">
              Which artist draws the garments. Pick a desk that will not balk at the odd corset; the
              Concierge&apos;s uncensored desk stands in if it does.
            </p>
            <select
              id="wardrobe-image-profile"
              class="qt-select w-full max-w-md disabled:opacity-50"
              [disabled]="saving() || loadingProfiles()"
              (change)="onProfileChange($any($event.target).value)"
            >
              <option value="" [selected]="!selectedProfileId()">{{ defaultOptionLabel() }}</option>
              @for (profile of profiles(); track profile.id) {
                <option [value]="profile.id" [selected]="selectedProfileId() === profile.id">
                  {{ optionLabel(profile) }}
                </option>
              }
            </select>
            @if (profiles().length === 0 && !loadingProfiles()) {
              <p class="qt-text-small qt-text-warning">
                The studio stands empty: no image profiles have been engaged. Commission one in Image
                Profiles above before any garment can sit for its portrait.
              </p>
            }
          </div>
        </div>
      </qt-settings-card>
    }
  `,
})
export class WardrobeImagesCard extends ChatSettingsCard {
  private readonly profilesQuery = injectQuery(() => ({
    queryKey: imageProfileKeys.list(),
    queryFn: () => fetchImageProfiles(this.core),
  }));

  protected readonly loadingProfiles = computed(() => this.profilesQuery.isPending());
  protected readonly profiles = computed<ImageProfileDto[]>(() => this.profilesQuery.data() ?? []);

  /** v4 `settings.wardrobeImageSettings ?? DEFAULT_WARDROBE_IMAGE_SETTINGS`. */
  protected readonly current = computed<WardrobeImageSettingsDto>(
    () => this.settings()?.wardrobeImageSettings ?? DEFAULT_WARDROBE_IMAGE_SETTINGS,
  );

  protected readonly selectedProfileId = computed(() => this.current().imageProfileId ?? '');

  /** v4 `:91-95` — the empty option names the default profile when there is one. */
  protected readonly defaultOptionLabel = computed(() => {
    const defaultProfile = this.profiles().find((p) => p.isDefault);
    return defaultProfile
      ? `The default image profile (${defaultProfile.name})`
      : 'The default image profile';
  });

  protected optionLabel(profile: ImageProfileDto): string {
    return wardrobeImageProfileLabel(profile);
  }

  /** v4 `handleWardrobeImageProfileChange` (`useChatSettings.ts:654-666`) —
   *  `value || null`, riding the whole bag. */
  protected async onProfileChange(raw: string): Promise<void> {
    await this.save(
      { wardrobeImageSettings: { ...this.current(), imageProfileId: raw || null } },
      'Failed to update wardrobe image settings',
    );
  }

  /** v4 `handleWardrobeImageGenerateFromToolsChange` (`:672-686`, `b3f937076`). */
  protected async onGenerateFromToolsChange(enabled: boolean): Promise<void> {
    await this.save(
      { wardrobeImageSettings: { ...this.current(), generateFromTools: enabled } },
      'Failed to update wardrobe image settings',
    );
  }
}
