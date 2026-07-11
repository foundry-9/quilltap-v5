import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  signal,
} from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, Router } from '@angular/router';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { CharacterDetail } from '../../../core/core-contract';
import { EntityTabs, type Tab } from '../../../ui/entity-tabs';
import { ErrorAlert } from '../../../ui/error-alert';
import { Icon } from '../../../ui/icon';
import { LoadingState } from '../../../ui/loading-state';
import { characterAvatarSrc, characterKeys, fetchCharacter } from '../characters.api';
import { CharacterDeleteDialog, type DeleteChoice } from '../list/character-delete-dialog';
import { CharacterAppearanceTab } from './appearance-tab';
import {
  buildCharacterUpdateBag,
  INITIAL_CHARACTER_FORM_DATA,
  loadCharacterIntoForm,
  type CharacterFormData,
} from './character-form';
import { AvatarPickerModal } from './avatar-picker-modal';
import { CharacterDetailsTab } from './details-tab';
import { CharacterSystemPromptsTab } from './system-prompts-tab';

const EDIT_TABS: Tab[] = [
  { id: 'details', label: 'Details', icon: 'user' },
  { id: 'system-prompts', label: 'System Prompts', icon: 'code' },
  { id: 'descriptions', label: 'Appearance', icon: 'file' },
];

/**
 * The character EDIT screen (v4 `app/aurora/[id]/edit/CharacterEditView.tsx`):
 * an avatar + name header (pencil opens the avatar picker), the Details /
 * System Prompts / Appearance tabs over `qt-entity-tabs`, and an explicit
 * "Save Character" that dispatches ONE `characterUpdate` with the whole
 * Details-tab form bag. Rename/Replace and the inline AI Wizard are named
 * deferrals (disabled affordances, v4 microcopy). Wardrobe is out of scope
 * for this vertical (the global Wardrobe dialog owns it in v4 too).
 *
 * Simplification vs v4: the unsaved-changes guard here is a plain
 * `window.confirm` on Cancel (v4's three-way Save/Discard/Cancel `showAlert`)
 * — acceptable per the work order. Cancel always returns to the character's
 * detail page (v4 special-cases NPCs to the Settings NPC list; that route
 * doesn't exist yet in the v5 SPA).
 */
@Component({
  selector: 'qt-character-edit',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    EntityTabs,
    ErrorAlert,
    LoadingState,
    Icon,
    CharacterDetailsTab,
    CharacterSystemPromptsTab,
    CharacterAppearanceTab,
    AvatarPickerModal,
    CharacterDeleteDialog,
  ],
  template: `
    <div class="character-edit qt-page-container text-foreground">
      @if (characterQuery.isPending()) {
        <qt-loading-state message="Loading character..." class="mt-12" />
      } @else if (characterQuery.isError()) {
        <qt-error-alert
          [message]="'Error: ' + errorMessage()"
          [retryable]="true"
          class="mt-8"
          (retry)="characterQuery.refetch()"
        />
      } @else {
        <div class="mb-8">
          <button
            type="button"
            class="mb-4 inline-flex items-center qt-label text-primary"
            (click)="cancel()"
          >
            ← Back
          </button>
          <div class="flex items-center gap-4">
            <div class="relative">
              @if (avatarSrc()) {
                <img
                  [src]="avatarSrc()"
                  [alt]="character()!.name"
                  class="w-20 h-20 rounded-full object-cover"
                />
              } @else {
                <div class="flex h-20 w-20 items-center justify-center rounded-full qt-bg-muted">
                  <span class="qt-heading-1 qt-text-secondary">{{ initial() }}</span>
                </div>
              }
              <button
                type="button"
                class="absolute -bottom-1 -right-1 rounded-full bg-primary p-1.5 text-primary-foreground"
                title="Change avatar"
                (click)="avatarPickerOpen.set(true)"
              >
                <qt-icon name="pencil" class="w-3 h-3" />
              </button>
            </div>

            <div class="flex-1 flex items-center justify-between">
              <h1 class="qt-heading-1">Edit: {{ character()!.name }}</h1>
              <button
                type="button"
                class="qt-button-secondary flex items-center gap-2"
                disabled
                title="Use AI to generate character details (not yet available)"
              >
                <qt-icon name="wand" class="w-4 h-4" />
                AI Wizard
              </button>
            </div>
          </div>
        </div>

        @if (saveError()) {
          <div class="mb-4 qt-alert-error">{{ saveError() }}</div>
        }

        <form (submit)="onSubmit($event)">
          <qt-entity-tabs [tabs]="tabs" defaultTab="details">
            <ng-template let-active>
              @switch (active) {
                @case ('details') {
                  <qt-character-details-tab
                    [form]="formData()"
                    (fieldChange)="formData.set($event)"
                  />
                }
                @case ('system-prompts') {
                  <qt-character-system-prompts-tab
                    [characterId]="characterId()!"
                    [characterName]="character()!.name"
                  />
                }
                @case ('descriptions') {
                  <qt-character-appearance-tab [characterId]="characterId()!" />
                }
              }
            </ng-template>
          </qt-entity-tabs>

          <div class="flex gap-4 mt-8">
            <button
              type="submit"
              class="qt-button qt-button-primary qt-button-lg flex-1"
              [disabled]="saving()"
            >
              {{ saving() ? 'Saving...' : 'Save Character' }}
            </button>
            <button
              type="button"
              class="qt-button qt-button-secondary qt-button-lg"
              (click)="cancel()"
            >
              Cancel
            </button>
          </div>
        </form>

        <div class="mt-6 flex gap-3">
          <button
            type="button"
            class="qt-button-secondary"
            disabled
            title="Rename this character and replace all references (not yet available)"
          >
            Rename/Replace
          </button>
          <button
            type="button"
            class="qt-button qt-button-destructive"
            (click)="deleteOpen.set(true)"
          >
            Delete Character
          </button>
        </div>
      }
    </div>

    @if (avatarPickerOpen()) {
      <qt-avatar-picker-modal
        [characterId]="characterId()!"
        (close)="avatarPickerOpen.set(false)"
        (saved)="onAvatarSaved()"
      />
    }

    @if (deleteOpen() && character(); as target) {
      <qt-character-delete-dialog
        [characterId]="characterId()!"
        [characterName]="target.name"
        (close)="deleteOpen.set(false)"
        (confirmed)="onDeleteConfirm($event)"
      />
    }
  `,
})
export class CharacterEdit {
  private readonly core = inject(CoreClient);
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);
  private readonly queryClient = injectQueryClient();

  protected readonly tabs = EDIT_TABS;

  private readonly params = toSignal(this.route.paramMap, { requireSync: true });
  protected readonly characterId = computed(() => this.params().get('id'));

  protected readonly characterQuery = injectQuery(() => ({
    queryKey: characterKeys.detail(this.characterId() ?? ''),
    enabled: !!this.characterId(),
    queryFn: (): Promise<CharacterDetail> => fetchCharacter(this.core, this.characterId()!),
  }));

  protected readonly character = computed(() => this.characterQuery.data() ?? null);

  protected readonly formData = signal<CharacterFormData>(INITIAL_CHARACTER_FORM_DATA);
  private readonly originalFormData = signal<CharacterFormData>(INITIAL_CHARACTER_FORM_DATA);
  private seeded = false;

  protected readonly avatarPickerOpen = signal(false);
  protected readonly deleteOpen = signal(false);
  protected readonly saving = signal(false);
  protected readonly saveError = signal<string | null>(null);

  protected readonly dirty = computed(
    () => JSON.stringify(this.formData()) !== JSON.stringify(this.originalFormData()),
  );

  protected readonly avatarSrc = computed(() => {
    const c = this.character();
    return c ? characterAvatarSrc(c.defaultImage, c.defaultImageId) : null;
  });

  protected readonly initial = computed(() => (this.character()?.name[0] ?? '?').toUpperCase());

  constructor() {
    // Seed the form once from the loaded character (not on every background
    // refetch, so an in-progress edit isn't clobbered — v4's `fetchCharacter`
    // replaces `formData` wholesale on load but the SPA only needs the first).
    effect(() => {
      const data = this.characterQuery.data();
      if (data && !this.seeded) {
        const seeded = loadCharacterIntoForm(data);
        this.formData.set(seeded);
        this.originalFormData.set(seeded);
        this.seeded = true;
      }
    });
  }

  protected errorMessage(): string {
    const err = this.characterQuery.error();
    return err instanceof Error ? err.message : 'An error occurred';
  }

  protected async onSubmit(event: Event): Promise<void> {
    event.preventDefault();
    const id = this.characterId();
    if (!id) {
      return;
    }
    this.saving.set(true);
    this.saveError.set(null);
    try {
      await this.core.dispatchData({
        type: 'characterUpdate',
        characterId: id,
        character: buildCharacterUpdateBag(this.formData()),
      });
      this.originalFormData.set(this.formData());
      await this.queryClient.invalidateQueries({ queryKey: characterKeys.detail(id) });
      this.router.navigate(['/characters', id]);
    } catch (err) {
      this.saveError.set(err instanceof Error ? err.message : 'Failed to update character');
    } finally {
      this.saving.set(false);
    }
  }

  /**
   * Delete from the edit view: dispatch `characterDelete` with the cascade
   * flags, drop the roster + detail caches, and return to the roster (the
   * character's own pages no longer exist). v4 only deletes from the roster
   * (`AuroraView`); this detail/edit entry point is an additive SPA affordance
   * the work order asks for, navigating away since we can't stay on the page.
   */
  protected async onDeleteConfirm(choice: DeleteChoice): Promise<void> {
    const id = this.characterId();
    if (!id) {
      return;
    }
    try {
      await this.core.dispatchData({
        type: 'characterDelete',
        characterId: id,
        cascadeChats: choice.cascadeChats,
        cascadeImages: choice.cascadeImages,
      });
    } finally {
      this.deleteOpen.set(false);
      await this.queryClient.invalidateQueries({ queryKey: characterKeys.list() });
      this.router.navigate(['/characters']);
    }
  }

  protected cancel(): void {
    if (this.dirty()) {
      const proceed = window.confirm('You have unsaved changes. Leave without saving?');
      if (!proceed) {
        return;
      }
    }
    const id = this.characterId();
    this.router.navigate(id ? ['/characters', id] : ['/characters']);
  }

  protected onAvatarSaved(): void {
    const id = this.characterId();
    if (id) {
      void this.queryClient.invalidateQueries({ queryKey: characterKeys.detail(id) });
    }
  }
}
