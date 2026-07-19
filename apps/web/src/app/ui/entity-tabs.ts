import { NgTemplateOutlet } from '@angular/common';
import {
  ChangeDetectionStrategy,
  Component,
  ContentChild,
  TemplateRef,
  computed,
  inject,
  input,
  signal,
} from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, Router } from '@angular/router';

import { WORKSPACE_TAB_ID } from '../workspace/workspace-contract';
import { Icon, type IconName } from './icon';

/** One tab (v4 `components/tabs/entity-tabs.tsx` `Tab`). */
export interface Tab {
  id: string;
  label: string;
  /** Optional leading icon name. */
  icon?: IconName;
}

/**
 * The URL-driven tab shell (v4 `components/tabs/entity-tabs.tsx`). The active tab
 * lives in the `?tab=` query param (falling back to `defaultTab`); switching a
 * tab replaces the URL and clears the `&section=` deep-link, exactly as v4 does.
 * The `qt-tab-*` classes carry over verbatim.
 *
 * Project the content as a single `<ng-template>` receiving the active tab id:
 * `<qt-entity-tabs [tabs]="…"><ng-template let-active>…</ng-template></qt-entity-tabs>`.
 *
 * **Workspace-tab mode (p4.9j2).** When hosted as a workspace tab there is no URL
 * to persist to, so the active tab is LOCAL STATE seeded from `defaultTab`
 * (v4 `EntityTabs` `persistToUrl={false}` — the host screen passes the payload's
 * tab as `defaultTab`, exactly as v4's `SettingsView` does with `defaultTab={tab}`).
 * The mode is keyed off an optional `WORKSPACE_TAB_ID` injection: null ⇒ the URL
 * path, byte-identical to today. Router/route are injected optionally so a hosted
 * subtree without an `ActivatedRoute` never throws.
 */
@Component({
  selector: 'qt-entity-tabs',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, NgTemplateOutlet],
  template: `
    <div>
      <div class="mb-6">
        <nav class="qt-tab-group" aria-label="Tabs">
          @for (tab of tabs(); track tab.id) {
            <button
              type="button"
              [class]="'qt-tab ' + (activeTab() === tab.id ? 'qt-tab-active' : '')"
              (click)="select(tab.id)"
            >
              @if (tab.icon) {
                <span class="qt-tab-icon"><qt-icon [name]="tab.icon" class="w-4 h-4" /></span>
              }
              <span>{{ tab.label }}</span>
            </button>
          }
        </nav>
        <div class="qt-tab-divider"></div>
      </div>

      <div [class]="contentClassName()">
        <ng-container
          [ngTemplateOutlet]="content"
          [ngTemplateOutletContext]="{ $implicit: activeTab() }"
        />
      </div>
    </div>
  `,
})
export class EntityTabs {
  private readonly router = inject(Router, { optional: true });
  private readonly route = inject(ActivatedRoute, { optional: true });
  /** Non-null ⇒ hosted as a workspace tab; drive the active tab off local state. */
  private readonly workspaceTabId = inject(WORKSPACE_TAB_ID, { optional: true });

  readonly tabs = input.required<Tab[]>();
  readonly defaultTab = input<string | undefined>(undefined);
  readonly contentClassName = input<string>('');

  @ContentChild(TemplateRef) protected content!: TemplateRef<{ $implicit: string }>;

  private readonly queryParams = this.route
    ? toSignal(this.route.queryParamMap, { requireSync: true })
    : undefined;
  /** Tab-mode active tab (null ⇒ not yet switched; falls back to `fallbackTab`). */
  private readonly localTab = signal<string | null>(null);

  protected readonly fallbackTab = computed(() => this.defaultTab() ?? this.tabs()[0]?.id ?? '');

  protected readonly activeTab = computed(() => {
    if (this.workspaceTabId != null) {
      const local = this.localTab();
      return local && this.tabs().some((t) => t.id === local) ? local : this.fallbackTab();
    }
    const fromUrl = this.queryParams?.().get('tab');
    return fromUrl && this.tabs().some((t) => t.id === fromUrl) ? fromUrl : this.fallbackTab();
  });

  protected select(tabId: string): void {
    if (this.workspaceTabId != null) {
      // No URL to persist to when hosted — keep the active tab in local state
      // (v4 `persistToUrl={false}`).
      this.localTab.set(tabId);
      return;
    }
    // Switching a tab clears any `&section=` deep-link (v4 `handleTabChange`).
    const tab = tabId === this.fallbackTab() ? null : tabId;
    void this.router?.navigate([], {
      relativeTo: this.route ?? undefined,
      queryParams: { tab, section: null },
      queryParamsHandling: 'merge',
      replaceUrl: true,
    });
  }
}
