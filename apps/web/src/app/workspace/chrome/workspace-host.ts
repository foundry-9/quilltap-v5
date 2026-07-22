/**
 * WorkspaceHost — the two-pane tab host (port of v4 `WorkspaceHost.tsx` +
 * `WorkspaceProviders.tsx` + the intent/interceptor/shortcuts wiring, baseline
 * `e646f58b`). The routed component at `/workspace`.
 *
 * Renders EVERY open tab's view at once as a flat list of siblings in one CSS
 * grid; each view is positioned into its pane via `grid-column` and hidden (not
 * unmounted) when it is not its pane's active tab. Because no view ever changes
 * its parent — moving a tab between panes only changes a CSS column — the
 * keep-alive constraint holds and a streaming Salon survives every tab switch
 * and the split.
 *
 * Owns the store hydration trigger, the `?open=` intent consumer (hydration-
 * gated + URL-stripping), the document-level link interceptor (capture +
 * preventDefault + stopPropagation), and the keyboard shortcuts. Provides the
 * contract tokens (`WORKSPACE_HANDLE` + the portal/backdrop registries) so
 * hosted screens resolve them.
 *
 * @module workspace/chrome/workspace-host
 */

import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  OnInit,
  computed,
  inject,
  signal,
} from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { ActivatedRoute, type ParamMap, Router } from '@angular/router';

import {
  DEFAULT_SPLIT_RATIO,
  WORKSPACE_BACKDROP_REGISTRY,
  WORKSPACE_HANDLE,
  WORKSPACE_PORTAL_REGISTRY,
  type PaneId,
  type WorkspaceTab,
} from '../workspace-contract';
import { WorkspaceService } from '../workspace.service';
import { isActiveInItsPane, paneOfTab } from '../core/reducer';
import {
  WorkspaceBackdropRegistryImpl,
  WorkspacePortalRegistryImpl,
} from './registries';
import { DEFAULT_TAB_REGISTRY, TAB_VIEW_REGISTRY } from './tab-registry';
import { TabStrip } from './tab-strip';
import { TabView } from './tab-view';
import { WorkspaceDivider } from './workspace-divider';
import { WorkspaceBackdrop } from './workspace-backdrop';
import { applyWorkspaceShortcut } from './workspace-shortcuts';
import { interpretWorkspaceLinkClick } from './link-interceptor';
import { applyOpenIntent, parseOpenIntent } from './workspace-intent';

/** Divider column thickness (px). */
const DIVIDER_PX = 8;

@Component({
  selector: 'qt-workspace-host',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [TabStrip, TabView, WorkspaceDivider, WorkspaceBackdrop],
  providers: [
    WorkspacePortalRegistryImpl,
    WorkspaceBackdropRegistryImpl,
    { provide: WORKSPACE_HANDLE, useExisting: WorkspaceService },
    { provide: WORKSPACE_PORTAL_REGISTRY, useExisting: WorkspacePortalRegistryImpl },
    { provide: WORKSPACE_BACKDROP_REGISTRY, useExisting: WorkspaceBackdropRegistryImpl },
    { provide: TAB_VIEW_REGISTRY, useValue: DEFAULT_TAB_REGISTRY },
  ],
  template: `
    <div
      #grid
      class="qt-workspace"
      [style.grid-template-columns]="gridCols()"
      style="grid-template-rows: auto minmax(0, 1fr)"
    >
      <qt-workspace-backdrop />

      <!-- Left pane chrome -->
      <div class="qt-workspace-pane-chrome" style="grid-column: 1; grid-row: 1">
        <qt-tab-strip
          pane="left"
          [paneState]="state().panes.left"
          [tabs]="state().tabs"
          [draggingId]="draggingId()"
          (dragStartTab)="draggingId.set($event)"
          (dragEndTab)="draggingId.set(null)"
          (select)="ws.setActive('left', $event)"
          (close)="ws.closeTab($event)"
          (move)="onMove('left', $event)"
        />
      </div>

      <!-- Divider + right pane chrome (only when split) -->
      @if (split() && state().panes.right; as right) {
        <div style="grid-column: 2; grid-row: 1 / 3" class="qt-workspace-divider-cell">
          <qt-workspace-divider
            [containerEl]="grid"
            [ratio]="state().splitRatio"
            (ratioChange)="ws.setSplitRatio($event)"
            (reset)="ws.setSplitRatio(DEFAULT_RATIO)"
          />
        </div>
        <div class="qt-workspace-pane-chrome" style="grid-column: 3; grid-row: 1">
          <qt-tab-strip
            pane="right"
            [paneState]="right"
            [tabs]="state().tabs"
            [draggingId]="draggingId()"
            (dragStartTab)="draggingId.set($event)"
            (dragEndTab)="draggingId.set(null)"
            (select)="ws.setActive('right', $event)"
            (close)="ws.closeTab($event)"
            (move)="onMove('right', $event)"
          />
        </div>
      }

      <!-- Flat, always-mounted content list (keep-alive) -->
      @for (tab of allTabs(); track tab.id) {
        @let visible = isVisible(tab.id);
        <div
          class="qt-tab-pane"
          [class.hidden]="!visible"
          [attr.aria-hidden]="!visible"
          [style.grid-column]="paneOf(tab.id) === 'left' ? 1 : 3"
          style="grid-row: 2"
          [style.display]="visible ? null : 'none'"
          (mousedown)="onPaneMouseDown(paneOf(tab.id))"
        >
          <qt-tab-view [tab]="tab" [active]="isMounted(tab)" />
        </div>
      }

      <!-- Empty-pane affordance (defensive) -->
      @for (p of PANES; track p) {
        @if (emptyPane(p)) {
          <div class="qt-workspace-empty" [style.grid-column]="p === 'left' ? 1 : 3" style="grid-row: 2">
            <p class="qt-workspace-empty-hint">
              This half of the desk is clear. Drag a tab across, or summon one from the rail.
            </p>
          </div>
        }
      }

      <!-- Split drop-zone: drag a tab here (unsplit) to open the right pane -->
      @if (!split() && draggingId()) {
        <div
          class="qt-tab-drop-zone"
          style="grid-column: 1; grid-row: 2"
          (dragover)="$event.preventDefault()"
          (drop)="onSplitDrop($event)"
        >
          <span class="qt-tab-drop-zone-hint">Release here to split the workspace</span>
        </div>
      }
    </div>
  `,
})
export class WorkspaceHost implements OnInit {
  protected readonly ws = inject(WorkspaceService);
  private readonly router = inject(Router);
  private readonly route = inject(ActivatedRoute);
  private readonly host = inject(ElementRef<HTMLElement>);
  private readonly destroyRef = inject(DestroyRef);

  protected readonly DEFAULT_RATIO = DEFAULT_SPLIT_RATIO;
  protected readonly PANES: PaneId[] = ['left', 'right'];

  protected readonly state = this.ws.state;
  protected readonly draggingId = signal<string | null>(null);

  protected readonly split = computed(() => this.state().panes.right != null);
  protected readonly allTabs = computed(() => Object.values(this.state().tabs));
  protected readonly gridCols = computed(() => {
    if (!this.split()) return '1fr';
    const r = this.state().splitRatio;
    return `minmax(0, ${r}fr) ${DIVIDER_PX}px minmax(0, ${1 - r}fr)`;
  });

  ngOnInit(): void {
    // Hydrate once, then apply any pending ?open= intent (AFTER hydration, or the
    // hydrate REPLACE_STATE would clobber the just-opened tab).
    void this.ws.hydrateOnce().then(() => this.applyIntent(this.route.snapshot.queryParamMap));
    // Subsequent in-workspace navigations (a redirect guard while already on
    // /workspace changes only the query params — the host stays mounted).
    this.route.queryParamMap.pipe(takeUntilDestroyed(this.destroyRef)).subscribe((pm) => {
      if (this.ws.hydrated()) this.applyIntent(pm);
    });

    // Keyboard shortcuts (Ctrl/Cmd+Alt namespace).
    const onKeyDown = (e: KeyboardEvent) => applyWorkspaceShortcut(e, this.ws);
    window.addEventListener('keydown', onKeyDown);

    // One delegated capture-phase click interceptor — keep every in-app link
    // keep-alive-safe while in the workspace.
    const onClick = (e: MouseEvent) => {
      const intent = interpretWorkspaceLinkClick(e);
      if (!intent) return;
      // preventDefault for anchors; stopImmediatePropagation so RouterLink's own
      // bubble listener never fires (Angular RouterLink ignores defaultPrevented).
      e.preventDefault();
      e.stopImmediatePropagation();
      e.stopPropagation();
      this.ws.openTab(intent.kind, intent.payload);
    };
    document.addEventListener('click', onClick, true);

    this.destroyRef.onDestroy(() => {
      window.removeEventListener('keydown', onKeyDown);
      document.removeEventListener('click', onClick, true);
    });
  }

  private applyIntent(pm: ParamMap): void {
    if (!pm.get('open')) return;
    const intent = parseOpenIntent(pm);
    if (intent) applyOpenIntent(this.ws, intent);
    // Strip the intent params; keep the resting URL a clean /workspace.
    void this.router.navigate(['/workspace'], { replaceUrl: true });
  }

  // --- template helpers (read the live state signal) ---
  protected paneOf(id: string): PaneId {
    return paneOfTab(this.state(), id) ?? 'left';
  }
  protected isVisible(id: string): boolean {
    return isActiveInItsPane(this.state(), id);
  }
  protected isMounted(tab: WorkspaceTab): boolean {
    if (this.isVisible(tab.id)) return true;
    // Mount (hidden) a Salon while one of its child tabs is visible, so it can
    // portal that pane into the child even while the Salon tab is hidden.
    if (tab.kind !== 'salon') return false;
    return this.allTabs().some(
      (t) => t.parentTabId === tab.id && isActiveInItsPane(this.state(), t.id),
    );
  }
  protected emptyPane(p: PaneId): boolean {
    const ps = p === 'left' ? this.state().panes.left : this.state().panes.right;
    if (!ps) return false;
    return !(ps.activeTabId != null && this.state().tabs[ps.activeTabId] != null);
  }

  // --- actions ---
  protected onMove(pane: PaneId, e: { id: string; toIndex: number }): void {
    this.ws.moveTab(e.id, pane, e.toIndex);
  }
  protected onPaneMouseDown(pane: PaneId): void {
    if (this.state().focusedPane !== pane) this.ws.setFocusedPane(pane);
  }
  protected onSplitDrop(e: DragEvent): void {
    e.preventDefault();
    const id = this.draggingId();
    if (id) this.ws.splitTo(id, 'right');
    this.draggingId.set(null);
  }
}
