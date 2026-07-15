import { ChangeDetectionStrategy, Component, computed } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import type { ConnectionProfileDto } from '../../../core/core-contract';
import { ErrorAlert } from '../../../ui/error-alert';
import { ChatSettingsCard } from './chat-settings.api';
import { SettingsCard } from './settings-card';

/**
 * The Image Description card (v4 `components/settings/chat-settings/
 * ImageDescriptionSettings.tsx`): which vision-capable profile describes an
 * attached image when the chat's own provider can't see it (Ollama, some
 * OpenRouter models, …).
 *
 * Two profiles, each a nullable-string scalar of its own (v4 sends them alone,
 * NOT inside a bag):
 *  - **Primary** (`imageDescriptionProfileId`) — tried for every attached image.
 *  - **Uncensored fallback** (`uncensoredImageDescriptionProfileId`) — used only
 *    when the primary refuses or returns an unusable response. Optional.
 *
 * Both selects filter the connection profiles to `supportsImageUpload === true`.
 * The engine side is already ported; this is the picker.
 *
 * The `[selected]`-per-option binding is BINDING here (the dogfood-#6 audit
 * rule): the options load async, and a `[value]` on the select would blank the
 * stored id on the render that lands before the profiles do.
 */
@Component({
  selector: 'qt-image-description-settings',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ErrorAlert, SettingsCard],
  template: `
    @if (loading()) {
      <p class="qt-text-small qt-text-muted">Loading image-description settings…</p>
    } @else {
      <qt-settings-card
        title="Image Description Profiles"
        subtitle="When you attach an image to a chat with a provider that doesn't support images (like Ollama, OpenRouter, etc.), the primary profile describes it in text. If the primary refuses or returns an unusable response, the uncensored fallback profile tries instead."
      >
        @if (saveError(); as msg) {
          <qt-error-alert [message]="msg" class="mb-3" />
        }

        <div class="space-y-4">
          <div>
            <label for="image-desc-primary" class="block qt-text-label mb-2">
              Primary image description profile
            </label>
            <p class="qt-text-xs mb-2">
              Select a vision-capable profile (like gpt-4o-mini, claude-haiku-4-5, or
              gemini-2.0-flash) to describe images. If not set, the system will automatically use
              any available vision-capable profile.
            </p>
            <select
              id="image-desc-primary"
              class="qt-select"
              [disabled]="saving() || loadingProfiles()"
              (change)="onProfileChange($any($event.target).value)"
            >
              <option value="" [selected]="!primaryId()">
                Auto-select vision-capable profile
              </option>
              @for (profile of visionProfiles(); track profile.id) {
                <option [value]="profile.id" [selected]="primaryId() === profile.id">
                  {{ optionLabel(profile) }}
                </option>
              }
            </select>
            @if (visionProfiles().length === 0) {
              <p class="mt-1 text-xs qt-text-warning">
                No vision-capable profiles found. Edit a connection profile and enable
                &ldquo;Supports image attachments&rdquo;, or create a new one.
              </p>
            }
          </div>

          <div>
            <label for="image-desc-fallback" class="block qt-text-label mb-2">
              Uncensored fallback profile
            </label>
            <p class="qt-text-xs mb-2">
              Optional. Used only when the primary profile refuses to describe an image. A more
              permissive vision model (a local Ollama llava variant, an uncensored router model,
              etc.) is the usual choice. Leave blank to skip the fallback.
            </p>
            <select
              id="image-desc-fallback"
              class="qt-select"
              [disabled]="saving() || loadingProfiles()"
              (change)="onUncensoredProfileChange($any($event.target).value)"
            >
              <option value="" [selected]="!fallbackId()">
                No fallback (recommended for benign content)
              </option>
              @for (profile of visionProfiles(); track profile.id) {
                <option [value]="profile.id" [selected]="fallbackId() === profile.id">
                  {{ optionLabel(profile) }}
                </option>
              }
            </select>
          </div>
        </div>
      </qt-settings-card>
    }
  `,
})
export class ImageDescriptionSettings extends ChatSettingsCard {
  private readonly profilesQuery = injectQuery(() => ({
    queryKey: ['connectionProfiles'],
    queryFn: async (): Promise<ConnectionProfileDto[]> => {
      const resp = await this.core.dispatchExpect(
        { type: 'connectionProfileList' },
        'connectionProfiles',
      );
      return resp.data.profiles;
    },
  }));

  protected readonly loadingProfiles = computed(() => this.profilesQuery.isPending());

  /** v4 `connectionProfiles.filter(p => p.supportsImageUpload === true)`. */
  protected readonly visionProfiles = computed(() =>
    (this.profilesQuery.data() ?? []).filter((p) => p.supportsImageUpload === true),
  );

  /** v4 `settings?.imageDescriptionProfileId || ''`. */
  protected readonly primaryId = computed(
    () => (this.settings()?.['imageDescriptionProfileId'] as string | null | undefined) ?? '',
  );
  protected readonly fallbackId = computed(
    () =>
      (this.settings()?.['uncensoredImageDescriptionProfileId'] as string | null | undefined) ?? '',
  );

  /** v4 `{profile.name} ({profile.provider} • {profile.modelName}){' ⚠️ No API Key'}`. */
  protected optionLabel(profile: ConnectionProfileDto): string {
    const suffix = profile.apiKey ? '' : ' ⚠️ No API Key';
    return `${profile.name} (${profile.provider} • ${profile.modelName})${suffix}`;
  }

  /** v4 `onProfileChange(e.target.value || null)` — a bare scalar, sent alone. */
  protected async onProfileChange(raw: string): Promise<void> {
    await this.save(
      { imageDescriptionProfileId: raw || null },
      'Failed to update image description profile',
    );
  }

  protected async onUncensoredProfileChange(raw: string): Promise<void> {
    await this.save(
      { uncensoredImageDescriptionProfileId: raw || null },
      'Failed to update uncensored image description profile',
    );
  }
}
