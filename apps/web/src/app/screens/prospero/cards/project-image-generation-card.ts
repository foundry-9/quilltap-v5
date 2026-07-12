import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { ImageProfileDto, ProjectDetail } from '../../../core/core-contract';
import { CollapsibleCard } from '../../../ui/collapsible-card';
import { ErrorAlert } from '../../../ui/error-alert';
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
 * The background RENDER defers with the Salon-side story-background work; the
 * display-mode select still saves.
 */
@Component({
  selector: 'qt-project-image-generation-card',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [CollapsibleCard, ErrorAlert, ProjectAestheticField],
  template: `
    <qt-collapsible-card
      title="Image Generation"
      description="Avatars, image profiles & story backgrounds"
      icon="image"
      [defaultOpen]="defaultOpen()"
      class="col-span-full block"
    >
      @if (saveError(); as msg) {
        <qt-error-alert [message]="msg" class="mb-3" />
      }

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
            <option value="project" [selected]="backgroundValue() === 'project'">
              Project-generated background
            </option>
            <option value="static" [selected]="backgroundValue() === 'static'">
              Static uploaded image
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
  protected readonly saveError = signal<string | null>(null);
  protected readonly savingProfile = signal(false);

  private readonly profilesQuery = injectQuery(() => ({
    queryKey: imageProfileKeys.list(),
    queryFn: (): Promise<ImageProfileDto[]> => fetchImageProfiles(this.core),
  }));

  protected readonly imageProfiles = computed(() => this.profilesQuery.data() ?? []);
  protected readonly imageProfileValue = computed(
    () => this.project().defaultImageProfileId ?? '',
  );

  protected readonly avatarValue = computed(() =>
    triState(this.project().defaultAvatarGenerationEnabled),
  );
  protected readonly announceValue = computed(() =>
    triState(this.project().defaultAlertCharactersOfLanternImages),
  );
  protected readonly backgroundValue = computed(
    () => this.project().backgroundDisplayMode ?? 'theme',
  );

  protected readonly backgroundHint = computed(() => {
    switch (this.backgroundValue()) {
      case 'latest_chat':
        return 'Shows the most recent background from any chat in this project.';
      case 'project':
        return 'Uses a background generated specifically for this project.';
      case 'static':
        return 'Uses a manually uploaded background image.';
      default:
        return 'No background image, uses your theme colors.';
    }
  });

  protected onAvatar(event: Event): void {
    const v = (event.target as HTMLSelectElement).value;
    void this.save(
      { defaultAvatarGenerationEnabled: v === 'inherit' ? null : v === 'enabled' },
      'Failed to update avatar generation',
    );
  }

  protected onAnnounce(event: Event): void {
    const v = (event.target as HTMLSelectElement).value;
    void this.save(
      { defaultAlertCharactersOfLanternImages: v === 'inherit' ? null : v === 'enabled' },
      'Failed to update Lantern image announcement setting',
    );
  }

  protected onBackground(event: Event): void {
    const v = (event.target as HTMLSelectElement).value;
    void this.save({ backgroundDisplayMode: v }, 'Failed to update background mode');
  }

  protected async onImageProfile(event: Event): Promise<void> {
    const v = (event.target as HTMLSelectElement).value;
    this.savingProfile.set(true);
    try {
      await this.save(
        { defaultImageProfileId: v || null },
        'Failed to update default image profile',
      );
    } finally {
      this.savingProfile.set(false);
    }
  }

  private async save(patch: Record<string, unknown>, fallback: string): Promise<void> {
    this.saveError.set(null);
    try {
      await updateProject(this.core, this.project().id, patch);
      await this.queryClient.invalidateQueries({ queryKey: projectKeys.detail(this.project().id) });
    } catch (err) {
      this.saveError.set(err instanceof Error ? err.message : fallback);
    }
  }
}

function triState(v: boolean | null | undefined): 'inherit' | 'enabled' | 'disabled' {
  return v === null || v === undefined ? 'inherit' : v ? 'enabled' : 'disabled';
}
