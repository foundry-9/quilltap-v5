import { ChangeDetectionStrategy, Component, computed } from '@angular/core';
import { RouterLink } from '@angular/router';
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
 * One profile, a nullable-string scalar sent alone (NOT inside a bag):
 * `imageDescriptionProfileId`, tried for every attached image. The select
 * filters the connection profiles to `supportsImageUpload === true`.
 *
 * The uncensored fallback (used when this profile refuses) left this card with
 * v4 #76 (`3b463d6b1`): it is the Concierge's vision profile now, and the card
 * links there instead. The retired `uncensoredImageDescriptionProfileId` must
 * never be sent — the server answers 400 to any PUT carrying it (P4.D230).
 * The engine side is already ported; this is the picker.
 *
 * The `[selected]`-per-option binding is BINDING here (the dogfood-#6 audit
 * rule): the options load async, and a `[value]` on the select would blank the
 * stored id on the render that lands before the profiles do.
 */
@Component({
  selector: 'qt-image-description-settings',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ErrorAlert, RouterLink, SettingsCard],
  template: `
    @if (loading()) {
      <p class="qt-text-small qt-text-muted">Loading image-description settings…</p>
    } @else {
      <qt-settings-card
        title="Image Description Profiles"
        subtitle="When you attach an image to a chat with a provider that doesn't support images (like Ollama, OpenRouter, etc.), this profile describes it in text."
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

          <p class="qt-text-xs">
            The uncensored fallback, for when this profile refuses to describe an image, now keeps
            company with the Concierge: see its vision profile under
            <a
              routerLink="/settings"
              [queryParams]="{ tab: 'concierge', section: 'uncensored-desk' }"
              class="qt-link"
              >The Concierge → The Uncensored Desk</a
            >.
          </p>
        </div>
      </qt-settings-card>
    }
  `,
})
export class ImageDescriptionSettings extends ChatSettingsCard {
  private readonly profilesQuery = injectQuery(() => ({
    queryKey: ['connection-profiles'],
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
}
