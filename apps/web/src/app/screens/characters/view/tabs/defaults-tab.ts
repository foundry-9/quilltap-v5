import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
import { RouterLink } from '@angular/router';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../../core/core-client';
import type {
  CharacterConnectionProfile,
  CharacterDetail,
  CharacterListItem,
  ImageProfileDto,
} from '../../../../core/core-contract';
import { fetchImageProfiles, imageProfileKeys } from '../../../settings/images/image-profiles.api';
import { characterKeys } from '../../characters.api';

/** v4 `lib/constants/character.ts` — the virtual "User Acts As Character" profile id. */
const USER_CONTROLLED_PROFILE_ID = '__user_controlled__';

type TriState = boolean | null;

interface SavingState {
  connectionProfile: boolean;
  partner: boolean;
  systemPrompt: boolean;
  scenario: boolean;
  agentMode: boolean;
  helpTools: boolean;
  canDressThemselves: boolean;
  canCreateOutfits: boolean;
  timestampConfig: boolean;
  imageProfile: boolean;
}

/** v4 `components/settings/chat-settings/types.ts` `TIMESTAMP_MODES` (labels/copy verbatim). */
const TIMESTAMP_MODES: Array<{ value: string; label: string }> = [
  { value: 'NONE', label: 'Disabled' },
  { value: 'START_ONLY', label: 'Conversation Start' },
  { value: 'EVERY_MESSAGE', label: 'Every Message' },
  { value: 'EVERY_N_MINUTES', label: 'Every X Minutes' },
];

const TIMESTAMP_FORMATS: Array<{ value: string; label: string }> = [
  { value: 'FRIENDLY', label: 'Friendly' },
  { value: 'ISO8601', label: 'ISO 8601' },
  { value: 'DATE_ONLY', label: 'Date Only' },
  { value: 'TIME_ONLY', label: 'Time Only' },
];

/**
 * The Default Settings tab (v4 `components/ProfilesTab.tsx` +
 * `useCharacterView.ts`): per-control autosave (no Save button) over the
 * `characterUpdate` PUT — except the default partner, which rides the
 * dedicated `characterSetDefaultPartner` op. Every save invalidates the
 * shared character-detail query so the rest of the screen resyncs. The
 * image-generation-profile picker fetches the P4.6p image-profiles listing and
 * binds the character's `defaultImageProfileId` (v4's shared `ImageProfilePicker`
 * rendered as a plain `[selected]`-per-option select this round).
 *
 * The timestamp-config card is a SIMPLIFIED port of v4's
 * `TimestampConfigCard` — mode, format, and the `EVERY_N_MINUTES` interval
 * only; the timezone override, custom-format string, auto-prepend toggle, and
 * fictional-time simulation are omitted (noted in the P4.6g report).
 */
@Component({
  selector: 'qt-character-defaults-tab',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink],
  template: `
    <div class="space-y-8">
      @if (error()) {
        <div class="qt-alert-error">{{ error() }}</div>
      }

      <!-- Connection Profile -->
      <div class="character-section-card rounded-lg border qt-border-default qt-bg-card p-6">
        <h2 class="qt-heading-4 text-foreground mb-2">Default Connection Profile</h2>
        <p class="qt-text-small mb-4">
          The default AI provider and model to use when chatting with this character. Can be
          overridden per chat.
        </p>
        <div class="flex items-center gap-3">
          <!-- [selected] per option, not [value] on the select: the profiles
               list loads async, and a select-level value binding fires before
               the options exist, silently resetting to "" (finding #6). -->
          <select
            class="qt-select flex-1"
            [disabled]="saving().connectionProfile"
            (change)="onConnectionProfileChange($any($event.target).value)"
          >
            <option value="" [selected]="connectionProfileValue() === ''">
              No default profile
            </option>
            <option
              [value]="userControlledProfileId"
              [selected]="connectionProfileValue() === userControlledProfileId"
            >
              User Acts As Character
            </option>
            @for (profile of connectionProfiles(); track profile.id) {
              <option [value]="profile.id" [selected]="connectionProfileValue() === profile.id">
                {{ profile.name }}
              </option>
            }
          </select>
          @if (saving().connectionProfile) {
            <span class="qt-text-small">Saving…</span>
          }
        </div>
        @if (connectionProfiles().length === 0) {
          <p class="mt-2 text-sm qt-text-warning">
            No connection profiles available.
            <a
              routerLink="/settings"
              [queryParams]="{ tab: 'providers' }"
              class="underline hover:no-underline"
              >Create one in AI Providers</a
            >.
          </p>
        }
      </div>

      <!-- Default Partner -->
      <div
        [class]="
          'character-section-card rounded-lg border qt-border-default qt-bg-card p-6 ' +
          (isUserControlled() ? 'opacity-50' : '')
        "
      >
        <h2 class="qt-heading-4 text-foreground mb-2">Default Conversation Partner</h2>
        <p class="qt-text-small mb-4">
          {{
            isUserControlled()
              ? 'Not applicable when this character is user-controlled.'
              : 'The default user-controlled character to represent you when chatting with this character.'
          }}
        </p>
        <div class="flex items-center gap-3">
          <select
            class="qt-select flex-1"
            [disabled]="saving().partner || isUserControlled()"
            (change)="onPartnerChange($any($event.target).value)"
          >
            <option value="" [selected]="(defaultPartnerId() ?? '') === ''">
              No default partner
            </option>
            @for (char of otherUserControlled(); track char.id) {
              <option [value]="char.id" [selected]="defaultPartnerId() === char.id">
                {{ char.name }}{{ char.title ? ' - ' + char.title : '' }}
              </option>
            }
          </select>
          @if (saving().partner) {
            <span class="qt-text-small">Saving…</span>
          }
        </div>
      </div>

      <!-- Image Profile -->
      <div class="character-section-card rounded-lg border qt-border-default qt-bg-card p-6">
        <h2 class="qt-heading-4 text-foreground mb-2">Image Generation Profile</h2>
        <p class="qt-text-small mb-4">
          The default image generation profile for creating images during chats. Optional.
        </p>
        <div class="flex items-center gap-3">
          <!-- [selected] per option (async options load after first render). -->
          <select
            class="qt-select flex-1"
            [disabled]="saving().imageProfile"
            (change)="onImageProfileChange($any($event.target).value)"
          >
            <option value="" [selected]="imageProfileValue() === ''">No image generation</option>
            @for (profile of imageProfiles(); track profile.id) {
              <option [value]="profile.id" [selected]="imageProfileValue() === profile.id">
                {{ profileLabel(profile) }}
              </option>
            }
          </select>
          @if (saving().imageProfile) {
            <span class="qt-text-small">Saving…</span>
          }
        </div>
      </div>

      <!-- Default System Prompt -->
      @if (character().systemPrompts.length > 1) {
        <div class="character-section-card rounded-lg border qt-border-default qt-bg-card p-6">
          <h2 class="qt-heading-4 text-foreground mb-2">Default System Prompt</h2>
          <p class="qt-text-small mb-4">
            The system prompt to use by default when starting new chats with this character. Can be
            overridden per chat.
          </p>
          <div class="flex items-center gap-3">
            <select
              class="qt-select flex-1"
              [disabled]="saving().systemPrompt"
              (change)="onDefaultSystemPromptChange($any($event.target).value)"
            >
              <option value="" [selected]="(character().defaultSystemPromptId ?? '') === ''">
                Use first prompt marked as default
              </option>
              @for (prompt of character().systemPrompts; track prompt.id) {
                <option
                  [value]="prompt.id"
                  [selected]="character().defaultSystemPromptId === prompt.id"
                >
                  {{ prompt.name }}{{ prompt.isDefault ? ' (current default)' : '' }}
                </option>
              }
            </select>
            @if (saving().systemPrompt) {
              <span class="qt-text-small">Saving…</span>
            }
          </div>
        </div>
      }

      <!-- Default Scenario -->
      @if (character().scenarios.length > 1) {
        <div class="character-section-card rounded-lg border qt-border-default qt-bg-card p-6">
          <h2 class="qt-heading-4 text-foreground mb-2">Default Scenario</h2>
          <p class="qt-text-small mb-4">
            The scenario to pre-select by default when starting new chats with this character. Can
            be overridden per chat.
          </p>
          <div class="flex items-center gap-3">
            <select
              class="qt-select flex-1"
              [disabled]="saving().scenario"
              (change)="onDefaultScenarioChange($any($event.target).value)"
            >
              <option value="" [selected]="(character().defaultScenarioId ?? '') === ''">
                No default scenario
              </option>
              @for (scenario of character().scenarios; track scenario.id) {
                <option
                  [value]="scenario.id"
                  [selected]="character().defaultScenarioId === scenario.id"
                >
                  {{ scenario.title }}
                </option>
              }
            </select>
            @if (saving().scenario) {
              <span class="qt-text-small">Saving…</span>
            }
          </div>
        </div>
      }

      <!-- Agent Mode -->
      <div class="character-section-card rounded-lg border qt-border-default qt-bg-card p-6">
        <h2 class="qt-heading-4 text-foreground mb-2">Agent Mode</h2>
        <p class="qt-text-small mb-4">
          Control whether agent mode is enabled by default for chats with this character. Agent mode
          allows the AI to iteratively use tools, verify results, and self-correct before delivering
          a final response.
        </p>
        <div class="flex items-center gap-3">
          <select
            class="qt-select flex-1 max-w-xs"
            [disabled]="saving().agentMode"
            [value]="triValue(character().defaultAgentModeEnabled)"
            (change)="onAgentModeChange($any($event.target).value)"
          >
            <option value="inherit">Inherit from global settings</option>
            <option value="enabled">Enabled by default</option>
            <option value="disabled">Disabled by default</option>
          </select>
          @if (saving().agentMode) {
            <span class="qt-text-small">Saving…</span>
          }
        </div>
      </div>

      <!-- Help Tools -->
      <div class="character-section-card rounded-lg border qt-border-default qt-bg-card p-6">
        <h2 class="qt-heading-4 text-foreground mb-2">Help Tools</h2>
        <p class="qt-text-small mb-4">
          Control whether help tools are available for this character. When enabled, the character
          can search Quilltap documentation and read instance settings to assist users with
          configuration.
        </p>
        <div class="flex items-center gap-3">
          <select
            class="qt-select flex-1 max-w-xs"
            [disabled]="saving().helpTools"
            [value]="triValue(character().defaultHelpToolsEnabled)"
            (change)="onHelpToolsChange($any($event.target).value)"
          >
            <option value="inherit">Inherit from global settings (disabled)</option>
            <option value="enabled">Enabled</option>
            <option value="disabled">Disabled</option>
          </select>
          @if (saving().helpTools) {
            <span class="qt-text-small">Saving…</span>
          }
        </div>
      </div>

      <!-- Wardrobe (self-dressing / outfit creation) -->
      <div class="character-section-card rounded-lg border qt-border-default qt-bg-card p-6">
        <h2 class="qt-heading-4 text-foreground mb-2">Wardrobe</h2>

        <div class="mb-6">
          <h3 class="qt-text-label mb-1">Self-Dressing</h3>
          <p class="qt-text-small mb-3">
            Control whether this character can change their own outfit during conversations using
            wardrobe tools.
          </p>
          <div class="flex items-center gap-3">
            <select
              class="qt-select flex-1 max-w-xs"
              [disabled]="saving().canDressThemselves"
              [value]="triValue(character().canDressThemselves)"
              (change)="onCanDressThemselvesChange($any($event.target).value)"
            >
              <option value="inherit">Inherit from global settings (enabled)</option>
              <option value="enabled">Enabled</option>
              <option value="disabled">Disabled</option>
            </select>
            @if (saving().canDressThemselves) {
              <span class="qt-text-small">Saving…</span>
            }
          </div>
        </div>

        <div>
          <h3 class="qt-text-label mb-1">Outfit Creation</h3>
          <p class="qt-text-small mb-3">
            Control whether this character can create new wardrobe items mid-conversation. Requires
            tool use.
          </p>
          <div class="flex items-center gap-3">
            <select
              class="qt-select flex-1 max-w-xs"
              [disabled]="saving().canCreateOutfits"
              [value]="triValue(character().canCreateOutfits)"
              (change)="onCanCreateOutfitsChange($any($event.target).value)"
            >
              <option value="inherit">Inherit from global settings (enabled)</option>
              <option value="enabled">Enabled</option>
              <option value="disabled">Disabled</option>
            </select>
            @if (saving().canCreateOutfits) {
              <span class="qt-text-small">Saving…</span>
            }
          </div>
        </div>
      </div>

      <!-- Default Timestamp Settings (simplified) -->
      <div class="character-section-card rounded-lg border qt-border-default qt-bg-card p-6">
        <h2 class="qt-heading-4 text-foreground mb-2">Default Timestamp Settings</h2>
        <p class="qt-text-small mb-4">
          Default timestamp injection settings for new chats with this character. When this
          character is the only participant in a new chat, these settings will be pre-filled in the
          chat creation dialog.
        </p>
        <div class="space-y-3">
          @for (mode of timestampModes; track mode.value) {
            <label
              class="flex items-start gap-3 p-3 border qt-border-default rounded qt-hover-accent cursor-pointer"
            >
              <input
                type="radio"
                name="defaultsTimestampMode"
                class="mt-1"
                [checked]="timestampMode() === mode.value"
                [disabled]="saving().timestampConfig"
                (change)="onTimestampModeChange(mode.value)"
              />
              <span class="font-medium qt-text-primary">{{ mode.label }}</span>
            </label>
          }
        </div>
        @if (timestampMode() === 'EVERY_N_MINUTES') {
          <div class="mt-3">
            <label class="block qt-text-label mb-2">Interval (minutes)</label>
            <input
              type="number"
              min="1"
              step="1"
              class="qt-input w-32"
              [value]="timestampIntervalMinutes()"
              [disabled]="saving().timestampConfig"
              (change)="onTimestampIntervalChange($any($event.target).value)"
            />
          </div>
        }
        @if (timestampMode() !== 'NONE') {
          <div class="mt-3">
            <label class="block qt-text-label mb-2">Timestamp Format</label>
            <select
              class="qt-select w-full"
              [disabled]="saving().timestampConfig"
              [value]="timestampFormat()"
              (change)="onTimestampFormatChange($any($event.target).value)"
            >
              @for (format of timestampFormats; track format.value) {
                <option [value]="format.value">{{ format.label }}</option>
              }
            </select>
          </div>
        }
        @if (saving().timestampConfig) {
          <p class="mt-2 qt-text-small">Saving…</p>
        }
      </div>
    </div>
  `,
})
export class CharacterDefaultsTab {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();

  protected readonly userControlledProfileId = USER_CONTROLLED_PROFILE_ID;
  protected readonly timestampModes = TIMESTAMP_MODES;
  protected readonly timestampFormats = TIMESTAMP_FORMATS;

  readonly characterId = input.required<string>();
  readonly character = input.required<CharacterDetail>();
  readonly connectionProfiles = input<CharacterConnectionProfile[]>([]);
  readonly userControlledCharacters = input<CharacterListItem[]>([]);
  readonly defaultPartnerId = input<string | null>(null);

  protected readonly saving = signal<SavingState>({
    connectionProfile: false,
    partner: false,
    systemPrompt: false,
    scenario: false,
    agentMode: false,
    helpTools: false,
    canDressThemselves: false,
    canCreateOutfits: false,
    timestampConfig: false,
    imageProfile: false,
  });

  private readonly profilesQuery = injectQuery(() => ({
    queryKey: imageProfileKeys.list(),
    queryFn: (): Promise<ImageProfileDto[]> => fetchImageProfiles(this.core),
  }));

  protected readonly imageProfiles = computed(() => this.profilesQuery.data() ?? []);
  protected readonly imageProfileValue = computed(
    () => this.character().defaultImageProfileId ?? '',
  );

  /** v4 shared `ImageProfilePicker` option label: name (model) + default / no-key suffixes. */
  protected profileLabel(profile: ImageProfileDto): string {
    let label = `${profile.name} (${profile.modelName})`;
    if (profile.isDefault) {
      label += ' [Default]';
    }
    if (!profile.apiKey) {
      label += ' ⚠️ No API Key';
    }
    return label;
  }

  /** v4 surfaces every failed save via `showErrorToast`; v5's affordance is
   *  the tab-level alert. Without this, a server-rejected save silently snaps
   *  the control back and reads as "the tab doesn't accept edits". */
  protected readonly error = signal<string | null>(null);

  protected readonly isUserControlled = computed(() => this.character().controlledBy === 'user');

  protected readonly otherUserControlled = computed(() =>
    this.userControlledCharacters().filter((c) => c.id !== this.characterId()),
  );

  protected readonly connectionProfileValue = computed(() =>
    this.isUserControlled()
      ? USER_CONTROLLED_PROFILE_ID
      : (this.character().defaultConnectionProfileId ?? ''),
  );

  private readonly timestampConfig = computed<Record<string, unknown>>(
    () =>
      this.character().defaultTimestampConfig ?? {
        mode: 'NONE',
        format: 'FRIENDLY',
        intervalMinutes: 15,
      },
  );
  protected readonly timestampMode = computed(() =>
    String(this.timestampConfig()['mode'] ?? 'NONE'),
  );
  protected readonly timestampFormat = computed(() =>
    String(this.timestampConfig()['format'] ?? 'FRIENDLY'),
  );
  protected readonly timestampIntervalMinutes = computed(() =>
    Number(this.timestampConfig()['intervalMinutes'] ?? 15),
  );

  protected triValue(v: TriState | undefined): 'inherit' | 'enabled' | 'disabled' {
    return v === null || v === undefined ? 'inherit' : v ? 'enabled' : 'disabled';
  }

  private fromTri(value: string): TriState {
    return value === 'inherit' ? null : value === 'enabled';
  }

  /** `fallback` = v4's `showErrorToast` fallback microcopy for this control. */
  private async save(
    field: keyof SavingState,
    body: Record<string, unknown>,
    fallback: string,
  ): Promise<void> {
    this.saving.update((s) => ({ ...s, [field]: true }));
    this.error.set(null);
    try {
      await this.core.dispatchData({
        type: 'characterUpdate',
        characterId: this.characterId(),
        character: body,
      });
    } catch (err) {
      this.error.set(err instanceof Error && err.message ? err.message : fallback);
    } finally {
      this.saving.update((s) => ({ ...s, [field]: false }));
      await this.queryClient.invalidateQueries({
        queryKey: characterKeys.detail(this.characterId()),
      });
    }
  }

  protected onConnectionProfileChange(value: string): void {
    const body =
      value === USER_CONTROLLED_PROFILE_ID
        ? { controlledBy: 'user' as const, defaultConnectionProfileId: undefined }
        : { controlledBy: 'llm' as const, defaultConnectionProfileId: value || undefined };
    void this.save('connectionProfile', body, 'Failed to update connection profile');
  }

  protected async onPartnerChange(value: string): Promise<void> {
    this.saving.update((s) => ({ ...s, partner: true }));
    this.error.set(null);
    try {
      await this.core.dispatchData({
        type: 'characterSetDefaultPartner',
        characterId: this.characterId(),
        partnerId: value || null,
      });
    } catch (err) {
      this.error.set(
        err instanceof Error && err.message ? err.message : 'Failed to update partner',
      );
    } finally {
      this.saving.update((s) => ({ ...s, partner: false }));
      await this.queryClient.invalidateQueries({
        queryKey: characterKeys.defaultPartner(this.characterId()),
      });
      await this.queryClient.invalidateQueries({
        queryKey: characterKeys.detail(this.characterId()),
      });
    }
  }

  protected onDefaultSystemPromptChange(value: string): void {
    void this.save(
      'systemPrompt',
      { defaultSystemPromptId: value || null },
      'Failed to update default system prompt',
    );
  }

  protected onImageProfileChange(value: string): void {
    void this.save(
      'imageProfile',
      { defaultImageProfileId: value || null },
      'Failed to update image profile',
    );
  }

  protected onDefaultScenarioChange(value: string): void {
    void this.save(
      'scenario',
      { defaultScenarioId: value || null },
      'Failed to update default scenario',
    );
  }

  protected onAgentModeChange(value: string): void {
    void this.save(
      'agentMode',
      { defaultAgentModeEnabled: this.fromTri(value) },
      'Failed to update agent mode',
    );
  }

  protected onHelpToolsChange(value: string): void {
    void this.save(
      'helpTools',
      { defaultHelpToolsEnabled: this.fromTri(value) },
      'Failed to update help tools',
    );
  }

  protected onCanDressThemselvesChange(value: string): void {
    void this.save(
      'canDressThemselves',
      { canDressThemselves: this.fromTri(value) },
      'Failed to update self-dressing setting',
    );
  }

  protected onCanCreateOutfitsChange(value: string): void {
    void this.save(
      'canCreateOutfits',
      { canCreateOutfits: this.fromTri(value) },
      'Failed to update outfit creation setting',
    );
  }

  protected onTimestampModeChange(mode: string): void {
    const next: Record<string, unknown> = { ...this.timestampConfig(), mode };
    if (mode === 'EVERY_N_MINUTES' && !next['intervalMinutes']) {
      next['intervalMinutes'] = 15;
    }
    this.saveTimestampConfig(next);
  }

  protected onTimestampIntervalChange(raw: string): void {
    const parsed = Number.parseInt(raw, 10);
    const intervalMinutes = Number.isFinite(parsed) && parsed >= 1 ? parsed : 1;
    this.saveTimestampConfig({ ...this.timestampConfig(), intervalMinutes });
  }

  protected onTimestampFormatChange(format: string): void {
    this.saveTimestampConfig({ ...this.timestampConfig(), format });
  }

  private saveTimestampConfig(config: Record<string, unknown>): void {
    const value = config['mode'] === 'NONE' ? null : config;
    void this.save(
      'timestampConfig',
      { defaultTimestampConfig: value },
      'Failed to update timestamp config',
    );
  }
}
