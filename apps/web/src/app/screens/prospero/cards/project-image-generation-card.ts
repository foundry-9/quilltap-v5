import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { ImageProfileDto, ProjectDetail } from '../../../core/core-contract';
import { CollapsibleCard } from '../../../ui/collapsible-card';
import { ToastService } from '../../../ui/toast.service';
import { fetchImageProfiles, imageProfileKeys } from '../../settings/images/image-profiles.api';
import { projectKeys, updateProject } from '../projects.api';
import { ProjectAestheticField } from './project-aesthetic-field';

/**
 * The Image Generation card (v4 `ImageGenerationCard.tsx`, full-width): four
 * immediate-save selects plus the two aesthetic editors.
 *
 * LIVE: Avatar Generation, Announce Lantern Images, Story-Background display
 * mode, and the Default Image Profile picker (over the P4.6p image-profiles
 * listing, binding the project's `defaultImageProfileId`) — each PUTs one field.
 *
 * The background RENDER landed with P4.D92 (v4 bug 80): the project detail
 * reports the resolved image to the workspace backdrop. Saving a mode here does
 * NOT invalidate that query — v4 never invalidates its background key either, so
 * the new mode reaches the backdrop on the next fetch (remount, focus, or the
 * 30s poll). See `project-detail.ts`.
 */
/** The surviving project background modes, straight off the contract union. */
type BackgroundMode = NonNullable<ProjectDetail['backgroundDisplayMode']>;

@Component({
  selector: 'qt-project-image-generation-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [CollapsibleCard, ProjectAestheticField],
  template: `
    <qt-collapsible-card
      title="Image Generation"
      description="Avatars, image profiles & story backgrounds"
      icon="image"
      [defaultOpen]="defaultOpen()"
      class="col-span-full block"
    >
      <div class="space-y-4">
        <div class="p-3 rounded-lg qt-border qt-bg-surface">
          <h4 class="qt-label text-foreground mb-1">Avatar Generation</h4>
          <p class="qt-text-xs qt-text-secondary mb-2">
            Auto-generate character avatars when outfits change in new chats.
          </p>
          <select
            class="qt-input w-full max-w-xs"
            aria-label="Avatar Generation"
            (change)="onAvatar($event)"
          >
            <option value="inherit" [selected]="avatarValue() === 'inherit'">
              Inherit from global
            </option>
            <option value="enabled" [selected]="avatarValue() === 'enabled'">
              Enabled by default
            </option>
            <option value="disabled" [selected]="avatarValue() === 'disabled'">
              Disabled by default
            </option>
          </select>
        </div>

        <div class="p-3 rounded-lg qt-border qt-bg-surface">
          <h4 class="qt-label text-foreground mb-1">Default Image Profile</h4>
          <p class="qt-text-xs qt-text-secondary mb-2">
            Image generation profile for new chats in this project. Overrides both the global
            default and character defaults.
          </p>
          <select
            class="qt-input w-full max-w-xs"
            aria-label="Default Image Profile"
            [disabled]="savingProfile()"
            (change)="onImageProfile($event)"
          >
            <option value="" [selected]="imageProfileValue() === ''">
              Inherit from global default
            </option>
            @for (p of imageProfiles(); track p.id) {
              <option [value]="p.id" [selected]="imageProfileValue() === p.id">
                {{ p.name }} ({{ p.provider }} / {{ p.modelName }})
              </option>
            }
          </select>
          @if (savingProfile()) {
            <p class="qt-text-xs qt-text-secondary mt-1">Saving…</p>
          }
        </div>

        <div class="p-3 rounded-lg qt-border qt-bg-surface">
          <h4 class="qt-label text-foreground mb-1">Announce Lantern Images to Characters</h4>
          <p class="qt-text-xs qt-text-secondary mb-2">
            When the Lantern produces a background, an avatar, or any generated picture, post an
            announcement in the chat so every character may behold it on their next turn.
          </p>
          <select
            class="qt-input w-full max-w-xs"
            aria-label="Announce Lantern Images"
            (change)="onAnnounce($event)"
          >
            <option value="inherit" [selected]="announceValue() === 'inherit'">
              Inherit from global
            </option>
            <option value="enabled" [selected]="announceValue() === 'enabled'">
              Announce to characters
            </option>
            <option value="disabled" [selected]="announceValue() === 'disabled'">
              Keep silent
            </option>
          </select>
        </div>

        <div class="p-3 rounded-lg qt-border qt-bg-surface">
          <h4 class="qt-label text-foreground mb-1">Story Backgrounds</h4>
          <p class="qt-text-xs qt-text-secondary mb-2">
            Choose how the project background is displayed. Backgrounds are generated from chat
            titles and characters.
          </p>
          <select
            class="qt-input w-full max-w-xs"
            aria-label="Story Backgrounds"
            (change)="onBackground($event)"
          >
            <option value="theme" [selected]="backgroundValue() === 'theme'">
              Use theme background (no image)
            </option>
            <option value="latest_chat" [selected]="backgroundValue() === 'latest_chat'">
              Latest chat background
            </option>
          </select>
          <p class="qt-text-xs qt-text-secondary mt-2">{{ backgroundHint() }}</p>
        </div>

        <div class="p-3 rounded-lg qt-border qt-bg-surface space-y-6">
          <div>
            <h4 class="qt-label text-foreground mb-1">Default Aesthetics</h4>
            <p class="qt-text-xs qt-text-secondary">
              Override the global house style for images in this project. Leave a field empty to
              inherit the global default.
            </p>
          </div>
          <qt-project-aesthetic-field
            [projectId]="project().id"
            kind="lantern"
            label="Default Image Aesthetic"
            description="Overall look for scenes and backgrounds in this project."
          />
          <qt-project-aesthetic-field
            [projectId]="project().id"
            kind="aurora"
            label="Default Character Aesthetic"
            description="How people and outfits are depicted in this project's images."
          />
        </div>
      </div>
    </qt-collapsible-card>
  `,
})
export class ProjectImageGenerationCard {
  readonly project = input.required<ProjectDetail>();
  readonly defaultOpen = input(false);

  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();
  private readonly toasts = inject(ToastService);
  protected readonly savingProfile = signal(false);

  private readonly profilesQuery = injectQuery(() => ({
    queryKey: imageProfileKeys.list(),
    queryFn: (): Promise<ImageProfileDto[]> => fetchImageProfiles(this.core),
  }));

  protected readonly imageProfiles = computed(() => this.profilesQuery.data() ?? []);
  protected readonly imageProfileValue = computed(() => this.project().defaultImageProfileId ?? '');

  protected readonly avatarValue = computed(() =>
    triState(this.project().defaultAvatarGenerationEnabled),
  );
  protected readonly announceValue = computed(() =>
    triState(this.project().defaultAlertCharactersOfLanternImages),
  );
  protected readonly backgroundValue = computed(
    () => this.project().backgroundDisplayMode ?? 'theme',
  );

  /**
   * [70505745a] Two of the four hints went with their options: 'project' and
   * 'static' were retired in 4.9 because neither ever produced an image.
   */
  protected readonly backgroundHint = computed(() => {
    switch (this.backgroundValue()) {
      case 'latest_chat':
        return 'Shows the most recent background from any chat in this project.';
      default:
        return 'No background image, uses your theme colors.';
    }
  });

  /** v4 `useProjectDetail.ts:155-177`. */
  protected onAvatar(event: Event): void {
    const v = (event.target as HTMLSelectElement).value;
    const enabled = v === 'inherit' ? null : v === 'enabled';
    void this.save(
      { defaultAvatarGenerationEnabled: enabled },
      enabled === null
        ? 'Avatar generation set to inherit from global'
        : enabled
          ? 'Avatar generation enabled by default for project'
          : 'Avatar generation disabled by default for project',
      'Failed to update avatar generation',
    );
  }

  /** v4 `useProjectDetail.ts:221-243`. */
  protected onAnnounce(event: Event): void {
    const v = (event.target as HTMLSelectElement).value;
    const enabled = v === 'inherit' ? null : v === 'enabled';
    void this.save(
      { defaultAlertCharactersOfLanternImages: enabled },
      enabled === null
        ? 'Lantern image announcements set to inherit from global'
        : enabled
          ? 'Lantern image announcements enabled by default for project'
          : 'Lantern image announcements disabled by default for project',
      'Failed to update Lantern image announcement setting',
    );
  }

  /** v4 `useProjectDetail.ts:245-268`. */
  protected onBackground(event: Event): void {
    const v = (event.target as HTMLSelectElement).value as BackgroundMode;
    // Typed over the contract's union, as v4's `Record<BackgroundDisplayMode,
    // string>` is: retiring a mode there is a compile error here, never a
    // `Background set to undefined` toast (unification review, 2026-09-02).
    const modeLabels: Record<BackgroundMode, string> = {
      theme: 'theme background',
      latest_chat: 'latest chat background',
    };
    void this.save(
      { backgroundDisplayMode: v },
      `Background set to ${modeLabels[v]}`,
      'Failed to update background mode',
    );
  }

  /** v4 `useProjectDetail.ts:179-198`. */
  protected async onImageProfile(event: Event): Promise<void> {
    const v = (event.target as HTMLSelectElement).value;
    this.savingProfile.set(true);
    try {
      await this.save(
        { defaultImageProfileId: v || null },
        v ? 'Default image profile set for project' : 'Image profile set to inherit from global',
        'Failed to update default image profile',
      );
    } finally {
      this.savingProfile.set(false);
    }
  }

  private async save(
    patch: Record<string, unknown>,
    successMsg: string,
    fallback: string,
  ): Promise<void> {
    try {
      await updateProject(this.core, this.project().id, patch);
      await this.queryClient.invalidateQueries({ queryKey: projectKeys.detail(this.project().id) });
      this.toasts.showSuccess(successMsg);
    } catch (err) {
      this.toasts.showError(err instanceof Error ? err.message : fallback);
    }
  }
}

function triState(v: boolean | null | undefined): 'inherit' | 'enabled' | 'disabled' {
  return v === null || v === undefined ? 'inherit' : v ? 'enabled' : 'disabled';
}
