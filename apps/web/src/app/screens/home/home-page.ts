import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../core/core-client';
import type { CharacterConnectionProfile } from '../../core/core-contract';
import { fetchConnectionProfiles } from '../characters/characters.api';
import { CharactersSection } from './characters-section';
import { ProjectsSection } from './projects-section';
import { QuickActionsRow } from './quick-actions-row';
import { RecentChatsSection } from './recent-chats-section';
import { fetchHome, homeKeys } from './home.api';

/**
 * The Home dashboard at `/` (v4 `components/homepage/HomeView.tsx` +
 * `HomeViewContainer.tsx`'s fetch-side states): the welcome greeting (v4
 * `WelcomeSection.tsx`, inlined — it's one heading), the quick-actions row,
 * and the three-column grid (Recent Chats / Active Projects / Characters),
 * fed by ONE `systemHome` fetch (§1). The loading / error copy is v4's
 * verbatim. v4's tabbed-workspace glue (`HomeViewContainer`'s reason to
 * exist) is not ported — that family is unowed here.
 *
 * A second, non-blocking `connectionProfileList` query feeds the character
 * cards' provider badges (v4 `useConnectionProfiles`) — its failure just
 * means no badges.
 */
@Component({
  selector: 'qt-home-page',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [QuickActionsRow, RecentChatsSection, ProjectsSection, CharactersSection],
  template: `
    @if (homeQuery.isPending()) {
      <div class="flex h-full items-center justify-center">
        <p class="qt-text-muted text-sm">Setting the table…</p>
      </div>
    } @else if (homeQuery.isError() || !home()) {
      <div class="flex h-full items-center justify-center">
        <p class="qt-text-destructive text-sm">
          The home parlour could not be readied just now.
        </p>
      </div>
    } @else if (home(); as home) {
      <div class="qt-homepage-container">
        <!-- Welcome section (v4 WelcomeSection.tsx) -->
        <div class="text-center py-6">
          <h1 class="qt-heading-1 mb-2">
            Welcome back, <span class="text-primary">{{ home.displayName }}</span
            >!
          </h1>
          <p class="qt-text-secondary">What would you like to do today?</p>
        </div>

        <!-- Quick action buttons -->
        <qt-quick-actions-row [lastChatId]="home.lastChatId" />

        <!-- Three-column grid - fills remaining space -->
        <div class="qt-homepage-grid">
          <qt-recent-chats-section [chats]="home.recentChats" />
          <qt-projects-section [projects]="home.projects" />
          <qt-characters-section [characters]="home.characters" [profiles]="profiles()" />
        </div>
      </div>
    }
  `,
})
export class HomePage {
  private readonly core = inject(CoreClient);

  protected readonly homeQuery = injectQuery(() => ({
    queryKey: homeKeys.all,
    queryFn: () => fetchHome(this.core),
  }));

  protected readonly home = computed(() => this.homeQuery.data());

  protected readonly profilesQuery = injectQuery(() => ({
    queryKey: ['connectionProfiles'] as const,
    queryFn: (): Promise<CharacterConnectionProfile[]> => fetchConnectionProfiles(this.core),
  }));

  protected readonly profiles = computed(() => this.profilesQuery.data() ?? []);
}
