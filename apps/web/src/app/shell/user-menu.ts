import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';
import { Router } from '@angular/router';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { apiUrl } from '../core/api-url';
import { CoreClient } from '../core/core-client';
import { fetchProfile, profileKeys } from '../screens/profile/profile.api';
import { Icon } from '../ui/icon';
import { QuickHideMenuSection } from '../quick-hide/quick-hide-menu-section';

/**
 * The shell-footer user menu (v4
 * `components/layout/left-sidebar/profile-menu.tsx`, mounted by
 * `sidebar-footer.tsx:19,:309`): an avatar/name trigger opening a small
 * dropdown with Profile and About.
 *
 * v4 reads the user from its session provider; v5 has no session (single-user),
 * so the trigger's user comes from the `userProfileGet` verb — the same query
 * the Profile screen uses, so opening the menu after visiting the screen is
 * cache-warm.
 *
 * v4's rail is collapsible and the trigger renders a name/email block when it
 * is expanded; v5's rail is collapsed-only (`qt-left-sidebar-collapsed`), so
 * this ports the COLLAPSED arm — the avatar with a `title`, and the name/email
 * inside the open dropdown where there is room for them.
 *
 * The navigation is a plain router push. v4's `useWorkspaceNavigate` opens
 * Profile/About as workspace TABS when inside the workspace; that family is
 * `p4.9j`'s, and its absence is why this is a push.
 */
@Component({
  selector: 'qt-user-menu',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, QuickHideMenuSection],
  template: `
    <div class="relative">
      <button
        type="button"
        (click)="open.set(!open())"
        class="qt-left-sidebar-item justify-center px-0"
        [title]="triggerTitle()"
        aria-label="User menu"
        [attr.aria-expanded]="open()"
        aria-haspopup="menu"
      >
        @if (avatar(); as src) {
          <img [src]="src" [alt]="displayName()" class="qt-left-sidebar-profile-avatar object-cover" />
        } @else {
          <qt-icon name="profile" class="w-7 h-7" />
        }
      </button>

      @if (open()) {
        <div
          class="absolute bottom-full left-0 mb-1 w-64 bg-popover border qt-border-default rounded-lg qt-shadow-lg z-50 p-2 space-y-1"
          role="menu"
        >
          <div class="px-3 py-2">
            <p class="qt-text-primary text-sm font-medium truncate">{{ displayName() }}</p>
            @if (email(); as email) {
              <p class="qt-text-muted text-xs truncate">{{ email }}</p>
            }
          </div>

          <div class="qt-divider my-1"></div>

          <!-- §2b wire (mounted at unification): v4 sidebar-footer.tsx:302 renders
               the quick-hide content above the profile entries. -->
          <qt-quick-hide-menu-section />

          <div class="qt-divider my-1"></div>

          <button
            type="button"
            role="menuitem"
            (click)="go('/profile')"
            class="w-full flex items-center gap-3 p-2 rounded-md hover:qt-bg-muted text-left text-sm"
          >
            <qt-icon name="profile" class="w-4 h-4" />
            Profile
          </button>
          <button
            type="button"
            role="menuitem"
            (click)="go('/about')"
            class="w-full flex items-center gap-3 p-2 rounded-md hover:qt-bg-muted text-left text-sm"
          >
            <qt-icon name="info" class="w-4 h-4" />
            About
          </button>
        </div>
      }
    </div>
  `,
})
export class UserMenu {
  private readonly core = inject(CoreClient);
  private readonly router = inject(Router);

  protected readonly open = signal(false);

  private readonly profileQuery = injectQuery(() => ({
    queryKey: profileKeys.detail,
    queryFn: () => fetchProfile(this.core),
  }));

  /** v4 `:81` — `user.name || 'User'`. */
  protected displayName(): string {
    return this.profileQuery.data()?.name || 'User';
  }

  protected email(): string | null {
    return this.profileQuery.data()?.email ?? null;
  }

  protected avatar(): string | null {
    const image = this.profileQuery.data()?.image;
    return image ? apiUrl(image) : null;
  }

  /** v4 `:61` — the collapsed rail's trigger is titled 'Profile'. */
  protected triggerTitle(): string {
    return 'Profile';
  }

  protected go(route: string): void {
    this.open.set(false);
    void this.router.navigateByUrl(route);
  }
}
