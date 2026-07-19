/**
 * Workspace backdrop (port of v4 `workspace-backdrop.tsx` render half, baseline
 * `b8b12695`): one arbitrated story/subsystem background behind both panes.
 *
 * Rule:
 *  - A conversation (Salon) WITH a background wins, full-screen (focused pane
 *    preferred when both panes are Salons-with-backgrounds).
 *  - Otherwise, no split → the single active tab's background fills the screen.
 *  - Otherwise (split) → each pane's active-tab background dominates its own side
 *    and softly crossfades across the divider at `--qt-split`.
 *
 * The registry (`report`/`clear`) is the reporting half — views call it via the
 * `WORKSPACE_BACKDROP_REGISTRY` token; this component reads the entries and
 * paints the winner.
 *
 * @module workspace/chrome/workspace-backdrop
 */

import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';

import type { WorkspaceBackdropEntry, WorkspaceState } from '../workspace-contract';
import { WorkspaceService } from '../workspace.service';
import { WorkspaceBackdropRegistryImpl } from './registries';

export interface ArbitratedBackdrop {
  fullUrl: string | null;
  leftUrl: string | null;
  rightUrl: string | null;
  splitPct: string;
}

/**
 * The v4 backdrop arbitration rule, extracted pure so it can be diffed against
 * v4's `WorkspaceBackdrop` branch logic without a render harness.
 */
export function arbitrateBackdrop(
  state: WorkspaceState,
  entries: Readonly<Record<string, WorkspaceBackdropEntry>>,
): ArbitratedBackdrop | null {
  const split = state.panes.right != null;
  const leftActive = state.panes.left.activeTabId;
  const rightActive = state.panes.right?.activeTabId ?? null;
  const leftBg = leftActive ? entries[leftActive] : undefined;
  const rightBg = rightActive ? entries[rightActive] : undefined;

  // A Salon with a background always wins full-screen; prefer the focused pane
  // when both panes are Salons-with-backgrounds.
  const order = state.focusedPane === 'right' ? [rightBg, leftBg] : [leftBg, rightBg];
  const salonWin = order.find((b) => b?.isSalon && b.url);

  let fullUrl: string | null = null;
  let leftUrl: string | null = null;
  let rightUrl: string | null = null;

  if (salonWin) {
    fullUrl = salonWin.url;
  } else if (!split) {
    fullUrl = leftBg?.url ?? null;
  } else {
    leftUrl = leftBg?.url ?? null;
    rightUrl = rightBg?.url ?? null;
  }

  if (!fullUrl && !leftUrl && !rightUrl) return null;
  return { fullUrl, leftUrl, rightUrl, splitPct: `${Math.round(state.splitRatio * 100)}%` };
}

@Component({
  selector: 'qt-workspace-backdrop',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @let v = view();
    @if (v) {
      <div
        class="qt-workspace-backdrop"
        aria-hidden="true"
        [style.--qt-split]="v.splitPct"
      >
        @if (v.fullUrl) {
          <div class="qt-workspace-backdrop-layer" [style.background-image]="bg(v.fullUrl)"></div>
        }
        @if (v.rightUrl) {
          <div
            class="qt-workspace-backdrop-layer qt-workspace-backdrop-right"
            [style.background-image]="bg(v.rightUrl)"
          ></div>
        }
        @if (v.leftUrl) {
          <div
            class="qt-workspace-backdrop-layer qt-workspace-backdrop-left"
            [style.background-image]="bg(v.leftUrl)"
          ></div>
        }
      </div>
    }
  `,
})
export class WorkspaceBackdrop {
  private readonly ws = inject(WorkspaceService);
  private readonly registry = inject(WorkspaceBackdropRegistryImpl);

  protected readonly view = computed(() => arbitrateBackdrop(this.ws.state(), this.registry.entries()));

  protected bg(url: string): string {
    return `url('${url}')`;
  }
}
