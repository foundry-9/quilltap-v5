import { ChangeDetectionStrategy, Component, OnInit, inject } from '@angular/core';
import { Router, RouterLink, RouterLinkActive, RouterOutlet } from '@angular/router';

import { FirstRunService } from '../startup/first-run.service';
import { ThemeService } from '../theme/theme.service';
import { ThemeSwitcher } from '../theme/theme-switcher';
import { Icon, type IconName } from '../ui/icon';

interface NavItem {
  id: string;
  label: string;
  tooltip: string;
  /** In-app route (routerLink) if live, else `null` for a disabled placeholder. */
  route: string | null;
  icon: IconName;
}

/** v4 `collapsed-nav.tsx` navItems — the foundation nav skeleton. Only the Salon
 *  (Chats) route is live this round; the rest stay disabled until their vertical. */
const NAV_ITEMS: NavItem[] = [
  {
    id: 'projects',
    label: 'Projects',
    tooltip: 'View all projects',
    route: null,
    icon: 'projects',
  },
  { id: 'files', label: 'Files', tooltip: 'View all files', route: null, icon: 'files' },
  {
    id: 'scriptorium',
    label: 'The Scriptorium',
    tooltip: 'View document stores',
    route: null,
    icon: 'scriptorium',
  },
  {
    id: 'characters',
    label: 'Characters',
    tooltip: 'View all characters',
    route: null,
    icon: 'characters',
  },
  {
    id: 'photos',
    label: 'My Photos',
    tooltip: 'Your saved photo gallery',
    route: null,
    icon: 'photos',
  },
  {
    id: 'scenarios',
    label: 'Scenarios',
    tooltip: 'Manage general scenarios',
    route: null,
    icon: 'scenarios',
  },
  { id: 'chats', label: 'Chats', tooltip: 'View all chats', route: '/salon', icon: 'chat' },
];

/**
 * The app shell (v4 `app-layout.tsx` + `left-sidebar`): the icon-only collapsed
 * nav rail + a footer with the theme switcher, and the chats list as the first
 * real screen. Scaffolding for the P4.6 verticals — the nav items are a skeleton
 * (their targets land with each vertical), not live routes yet.
 */
@Component({
  selector: 'qt-shell',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, ThemeSwitcher, RouterLink, RouterLinkActive, RouterOutlet],
  template: `
    <div class="qt-app-layout">
      <aside class="qt-left-sidebar qt-left-sidebar-collapsed" aria-label="Main navigation">
        <div class="qt-left-sidebar-content">
          <nav class="qt-collapsed-nav" aria-label="Quick navigation">
            <a class="qt-collapsed-nav-button" routerLink="/salon" title="Home" aria-label="Home">
              <qt-icon name="brand" class="w-8 h-8" />
            </a>
            @for (item of navItems; track item.id) {
              @if (item.route) {
                <a
                  class="qt-collapsed-nav-button"
                  [routerLink]="item.route"
                  [title]="item.tooltip"
                  [attr.aria-label]="item.label"
                >
                  <qt-icon [name]="item.icon" class="w-7 h-7" />
                </a>
              } @else {
                <button
                  type="button"
                  class="qt-collapsed-nav-button"
                  [title]="item.tooltip"
                  [attr.aria-label]="item.label"
                  disabled
                >
                  <qt-icon [name]="item.icon" class="w-7 h-7" />
                </button>
              }
            }
          </nav>
        </div>
        <div class="qt-left-sidebar-footer">
          <div class="qt-left-sidebar-footer-actions">
            @if (showNavThemeSelector()) {
              <qt-theme-switcher />
            }
            <a
              class="qt-collapsed-nav-button"
              routerLink="/settings"
              routerLinkActive="qt-collapsed-nav-button-active"
              title="Settings"
              aria-label="Settings"
            >
              <qt-icon name="settings" class="w-7 h-7" />
            </a>
          </div>
        </div>
      </aside>

      <div class="qt-app-main">
        <main class="flex flex-col flex-1 min-h-0 overflow-hidden">
          <!-- v4 app-layout.tsx: the inner scroller wrapper — page content
               scrolls HERE; full-height views (the chat) size to it exactly
               and run their own inner scroller instead. -->
          <div class="flex-1 min-h-0 overflow-y-auto">
            <router-outlet />
          </div>
        </main>
      </div>
    </div>
  `,
})
export class Shell implements OnInit {
  private readonly theme = inject(ThemeService);
  private readonly firstRun = inject(FirstRunService);
  private readonly router = inject(Router);

  protected readonly navItems = NAV_ITEMS;
  protected readonly showNavThemeSelector = this.theme.showNavThemeSelector;

  ngOnInit(): void {
    // Re-apply the server-persisted theme preference (localStorage is the fallback).
    void this.theme.loadFromServer();
    // Fresh-instance → provider wizard handoff (v4 `navigateAfterSetup`).
    if (this.firstRun.consume()) {
      void this.router.navigateByUrl('/settings/wizard?mode=setup');
    }
  }
}
