/**
 * TabView — renders one workspace tab's surface (port of v4 `TabView.tsx`,
 * baseline `b8b12695`).
 *
 * **Lazy-mount, then keep alive:** a tab's view does not mount until the tab is
 * first activated (the `everActive` latch); once mounted it is never unmounted
 * by a tab switch (the host hides inactive tabs with CSS). This is what lets a
 * streaming Salon survive tab switches without touching its SSE hooks.
 *
 * The hosted component + its per-tab injector are resolved ONCE and cached, so
 * neither reference changes across change detection — `NgComponentOutlet` only
 * re-creates when the component or injector reference changes, so a stable pair
 * guarantees the instance persists (the keep-alive invariant). Inputs are
 * re-applied reactively (a payload refresh updates inputs without re-creating).
 *
 * The per-tab injector provides `WORKSPACE_TAB_ID` (v4 `WorkspaceTabProvider`)
 * and `WORKSPACE_TAB_VISIBLE` (v4 `WorkspaceTabVisibilityProvider`), parented to
 * the host injector so the hosted screen also resolves `WORKSPACE_HANDLE` / the
 * portal + backdrop registries.
 *
 * Because a kept-alive view never remounts, returning to a tab would otherwise
 * show the world as it stood when the user left. On each hidden→visible
 * transition this host invalidates the query-key prefixes mapped from the tab
 * kind in `core/tab-refetch.ts` (v4's `TabActivationInvalidator`); views that
 * fetch outside TanStack Query re-run their own loads via `onTabActivated`.
 *
 * @module workspace/chrome/tab-view
 */

import { NgComponentOutlet } from '@angular/common';
import { injectQueryClient } from '@tanstack/angular-query-experimental';
import {
  ChangeDetectionStrategy,
  Component,
  Injector,
  computed,
  effect,
  inject,
  input,
  signal,
  type Type,
} from '@angular/core';

import {
  WORKSPACE_TAB_ID,
  WORKSPACE_TAB_VISIBLE,
  watchTabActivation,
  type WorkspaceTab,
} from '../workspace-contract';
import { tabActivationQueryKeys } from '../core/tab-refetch';
import { TAB_VIEW_REGISTRY, type TabViewEntry } from './tab-registry';

@Component({
  selector: 'qt-tab-view',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [NgComponentOutlet],
  template: `
    @if (everActive()) {
      <ng-container
        [ngComponentOutlet]="component()"
        [ngComponentOutletInputs]="inputs()"
        [ngComponentOutletInjector]="tabInjector()"
      />
    }
  `,
})
export class TabView {
  readonly tab = input.required<WorkspaceTab>();
  readonly active = input.required<boolean>();
  /**
   * The tab is its pane's active tab (actually on screen). v4 `TabView`'s
   * `visible` prop — NOT the same as `active`, which also covers a hidden Salon
   * mounted only to portal into a child tab.
   */
  readonly visible = input.required<boolean>();

  private readonly parentInjector = inject(Injector);
  private readonly registry = inject(TAB_VIEW_REGISTRY);
  private readonly queryClient = injectQueryClient();

  private readonly _everActive = signal(false);
  protected readonly everActive = this._everActive.asReadonly();

  // Cached once per TabView instance (a TabView is 1:1 with a tab id via the
  // host's keyed @for), so both references stay stable across CD → keep-alive.
  private _entry: TabViewEntry | null = null;
  private _injector: Injector | null = null;

  protected readonly inputs = computed<Record<string, unknown>>(() => {
    const tab = this.tab();
    return this.entry().inputs?.(tab) ?? {};
  });

  constructor() {
    // Latch: mount on first activation, never unmount thereafter.
    effect(() => {
      if (this.active()) this._everActive.set(true);
    });

    // v4's `TabActivationInvalidator`: refresh the tab's TanStack reads when it
    // is navigated back to. `enabled: everActive` reproduces v4 mounting that
    // component only once the tab has been activated — the FIRST activation
    // invalidates nothing (the view is mounting and fetching for itself).
    watchTabActivation(
      this.visible,
      () => {
        for (const queryKey of tabActivationQueryKeys(this.tab())) {
          void this.queryClient.invalidateQueries({ queryKey });
        }
      },
      { enabled: this.everActive },
    );
  }

  private entry(): TabViewEntry {
    if (!this._entry) this._entry = this.registry[this.tab().kind];
    return this._entry;
  }

  protected component(): Type<unknown> {
    return this.entry().component;
  }

  protected tabInjector(): Injector {
    if (!this._injector) {
      this._injector = Injector.create({
        providers: [
          { provide: WORKSPACE_TAB_ID, useValue: this.tab().id },
          { provide: WORKSPACE_TAB_VISIBLE, useValue: this.visible },
        ],
        parent: this.parentInjector,
      });
    }
    return this._injector;
  }
}
