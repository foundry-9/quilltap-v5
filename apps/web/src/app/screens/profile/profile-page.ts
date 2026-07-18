import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { RouterLink } from '@angular/router';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../core/core-client';
import { DataDirectorySection } from './data-directory-section';
import { ProfileEditSection } from './profile-edit-section';
import { ProfileInfoSection } from './profile-info-section';
import { fetchProfile, profileKeys } from './profile.api';

/**
 * The Profile screen at `/profile` (v4 `app/profile/ProfileView.tsx`): one
 * `userProfileGet` fetch feeding three sections — the editable settings, the
 * read-only account information, and the data directory (which fetches its own
 * payload, as v4's section does). Loading / error copy is v4's verbatim.
 *
 * v4's workspace-tab glue (`redirectToWorkspaceTab('profile')`) is NOT ported —
 * the tabbed workspace is `p4.9j`'s.
 */
@Component({
  selector: 'qt-profile-page',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [RouterLink, ProfileEditSection, ProfileInfoSection, DataDirectorySection],
  template: `
    @if (profileQuery.isPending()) {
      <div class="qt-page-container">
        <div class="flex items-center justify-center py-12">
          <div class="qt-text-muted">Loading profile...</div>
        </div>
      </div>
    } @else if (profileQuery.isError() || !profile()) {
      <div class="qt-page-container">
        <div class="qt-alert-error">Failed to load profile</div>
        <a routerLink="/" class="qt-button qt-button-secondary mt-4 inline-block">Return to Home</a>
      </div>
    } @else if (profile(); as profile) {
      <div class="qt-page-container">
        <div class="mb-8">
          <a
            routerLink="/"
            class="mb-4 inline-flex items-center qt-label text-primary transition hover:text-primary/80"
          >
            ← Back to Home
          </a>
          <h1 class="qt-heading-1">Profile</h1>
          <p class="qt-text-muted mt-2">Manage your profile settings</p>
        </div>

        <div class="space-y-6">
          <qt-profile-edit-section [profile]="profile" (profileUpdated)="refetch()" />
          <qt-profile-info-section [profile]="profile" />
          <qt-data-directory-section />
        </div>
      </div>
    }
  `,
})
export class ProfilePage {
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();

  protected readonly profileQuery = injectQuery(() => ({
    queryKey: profileKeys.detail,
    queryFn: () => fetchProfile(this.core),
  }));

  protected readonly profile = computed(() => this.profileQuery.data() ?? null);

  /**
   * v4's `handleProfileUpdate` is a no-op with the comment "Profile will be
   * refreshed by the query on next fetch" (`ProfileView.tsx:28-30`) — which is
   * why v4's own screen shows a stale name until something else refetches.
   * v5 invalidates instead, so the header and the info rows update at once.
   */
  protected refetch(): void {
    void this.queryClient.invalidateQueries({ queryKey: profileKeys.detail });
  }
}
