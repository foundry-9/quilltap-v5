import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { RouterLink } from '@angular/router';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { WORKSPACE_TAB_ID } from '../../../workspace/workspace-contract';
import { CoreClient } from '../../../core/core-client';
import type { CharacterConnectionProfile, CharacterListItem } from '../../../core/core-contract';
import { QuickHideService } from '../../../quick-hide/quick-hide.service';
import { ErrorAlert } from '../../../ui/error-alert';
import { LoadingState } from '../../../ui/loading-state';
import {
  characterKeys,
  downloadCharacterPng,
  fetchCharacterExport,
  fetchCharacterList,
  fetchConnectionProfiles,
  triggerJsonDownload,
} from '../characters.api';
import { GroupsSection } from '../../groups/groups-section';
import { GroupEditor } from '../../groups/group-editor';
import { CharacterCard } from './character-card';
import { CharacterDeleteDialog, type DeleteChoice } from './character-delete-dialog';
import { CharacterImportDialog } from './character-import-dialog';
import { ResetBuiltinsDialog } from './reset-builtins-dialog';
import { CharacterDetail } from '../view/character-detail';

/** v4 `AuroraView.tsx:125-140` sort: NPCs last → favorites first → chats desc → name. */
export function sortCharacters(list: CharacterListItem[]): CharacterListItem[] {
  return [...list].sort((a, b) => {
    if (a.npc !== b.npc) {
      return a.npc ? 1 : -1;
    }
    if (a.isFavorite !== b.isFavorite) {
      return a.isFavorite ? -1 : 1;
    }
    if ((a._count?.chats ?? 0) !== (b._count?.chats ?? 0)) {
      return (b._count?.chats ?? 0) - (a._count?.chats ?? 0);
    }
    return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' });
  });
}

/**
 * The character roster (v4 `app/aurora/AuroraView.tsx`). Cards over the
 * `characterList` dispatch (TanStack Query), the v4 sort, the three inline
 * toggles with optimistic updates, the Create / Import toolbar, and the Groups
 * section (P4.6l) above the grid. "Reset Built-in Characters" is LIVE (the
 * WEB-EDGE `?action=reset-builtins` route, live since P4.4u4) via a confirm
 * dialog + result banner. "Summon From Lore" (AI import) remains a deferral —
 * disabled with v4 microcopy. Copy + `qt-*` classes carry over verbatim.
 */
@Component({
  selector: 'qt-characters-list',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    RouterLink,
    LoadingState,
    ErrorAlert,
    CharacterCard,
    CharacterDeleteDialog,
    CharacterImportDialog,
    ResetBuiltinsDialog,
    GroupsSection,
    CharacterDetail,
    GroupEditor,
  ],
  template: `
    @if (selectedCharacterId(); as cid) {
      <!-- In-tab drill (v4 AuroraView selectedCharacterId): the detail in place. -->
      <qt-character-detail
        [characterId]="cid"
        [embedded]="true"
        (back)="selectedCharacterId.set(null)"
      />
    } @else if (selectedGroupId(); as gid) {
      <qt-group-editor [groupId]="gid" (back)="onGroupBack()" />
    } @else {
    <div class="character-page qt-page-container text-foreground">
      <div
        class="flex flex-wrap items-center justify-between gap-4 border-b qt-border-default/60 pb-6"
      >
        <h1 class="qt-page-title">Characters</h1>
        <div class="flex flex-wrap gap-3">
          <button
            type="button"
            class="qt-button character-toolbar__button inline-flex items-center rounded-lg border qt-border-default qt-bg-muted/70 px-4 py-2 text-sm qt-text-primary qt-shadow-sm transition hover:qt-bg-muted"
            title="Reset built-in characters to first-run defaults"
            (click)="resetOpen.set(true)"
          >
            Reset Built-in Characters
          </button>
          <button
            type="button"
            class="qt-button character-toolbar__button inline-flex items-center rounded-lg border qt-border-default qt-bg-muted/70 px-4 py-2 text-sm qt-text-primary qt-shadow-sm transition hover:qt-bg-muted"
            (click)="importOpen.set(true)"
          >
            Import from SillyTavern
          </button>
          <button
            type="button"
            class="qt-button character-toolbar__button inline-flex items-center rounded-lg border qt-border-default qt-bg-muted/70 px-4 py-2 text-sm qt-text-primary qt-shadow-sm transition disabled:cursor-not-allowed disabled:opacity-50"
            title="AI generation of character from any text source (not yet available)"
            disabled
          >
            Summon From Lore
          </button>
          <button
            type="button"
            class="qt-button character-toolbar__button inline-flex items-center rounded-lg border qt-border-default qt-bg-muted/70 px-4 py-2 text-sm qt-text-primary qt-shadow-sm transition hover:qt-bg-muted"
            title="Create a new group"
            (click)="groupsSection.openCreate()"
          >
            Create Group
          </button>
          <a
            routerLink="/characters/new"
            class="qt-button character-toolbar__button character-toolbar__button--primary inline-flex items-center rounded-lg bg-primary px-4 py-2 text-sm font-semibold text-primary-foreground qt-shadow-md transition hover:qt-bg-primary/90"
          >
            Create Character
          </a>
        </div>
      </div>

      @if (resetMessage(); as msg) {
        <div class="qt-alert-success mt-4">{{ msg }}</div>
      }

      <!-- Groups section (v4 AuroraView.tsx) — above the characters grid. -->
      <qt-groups-section #groupsSection (openGroup)="selectedGroupId.set($event)" />

      @if (charactersQuery.isPending()) {
        <qt-loading-state message="Loading characters..." class="mt-12" />
      } @else if (charactersQuery.isError()) {
        <qt-error-alert
          [message]="'Error: ' + errorMessage()"
          [retryable]="true"
          class="mt-8"
          (retry)="charactersQuery.refetch()"
        />
      } @else if (visibleCharacters().length === 0) {
        <div
          class="character-empty-state mt-12 rounded-2xl border border-dashed qt-border-default/70 qt-bg-card/80 px-8 py-12 text-center qt-shadow-sm"
        >
          <p class="mb-4 text-lg qt-text-secondary">No characters yet</p>
          <a routerLink="/characters/new" class="qt-text-primary hover:text-primary/80">
            Create your first character
          </a>
        </div>
      } @else {
        <div class="character-card-grid mt-8 grid grid-cols-1 gap-6 md:grid-cols-2 lg:grid-cols-3">
          @for (character of visibleCharacters(); track character.id) {
            <qt-character-card
              [character]="character"
              [profile]="profileFor(character.defaultConnectionProfileId)"
              [inTab]="inTab()"
              (view)="selectedCharacterId.set(character.id)"
              (favorite)="toggleFavorite(character)"
              (toggleCarina)="toggleCarina(character)"
              (toggleControlledBy)="toggleControlledBy(character)"
              (exportCharacter)="exportCharacter(character)"
              (exportPng)="exportPng(character)"
              (deleteCharacter)="deleteTarget.set(character)"
            />
          }
        </div>
      }
    </div>

    @if (importOpen()) {
      <qt-character-import-dialog (close)="importOpen.set(false)" (imported)="onImported()" />
    }

    @if (deleteTarget(); as target) {
      <qt-character-delete-dialog
        [characterId]="target.id"
        [characterName]="target.name"
        (close)="deleteTarget.set(null)"
        (confirmed)="onDeleteConfirm(target, $event)"
      />
    }

    @if (resetOpen()) {
      <qt-reset-builtins-dialog (close)="resetOpen.set(false)" (done)="onReset($event)" />
    }
    }
  `,
})
export class CharactersList {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();
  private readonly quickHide = inject(QuickHideService);
  /** Non-null ⇒ hosted as a workspace tab; card / group opens drill in place. */
  private readonly tabId = inject(WORKSPACE_TAB_ID, { optional: true });

  /** v4 `AuroraView` `selectedCharacterId` / `selectedGroupId` — the drill targets. */
  protected readonly selectedCharacterId = signal<string | null>(null);
  protected readonly selectedGroupId = signal<string | null>(null);
  protected readonly inTab = computed(() => this.tabId != null);

  /** Drilled group's back — restore the list AND refetch (v4 refetches on remount). */
  protected onGroupBack(): void {
    this.selectedGroupId.set(null);
    void this.queryClient.invalidateQueries({ queryKey: characterKeys.list() });
  }

  protected readonly importOpen = signal(false);
  protected readonly resetOpen = signal(false);
  protected readonly resetMessage = signal<string | null>(null);
  protected readonly deleteTarget = signal<CharacterListItem | null>(null);

  protected readonly charactersQuery = injectQuery(() => ({
    queryKey: characterKeys.list(),
    queryFn: (): Promise<CharacterListItem[]> => fetchCharacterList(this.core),
  }));

  private readonly profilesQuery = injectQuery(() => ({
    queryKey: ['connection-profiles'],
    queryFn: (): Promise<CharacterConnectionProfile[]> => fetchConnectionProfiles(this.core),
  }));

  /**
   * v4 `AuroraView.tsx:124-140`: the quick-hide filter runs BEFORE the sort, so
   * hidden characters never influence ordering.
   */
  protected readonly visibleCharacters = computed(() =>
    sortCharacters(
      (this.charactersQuery.data() ?? []).filter(
        (character) => !this.quickHide.shouldHideByIds(character.tags ?? []),
      ),
    ),
  );

  protected profileFor(id: string | null): CharacterConnectionProfile | null {
    if (!id) {
      return null;
    }
    return (this.profilesQuery.data() ?? []).find((p) => p.id === id) ?? null;
  }

  protected errorMessage(): string {
    const err = this.charactersQuery.error();
    return err instanceof Error ? err.message : 'An error occurred';
  }

  // --- Optimistic toggles (v4 `mutateCharacters(updater, {revalidate:false})`) ---

  protected async toggleFavorite(character: CharacterListItem): Promise<void> {
    await this.optimisticToggle(character.id, 'characterFavorite', (c) => ({
      ...c,
      isFavorite: !c.isFavorite,
    }));
  }

  protected async toggleCarina(character: CharacterListItem): Promise<void> {
    await this.optimisticToggle(character.id, 'characterToggleCarina', (c) => ({
      ...c,
      canBeCarina: !c.canBeCarina,
    }));
  }

  protected async toggleControlledBy(character: CharacterListItem): Promise<void> {
    await this.optimisticToggle(character.id, 'characterToggleControlledBy', (c) => ({
      ...c,
      controlledBy: c.controlledBy === 'user' ? 'llm' : 'user',
    }));
  }

  private async optimisticToggle(
    id: string,
    type: 'characterFavorite' | 'characterToggleCarina' | 'characterToggleControlledBy',
    apply: (c: CharacterListItem) => CharacterListItem,
  ): Promise<void> {
    const key = characterKeys.list();
    const previous = this.queryClient.getQueryData<CharacterListItem[]>(key);
    this.queryClient.setQueryData<CharacterListItem[]>(key, (prev) =>
      (prev ?? []).map((c) => (c.id === id ? apply(c) : c)),
    );
    try {
      await this.core.dispatchData({ type, characterId: id });
    } catch {
      // Roll back to server truth on failure (v4 shows an error toast + revalidates).
      this.queryClient.setQueryData<CharacterListItem[]>(key, previous);
      await this.queryClient.invalidateQueries({ queryKey: key });
    }
  }

  protected async exportCharacter(character: CharacterListItem): Promise<void> {
    // Dispatch the JSON export (v4 `?action=export&format=json`) and download the
    // returned ST card client-side as `<name>.json`.
    const card = await fetchCharacterExport(this.core, character.id);
    triggerJsonDownload(`${character.name}.json`, card);
  }

  protected async exportPng(character: CharacterListItem): Promise<void> {
    // Download the SillyTavern PNG card via lane C's binary web route
    // (`?action=export&format=png`). Live at unification.
    await downloadCharacterPng(character.id, character.name);
  }

  protected async onDeleteConfirm(
    character: CharacterListItem,
    choice: DeleteChoice,
  ): Promise<void> {
    try {
      await this.core.dispatchData({
        type: 'characterDelete',
        characterId: character.id,
        cascadeChats: choice.cascadeChats,
        cascadeImages: choice.cascadeImages,
      });
    } finally {
      this.deleteTarget.set(null);
      await this.queryClient.invalidateQueries({ queryKey: characterKeys.list() });
    }
  }

  protected async onImported(): Promise<void> {
    await this.queryClient.invalidateQueries({ queryKey: characterKeys.list() });
  }

  protected async onReset(message: string): Promise<void> {
    this.resetOpen.set(false);
    this.resetMessage.set(message);
    await this.queryClient.invalidateQueries({ queryKey: characterKeys.list() });
  }
}
