import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { injectQuery, injectQueryClient } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../../core/core-client';
import type { ChatSettingsDto } from '../../../core/core-contract';
import { ThemeService, type ColorMode } from '../../../theme/theme.service';
import { Icon } from '../../../ui/icon';
import { SettingsTagsCard } from './tags-tab';

type AvatarDisplayMode = 'ALWAYS' | 'GROUP_ONLY' | 'NEVER';
type AvatarDisplayStyle = 'CIRCULAR' | 'RECTANGULAR';

const AVATAR_MODES: Array<{ value: AvatarDisplayMode; label: string; description: string }> = [
  {
    value: 'ALWAYS',
    label: 'Always Show Avatars',
    description: 'Display avatar for every message (character on left, user on right)',
  },
  {
    value: 'GROUP_ONLY',
    label: 'Group Chats Only',
    description: 'Only show avatars in group chats (will be implemented in the future)',
  },
  { value: 'NEVER', label: 'Never Show Avatars', description: 'Hide avatars in all chats' },
];

const AVATAR_STYLES: Array<{
  value: AvatarDisplayStyle;
  label: string;
  description: string;
  preview: string;
}> = [
  {
    value: 'CIRCULAR',
    label: 'Circular',
    description: 'Display avatars as circles',
    preview: '⭕',
  },
  {
    value: 'RECTANGULAR',
    label: 'Rectangular (5:4)',
    description: 'Display avatars as rectangles with 5:4 aspect ratio',
    preview: '▭',
  },
];

const COLOR_MODES: Array<{ value: ColorMode; label: string; icon: 'sun' | 'moon' | 'monitor' }> = [
  { value: 'light', label: 'Light', icon: 'sun' },
  { value: 'dark', label: 'Dark', icon: 'moon' },
  { value: 'system', label: 'System', icon: 'monitor' },
];

/**
 * The Appearance tab (v4 `components/settings/appearance/` + `AvatarSettings`):
 * the nav quick-theme toggle, the color-mode selector, the bundled-theme grid
 * (over {@link ThemeService}), and avatar display mode/style (via
 * `chatSettingsUpdate`). The `.qtap-theme` registry/browser/install UI is a named
 * deferral. Copy carries over verbatim.
 */
@Component({
  selector: 'qt-settings-appearance',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, SettingsTagsCard],
  template: `
    <div class="space-y-8">
      <!-- Quick Theme Access -->
      <div class="qt-card p-5">
        <h2 class="qt-text-section text-foreground">Quick Theme Access</h2>
        <p class="qt-text-small qt-text-muted mt-1 mb-4">
          Show a theme selector dropdown in the navigation bar for quick theme switching.
        </p>
        <label
          [class]="
            'flex items-center justify-between gap-4 p-4 border rounded-lg transition-colors cursor-pointer ' +
            (showNav() ? 'qt-border-primary qt-bg-muted' : 'qt-border-default qt-hover-accent')
          "
        >
          <div class="flex items-center gap-4">
            <div
              [class]="
                'flex-shrink-0 p-2 rounded-full ' +
                (showNav() ? 'qt-bg-muted text-primary' : 'qt-bg-muted qt-text-secondary')
              "
            >
              <qt-icon name="themes" class="w-5 h-5" />
            </div>
            <div>
              <div class="qt-text-primary">Show theme selector in navigation</div>
              <div class="qt-text-small">Quickly switch themes from the navigation bar</div>
            </div>
          </div>
          <button
            type="button"
            role="switch"
            [attr.aria-checked]="showNav()"
            [class]="
              'relative inline-flex h-6 w-11 flex-shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors ' +
              (showNav() ? 'bg-primary' : 'qt-bg-muted')
            "
            (click)="toggleNav()"
          >
            <span
              [class]="
                'pointer-events-none inline-block h-5 w-5 transform rounded-full bg-background shadow transition ' +
                (showNav() ? 'translate-x-5' : 'translate-x-0')
              "
            ></span>
          </button>
        </label>
      </div>

      <!-- Color Mode -->
      <div class="qt-card p-5">
        <h2 class="qt-text-section text-foreground">Color Mode</h2>
        <p class="qt-text-small qt-text-muted mt-1 mb-4">
          Choose how Quilltap should appear. You can select light mode, dark mode, or follow your
          system settings.
        </p>
        <div class="flex gap-2">
          @for (m of colorModes; track m.value) {
            <button
              type="button"
              [class]="
                'flex-1 flex items-center justify-center gap-2 px-3 py-2 qt-label border rounded-lg transition-colors ' +
                (colorMode() === m.value
                  ? 'qt-border-primary bg-primary text-primary-foreground'
                  : 'qt-border-default qt-bg-card qt-hover-accent text-foreground')
              "
              [attr.aria-pressed]="colorMode() === m.value"
              (click)="setColorMode(m.value)"
            >
              <qt-icon [name]="m.icon" class="w-4 h-4" />
              <span>{{ m.value === 'system' ? 'System (' + resolvedMode() + ')' : m.label }}</span>
            </button>
          }
        </div>
      </div>

      <!-- Theme selector -->
      <div class="qt-card p-5">
        <h2 class="qt-text-section text-foreground">Theme</h2>
        <p class="qt-text-small qt-text-muted mt-1 mb-4">
          Select a theme to customize the colors and appearance of Quilltap.
        </p>
        <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
          @for (opt of themes(); track opt.id ?? 'default') {
            <button
              type="button"
              [class]="
                'text-left p-4 border rounded-lg transition-colors ' +
                (activeThemeId() === opt.id
                  ? 'qt-border-primary qt-bg-muted'
                  : 'qt-border-default qt-hover-accent')
              "
              [attr.aria-pressed]="activeThemeId() === opt.id"
              (click)="applyTheme(opt.id)"
            >
              <div class="flex items-center justify-between gap-2 mb-2">
                <span class="font-medium qt-text-primary">{{ opt.name }}</span>
                @if (activeThemeId() === opt.id) {
                  <qt-icon name="check" class="w-4 h-4 qt-text-primary" />
                }
              </div>
              <div class="flex gap-1 mb-2">
                <span
                  class="w-6 h-6 rounded border qt-border-default"
                  [style.background]="opt.previewColors.background"
                ></span>
                <span class="w-6 h-6 rounded" [style.background]="opt.previewColors.primary"></span>
                <span
                  class="w-6 h-6 rounded"
                  [style.background]="opt.previewColors.secondary"
                ></span>
              </div>
              <p class="qt-text-xs qt-text-muted">{{ opt.description }}</p>
            </button>
          }
        </div>
      </div>

      <!-- Avatar display mode -->
      <div class="qt-card p-5">
        <h2 class="qt-text-section text-foreground">Message Avatar Display</h2>
        <p class="qt-text-small qt-text-muted mt-1 mb-4">
          Control how avatars are displayed in chat messages
        </p>
        <div class="space-y-3">
          @for (mode of avatarModes; track mode.value) {
            <label
              class="flex items-start gap-3 p-4 border qt-border-default rounded qt-hover-accent cursor-pointer transition-colors"
            >
              <input
                type="radio"
                name="avatarDisplayMode"
                class="mt-1"
                [checked]="avatarMode() === mode.value"
                [disabled]="saving()"
                (change)="setAvatarMode(mode.value)"
              />
              <div class="flex-1">
                <div class="font-medium">{{ mode.label }}</div>
                <div class="qt-text-small">{{ mode.description }}</div>
              </div>
            </label>
          }
        </div>
      </div>

      <!-- Avatar display style -->
      <div class="qt-card p-5">
        <h2 class="qt-text-section text-foreground">Avatar Display Style</h2>
        <p class="qt-text-small qt-text-muted mt-1 mb-4">
          Choose how avatars are shaped and displayed throughout the application
        </p>
        <div class="space-y-3">
          @for (style of avatarStyles; track style.value) {
            <label
              class="flex items-start gap-3 p-4 border qt-border-default rounded qt-hover-accent cursor-pointer transition-colors"
            >
              <input
                type="radio"
                name="avatarDisplayStyle"
                class="mt-1"
                [checked]="avatarStyle() === style.value"
                [disabled]="saving()"
                (change)="setAvatarStyle(style.value)"
              />
              <div class="flex-1">
                <div class="font-medium">{{ style.label }}</div>
                <div class="qt-text-small">{{ style.description }}</div>
              </div>
              <div class="text-3xl">{{ style.preview }}</div>
            </label>
          }
        </div>
      </div>

      <!-- Tags — v4 mounts this LAST in the Appearance tab
           (AppearanceTabContent.tsx:48-50), after Appearance and Avatar Settings. -->
      <div class="qt-card p-5">
        <qt-settings-tags />
      </div>
    </div>
  `,
})
export class AppearanceTab {
  private readonly theme = inject(ThemeService);
  private readonly core = inject(CoreClient);
  private readonly queryClient = injectQueryClient();

  protected readonly colorModes = COLOR_MODES;
  protected readonly avatarModes = AVATAR_MODES;
  protected readonly avatarStyles = AVATAR_STYLES;
  protected readonly saving = signal(false);

  protected readonly activeThemeId = this.theme.activeThemeId;
  protected readonly colorMode = this.theme.colorMode;
  protected readonly resolvedMode = this.theme.resolvedColorMode;
  protected readonly showNav = this.theme.showNavThemeSelector;
  protected readonly themes = () => this.theme.listThemes();

  private readonly settingsQuery = injectQuery(() => ({
    queryKey: ['chatSettings'],
    queryFn: async (): Promise<ChatSettingsDto> => {
      const resp = await this.core.dispatchExpect({ type: 'chatSettings' }, 'chatSettings');
      return resp.data;
    },
  }));

  protected readonly avatarMode = computed<AvatarDisplayMode>(
    () => (this.settingsQuery.data()?.avatarDisplayMode as AvatarDisplayMode) ?? 'ALWAYS',
  );
  protected readonly avatarStyle = computed<AvatarDisplayStyle>(
    () => (this.settingsQuery.data()?.avatarDisplayStyle as AvatarDisplayStyle) ?? 'CIRCULAR',
  );

  protected applyTheme(id: string | null): void {
    this.theme.applyTheme(id);
  }
  protected setColorMode(mode: ColorMode): void {
    this.theme.setColorMode(mode);
  }
  protected toggleNav(): void {
    this.theme.setShowNavThemeSelector(!this.showNav());
  }

  protected async setAvatarMode(mode: AvatarDisplayMode): Promise<void> {
    await this.updateSettings({ avatarDisplayMode: mode });
  }
  protected async setAvatarStyle(style: AvatarDisplayStyle): Promise<void> {
    await this.updateSettings({ avatarDisplayStyle: style });
  }

  private async updateSettings(patch: Record<string, unknown>): Promise<void> {
    this.saving.set(true);
    try {
      await this.core.dispatchExpect(
        { type: 'chatSettingsUpdate', settings: patch },
        'chatSettings',
      );
      await this.queryClient.invalidateQueries({ queryKey: ['chatSettings'] });
    } catch {
      // leave current; the next refetch restores server truth
    } finally {
      this.saving.set(false);
    }
  }
}
