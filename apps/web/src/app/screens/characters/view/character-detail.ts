import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
  untracked,
  type WritableSignal,
} from '@angular/core';
import { NgTemplateOutlet } from '@angular/common';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';
import { map } from 'rxjs';

import {
  WORKSPACE_HANDLE,
  WORKSPACE_TAB_ID,
  onTabActivated,
} from '../../../workspace/workspace-contract';
import { CoreClient } from '../../../core/core-client';
import type {
  CharacterConnectionProfile,
  CharacterDetail as CharacterDetailDto,
  CharacterGroupBadge,
  CharacterListItem,
  CharacterStats,
} from '../../../core/core-contract';
import { HiddenPlaceholder } from '../../../quick-hide/hidden-placeholder';
import { QuickHideService } from '../../../quick-hide/quick-hide.service';
import { EntityTabs, type Tab } from '../../../ui/entity-tabs';
import { ErrorAlert } from '../../../ui/error-alert';
import { LoadingState } from '../../../ui/loading-state';
import { ToastService } from '../../../ui/toast.service';
import {
  archiveCharacter,
  characterKeys,
  fetchCharacter,
  fetchCharacterList,
  fetchCharacterStats,
  fetchConnectionProfiles,
  fetchDefaultPartner,
  rehydrateCharacter,
} from '../characters.api';
import { memoryKeys } from '../../../memory/memory.api';
import { resolveUserToken } from '../templates';
import { ArchiveCharacterDialog } from './archive-character-dialog';
import { CharacterHeader } from './character-header';
import { RehydrateBundleDialog } from './rehydrate-bundle-dialog';
import { CharacterAppearanceTab } from './tabs/character-appearance-tab';
import { CharacterConversationsTab } from './tabs/conversations-tab';
import { CharacterDefaultsTab } from './tabs/defaults-tab';
import { CharacterDetailsTab } from './tabs/details-tab';
import { CharacterGalleryTab } from './tabs/gallery-tab';
import { CharacterMemoriesTab } from './tabs/memories-tab';
import { CharacterSystemPromptsTab } from './tabs/system-prompts-tab';
import { CharacterTagsTab } from './tabs/tags-tab';
import { CharacterWardrobeTab } from './tabs/wardrobe-tab';

/** The nine tabs (v4 `app/aurora/[id]/view/constants.ts` `CHARACTER_TABS`). */
const CHARACTER_TABS: Tab[] = [
  { id: 'details', label: 'Details', icon: 'user' },
  { id: 'system-prompts', label: 'System Prompts', icon: 'code' },
  { id: 'conversations', label: 'Conversations', icon: 'chat' },
  { id: 'memories', label: 'Memories', icon: 'book' },
  { id: 'tags', label: 'Tags', icon: 'tag' },
  { id: 'wardrobe', label: 'Wardrobe', icon: 'wardrobe' },
  { id: 'defaults', label: 'Default Settings', icon: 'settings' },
  { id: 'gallery', label: 'Photo Gallery', icon: 'photos' },
  { id: 'descriptions', label: 'Appearance', icon: 'file' },
];

/**
 * The Characters DETAIL/VIEW screen (v4 `app/aurora/[id]/view/CharacterDetailView.tsx`):
 * the "← Back to Characters" link, the header card, and the nine-tab hall.
 * Parallel-loads the character, connection profiles, the user-controlled
 * partner options, the resolved default partner, and the header stats. The
 * three header toggles are optimistic (same dispatch names as the list
 * screen); "Convert to NPC/Character" rides the general `characterUpdate` PUT.
 * The image-generation-profile picker (Default Settings tab) has no contract
 * variant this round and renders disabled — see the P4.6g report.
 */
@Component({
  selector: 'qt-character-detail',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    RouterLink,
    NgTemplateOutlet,
    LoadingState,
    ErrorAlert,
    EntityTabs,
    ArchiveCharacterDialog,
    RehydrateBundleDialog,
    CharacterHeader,
    CharacterDetailsTab,
    CharacterSystemPromptsTab,
    CharacterConversationsTab,
    CharacterMemoriesTab,
    CharacterTagsTab,
    CharacterWardrobeTab,
    CharacterDefaultsTab,
    CharacterGalleryTab,
    CharacterAppearanceTab,
    HiddenPlaceholder,
  ],
  template: `
    <div class="character-view qt-page-container min-h-screen text-foreground">
      @if (embedded() || canClose()) {
        <!-- Hosted: drilled inside the aurora list ⇒ back restores the list (v4
             onBack); standalone tab ⇒ back closes the tab (v4 CharacterViewTab). -->
        <button
          type="button"
          class="mb-4 inline-flex items-center qt-label text-primary hover:text-primary/80"
          (click)="onBack()"
        >
          ← Back to Characters
        </button>
      } @else {
        <a
          routerLink="/characters"
          class="mb-4 inline-flex items-center qt-label text-primary hover:text-primary/80"
        >
          ← Back to Characters
        </a>
      }

      @if (characterQuery.isPending()) {
        <qt-loading-state message="Loading character..." class="mt-12" />
      } @else if (characterQuery.isError()) {
        <qt-error-alert
          [message]="'Error: ' + errorMessage()"
          [retryable]="true"
          class="mt-8"
          (retry)="characterQuery.refetch()"
        />
      } @else if (isQuickHidden()) {
        <div class="flex min-h-screen items-center justify-center qt-bg-muted">
          <qt-hidden-placeholder />
        </div>
      } @else if (character(); as character) {
        @if (isArchived()) {
          <!-- P4.D64 (v4 CharacterDetailView.tsx:331-346): the archived banner,
               above the header card. -->
          <div
            class="mb-6 rounded-2xl border qt-border-default qt-bg-muted/60 p-4 flex items-start gap-3"
          >
            <span class="text-2xl" aria-hidden="true">🗄️</span>
            <div class="flex-1">
              <p class="qt-text-label text-foreground">
                {{ character.name || 'This character' }} rests in the archive.
              </p>
              <p class="qt-text-small qt-text-secondary mt-1">
                Their effects — memories, correspondence, photographs, summaries — are packed into a
                sealed bundle on the shelf. What you see below is kept for the record and may be read
                but not altered; rehydrate them to pick up the pen again. Old conversations keep
                their words and their face throughout.
              </p>
            </div>
          </div>
        }

        <qt-character-header
          [character]="character"
          [stats]="stats()"
          [groups]="groups()"
          [togglingFavorite]="togglingFavorite()"
          [togglingCarina]="togglingCarina()"
          [togglingControlledBy]="togglingControlledBy()"
          [togglingNpc]="togglingNpc()"
          [rehydrating]="rehydrating()"
          (toggleFavorite)="toggleFavorite()"
          (toggleCarina)="toggleCarina()"
          (toggleControlledBy)="toggleControlledBy()"
          (toggleNpc)="toggleNpc()"
          (archive)="archiveDialogOpen.set(true)"
          (rehydrate)="handleRehydrate()"
        />

        <!-- Tabbed content. For an archived character the content renders inside
             a disabled fieldset: every form control inert, the page still
             readable. The repository write guard is the real lock — this is the
             courtesy (v4 CharacterDetailView.tsx:371-373). -->
        <qt-entity-tabs [tabs]="tabs" [defaultTab]="tab() ?? 'details'">
          <ng-template let-active>
            @if (isArchived()) {
              <fieldset disabled class="opacity-90">
                <ng-container
                  [ngTemplateOutlet]="tabBody"
                  [ngTemplateOutletContext]="{ $implicit: active }"
                />
              </fieldset>
            } @else {
              <ng-container
                [ngTemplateOutlet]="tabBody"
                [ngTemplateOutletContext]="{ $implicit: active }"
              />
            }
          </ng-template>
        </qt-entity-tabs>

        <ng-template #tabBody let-active>
            @switch (active) {
              @case ('details') {
                <qt-character-details-tab
                  [characterId]="id()"
                  [character]="character"
                  [defaultPartnerName]="defaultPartnerName()"
                  [userControlledCharacters]="userControlledCharacters()"
                />
              }
              @case ('system-prompts') {
                <qt-character-system-prompts-tab
                  [characterId]="id()"
                  [character]="character"
                  [defaultPartnerName]="defaultPartnerName()"
                />
              }
              @case ('conversations') {
                <qt-character-conversations-tab
                  [characterId]="id()"
                  [characterName]="character.name"
                />
              }
              @case ('memories') {
                <qt-character-memories-tab [characterId]="id()" />
              }
              @case ('tags') {
                <qt-character-tags-tab [characterId]="id()" />
              }
              @case ('wardrobe') {
                <qt-character-wardrobe-tab
                  [characterId]="id()"
                  [characterName]="character.name"
                  [canChooseOutfit]="character.canChooseOutfit ?? false"
                />
              }
              @case ('defaults') {
                <qt-character-defaults-tab
                  [characterId]="id()"
                  [character]="character"
                  [connectionProfiles]="connectionProfiles()"
                  [userControlledCharacters]="userControlledCharacters()"
                  [defaultPartnerId]="defaultPartnerId()"
                />
              }
              @case ('gallery') {
                <qt-character-gallery-tab [characterId]="id()" />
              }
              @case ('descriptions') {
                <qt-character-appearance-tab [characterId]="id()" [character]="character" />
              }
            }
        </ng-template>

        @if (archiveDialogOpen()) {
          <qt-archive-character-dialog
            [characterName]="character.name"
            [working]="archiving()"
            (confirm)="handleArchiveConfirm()"
            (cancel)="archiveDialogOpen.set(false)"
          />
        }

        <!-- Post-rehydrate bundle disposal (v4's spec §6 step 6). -->
        @if (leftoverBundleFileId(); as bundleId) {
          <qt-rehydrate-bundle-dialog
            [characterName]="character.name"
            [bundleFileId]="bundleId"
            (closed)="leftoverBundleFileId.set(null)"
            (deleted)="onBundleDeleted()"
          />
        }
      }
    </div>
  `,
})
export class CharacterDetail {
  private readonly core = inject(CoreClient);
  private readonly route = inject(ActivatedRoute, { optional: true });
  private readonly router = inject(Router);
  private readonly queryClient = injectQueryClient();
  private readonly quickHide = inject(QuickHideService);
  private readonly toasts = inject(ToastService);
  /** Workspace-tab seams (p4.9j2); null ⇒ routed mode. */
  private readonly handle = inject(WORKSPACE_HANDLE, { optional: true });
  private readonly tabId = inject(WORKSPACE_TAB_ID, { optional: true });

  /**
   * Tab-mode inputs (v4 `CharacterViewTabPayload`): `characterId` supplies the
   * identity in place of the route `:id`; `tab` deep-links a sub-tab. Null ⇒
   * routed mode, byte-identical.
   */
  readonly characterIdInput = input<string | null>(null, { alias: 'characterId' });
  readonly tab = input<string | null>(null);
  /**
   * Drilled inside the aurora list (v4 `AuroraView` `selectedCharacterId`) ⇒
   * back restores the list via `(back)` rather than closing a tab.
   */
  readonly embedded = input<boolean>(false);
  /**
   * v4 `CharacterDetailView` `openChatOnMount` (`:44,:136-143`): open the
   * start-chat flow ONCE as soon as the character loads. Set true by the
   * in-tab drill arm (the roster card's Chat action); the routed `?action=chat`
   * deep-link is honored in parallel below.
   */
  readonly openChatOnMount = input<boolean>(false);
  readonly back = output<void>();

  /** Both seams present ⇒ hosted as a standalone workspace tab; "back" closes it. */
  protected canClose(): boolean {
    return this.handle != null && this.tabId != null;
  }

  /** Drill ⇒ restore the list; standalone tab ⇒ close it (v4 `CharacterViewTab`). */
  protected onBack(): void {
    if (this.embedded()) {
      this.back.emit();
      return;
    }
    if (this.handle && this.tabId != null) this.handle.closeTab(this.tabId);
  }

  /**
   * v4 `CharacterDetailView.tsx:54,:316` — the deep-link guard. Note v4's extra
   * `quickHideActive` arm (`hiddenTagIds.size > 0`): with nothing hidden the
   * check is skipped entirely, so a character whose tags are empty can never be
   * accidentally placeholdered.
   */
  protected readonly isQuickHidden = computed(() => {
    const character = this.character();
    if (!character || this.quickHide.hiddenTagIds().size === 0) {
      return false;
    }
    return this.quickHide.shouldHideByIds(character.tags ?? []);
  });

  protected readonly tabs = CHARACTER_TABS;

  private readonly routeId = this.route
    ? toSignal(this.route.paramMap.pipe(map((p) => p.get('id') ?? '')), { requireSync: true })
    : undefined;
  protected readonly id = computed(() => this.characterIdInput() ?? this.routeId?.() ?? '');

  /**
   * v4 `page.tsx:21` — the routed `?action=chat` deep-link. Reactive (not a
   * snapshot) so the header's own "Start Chat" link, which appends the param on
   * the same route, re-triggers the guard without a remount. Null route (tab
   * mode) ⇒ never.
   */
  private readonly routedChatAction = this.route
    ? toSignal(this.route.queryParamMap.pipe(map((p) => p.get('action') === 'chat')), {
        initialValue: false,
      })
    : undefined;
  private chatStartInitialized = false;

  constructor() {
    // v4 `CharacterDetailView.tsx:136-143` — auto-open the start-chat flow ONCE
    // on mount when arriving via `?action=chat` (routed) or `openChatOnMount`
    // (the in-tab drill), and only after the character has loaded. v5 DIVERGENCE:
    // there is no in-place NewChatModal — starting a chat navigates to the
    // established `/salon/new?characterId=` entry (the P4.6q precedent the home
    // character card also uses). The guard field keeps it to a single fire.
    effect(() => {
      const want = this.openChatOnMount() || (this.routedChatAction?.() ?? false);
      const character = this.character();
      if (!want || this.chatStartInitialized || !character) return;
      this.chatStartInitialized = true;
      untracked(() =>
        this.router.navigate(['/salon/new'], { queryParams: { characterId: this.id() } }),
      );
    });

    // Navigating back to the containing workspace tab refreshes the character
    // and everything v4 keys off `dataRefreshKey` (v4
    // `CharacterDetailView.tsx:202-208`). The tab-activation map already sweeps
    // detail / prompts / photos for this kind; v4 additionally bumps a React
    // counter that re-runs the stats fetch, the Conversations tab and the
    // Memories tab, which in v5 are three separate query keys — so they are
    // named here rather than left unrefreshed. No `loading` flips: TanStack
    // keeps the previous data on screen while it refetches.
    onTabActivated(() => {
      const id = this.id();
      if (!id) return;
      for (const queryKey of [
        characterKeys.stats(id),
        characterKeys.chats(id),
        memoryKeys.list(id),
      ]) {
        void this.queryClient.invalidateQueries({ queryKey });
      }
    });
  }

  protected readonly characterQuery = injectQuery(() => ({
    queryKey: characterKeys.detail(this.id()),
    queryFn: (): Promise<CharacterDetailDto> => fetchCharacter(this.core, this.id()),
  }));

  private readonly profilesQuery = injectQuery(() => ({
    queryKey: ['connection-profiles'],
    queryFn: (): Promise<CharacterConnectionProfile[]> => fetchConnectionProfiles(this.core),
  }));

  private readonly userControlledQuery = injectQuery(() => ({
    queryKey: characterKeys.list({ controlledBy: 'user' }),
    queryFn: (): Promise<CharacterListItem[]> =>
      fetchCharacterList(this.core, { controlledBy: 'user' }),
  }));

  private readonly defaultPartnerQuery = injectQuery(() => ({
    queryKey: characterKeys.defaultPartner(this.id()),
    queryFn: (): Promise<string | null> => fetchDefaultPartner(this.core, this.id()),
  }));

  private readonly statsQuery = injectQuery(() => ({
    queryKey: characterKeys.stats(this.id()),
    queryFn: (): Promise<{ stats: CharacterStats; groups: CharacterGroupBadge[] }> =>
      fetchCharacterStats(this.core, this.id()),
  }));

  protected readonly character = computed(() => this.characterQuery.data() ?? null);
  protected readonly connectionProfiles = computed(() => this.profilesQuery.data() ?? []);
  protected readonly userControlledCharacters = computed(
    () => this.userControlledQuery.data() ?? [],
  );
  protected readonly defaultPartnerId = computed(
    () => this.defaultPartnerQuery.data() ?? this.character()?.defaultPartnerId ?? null,
  );
  protected readonly stats = computed(() => this.statsQuery.data()?.stats ?? null);
  protected readonly groups = computed(() => this.statsQuery.data()?.groups ?? []);

  /** v4 `AuroraView.tsx:498-505` — the `{{user}}` token resolution. */
  protected readonly defaultPartnerName = computed(() => {
    const partnerId = this.defaultPartnerId();
    const partner = this.userControlledCharacters().find((c) => c.id === partnerId);
    const character = this.character();
    return character
      ? resolveUserToken(character.controlledBy, character.name, partner?.name)
      : null;
  });

  // --- The archive family (P4.D64; v4 `CharacterDetailView.tsx:266-322`) ---

  /** v4 `isArchived = Boolean(character?.archivedAt)`. */
  protected readonly isArchived = computed(() => Boolean(this.character()?.archivedAt));
  protected readonly archiveDialogOpen = signal(false);
  protected readonly archiving = signal(false);
  protected readonly rehydrating = signal(false);
  /**
   * Set after a successful rehydrate: the ARCHIVE bundle left on the shelf,
   * whose disposal the user gets to decide (v4's spec §6 step 6).
   */
  protected readonly leftoverBundleFileId = signal<string | null>(null);

  /**
   * Archive (v4 `handleArchiveConfirm`, `:274-293`). On success: close the
   * dialog, refetch the character, nudge the header stats (the memory count just
   * changed), invalidate the characters PREFIX, and toast. On failure: leave the
   * dialog OPEN and toast the server's sentence.
   */
  protected async handleArchiveConfirm(): Promise<void> {
    const name = this.character()?.name || 'The character';
    this.archiving.set(true);
    try {
      const result = await archiveCharacter(this.core, this.id());
      this.archiveDialogOpen.set(false);
      await this.characterQuery.refetch();
      await this.statsQuery.refetch();
      await this.queryClient.invalidateQueries({ queryKey: characterKeys.all });
      this.toasts.showSuccess(
        result.pruneComplete === false
          ? `${name} is archived, but some effects resisted packing — archive them again to finish the sweep.`
          : `${name} rests in the archive, bundle sealed and shelved.`,
      );
    } catch (err) {
      this.toasts.showError(
        err instanceof Error && err.message ? err.message : 'Failed to archive character',
      );
    } finally {
      this.archiving.set(false);
    }
  }

  /**
   * Rehydrate (v4 `handleRehydrate`, `:295-322`). The restored-counts toast when
   * the body carries them, the bare one otherwise; a NON-EMPTY
   * `archiveBundleFileId` opens the disposal dialog.
   */
  protected async handleRehydrate(): Promise<void> {
    const name = this.character()?.name || 'The character';
    this.rehydrating.set(true);
    try {
      const result = await rehydrateCharacter(this.core, this.id());
      await this.characterQuery.refetch();
      await this.statsQuery.refetch();
      await this.queryClient.invalidateQueries({ queryKey: characterKeys.all });
      const restored = result.restored;
      this.toasts.showSuccess(
        restored
          ? `${name} is awake again — ${restored.memories} memories, ${restored.documents} papers and ${restored.blobs} photographs unpacked.`
          : `${name} is awake again.`,
      );
      if (typeof result.archiveBundleFileId === 'string' && result.archiveBundleFileId) {
        this.leftoverBundleFileId.set(result.archiveBundleFileId);
      }
    } catch (err) {
      this.toasts.showError(
        err instanceof Error && err.message ? err.message : 'Failed to rehydrate character',
      );
    } finally {
      this.rehydrating.set(false);
    }
  }

  /** v4 `:401` — the dialog's `onDeleted` toast. */
  protected onBundleDeleted(): void {
    this.toasts.showSuccess('The bundle is off the shelf.');
  }

  protected readonly togglingFavorite = signal(false);
  protected readonly togglingCarina = signal(false);
  protected readonly togglingControlledBy = signal(false);
  protected readonly togglingNpc = signal(false);

  protected errorMessage(): string {
    const err = this.characterQuery.error();
    return err instanceof Error ? err.message : 'An error occurred';
  }

  protected async toggleFavorite(): Promise<void> {
    await this.optimisticToggle(
      this.togglingFavorite,
      'characterFavorite',
      (c) => ({ ...c, isFavorite: !c.isFavorite }),
      'Failed to toggle favorite',
    );
  }

  protected async toggleCarina(): Promise<void> {
    await this.optimisticToggle(
      this.togglingCarina,
      'characterToggleCarina',
      (c) => ({ ...c, canBeCarina: !c.canBeCarina }),
      'Failed to toggle Carina eligibility',
    );
  }

  protected async toggleControlledBy(): Promise<void> {
    await this.optimisticToggle(
      this.togglingControlledBy,
      'characterToggleControlledBy',
      (c) => ({ ...c, controlledBy: c.controlledBy === 'user' ? 'llm' : 'user' }),
      'Failed to toggle controlled-by',
    );
  }

  /** v4 `useCharacterView.ts:606-625` — "Convert to NPC/Character" toasts both ways. */
  protected async toggleNpc(): Promise<void> {
    const c = this.character();
    if (!c) return;
    const key = characterKeys.detail(this.id());
    const nextNpc = !c.npc;
    this.togglingNpc.set(true);
    this.queryClient.setQueryData<CharacterDetailDto>(key, { ...c, npc: nextNpc });
    try {
      await this.core.dispatchData({
        type: 'characterUpdate',
        characterId: this.id(),
        character: { npc: nextNpc },
      });
      this.toasts.showSuccess(nextNpc ? 'Converted to NPC' : 'Converted to Character');
    } catch (err) {
      this.queryClient.setQueryData<CharacterDetailDto>(key, c);
      this.toasts.showError(err instanceof Error ? err.message : 'Failed to toggle NPC status');
    } finally {
      this.togglingNpc.set(false);
    }
  }

  private async optimisticToggle(
    busy: WritableSignal<boolean>,
    type: 'characterFavorite' | 'characterToggleCarina' | 'characterToggleControlledBy',
    apply: (c: CharacterDetailDto) => CharacterDetailDto,
    failureMessage: string,
  ): Promise<void> {
    const c = this.character();
    if (!c) return;
    const key = characterKeys.detail(this.id());
    const previous = c;
    busy.set(true);
    this.queryClient.setQueryData<CharacterDetailDto>(key, apply(previous));
    try {
      await this.core.dispatchData({ type, characterId: this.id() });
    } catch (err) {
      this.queryClient.setQueryData<CharacterDetailDto>(key, previous);
      this.toasts.showError(err instanceof Error ? err.message : failureMessage);
    } finally {
      busy.set(false);
    }
  }
}
