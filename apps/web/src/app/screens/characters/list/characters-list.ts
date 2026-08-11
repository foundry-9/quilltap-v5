import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  signal,
  untracked,
} from '@angular/core';
import { RouterLink } from '@angular/router';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { WORKSPACE_HANDLE, WORKSPACE_TAB_ID } from '../../../workspace/workspace-contract';
import { CoreClient } from '../../../core/core-client';
import type { CharacterConnectionProfile, CharacterListItem } from '../../../core/core-contract';
import { QuickHideService } from '../../../quick-hide/quick-hide.service';
import { ErrorAlert } from '../../../ui/error-alert';
import { Icon } from '../../../ui/icon';
import { LoadingState } from '../../../ui/loading-state';
import { ToastService } from '../../../ui/toast.service';
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

/** v4 `AuroraView.tsx:188-199`: the delete-success sentence, pluralized per count. */
export function formatCharacterDeleteSuccess(data: Record<string, unknown>): string {
  const parts: string[] = ['Character deleted'];
  const chats = Number(data['deletedChats'] ?? 0);
  const images = Number(data['deletedImages'] ?? 0);
  const memories = Number(data['deletedMemories'] ?? 0);
  if (chats > 0) {
    parts.push(`${chats} chat${chats === 1 ? '' : 's'} deleted`);
  }
  if (images > 0) {
    parts.push(`${images} image${images === 1 ? '' : 's'} deleted`);
  }
  if (memories > 0) {
    parts.push(`${memories} memor${memories === 1 ? 'y' : 'ies'} deleted`);
  }
  return parts.join('. ');
}

/**
 * v4 `AuroraView.tsx:125-140` sort: archived last → NPCs last → favorites first
 * → chats desc → name.
 *
 * P4.D64 added rule 0 AHEAD of the other three (v4 `:147-153`): "Archived
 * characters at the very end of the shelf". It outranks every other key, so a
 * favorite with nine chats still sinks below a plain NPC once archived.
 */
export function sortCharacters(list: CharacterListItem[]): CharacterListItem[] {
  return [...list].sort((a, b) => {
    const aArchived = Boolean(a.archivedAt);
    const bArchived = Boolean(b.archivedAt);
    if (aArchived !== bArchived) {
      return aArchived ? 1 : -1;
    }
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
    Icon,
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
      <!-- In-tab drill (v4 AuroraView selectedCharacterId): the detail in place.
           openChatForSelected auto-opens the start-chat flow (v4 :325-335). -->
      <qt-character-detail
        [characterId]="cid"
        [embedded]="true"
        [openChatOnMount]="openChatForSelected()"
        (back)="onDetailBack()"
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
            <!-- P4.D64: "Show Archived" (v4 AuroraView.tsx:402-409) — FIRST in
                 the toolbar row, plain local state with no URL component. -->
            <button
              type="button"
              [class]="
                'qt-button character-toolbar__button inline-flex items-center gap-1.5 rounded-lg border qt-border-default px-4 py-2 text-sm qt-shadow-sm transition focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ' +
                (showArchived()
                  ? 'qt-bg-primary/20 qt-text-primary'
                  : 'qt-bg-muted/70 qt-text-primary hover:qt-bg-muted')
              "
              [title]="
                showArchived()
                  ? 'Tuck the archived characters back out of sight'
                  : 'Show characters resting in the archive'
              "
              (click)="showArchived.set(!showArchived())"
            >
              <qt-icon name="folder" class="w-4 h-4" />
              {{ showArchived() ? 'Hide Archived' : 'Show Archived' }}
            </button>
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
              [routerLink]="inTab() ? null : '/characters/new'"
              (click)="inTab() && openNewCharacter($event)"
              class="qt-button character-toolbar__button character-toolbar__button--primary inline-flex items-center rounded-lg bg-primary px-4 py-2 text-sm font-semibold text-primary-foreground qt-shadow-md transition hover:qt-bg-primary/90"
            >
              Create Character
            </a>
          </div>
        </div>

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
            <a
              [routerLink]="inTab() ? null : '/characters/new'"
              (click)="inTab() && openNewCharacter($event)"
              class="qt-text-primary hover:text-primary/80"
            >
              Create your first character
            </a>
          </div>
        } @else {
          <div
            class="character-card-grid mt-8 grid grid-cols-1 gap-6 md:grid-cols-2 lg:grid-cols-3"
          >
            @for (character of visibleCharacters(); track character.id) {
              <qt-character-card
                [character]="character"
                [profile]="profileFor(character.defaultConnectionProfileId)"
                [inTab]="inTab()"
                (view)="drillView(character.id)"
                (openChat)="drillChat(character.id)"
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
        <qt-reset-builtins-dialog (close)="resetOpen.set(false)" (done)="onReset()" />
      }
    }
  `,
})
export class CharactersList {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();
  private readonly quickHide = inject(QuickHideService);
  private readonly toasts = inject(ToastService);
  /** Non-null ⇒ hosted as a workspace tab; card / group opens drill in place. */
  private readonly tabId = inject(WORKSPACE_TAB_ID, { optional: true });
  /** Hosted ⇒ Create-Character opens a tab rather than routing (v4 openTab). */
  private readonly handle = inject(WORKSPACE_HANDLE, { optional: true });

  /** v4 `AuroraView` `selectedCharacterId` / `selectedGroupId` — the drill targets. */
  protected readonly selectedCharacterId = signal<string | null>(null);
  protected readonly selectedGroupId = signal<string | null>(null);
  /** v4 `AuroraView` `openChatForSelected` (`:87`): the drilled detail's Chat
   *  auto-open, set only by the card's in-tab Chat arm. */
  protected readonly openChatForSelected = signal(false);
  protected readonly inTab = computed(() => this.tabId != null);

  /**
   * Deep-link target (v4 `8d86847a` `AuroraViewProps.initialGroupId`):
   * `/characters/groups/<id>` (v4 `/aurora/groups/<id>`) redirects into the
   * Characters tab carrying this id, and a RE-open of the already-open tab
   * refreshes the payload — so the effect follows every truthy change, not just
   * the first (v4's derive-from-prop-change guard does the same). A drill-back
   * leaves the input untouched, so the effect does not re-run.
   */
  readonly initialGroupId = input<string | null>(null);

  constructor() {
    effect(() => {
      const id = this.initialGroupId();
      if (id) untracked(() => this.selectedGroupId.set(id));
    });
  }

  /** Drill into a character's detail (v4 `setSelectedCharacterId`). */
  protected drillView(id: string): void {
    this.openChatForSelected.set(false);
    this.selectedCharacterId.set(id);
  }

  /** Card Chat drill (v4 `:509-521`): drill AND flag the start-chat auto-open. */
  protected drillChat(id: string): void {
    this.openChatForSelected.set(true);
    this.selectedCharacterId.set(id);
  }

  /** Restore the roster (v4 `onBack`, `:328-331`): clear the drill + the flag. */
  protected onDetailBack(): void {
    this.selectedCharacterId.set(null);
    this.openChatForSelected.set(false);
  }

  /** Hosted ⇒ Create-Character opens a `character-new` tab (v4 redirectToWorkspaceTab). */
  protected openNewCharacter(event: Event): void {
    event.preventDefault();
    this.handle?.openTab('character-new');
  }

  /** Drilled group's back — restore the list AND refetch (v4 refetches on remount). */
  protected onGroupBack(): void {
    this.selectedGroupId.set(null);
    void this.queryClient.invalidateQueries({ queryKey: characterKeys.all });
  }

  protected readonly importOpen = signal(false);
  protected readonly resetOpen = signal(false);
  protected readonly deleteTarget = signal<CharacterListItem | null>(null);

  /**
   * P4.D64 (v4 `AuroraView.tsx:47-49`): "Show archived" opts into the tombstones
   * the API hides by default. Plain local state — v4 puts nothing in the URL, so
   * a reload lands back on the live shelf.
   */
  protected readonly showArchived = signal(false);

  /**
   * The active list key / fetch filter. The two toggle states cache under
   * DISTINCT keys so they never cross-contaminate (v4 `:53`), and the param is
   * OMITTED entirely when the toggle is off — absent IS the exclude default.
   */
  private readonly listFilter = computed(() =>
    this.showArchived() ? ({ archived: 'include' } as const) : undefined,
  );

  protected readonly charactersQuery = injectQuery(() => ({
    queryKey: characterKeys.list(this.listFilter()),
    queryFn: (): Promise<CharacterListItem[]> =>
      fetchCharacterList(this.core, this.listFilter()),
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
    await this.optimisticToggle(
      character.id,
      'characterFavorite',
      (c) => ({ ...c, isFavorite: !c.isFavorite }),
      'Failed to toggle favorite',
    );
  }

  protected async toggleCarina(character: CharacterListItem): Promise<void> {
    await this.optimisticToggle(
      character.id,
      'characterToggleCarina',
      (c) => ({ ...c, canBeCarina: !c.canBeCarina }),
      'Failed to toggle Carina eligibility',
    );
  }

  protected async toggleControlledBy(character: CharacterListItem): Promise<void> {
    await this.optimisticToggle(
      character.id,
      'characterToggleControlledBy',
      (c) => ({ ...c, controlledBy: c.controlledBy === 'user' ? 'llm' : 'user' }),
      'Failed to toggle controlled-by',
    );
  }

  private async optimisticToggle(
    id: string,
    type: 'characterFavorite' | 'characterToggleCarina' | 'characterToggleControlledBy',
    apply: (c: CharacterListItem) => CharacterListItem,
    failureMessage: string,
  ): Promise<void> {
    // The ACTIVE key, not the unfiltered one: with "Show Archived" on, the rows
    // on screen live under `list({archived:'include'})` (v4 `:66` `activeKey`).
    const key = characterKeys.list(this.listFilter());
    const previous = this.queryClient.getQueryData<CharacterListItem[]>(key);
    this.queryClient.setQueryData<CharacterListItem[]>(key, (prev) =>
      (prev ?? []).map((c) => (c.id === id ? apply(c) : c)),
    );
    try {
      await this.core.dispatchData({ type, characterId: id });
    } catch (err) {
      // Roll back to server truth on failure (v4 `AuroraView.tsx:206-249`).
      this.queryClient.setQueryData<CharacterListItem[]>(key, previous);
      // Prefix invalidation so the other archived-filter variant refreshes too
      // (v4 `:72-73`).
      await this.queryClient.invalidateQueries({ queryKey: characterKeys.all });
      this.toasts.showError(err instanceof Error ? err.message : failureMessage);
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
      const data = await this.core.dispatchData({
        type: 'characterDelete',
        characterId: character.id,
        cascadeChats: choice.cascadeChats,
        cascadeImages: choice.cascadeImages,
      });
      this.deleteTarget.set(null);
      await this.queryClient.invalidateQueries({ queryKey: characterKeys.all });
      this.toasts.showSuccess(formatCharacterDeleteSuccess(data));
    } catch (err) {
      // v4 `AuroraView.tsx:201-203` leaves the dialog open and does not refetch.
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to delete character');
    }
  }

  protected async onImported(): Promise<void> {
    await this.queryClient.invalidateQueries({ queryKey: characterKeys.all });
  }

  protected async onReset(): Promise<void> {
    this.resetOpen.set(false);
    await this.queryClient.invalidateQueries({ queryKey: characterKeys.all });
  }
}
