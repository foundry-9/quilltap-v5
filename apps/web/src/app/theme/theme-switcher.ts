import { ChangeDetectionStrategy, Component, inject, signal } from '@angular/core';

import { Icon } from '../ui/icon';
import { ThemeService, type ColorMode } from './theme.service';

/**
 * The theme switcher (v4 `NavUserMenuThemeContent`). Lists the Default option +
 * the bundled packs with colour swatches and an active check, then the three
 * colour modes. Applies via {@link ThemeService}. Copy verbatim from v4.
 */
@Component({
  selector: 'qt-theme-switcher',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div class="relative">
      <button
        type="button"
        class="qt-left-sidebar-item justify-center px-0"
        title="Themes"
        aria-label="Themes"
        (click)="open.set(!open())"
      >
        <qt-icon name="themes" class="qt-left-sidebar-item-icon w-7 h-7" />
      </button>

      @if (open()) {
        <div
          class="absolute bottom-full left-0 mb-1 w-64 bg-popover border qt-border-default rounded-lg qt-shadow-lg z-50 p-2 space-y-1"
          role="menu"
        >
          @for (opt of themes(); track opt.id ?? 'default') {
            <button
              type="button"
              class="w-full flex items-center gap-3 p-2 rounded-md hover:qt-bg-muted text-left"
              role="menuitemradio"
              [attr.aria-checked]="activeId() === opt.id"
              (click)="pick(opt.id)"
            >
              <span class="flex gap-0.5 flex-shrink-0">
                <span
                  class="w-3 h-4 rounded-sm border qt-border-default"
                  title="Background"
                  [style.background]="opt.previewColors.background"
                ></span>
                <span class="w-3 h-4 rounded-sm" title="Primary" [style.background]="opt.previewColors.primary"></span>
                <span
                  class="w-3 h-4 rounded-sm"
                  title="Secondary"
                  [style.background]="opt.previewColors.secondary"
                ></span>
              </span>
              <span class="flex-1 qt-text-primary text-sm">{{ opt.name }}</span>
              @if (activeId() === opt.id) {
                <qt-icon name="check" class="w-4 h-4 qt-text-primary" />
              }
            </button>
          }

          <div class="qt-divider my-1"></div>

          @for (m of modes; track m.mode) {
            <button
              type="button"
              class="w-full flex items-center gap-3 p-2 rounded-md hover:qt-bg-muted text-left"
              role="menuitemradio"
              [attr.aria-checked]="colorMode() === m.mode"
              (click)="setMode(m.mode)"
            >
              <qt-icon [name]="m.icon" class="w-4 h-4 qt-text-secondary" />
              <span class="flex-1 qt-text-primary text-sm">{{ m.label }}</span>
              @if (colorMode() === m.mode) {
                <qt-icon name="check" class="w-4 h-4 qt-text-primary" />
              }
            </button>
          }
        </div>
      }
    </div>
  `,
})
export class ThemeSwitcher {
  private readonly theme = inject(ThemeService);

  protected readonly open = signal(false);
  protected readonly themes = () => this.theme.listThemes();
  protected readonly activeId = this.theme.activeThemeId;
  protected readonly colorMode = this.theme.colorMode;

  protected readonly modes: Array<{ mode: ColorMode; label: string; icon: 'sun' | 'moon' | 'monitor' }> = [
    { mode: 'light', label: 'Light', icon: 'sun' },
    { mode: 'dark', label: 'Dark', icon: 'moon' },
    { mode: 'system', label: 'System', icon: 'monitor' },
  ];

  protected pick(id: string | null): void {
    this.theme.applyTheme(id);
  }

  protected setMode(mode: ColorMode): void {
    this.theme.setColorMode(mode);
  }
}
