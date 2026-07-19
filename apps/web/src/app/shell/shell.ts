import { ChangeDetectionStrategy, Component, OnInit, inject } from '@angular/core';
import { Router, RouterLink, RouterLinkActive, RouterOutlet } from '@angular/router';

import { AutonomousRoomBadges } from '../autonomous/autonomous-room-badges';
import { CoreClient } from '../core/core-client';
import { FirstRunService } from '../startup/first-run.service';
import { ThemeService } from '../theme/theme.service';
import { ThemeSwitcher } from '../theme/theme-switcher';
import { Icon, type IconName } from '../ui/icon';
import {
  resolveDefaultCharacterForChat,
  SALON_CHAT_PATH_RE,
} from '../wardrobe/default-character';
import { WardrobeControlDialog } from '../wardrobe/wardrobe-control-dialog';
import { WardrobeDialogService } from '../wardrobe/wardrobe-dialog.service';
import { isWorkspaceTabsEnabled } from '../workspace/workspace-flag';
import { WorkspaceService } from '../workspace/workspace.service';
import { UserMenu } from './user-menu';

interface NavItem {
  id: string;
  label: string;
  tooltip: string;
  /** In-app route (routerLink) if live, else `null` for a disabled placeholder. */
  route: string | null;
  icon: IconName;
}

/** v4 `collapsed-nav.tsx` navItems. Every entry whose vertical has landed carries
 *  a real `route`; an entry still awaiting its vertical keeps `route: null` and
 *  renders as a disabled placeholder. */
const NAV_ITEMS: NavItem[] = [
  {
    id: 'projects',
    label: 'Projects',
    tooltip: 'View all projects',
    route: '/prospero',
    icon: 'projects',
  },
  { id: 'files', label: 'Files', tooltip: 'View all files', route: '/files', icon: 'files' },
  {
    id: 'scriptorium',
    label: 'The Scriptorium',
    tooltip: 'View document stores',
    route: '/scriptorium',
    icon: 'scriptorium',
  },
  {
    id: 'characters',
    label: 'Characters',
    tooltip: 'View all characters',
    route: '/characters',
    icon: 'characters',
  },
  {
    id: 'photos',
    label: 'My Photos',
    tooltip: 'Your saved photo gallery',
    route: '/photos',
    icon: 'photos',
  },
  {
    id: 'scenarios',
    label: 'Scenarios',
    tooltip: 'Manage general scenarios',
    route: '/scenarios',
    icon: 'scenarios',
  },
  {
    id: 'custom-tools',
    label: "Pascal's Workbench",
    tooltip: 'Build and prove custom tools',
    route: '/custom-tools',
    icon: 'wrench',
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
  imports: [
    Icon,
    ThemeSwitcher,
    RouterLink,
    RouterLinkActive,
    RouterOutlet,
    AutonomousRoomBadges,
    UserMenu,
    WardrobeControlDialog,
  ],
  template: `
    <div class="qt-app-layout">
      <aside class="qt-left-sidebar qt-left-sidebar-collapsed" aria-label="Main navigation">
        <div class="qt-left-sidebar-content">
          <nav class="qt-collapsed-nav" aria-label="Quick navigation">
            <a class="qt-collapsed-nav-button" routerLink="/" title="Home" aria-label="Home">
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
          <!-- Autonomous run-state badges (renders nothing when no rooms are live). -->
          <qt-autonomous-room-badges />
          <div class="qt-left-sidebar-footer-actions">
            <!-- v4 sidebar-footer.tsx:309 — the profile dropdown. -->
            <qt-user-menu />
            <!-- v4 sidebar-footer.tsx:227-253 — the Wardrobe entry (above
                 Settings/Themes in v4's footer order). A salon chat path
                 passes its chat id + resolved default character along. -->
            <button
              type="button"
              class="qt-collapsed-nav-button"
              title="Wardrobe"
              aria-label="Wardrobe"
              (click)="openWardrobe()"
            >
              <qt-icon name="wardrobe" class="w-7 h-7" />
            </button>
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

      <!-- The global wardrobe dialog, mounted once at the layout level
           (v4 app-layout.tsx:126-138). Renders nothing while closed. -->
      <qt-wardrobe-control-dialog />
    </div>
  `,
})
export class Shell implements OnInit {
  private readonly theme = inject(ThemeService);
  private readonly firstRun = inject(FirstRunService);
  private readonly router = inject(Router);
  private readonly core = inject(CoreClient);
  private readonly wardrobeDialog = inject(WardrobeDialogService);
  private readonly workspace = inject(WorkspaceService);

  protected readonly navItems = NAV_ITEMS;
  protected readonly showNavThemeSelector = this.theme.showNavThemeSelector;

  /**
   * v4 `sidebar-footer.tsx:239-244`: a plain `open()` off a salon chat; on a
   * salon chat path, resolve the default character first and pass the chat
   * scope along. The `inWorkspace` arm (v4) opens the Wardrobe as a rail-scoped
   * tab instead of the modal — ported here (P4.9J1): while the flag is on and we
   * are in the workspace, `workspace.openTab('wardrobe')`.
   */
  protected async openWardrobe(): Promise<void> {
    if (isWorkspaceTabsEnabled() && this.router.url.split('?')[0] === '/workspace') {
      this.workspace.openTab('wardrobe');
      return;
    }
    const chatMatch = this.router.url.match(SALON_CHAT_PATH_RE);
    if (!chatMatch) {
      this.wardrobeDialog.open();
      return;
    }
    const chatId = chatMatch[1];
    const characterId = await resolveDefaultCharacterForChat(this.core, chatId);
    this.wardrobeDialog.open(characterId ? { chatId, characterId } : { chatId });
  }

  ngOnInit(): void {
    // Re-apply the server-persisted theme preference (localStorage is the fallback).
    void this.theme.loadFromServer();
    // Fresh-instance → provider wizard handoff (v4 `navigateAfterSetup`).
    if (this.firstRun.consume()) {
      void this.router.navigateByUrl('/settings/wizard?mode=setup');
    }
  }
}
