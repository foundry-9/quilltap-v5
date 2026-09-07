import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  signal,
} from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, Router } from '@angular/router';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { WORKSPACE_HANDLE, WORKSPACE_TAB_ID } from '../../../workspace/workspace-contract';
import { CoreClient } from '../../../core/core-client';
import type { CharacterDetail } from '../../../core/core-contract';
import { EntityTabs, type Tab } from '../../../ui/entity-tabs';
import { ErrorAlert } from '../../../ui/error-alert';
import { Icon } from '../../../ui/icon';
import { LoadingState } from '../../../ui/loading-state';
import { ToastService } from '../../../ui/toast.service';
import { characterAvatarSrc, characterKeys, fetchCharacter } from '../characters.api';
import { CharacterChooseOutfitCard } from '../choose-outfit-card';
import { WardrobeDialogService } from '../../../wardrobe/wardrobe-dialog.service';
import { CharacterDeleteDialog, type DeleteChoice } from '../list/character-delete-dialog';
import type { GeneratedCharacterData } from '../generators/edit-generators.api';
import { CharacterRenameReplaceTab } from '../generators/rename/rename-replace-tab';
import { AiWizardModal } from '../generators/wizard/ai-wizard-modal';
import { saveGeneratedPhysicalDescription, saveGeneratedScenarios, saveGeneratedWardrobeItems } from '../generators/wizard/save-generated';
import {
  getGeneratedCharacterTextEntries,
  mergeGeneratedAliases,
  normalizeGeneratedScenarios,
  type WizardCharacterData,
} from '../generators/wizard/wizard-types';
import { CharacterAppearanceTab } from './appearance-tab';
import {
  buildCharacterUpdateBag,
  INITIAL_CHARACTER_FORM_DATA,
  loadCharacterIntoForm,
  type CharacterFormData,
} from './character-form';
import { AvatarPickerModal } from './avatar-picker-modal';
import { CharacterDetailsTab } from './details-tab';
import { CharacterLlmLogsSection } from './llm-logs-section';
import { CharacterSystemPromptsTab } from './system-prompts-tab';

const EDIT_TABS: Tab[] = [
  { id: 'details', label: 'Details', icon: 'user' },
  { id: 'system-prompts', label: 'System Prompts', icon: 'code' },
  { id: 'wardrobe', label: 'Wardrobe', icon: 'wardrobe' },
  { id: 'descriptions', label: 'Appearance', icon: 'file' },
  { id: 'rename', label: 'Rename/Replace', icon: 'pencil' },
];

/**
 * The character EDIT screen (v4 `app/aurora/[id]/edit/CharacterEditView.tsx`):
 * an avatar + name header (pencil opens the avatar picker), the Details /
 * System Prompts / Appearance tabs over `qt-entity-tabs`, and an explicit
 * "Save Character" that dispatches ONE `characterUpdate` with the whole
 * Details-tab form bag. Rename/Replace and the inline AI Wizard are named
 * deferrals (disabled affordances, v4 microcopy). The Wardrobe tab opens the
 * global Wardrobe dialog with this character preselected (v4
 * `CharacterEditView.tsx:313-331` — the P4.9f2 entry point).
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
    CharacterChooseOutfitCard,
    CharacterLlmLogsSection,
    CharacterRenameReplaceTab,
    AiWizardModal,
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
                title="Use AI to generate character details"
                (click)="wizardOpen.set(true)"
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
          <qt-entity-tabs [tabs]="tabs" [defaultTab]="tab() ?? 'details'">
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
                @case ('wardrobe') {
                  <!-- v4 CharacterEditView.tsx (8bf3cb5f) — prose, the canChooseOutfit
                       checkbox card, and the dialog opener. -->
                  <div class="space-y-4">
                    <p class="qt-text-small qt-text-secondary">
                      The wardrobe is managed in the global Wardrobe dialog so you can drop in from
                      anywhere — including from inside a chat — and edit, layer, or save outfits
                      without leaving the page you're on.
                    </p>
                    <qt-character-choose-outfit-card
                      [characterId]="characterId()"
                      [canChooseOutfit]="character()?.canChooseOutfit ?? false"
                    />
                    <button type="button" class="qt-button-primary" (click)="openWardrobe()">
                      Open wardrobe for {{ character()?.name || 'this character' }}
                    </button>
                  </div>
                }
                @case ('descriptions') {
                  <qt-character-appearance-tab [characterId]="characterId()!" />
                }
                @case ('rename') {
                  <qt-character-rename-replace-tab
                    [characterId]="characterId()!"
                    [characterName]="character()?.name || ''"
                    (renameComplete)="onRenameComplete()"
                  />
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

        <!-- v4 CharacterEditView.tsx:436 — the collapsible LLM Logs section
             (M6 F2), mounted right after the form. -->
        <qt-character-llm-logs-section [characterId]="characterId()!" />

        <div class="mt-6 flex gap-3">
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

    @if (wizardOpen()) {
      <qt-ai-wizard-modal
        [characterId]="characterId()!"
        [characterName]="character()?.name || ''"
        [currentData]="wizardCurrentData()"
        (apply)="onWizardApply($event)"
        (closeModal)="wizardOpen.set(false)"
      />
    }

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
  private readonly toasts = inject(ToastService);
  private readonly wardrobeDialog = inject(WardrobeDialogService);
  private readonly route = inject(ActivatedRoute, { optional: true });
  private readonly router = inject(Router);
  private readonly queryClient = injectQueryClient();
  /** Workspace-tab seams (p4.9j2); null ⇒ routed mode. */
  private readonly handle = inject(WORKSPACE_HANDLE, { optional: true });
  private readonly tabId = inject(WORKSPACE_TAB_ID, { optional: true });

  /**
   * Tab-mode inputs (v4 `CharacterEditTabPayload`): `characterId` supplies the
   * identity in place of the route `:id`; `tab` deep-links a sub-tab. Null ⇒
   * routed mode, byte-identical.
   */
  readonly characterIdInput = input<string | null>(null, { alias: 'characterId' });
  readonly tab = input<string | null>(null);

  protected readonly tabs = EDIT_TABS;

  private readonly params = this.route
    ? toSignal(this.route.paramMap, { requireSync: true })
    : undefined;
  protected readonly characterId = computed(
    () => this.characterIdInput() ?? this.params?.().get('id') ?? null,
  );

  /**
   * Both seams present ⇒ hosted as a workspace tab; save/cancel/delete close the
   * tab instead of navigating (v4 `useCloseSelfTab`). Returns true when it closed.
   */
  private closeSelfTab(): boolean {
    if (this.handle && this.tabId != null) {
      this.handle.closeTab(this.tabId);
      return true;
    }
    return false;
  }

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
  protected readonly wizardOpen = signal(false);

  protected readonly dirty = computed(
    () => JSON.stringify(this.formData()) !== JSON.stringify(this.originalFormData()),
  );

  protected readonly avatarSrc = computed(() => {
    const c = this.character();
    return c ? characterAvatarSrc(c.defaultImage, c.defaultImageId) : null;
  });

  protected readonly initial = computed(() => (this.character()?.name[0] ?? '?').toUpperCase());

  /** v4 `CharacterEditView.tsx:375-378` — `buildWizardCurrentData(formData)` + `scenarios`. */
  protected readonly wizardCurrentData = computed<WizardCharacterData>(() => {
    const f = this.formData();
    return {
      title: f.title,
      identity: f.identity,
      description: f.description,
      manifesto: f.manifesto,
      personality: f.personality,
      firstMessage: f.firstMessage,
      exampleDialogues: f.exampleDialogues,
      systemPrompt: f.systemPrompt,
      pronouns: f.pronouns,
      aliases: f.aliases,
      scenarios: f.scenarios,
    };
  });

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
      // v4 `useCharacterEdit.ts:250` — inline block AND toast (BOTH).
      this.toasts.showSuccess('Character saved successfully!');
      // Hosted ⇒ close the tab (v4 `useCloseSelfTab`); routed ⇒ back to detail.
      if (!this.closeSelfTab()) this.router.navigate(['/characters', id]);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to update character';
      this.saveError.set(message);
      this.toasts.showError(message);
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
      // Hosted ⇒ close the tab; routed ⇒ the character's pages are gone, so leave.
      if (!this.closeSelfTab()) this.router.navigate(['/characters']);
    }
  }

  /** v4 `CharacterEditView.tsx:324` — the Wardrobe-tab dialog opener. */
  protected openWardrobe(): void {
    const id = this.characterId();
    if (id) {
      this.wardrobeDialog.open({ characterId: id });
    }
  }

  protected cancel(): void {
    if (this.dirty()) {
      const proceed = window.confirm('You have unsaved changes. Leave without saving?');
      if (!proceed) {
        return;
      }
    }
    // Hosted ⇒ close the tab (v4 `useCloseSelfTab`); routed ⇒ back to the detail.
    if (this.closeSelfTab()) return;
    const id = this.characterId();
    this.router.navigate(id ? ['/characters', id] : ['/characters']);
  }

  protected onAvatarSaved(): void {
    const id = this.characterId();
    if (id) {
      void this.queryClient.invalidateQueries({ queryKey: characterKeys.detail(id) });
    }
  }

  protected onRenameComplete(): void {
    const id = this.characterId();
    if (id) {
      void this.queryClient.invalidateQueries({ queryKey: characterKeys.detail(id) });
    }
  }

  /**
   * v4 `CharacterEditView.tsx:95-158` `handleWizardApply` — text fields land in
   * form state (remounting the markdown editors is a v5 non-issue: `qt-
   * markdown-field` binds `[value]` reactively, no `remountKey` needed);
   * physical description PATCHes immediately; pronouns/aliases stage into
   * form state for the user to review before "Save Character"; wardrobe items
   * and scenarios persist immediately (toasted), with new scenarios spliced
   * into form state directly rather than refetching (a refetch would clobber
   * the wizard-applied text fields before they're saved).
   */
  protected async onWizardApply(data: GeneratedCharacterData): Promise<void> {
    const id = this.characterId();
    if (!id) {
      return;
    }

    const textEntries = getGeneratedCharacterTextEntries(data);
    if (textEntries.length > 0) {
      this.formData.update((f) => {
        const next: Record<string, unknown> = { ...f };
        for (const entry of textEntries) {
          next[entry.field] = entry.value;
        }
        return next as unknown as CharacterFormData;
      });
    }

    if (data.physicalDescription) {
      await saveGeneratedPhysicalDescription(
        this.core,
        this.toasts,
        id,
        data.physicalDescription,
        'Failed to save physical description',
      );
    }

    if (data.properties) {
      if (data.properties.pronouns && !this.formData().pronouns) {
        this.formData.update((f) => ({ ...f, pronouns: data.properties!.pronouns }));
      }
      if (data.properties.aliases.length > 0) {
        const existing = this.formData().aliases;
        const merged = mergeGeneratedAliases(existing, data.properties.aliases);
        if (merged !== existing) {
          this.formData.update((f) => ({ ...f, aliases: merged }));
        }
      }
    }

    if (data.wardrobeItems && data.wardrobeItems.length > 0) {
      const { saved, outfits } = await saveGeneratedWardrobeItems(this.core, id, data.wardrobeItems);
      if (saved > 0) {
        const outfitText = outfits > 0 ? ` (including ${outfits} outfit${outfits > 1 ? 's' : ''})` : '';
        this.toasts.showSuccess(`${saved} wardrobe item${saved > 1 ? 's' : ''} created${outfitText}`);
      }
    }

    const normalizedScenarios = normalizeGeneratedScenarios(data.scenarios);
    if (normalizedScenarios.length > 0) {
      const { saved, scenarios } = await saveGeneratedScenarios(this.core, id, normalizedScenarios);
      if (saved > 0) {
        this.toasts.showSuccess(`${saved} scenario${saved > 1 ? 's' : ''} created`);
        this.formData.update((f) => ({ ...f, scenarios: [...f.scenarios, ...scenarios] }));
      }
    }
  }
}
