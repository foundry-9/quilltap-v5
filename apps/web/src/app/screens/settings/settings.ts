import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute } from '@angular/router';

import { BrandName } from '../../ui/brand-name';
import { EntityTabs, type Tab } from '../../ui/entity-tabs';
import { AppearanceTab } from './appearance/appearance-tab';
import { SettingsPlaceholder } from './placeholder-tab';
import { ProvidersTab } from './providers/providers-tab';

/**
 * The Settings screen shell (v4 `app/settings/SettingsView.tsx`): the seven-tab
 * hall over `EntityTabs` (`?tab=` deep links), a per-tab subsystem background
 * (v4 `--story-background-url`; here a `data-subsystem` attribute the theme CSS
 * can hook), and one populated slice — AI Providers + Appearance. The remaining
 * five tabs render the "not yet fitted out" placeholder.
 */
@Component({
  selector: 'qt-settings',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [EntityTabs, BrandName, ProvidersTab, AppearanceTab, SettingsPlaceholder],
  template: `
    <div class="qt-page-container" [attr.data-subsystem]="subsystem()">
      <div class="qt-settings-header mb-8">
        <h1 class="qt-heading-1">Settings</h1>
        <p class="qt-text-muted mt-2">
          Configure and manage every aspect of your <qt-brand-name /> workspace
        </p>
      </div>

      <qt-entity-tabs [tabs]="tabs" [defaultTab]="'providers'" contentClassName="qt-settings-panel">
        <ng-template let-active>
          @switch (active) {
            @case ('providers') {
              <qt-settings-providers />
            }
            @case ('appearance') {
              <qt-settings-appearance />
            }
            @case ('chat') {
              <qt-settings-placeholder title="Chat" />
            }
            @case ('memory') {
              <qt-settings-placeholder title="Commonplace Book" />
            }
            @case ('images') {
              <qt-settings-placeholder title="Images" />
            }
            @case ('templates') {
              <qt-settings-placeholder title="Templates &amp; Prompts" />
            }
            @case ('system') {
              <qt-settings-placeholder title="Data &amp; System" />
            }
            @default {
              <qt-settings-providers />
            }
          }
        </ng-template>
      </qt-entity-tabs>
    </div>
  `,
})
export class Settings {
  private readonly route = inject(ActivatedRoute);
  private readonly queryParams = toSignal(this.route.queryParamMap, { requireSync: true });

  protected readonly tabs: Tab[] = [
    { id: 'providers', label: 'AI Providers', icon: 'wrench' },
    { id: 'chat', label: 'Chat', icon: 'chat' },
    { id: 'appearance', label: 'Appearance', icon: 'themes' },
    { id: 'memory', label: 'Commonplace Book', icon: 'book' },
    { id: 'images', label: 'Images', icon: 'image' },
    { id: 'templates', label: 'Templates & Prompts', icon: 'user' },
    { id: 'system', label: 'Data & System', icon: 'database' },
  ];

  /** v4 `TAB_SUBSYSTEM_MAP` — the personified workshop behind each tab. */
  private readonly subsystemMap: Record<string, string> = {
    providers: 'forge',
    chat: 'salon',
    appearance: 'calliope',
    memory: 'commonplace-book',
    images: 'lantern',
    templates: 'aurora',
    system: 'prospero',
  };

  protected readonly subsystem = computed(() => {
    const tab = this.queryParams().get('tab') ?? 'providers';
    return this.subsystemMap[tab] ?? 'forge';
  });
}
