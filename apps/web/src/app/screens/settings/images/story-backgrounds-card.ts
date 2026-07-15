import { ChangeDetectionStrategy, Component, computed } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import type { ImageProfileDto } from '../../../core/core-contract';
import { ErrorAlert } from '../../../ui/error-alert';
import { ChatSettingsCard } from '../chat/chat-settings.api';
import { SettingsCard } from '../chat/settings-card';
import { fetchImageProfiles, imageProfileKeys } from './image-profiles.api';

/**
 * The `storyBackgroundsSettings` bag (v4 `StoryBackgroundsSettingsSchema`,
 * `lib/schemas/settings.types.ts:415-420`): `enabled` defaults false;
 * `defaultImageProfileId` is a nullable-optional UUID.
 *
 * Defined here rather than in the Chat tab's `chat-settings.types.ts` because
 * this card is the only consumer and the Images tab is where it mounts.
 */
export interface StoryBackgroundsSettings {
  enabled: boolean;
  defaultImageProfileId: string | null;
}

export const DEFAULT_STORY_BACKGROUNDS_SETTINGS: StoryBackgroundsSettings = {
  enabled: false,
  defaultImageProfileId: null,
};

/**
 * The Story Backgrounds card (v4 `components/settings/chat-settings/
 * StoryBackgroundsSettings.tsx`). Two controls: the enable toggle and the image
 * profile picker.
 *
 * **It lives in the IMAGES tab, not the Chat tab** — despite sitting in v4's
 * `chat-settings/` directory next to the Chat-tab cards, v4 mounts it from
 * `ImagesTabContent.tsx:39-46`. It reads and writes the same `chat_settings` row
 * through the shared `ChatSettingsCard` substrate, so it joins the one deduped
 * settings GET the P4.6an cards already share.
 *
 * The profile `<select>` binds `[selected]` per option rather than `[value]` on
 * the select (the standing dogfood-#6 rule): the options load async, and a
 * `[value]` would blank the stored id on the render that lands before the
 * profiles arrive.
 */
@Component({
  selector: 'qt-story-backgrounds-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ErrorAlert, SettingsCard],
  template: `
    @if (loading()) {
      <p class="qt-text-small qt-text-muted">Loading settings…</p>
    } @else {
      <qt-settings-card
        title="Story Backgrounds"
        subtitle="AI-generated background images for your chats"
      >
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
                class="mt-1 h-4 w-4 rounded border-input text-primary focus:ring-primary"
                [checked]="current().enabled"
                [disabled]="saving()"
                (change)="onEnabledChange($any($event.target).checked)"
              />
              <div class="flex-1">
                <div class="font-medium text-foreground">Enable Story Backgrounds</div>
                <div class="qt-text-small mt-1">
                  Automatically generate atmospheric background images for your chats based on the
                  story content and characters. Backgrounds are generated when the chat title is
                  updated.
                </div>
              </div>
            </label>
          </div>

          <div class="space-y-2">
            <label for="story-bg-profile" class="block font-medium text-foreground">
              Image Generation Profile
            </label>
            <p class="qt-text-small">
              Choose which image generation profile to use for creating story backgrounds. If not
              set, will use the character&rsquo;s image profile or your default profile.
            </p>
            <select
              id="story-bg-profile"
              class="w-full max-w-md rounded-lg border qt-border-default qt-bg-card px-3 py-2 text-foreground focus:qt-border-primary focus:outline-none focus:ring-1 focus:ring-primary disabled:opacity-50"
              [disabled]="saving() || loadingProfiles() || !current().enabled"
              (change)="onProfileChange($any($event.target).value)"
            >
              <option value="" [selected]="!selectedProfileId()">Use default profile</option>
              @for (profile of profiles(); track profile.id) {
                <option [value]="profile.id" [selected]="selectedProfileId() === profile.id">
                  {{ optionLabel(profile) }}
                </option>
              }
            </select>
            @if (profiles().length === 0 && !loadingProfiles()) {
              <p class="qt-text-small qt-text-warning">
                No image profiles configured. Please create an image generation profile in the
                Profiles settings.
              </p>
            }
          </div>

          <div class="rounded-lg border qt-border-default qt-bg-muted/50 p-4">
            <h4 class="font-medium text-foreground mb-2">How Story Backgrounds Work</h4>
            <ul class="qt-text-small space-y-1 list-disc list-inside">
              <li>Backgrounds are generated automatically when the chat title is updated</li>
              <li>
                The AI creates atmospheric landscape scenes featuring your characters as small
                figures
              </li>
              <li>Scenes are based on the chat title and character descriptions</li>
              <li>Generated images are saved to your file storage and linked to the chat</li>
              <li>Projects can inherit the latest chat background or use their own</li>
            </ul>
          </div>
        </div>
      </qt-settings-card>
    }
  `,
})
export class StoryBackgroundsCard extends ChatSettingsCard {
  private readonly profilesQuery = injectQuery(() => ({
    queryKey: imageProfileKeys.list(),
    queryFn: () => fetchImageProfiles(this.core),
  }));

  protected readonly loadingProfiles = computed(() => this.profilesQuery.isPending());
  protected readonly profiles = computed<ImageProfileDto[]>(() => this.profilesQuery.data() ?? []);

  /** v4 `settings.storyBackgroundsSettings ?? {enabled: false, defaultImageProfileId: null}`. */
  protected readonly current = computed<StoryBackgroundsSettings>(
    () =>
      (this.settings()?.['storyBackgroundsSettings'] as StoryBackgroundsSettings | undefined) ??
      DEFAULT_STORY_BACKGROUNDS_SETTINGS,
  );

  protected readonly selectedProfileId = computed(() => this.current().defaultImageProfileId ?? '');

  /** v4 `{profile.name} ({profile.provider} - {profile.modelName})` — a hyphen, not a bullet. */
  protected optionLabel(profile: ImageProfileDto): string {
    return `${profile.name} (${profile.provider} - ${profile.modelName})`;
  }

  /**
   * v4 `handleStoryBackgroundsEnabledChange` — the WHOLE bag is PUT with the one
   * key replaced. Never a nested patch: the server replaces the JSON column
   * wholesale, so a partial bag would drop the sibling key (the standing
   * nested-bag rule).
   */
  protected async onEnabledChange(enabled: boolean): Promise<void> {
    await this.save(
      { storyBackgroundsSettings: { ...this.current(), enabled } },
      'Failed to update story backgrounds settings',
    );
  }

  /** v4 `onProfileChange(e.target.value || null)` — riding the whole bag. */
  protected async onProfileChange(raw: string): Promise<void> {
    await this.save(
      { storyBackgroundsSettings: { ...this.current(), defaultImageProfileId: raw || null } },
      'Failed to update story backgrounds profile',
    );
  }
}
