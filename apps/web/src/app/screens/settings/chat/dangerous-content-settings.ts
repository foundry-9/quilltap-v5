import { ChangeDetectionStrategy, Component, computed } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import type { ConnectionProfileDto, ImageProfileDto } from '../../../core/core-contract';
import { ErrorAlert } from '../../../ui/error-alert';
import { fetchImageProfiles, imageProfileKeys } from '../images/image-profiles.api';
import { ChatSettingsCard } from './chat-settings.api';
import {
  DANGEROUS_DISPLAY_MODE_OPTIONS,
  DANGEROUS_MODE_OPTIONS,
  DEFAULT_DANGEROUS_CONTENT_SETTINGS,
  type CheapLLMSettings,
  type DangerousContentSettings as DangerBag,
} from './chat-settings.types';
import { SettingsCard } from './settings-card';

/**
 * The Dangerous Content card (v4 `components/settings/chat-settings/
 * DangerousContentSettings.tsx`, 359 lines — the largest of the eleven):
 * classify sensitive content and route it to uncensored-compatible providers.
 *
 * It is the only card driving TWO settings bags:
 *  - `dangerousContentSettings` — mode, threshold, the three scan toggles, the
 *    two uncensored pickers, display mode, warning badges, the custom prompt.
 *  - `cheapLLMSettings` — it hosts the image-prompt-expansion picker
 *    (`imagePromptProfileId`), which lives in the cheap-LLM bag and merges over
 *    the WHOLE stored bag (v4 `{ ...settings.cheapLLMSettings, ...updates }`).
 *
 * Everything below the mode select is gated on `mode !== 'OFF'`, and the two
 * uncensored pickers additionally on `mode === 'AUTO_ROUTE'`.
 *
 * The pickers' `[selected]`-per-option binding is BINDING (the dogfood-#6 audit
 * rule) — the profile lists load async.
 *
 * **This card is why P4.6an unit 1 exists:** picking "Auto-detect" in either
 * uncensored picker, or clearing the custom prompt, sends an explicit `null` in
 * the bag. The server's parse had been dropping those keys where v4's Zod keeps
 * them; unit 1 fixed it, with the differential to prove it.
 */
@Component({
  selector: 'qt-dangerous-content-settings',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ErrorAlert, SettingsCard],
  template: `
    @if (loading()) {
      <p class="qt-text-small qt-text-muted">Loading dangerous-content settings…</p>
    } @else {
      <qt-settings-card
        title="Dangerous Content Handling"
        subtitle="Classify and route sensitive content to uncensored-compatible providers"
      >
        @if (saveError(); as msg) {
          <qt-error-alert [message]="msg" class="mb-3" />
        }

        <div class="space-y-6">
          <!-- Mode -->
          <div class="space-y-2">
            <label for="danger-mode" class="block font-medium text-foreground">Mode</label>
            <select
              id="danger-mode"
              class="w-full max-w-xs rounded-lg border qt-border-default qt-bg-card px-3 py-2 text-foreground focus:qt-border-primary focus:outline-none focus:ring-1 focus:ring-primary"
              [disabled]="saving()"
              (change)="onUpdate({ mode: $any($event.target).value })"
            >
              @for (option of modeOptions; track option.value) {
                <option [value]="option.value" [selected]="current().mode === option.value">
                  {{ option.label }}
                </option>
              }
            </select>
            <p class="qt-text-small">{{ modeDescription() }}</p>
          </div>

          @if (isEnabled()) {
            <!-- Detection threshold -->
            <div class="space-y-2">
              <label for="danger-threshold" class="block font-medium text-foreground">
                Detection Threshold ({{ thresholdLabel() }})
              </label>
              <input
                id="danger-threshold"
                type="range"
                class="w-full max-w-xs"
                min="0.1"
                max="1.0"
                step="0.1"
                [value]="current().threshold"
                [disabled]="saving()"
                (change)="onUpdate({ threshold: parseThreshold($any($event.target).value) })"
              />
              <p class="qt-text-small">
                Lower values are more sensitive (more content flagged). Higher values only flag
                strongly dangerous content.
              </p>
            </div>

            <!-- Scan toggles -->
            <div class="space-y-3">
              <p class="block font-medium text-foreground">What to Scan</p>

              <label
                class="flex items-start gap-3 p-3 border qt-border-default rounded qt-hover-accent cursor-pointer"
              >
                <input
                  type="checkbox"
                  class="mt-1 h-4 w-4 rounded border-input text-primary focus:ring-primary"
                  [checked]="current().scanTextChat"
                  [disabled]="saving()"
                  (change)="onUpdate({ scanTextChat: $any($event.target).checked })"
                />
                <div class="flex-1">
                  <div class="text-sm text-foreground">Text Chat Messages</div>
                  <div class="qt-text-small">Classify user messages before sending to the LLM</div>
                </div>
              </label>

              <label
                class="flex items-start gap-3 p-3 border qt-border-default rounded qt-hover-accent cursor-pointer"
              >
                <input
                  type="checkbox"
                  class="mt-1 h-4 w-4 rounded border-input text-primary focus:ring-primary"
                  [checked]="current().scanImagePrompts"
                  [disabled]="saving()"
                  (change)="onUpdate({ scanImagePrompts: $any($event.target).checked })"
                />
                <div class="flex-1">
                  <div class="text-sm text-foreground">Image Prompts</div>
                  <div class="qt-text-small">Classify image generation prompts before expansion</div>
                </div>
              </label>

              <label
                class="flex items-start gap-3 p-3 border qt-border-default rounded qt-hover-accent cursor-pointer"
              >
                <input
                  type="checkbox"
                  class="mt-1 h-4 w-4 rounded border-input text-primary focus:ring-primary"
                  [checked]="current().scanImageGeneration"
                  [disabled]="saving()"
                  (change)="onUpdate({ scanImageGeneration: $any($event.target).checked })"
                />
                <div class="flex-1">
                  <div class="text-sm text-foreground">Image Generation</div>
                  <div class="qt-text-small">
                    Classify the expanded prompt before sending to the image generator
                  </div>
                </div>
              </label>
            </div>

            <!-- Uncensored providers (AUTO_ROUTE only) -->
            @if (current().mode === 'AUTO_ROUTE') {
              <div class="space-y-4">
                <p class="block font-medium text-foreground">Uncensored Providers</p>

                <div class="space-y-1">
                  <label for="danger-text-profile" class="block text-sm text-foreground">
                    Text LLM Profile
                  </label>
                  <select
                    id="danger-text-profile"
                    class="qt-select"
                    [disabled]="saving() || loadingProfiles()"
                    (change)="onUpdate({ uncensoredTextProfileId: $any($event.target).value || null })"
                  >
                    <option value="" [selected]="!current().uncensoredTextProfileId">
                      Auto-detect (scan all profiles)
                    </option>
                    @if (dangerousProfiles().length > 0) {
                      @for (profile of dangerousProfiles(); track profile.id) {
                        <option
                          [value]="profile.id"
                          [selected]="current().uncensoredTextProfileId === profile.id"
                        >
                          {{ profile.name }} ({{ profile.provider }} / {{ profile.modelName }})
                        </option>
                      }
                    } @else {
                      <option value="" disabled>No uncensored-compatible profiles found</option>
                    }
                  </select>
                  <p class="qt-text-small">
                    Select a specific profile or let the system scan for uncensored-compatible
                    profiles automatically.
                  </p>
                </div>

                <div class="space-y-1">
                  <label for="danger-image-profile" class="block text-sm text-foreground">
                    Image Generation Profile
                  </label>
                  <select
                    id="danger-image-profile"
                    class="qt-select"
                    [disabled]="saving() || loadingProfiles()"
                    (change)="
                      onUpdate({ uncensoredImageProfileId: $any($event.target).value || null })
                    "
                  >
                    <option value="" [selected]="!current().uncensoredImageProfileId">
                      Auto-detect (scan all profiles)
                    </option>
                    @if (dangerousImageProfiles().length > 0) {
                      @for (profile of dangerousImageProfiles(); track profile.id) {
                        <option
                          [value]="profile.id"
                          [selected]="current().uncensoredImageProfileId === profile.id"
                        >
                          {{ profile.name }} ({{ profile.provider }})
                        </option>
                      }
                    } @else {
                      <option value="" disabled>
                        No uncensored-compatible image profiles found
                      </option>
                    }
                  </select>
                  <p class="qt-text-small">
                    Select a specific image profile or let the system scan for
                    uncensored-compatible profiles automatically.
                  </p>
                </div>
              </div>
            }

            <!-- Display settings -->
            <div class="space-y-3">
              <p class="block font-medium text-foreground">Display Settings</p>

              <div class="space-y-2">
                <label for="danger-display-mode" class="block text-sm text-foreground">
                  Flagged Content Display Mode
                </label>
                <select
                  id="danger-display-mode"
                  class="qt-select"
                  [disabled]="saving()"
                  (change)="onUpdate({ displayMode: $any($event.target).value })"
                >
                  @for (option of displayModeOptions; track option.value) {
                    <option
                      [value]="option.value"
                      [selected]="current().displayMode === option.value"
                    >
                      {{ option.label }}
                    </option>
                  }
                </select>
                <p class="qt-text-small">{{ displayModeDescription() }}</p>
              </div>

              <label
                class="flex items-start gap-3 p-3 border qt-border-default rounded qt-hover-accent cursor-pointer"
              >
                <input
                  type="checkbox"
                  class="mt-1 h-4 w-4 rounded border-input text-primary focus:ring-primary"
                  [checked]="current().showWarningBadges"
                  [disabled]="saving()"
                  (change)="onUpdate({ showWarningBadges: $any($event.target).checked })"
                />
                <div class="flex-1">
                  <div class="text-sm text-foreground">Show Warning Badges</div>
                  <div class="qt-text-small">Display category badges on flagged messages</div>
                </div>
              </label>
            </div>

            <!-- Custom classification prompt -->
            <div class="space-y-2">
              <label for="danger-custom-prompt" class="block font-medium text-foreground">
                Custom Classification Prompt (Optional)
              </label>
              <textarea
                id="danger-custom-prompt"
                class="qt-input"
                rows="3"
                placeholder="Additional instructions for the content classifier..."
                [value]="current().customClassificationPrompt || ''"
                [disabled]="saving()"
                (change)="
                  onUpdate({ customClassificationPrompt: $any($event.target).value || null })
                "
              ></textarea>
              <p class="qt-text-small">
                Additional instructions appended to the classification prompt. Use this to adjust
                sensitivity for your use case.
              </p>
            </div>

            <!-- Image prompt expansion LLM (writes the cheap-LLM bag) -->
            <div class="space-y-2">
              <label for="danger-image-prompt-profile" class="block font-medium text-foreground">
                Image Prompt Expansion LLM (Uncensored - Optional)
              </label>
              <p class="qt-text-small">
                When an image prompt is flagged as dangerous, this profile is used for prompt
                expansion instead of the standard cheap LLM. Select an uncensored-compatible model
                that can handle sensitive content.
              </p>
              <select
                id="danger-image-prompt-profile"
                class="w-full max-w-md rounded-lg border qt-border-default qt-bg-card px-3 py-2 text-foreground focus:qt-border-primary focus:outline-none focus:ring-1 focus:ring-primary"
                [disabled]="saving() || loadingProfiles()"
                (change)="onImagePromptProfileChange($any($event.target).value)"
              >
                <option value="" [selected]="!imagePromptProfileId()">Use global cheap LLM</option>
                @for (profile of profiles(); track profile.id) {
                  <option
                    [value]="profile.id"
                    [selected]="imagePromptProfileId() === profile.id"
                  >
                    {{ optionLabel(profile) }}
                  </option>
                }
              </select>
              @if (profiles().length === 0 && !loadingProfiles()) {
                <p class="mt-1 qt-text-small qt-text-warning">
                  No connection profiles found. Create one in the Connection Profiles tab first.
                </p>
              }
            </div>
          }

          <!-- Warning box (shown in every mode) -->
          <div class="qt-alert-warning p-4">
            <h4 class="font-medium text-foreground mb-2">Important Notes</h4>
            <ul class="qt-text-small space-y-1 list-disc list-inside">
              <li>
                If you have an OpenAI API key configured, the free OpenAI moderation endpoint is
                used automatically for classification (no token cost)
              </li>
              <li>
                Without an OpenAI connection profile, classification falls back to your configured
                Cheap LLM, adding a small cost per message
              </li>
              <li>Classification is fail-safe: errors never block your messages</li>
              <li>
                To use Auto-Route, mark at least one connection profile as
                &quot;Uncensored-Compatible&quot; in the Connection Profiles settings
              </li>
              <li>
                If no uncensored provider is available, flagged messages are sent to your regular
                provider with a warning
              </li>
              <li>Some providers may refuse flagged content even with rerouting</li>
            </ul>
          </div>
        </div>
      </qt-settings-card>
    }
  `,
})
export class DangerousContentSettings extends ChatSettingsCard {
  protected readonly modeOptions = DANGEROUS_MODE_OPTIONS;
  protected readonly displayModeOptions = DANGEROUS_DISPLAY_MODE_OPTIONS;

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

  private readonly imageProfilesQuery = injectQuery(() => ({
    queryKey: imageProfileKeys.list(),
    queryFn: (): Promise<ImageProfileDto[]> => fetchImageProfiles(this.core),
  }));

  protected readonly loadingProfiles = computed(
    () => this.profilesQuery.isPending() || this.imageProfilesQuery.isPending(),
  );

  protected readonly profiles = computed(() => this.profilesQuery.data() ?? []);

  /** v4 `connectionProfiles.filter(p => p.isDangerousCompatible)`. */
  protected readonly dangerousProfiles = computed(() =>
    this.profiles().filter((p) => p.isDangerousCompatible),
  );
  protected readonly dangerousImageProfiles = computed(() =>
    (this.imageProfilesQuery.data() ?? []).filter((p) => p.isDangerousCompatible),
  );

  /** v4 `settings.dangerousContentSettings ?? DEFAULT_SETTINGS`. */
  protected readonly current = computed<DangerBag>(
    () =>
      (this.settings()?.['dangerousContentSettings'] as DangerBag | undefined) ??
      DEFAULT_DANGEROUS_CONTENT_SETTINGS,
  );

  private readonly cheapLLM = computed(
    () => (this.settings()?.['cheapLLMSettings'] ?? {}) as CheapLLMSettings,
  );
  protected readonly imagePromptProfileId = computed(
    () => this.cheapLLM().imagePromptProfileId ?? '',
  );

  /** v4 `dangerSettings.mode !== 'OFF'`. */
  protected readonly isEnabled = computed(() => this.current().mode !== 'OFF');

  protected readonly modeDescription = computed(
    () => DANGEROUS_MODE_OPTIONS.find((o) => o.value === this.current().mode)?.description,
  );
  protected readonly displayModeDescription = computed(
    () =>
      DANGEROUS_DISPLAY_MODE_OPTIONS.find((o) => o.value === this.current().displayMode)
        ?.description,
  );

  /** v4 `dangerSettings.threshold.toFixed(1)`. */
  protected readonly thresholdLabel = computed(() => this.current().threshold.toFixed(1));

  protected optionLabel(profile: ConnectionProfileDto): string {
    const suffix = profile.apiKey ? '' : ' ⚠️ No API Key';
    return `${profile.name} (${profile.provider} • ${profile.modelName})${suffix}`;
  }

  /** v4 `parseFloat(e.target.value)` on the threshold range. */
  protected parseThreshold(raw: string): number {
    return Number.parseFloat(raw);
  }

  /** v4 `{ dangerousContentSettings: { ...currentSettings, ...updates } }`. */
  protected async onUpdate(updates: Partial<DangerBag>): Promise<void> {
    await this.save(
      { dangerousContentSettings: { ...this.current(), ...updates } },
      'Failed to update dangerous content settings',
    );
  }

  /** v4 `handleCheapLLMUpdate({ imagePromptProfileId: id })` — merges the WHOLE cheap-LLM bag. */
  protected async onImagePromptProfileChange(raw: string): Promise<void> {
    await this.save(
      { cheapLLMSettings: { ...this.cheapLLM(), imagePromptProfileId: raw || null } },
      'Failed to update cheap LLM settings',
    );
  }
}
