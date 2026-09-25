import { ChangeDetectionStrategy, Component, computed } from '@angular/core';
import { RouterLink } from '@angular/router';
import { injectQuery } from '@tanstack/angular-query-experimental';

import type { ConnectionProfileDto, ImageProfileDto } from '../../../core/core-contract';
import { ErrorAlert } from '../../../ui/error-alert';
import { fetchImageProfiles, imageProfileKeys } from '../images/image-profiles.api';
import type { ConciergeSettingsUpdate } from '../chat/chat-settings.types';
import { ConciergeSettingsCard } from './concierge-settings.api';

/** v4 `ProfileLike` — the four fields a desk option reads (both profile kinds carry them). */
export interface ProfileLike {
  id: string;
  name: string;
  provider: string;
  modelName?: string;
  apiKey?: unknown;
}

/**
 * v4 `withSelected`: a stored choice that is no longer on the compatible list
 * (its tick was removed, or it arrived from an older setting that allowed any
 * profile) is still shown, so the select never silently misreports what is
 * saved.
 */
export function withSelected<T extends ProfileLike>(
  list: T[],
  all: T[],
  selectedId: string | null | undefined,
): T[] {
  if (!selectedId || list.some((p) => p.id === selectedId)) return list;
  const selected = all.find((p) => p.id === selectedId);
  return selected ? [...list, selected] : list;
}

/** v4 `profileLabel`: `Name (PROVIDER • model)`, the bullet only when a model is named. */
export function profileLabel(profile: ProfileLike): string {
  const model = profile.modelName ? ` • ${profile.modelName}` : '';
  return `${profile.name} (${profile.provider}${model})`;
}

/** The suffix a stale pick carries (v4 `DeskSelect`). */
export const NOT_COMPATIBLE_SUFFIX = ' — not marked uncensored-compatible';

type DeskKey = 'uncensoredTextProfileId' | 'uncensoredImageProfileId' | 'uncensoredVisionProfileId';

interface DeskSelect {
  id: string;
  label: string;
  help: string;
  key: DeskKey;
  emptyOptionLabel: string;
  profiles: ProfileLike[];
}

/**
 * The uncensored desk (v4 `components/settings/concierge-settings/
 * UncensoredDeskCard.tsx`, `3b463d6b1`): which profiles the Concierge asks when
 * the usual providers refuse, plus the LLM that crafts image prompts for it.
 * Always shown, whatever the on-duty switch says, so an Unmoderated chat can
 * always name its profile.
 *
 * The three desk pickers list only uncensored-compatible profiles (vision: of
 * those, only the ones that take image attachments); the image prompt crafter
 * lists EVERY connection profile — it only writes prompts. The
 * `[selected]`-per-option binding is BINDING (the dogfood-#6 audit rule): the
 * profile lists load async.
 */
@Component({
  selector: 'qt-concierge-uncensored-desk-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [ErrorAlert, RouterLink],
  template: `
    <div class="space-y-6">
      @if (saveError(); as msg) {
        <qt-error-alert [message]="msg" />
      }

      @if (noneCompatible()) {
        <div class="qt-alert-warning">
          <p class="qt-text-small">
            No profile is marked uncensored-compatible yet, so the desk has no one to send for.
            Tick &ldquo;Uncensored-compatible&rdquo; on a connection profile in
            <a routerLink="/settings" [queryParams]="{ tab: 'providers' }" class="qt-link"
              >AI Providers</a
            >
            or on an image profile in
            <a routerLink="/settings" [queryParams]="{ tab: 'images' }" class="qt-link">Images</a
            >.
          </p>
        </div>
      }

      @for (desk of deskSelects(); track desk.id) {
        <div class="space-y-1">
          <label [for]="desk.id" class="block qt-text-label">{{ desk.label }}</label>
          <select
            [id]="desk.id"
            class="qt-select"
            [disabled]="disabled()"
            (change)="onDeskChange(desk.key, $any($event.target).value)"
          >
            <option value="" [selected]="!concierge()[desk.key]">
              {{ desk.emptyOptionLabel }}
            </option>
            @for (profile of desk.profiles; track profile.id) {
              <option [value]="profile.id" [selected]="concierge()[desk.key] === profile.id">
                {{ deskOptionLabel(profile) }}
              </option>
            }
          </select>
          <p class="qt-text-small">{{ desk.help }}</p>
        </div>
      }

      <div class="space-y-1">
        <label for="concierge-image-prompt-profile" class="block qt-text-label">
          Image prompt crafter
        </label>
        <select
          id="concierge-image-prompt-profile"
          class="qt-select"
          [disabled]="disabled()"
          (change)="onCrafterChange($any($event.target).value)"
        >
          <option value="" [selected]="!concierge().imagePromptProfileId">Use the cheap LLM</option>
          @for (profile of connectionProfiles(); track profile.id) {
            <option
              [value]="profile.id"
              [selected]="concierge().imagePromptProfileId === profile.id"
            >
              {{ crafterOptionLabel(profile) }}
            </option>
          }
        </select>
        <p class="qt-text-small">
          Writes the image prompts for the uncensored desk. Any connection profile will do; leave it
          on the cheap LLM unless that one balks.
        </p>
      </div>

      <div class="qt-alert-info">
        <p class="qt-text-small">
          Want the warning badges but never an uncensored model? Set your chats to Locked, or leave
          every desk profile on auto-detect and untick &ldquo;Uncensored-compatible&rdquo; on every
          profile, so there is no one for the Concierge to send for.
        </p>
      </div>
    </div>
  `,
})
export class UncensoredDeskCard extends ConciergeSettingsCard {
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

  private readonly imageProfilesQuery = injectQuery(() => ({
    queryKey: imageProfileKeys.list(),
    queryFn: (): Promise<ImageProfileDto[]> => fetchImageProfiles(this.core),
  }));

  protected readonly loadingProfiles = computed(
    () => this.profilesQuery.isPending() || this.imageProfilesQuery.isPending(),
  );

  protected readonly connectionProfiles = computed(() => this.profilesQuery.data() ?? []);
  private readonly imageProfiles = computed(() => this.imageProfilesQuery.data() ?? []);

  /** v4 `connectionProfiles.filter(p => p.isDangerousCompatible)`. */
  private readonly compatibleText = computed(() =>
    this.connectionProfiles().filter((p) => p.isDangerousCompatible),
  );
  /** v4 `compatibleText.filter(p => p.supportsImageUpload === true)`. */
  private readonly compatibleVision = computed(() =>
    this.compatibleText().filter((p) => p.supportsImageUpload === true),
  );
  private readonly compatibleImage = computed(() =>
    this.imageProfiles().filter((p) => p.isDangerousCompatible),
  );

  private readonly compatibleIds = computed(
    () =>
      new Set<string>([
        ...this.compatibleText().map((p) => p.id),
        ...this.compatibleImage().map((p) => p.id),
      ]),
  );

  /** v4 `saving || loadingProfiles`. */
  protected readonly disabled = computed(() => this.saving() || this.loadingProfiles());

  /** v4 `!loadingProfiles && compatibleText.length === 0 && compatibleImage.length === 0`. */
  protected readonly noneCompatible = computed(
    () =>
      !this.loadingProfiles() &&
      this.compatibleText().length === 0 &&
      this.compatibleImage().length === 0,
  );

  protected readonly deskSelects = computed<DeskSelect[]>(() => {
    const c = this.concierge();
    return [
      {
        id: 'concierge-uncensored-text-profile',
        label: 'Text profile',
        help: 'Answers a chat when its provider refuses, and every turn of an Unmoderated chat.',
        key: 'uncensoredTextProfileId',
        emptyOptionLabel: 'Auto-detect (first uncensored-compatible profile)',
        profiles: withSelected<ProfileLike>(
          this.compatibleText(),
          this.connectionProfiles(),
          c.uncensoredTextProfileId,
        ),
      },
      {
        id: 'concierge-uncensored-image-profile',
        label: 'Image profile',
        help: 'Paints what the usual image provider refuses to.',
        key: 'uncensoredImageProfileId',
        emptyOptionLabel: 'Auto-detect (first uncensored-compatible profile)',
        profiles: withSelected<ProfileLike>(
          this.compatibleImage(),
          this.imageProfiles(),
          c.uncensoredImageProfileId,
        ),
      },
      {
        id: 'concierge-uncensored-vision-profile',
        label: 'Vision profile',
        help: 'Describes an attached image when the image-description profile refuses. Must support image attachments.',
        key: 'uncensoredVisionProfileId',
        emptyOptionLabel: 'Auto-detect (first uncensored-compatible vision profile)',
        profiles: withSelected<ProfileLike>(
          this.compatibleVision(),
          this.connectionProfiles(),
          c.uncensoredVisionProfileId,
        ),
      },
    ];
  });

  /** v4 `DeskSelect`'s option text: the label, suffixed when the pick is stale. */
  protected deskOptionLabel(profile: ProfileLike): string {
    const suffix = this.compatibleIds().has(profile.id) ? '' : NOT_COMPATIBLE_SUFFIX;
    return `${profileLabel(profile)}${suffix}`;
  }

  /** v4's crafter option text: the label, and a warning when the profile has no key. */
  protected crafterOptionLabel(profile: ProfileLike): string {
    return `${profileLabel(profile)}${!profile.apiKey ? ' ⚠️ No API Key' : ''}`;
  }

  /** v4 `onChange(e.target.value || null)`. */
  protected onDeskChange(key: DeskKey, raw: string): Promise<void> {
    const updates: ConciergeSettingsUpdate = { [key]: raw || null };
    return this.update(updates);
  }

  protected onCrafterChange(raw: string): Promise<void> {
    return this.update({ imagePromptProfileId: raw || null });
  }
}
