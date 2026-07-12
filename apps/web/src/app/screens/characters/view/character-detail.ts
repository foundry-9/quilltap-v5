import {
  ChangeDetectionStrategy,
  Component,
  computed,
  inject,
  signal,
  type WritableSignal,
} from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, RouterLink } from '@angular/router';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';
import { map } from 'rxjs';

import { CoreClient } from '../../../core/core-client';
import type {
  CharacterConnectionProfile,
  CharacterDetail as CharacterDetailDto,
  CharacterGroupBadge,
  CharacterListItem,
  CharacterStats,
} from '../../../core/core-contract';
import { EntityTabs, type Tab } from '../../../ui/entity-tabs';
import { ErrorAlert } from '../../../ui/error-alert';
import { LoadingState } from '../../../ui/loading-state';
import {
  characterKeys,
  fetchCharacter,
  fetchCharacterList,
  fetchCharacterStats,
  fetchConnectionProfiles,
  fetchDefaultPartner,
} from '../characters.api';
import { resolveUserToken } from '../templates';
import { CharacterHeader } from './character-header';
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
    LoadingState,
    ErrorAlert,
    EntityTabs,
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
  ],
  template: `
    <div class="character-view qt-page-container min-h-screen text-foreground">
      <a
        routerLink="/characters"
        class="mb-4 inline-flex items-center qt-label text-primary hover:text-primary/80"
      >
        ← Back to Characters
      </a>

      @if (characterQuery.isPending()) {
        <qt-loading-state message="Loading character..." class="mt-12" />
      } @else if (characterQuery.isError()) {
        <qt-error-alert
          [message]="'Error: ' + errorMessage()"
          [retryable]="true"
          class="mt-8"
          (retry)="characterQuery.refetch()"
        />
      } @else if (character(); as character) {
        <qt-character-header
          [character]="character"
          [stats]="stats()"
          [groups]="groups()"
          [togglingFavorite]="togglingFavorite()"
          [togglingCarina]="togglingCarina()"
          [togglingControlledBy]="togglingControlledBy()"
          [togglingNpc]="togglingNpc()"
          (toggleFavorite)="toggleFavorite()"
          (toggleCarina)="toggleCarina()"
          (toggleControlledBy)="toggleControlledBy()"
          (toggleNpc)="toggleNpc()"
        />

        <qt-entity-tabs [tabs]="tabs" defaultTab="details">
          <ng-template let-active>
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
                <qt-character-wardrobe-tab [characterName]="character.name" />
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
        </qt-entity-tabs>
      }
    </div>
  `,
})
export class CharacterDetail {
  private readonly core = inject(CoreClient);
  private readonly route = inject(ActivatedRoute);
  private readonly queryClient = injectQueryClient();

  protected readonly tabs = CHARACTER_TABS;

  protected readonly id = toSignal(this.route.paramMap.pipe(map((p) => p.get('id') ?? '')), {
    requireSync: true,
  });

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

  protected readonly togglingFavorite = signal(false);
  protected readonly togglingCarina = signal(false);
  protected readonly togglingControlledBy = signal(false);
  protected readonly togglingNpc = signal(false);

  protected errorMessage(): string {
    const err = this.characterQuery.error();
    return err instanceof Error ? err.message : 'An error occurred';
  }

  protected async toggleFavorite(): Promise<void> {
    await this.optimisticToggle(this.togglingFavorite, 'characterFavorite', (c) => ({
      ...c,
      isFavorite: !c.isFavorite,
    }));
  }

  protected async toggleCarina(): Promise<void> {
    await this.optimisticToggle(this.togglingCarina, 'characterToggleCarina', (c) => ({
      ...c,
      canBeCarina: !c.canBeCarina,
    }));
  }

  protected async toggleControlledBy(): Promise<void> {
    await this.optimisticToggle(this.togglingControlledBy, 'characterToggleControlledBy', (c) => ({
      ...c,
      controlledBy: c.controlledBy === 'user' ? 'llm' : 'user',
    }));
  }

  protected async toggleNpc(): Promise<void> {
    const c = this.character();
    if (!c) return;
    const key = characterKeys.detail(this.id());
    this.togglingNpc.set(true);
    this.queryClient.setQueryData<CharacterDetailDto>(key, { ...c, npc: !c.npc });
    try {
      await this.core.dispatchData({
        type: 'characterUpdate',
        characterId: this.id(),
        character: { npc: !c.npc },
      });
    } catch {
      this.queryClient.setQueryData<CharacterDetailDto>(key, c);
    } finally {
      this.togglingNpc.set(false);
    }
  }

  private async optimisticToggle(
    busy: WritableSignal<boolean>,
    type: 'characterFavorite' | 'characterToggleCarina' | 'characterToggleControlledBy',
    apply: (c: CharacterDetailDto) => CharacterDetailDto,
  ): Promise<void> {
    const c = this.character();
    if (!c) return;
    const key = characterKeys.detail(this.id());
    const previous = c;
    busy.set(true);
    this.queryClient.setQueryData<CharacterDetailDto>(key, apply(previous));
    try {
      await this.core.dispatchData({ type, characterId: this.id() });
    } catch {
      this.queryClient.setQueryData<CharacterDetailDto>(key, previous);
    } finally {
      busy.set(false);
    }
  }
}
