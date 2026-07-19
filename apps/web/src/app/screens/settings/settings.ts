import { ChangeDetectionStrategy, Component, computed, inject, input } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute } from '@angular/router';

import { BrandName } from '../../ui/brand-name';
import { EntityTabs, type Tab } from '../../ui/entity-tabs';
import { AppearanceTab } from './appearance/appearance-tab';
import { ChatTab } from './chat/chat-tab';
import { ImagesTab } from './images/images-tab';
import { MemoryTab } from './memory/memory-tab';
import { SettingsPlaceholder } from './placeholder-tab';
import { ProvidersTab } from './providers/providers-tab';
import { TemplatesTab } from './templates/templates-tab';

/**
 * The Settings screen shell (v4 `app/settings/SettingsView.tsx`): the seven-tab
 * hall over `EntityTabs` (`?tab=` deep links), a per-tab subsystem background
 * (v4 `--story-background-url`; here a `data-subsystem` attribute the theme CSS
 * can hook), and one populated slice — AI Providers + Appearance. The remaining
 * five tabs render the "not yet fitted out" placeholder.
 *
 * **Workspace-tab mode (p4.9j2).** `tab`/`section` inputs mirror v4's
 * `SettingsViewProps`. When hosted, `tab` seeds the initial tab (via
 * `EntityTabs`' `defaultTab`) and the subsystem background (v4
 * `useSettingsBackgroundStyle(tabOverride)`); `section` is accepted for the
 * contract's `SettingsTabPayload` but — exactly as v4's `_section` — is NOT
 * threaded to the sub-tabs' `?section=` force-open (v4 ignores it when hosted).
 * `WORKSPACE_TAB_ID` null ⇒ routed mode, reads `?tab=`, byte-identical.
 */
@Component({
  selector: 'qt-settings',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [
    EntityTabs,
    BrandName,
    ProvidersTab,
    AppearanceTab,
    ChatTab,
    TemplatesTab,
    ImagesTab,
    MemoryTab,
    SettingsPlaceholder,
  ],
  template: `
    <div class="qt-page-container" [attr.data-subsystem]="subsystem()">
      <div class="qt-settings-header mb-8">
        <h1 class="qt-heading-1">Settings</h1>
        <p class="qt-text-muted mt-2">
          Configure and manage every aspect of your <qt-brand-name /> workspace
        </p>
      </div>

      <qt-entity-tabs
        [tabs]="tabs"
        [defaultTab]="tab() ?? 'providers'"
        contentClassName="qt-settings-panel"
      >
        <ng-template let-active>
          @switch (active) {
            @case ('providers') {
              <qt-settings-providers />
            }
            @case ('appearance') {
              <qt-settings-appearance />
            }
            @case ('chat') {
              <qt-settings-chat />
            }
            @case ('memory') {
              <qt-settings-memory />
            }
            @case ('images') {
              <qt-settings-images />
            }
            @case ('templates') {
              <qt-settings-templates />
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
  private readonly route = inject(ActivatedRoute, { optional: true });
  private readonly queryParams = this.route
    ? toSignal(this.route.queryParamMap, { requireSync: true })
    : undefined;

  /** Deep-link target tab (v4 `SettingsViewProps.tab`); falls back to `?tab=`. */
  readonly tab = input<string | null>(null);
  /**
   * Deep-link target section (v4 `SettingsViewProps.section`). Accepted for the
   * `SettingsTabPayload` contract but, exactly as v4's `_section`, unused —
   * v4 does not thread it to the hosted sub-tabs' `?section=` force-open.
   */
  readonly section = input<string | null>(null);

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
    // v4 `useSettingsBackgroundStyle`: the `tab` override wins, else the URL.
    const tab = this.tab() ?? this.queryParams?.().get('tab') ?? 'providers';
    return this.subsystemMap[tab] ?? 'forge';
  });
}
