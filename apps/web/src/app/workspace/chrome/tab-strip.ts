/**
 * A pane's tab bar (port of v4 `TabStrip.tsx`, baseline `b8b12695`; the
 * roving-focus tabs of `94f041764` at `624d0d25c`, P4.D383): clickable tabs
 * with an active highlight, a close affordance, and HTML5 drag to reorder
 * within the strip or move a tab into this pane from the other one. Split/join
 * via the centre drop-zone lives in the host. v5 keeps `data-tab-id` and the
 * `qt-tab` classes on the OUTER element (R12) — the e2e selectors and the
 * active-tab scroll read them there; the inner button carries the tab role.
 *
 * @module workspace/chrome/tab-strip
 */

import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  effect,
  inject,
  input,
  output,
} from '@angular/core';

import { Icon, type IconName } from '../../ui/icon';
import type { PaneId, PaneState, WorkspaceTab } from '../workspace-contract';

@Component({
  selector: 'qt-tab-strip',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon],
  template: `
    <div
      class="qt-tab-strip"
      [attr.data-pane]="pane()"
      role="tablist"
      (dragover)="onStripDragOver($event)"
      (drop)="onStripDrop($event)"
    >
      @for (id of paneState().order; track id) {
        @let tab = tabs()[id];
        @if (tab) {
          @let isActive = id === paneState().activeTabId;
          <div
            role="presentation"
            [attr.data-tab-id]="id"
            draggable="true"
            class="qt-tab"
            [class.qt-tab-active]="isActive"
            [class.qt-tab-dragging]="id === draggingId()"
            (click)="select.emit(id)"
            (dragstart)="onDragStart($event, id)"
            (dragend)="dragEndTab.emit()"
            (dragover)="onTabDragOver($event)"
            (drop)="onTabDrop($event, $index)"
          >
            <!--
              The tab itself is a real button, so it is reachable and operable
              by keyboard; the close button sits beside it rather than inside
              it (an interactive control may not nest another). A tablist may
              own only tabs, so the close cross is a pointer convenience:
              hidden from assistive tech and out of the Tab order, with Delete
              on the focused tab as its keyboard equivalent.
            -->
            <button
              type="button"
              role="tab"
              [attr.aria-selected]="isActive"
              [attr.tabindex]="isActive ? 0 : -1"
              aria-keyshortcuts="Delete"
              class="qt-tab-select"
              (keydown)="onTabKeyDown($event, id)"
            >
              @if (tab.icon) {
                <qt-icon [name]="asIcon(tab.icon)" class="qt-tab-icon w-4 h-4" />
              }
              <span class="qt-tab-label">{{ tab.title }}</span>
            </button>
            <button
              type="button"
              class="qt-tab-close"
              tabindex="-1"
              aria-hidden="true"
              (click)="onCloseClick($event, id)"
              (pointerdown)="$event.stopPropagation()"
            >
              <qt-icon name="close" class="qt-tab-close-icon w-3 h-3" />
            </button>
          </div>
        }
      }
    </div>
  `,
})
export class TabStrip {
  private readonly host = inject(ElementRef<HTMLElement>);

  readonly pane = input.required<PaneId>();
  readonly paneState = input.required<PaneState>();
  readonly tabs = input.required<Record<string, WorkspaceTab>>();
  readonly draggingId = input<string | null>(null);

  readonly dragStartTab = output<string>();
  readonly dragEndTab = output<void>();
  readonly select = output<string>();
  readonly close = output<string>();
  /** Move the dragged tab into THIS pane at `toIndex` (strip-level = end). */
  readonly move = output<{ id: string; toIndex: number }>();

  constructor() {
    // Keep the active tab visible when the strip scrolls horizontally.
    effect(() => {
      const activeId = this.paneState().activeTabId;
      if (!activeId) return;
      queueMicrotask(() => {
        const el = (this.host.nativeElement as HTMLElement).querySelector(
          `.qt-tab[data-tab-id="${CSS.escape(activeId)}"]`,
        ) as HTMLElement | null;
        // jsdom has no scrollIntoView; guard so tests are safe.
        if (el && typeof el.scrollIntoView === 'function') {
          el.scrollIntoView({ inline: 'nearest', block: 'nearest' });
        }
      });
    });
  }

  /**
   * Roving focus across the strip's tabs (WAI-ARIA tabs pattern): only the
   * active tab is a Tab stop; arrows / Home / End move focus and select.
   * (v4 `TabStrip.tsx` `handleTabKeyDown`, `94f041764`.) `order` is read
   * before anything is emitted, so a select that re-renders the strip
   * cannot shift the index the focus move uses.
   */
  protected onTabKeyDown(e: KeyboardEvent, id: string): void {
    const tabs = this.tabs();
    const order = this.paneState().order.filter((tid) => tabs[tid]);
    const index = order.indexOf(id);
    if (index < 0) return;
    let next: number | null = null;
    if (e.key === 'ArrowRight') next = (index + 1) % order.length;
    else if (e.key === 'ArrowLeft') next = (index - 1 + order.length) % order.length;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = order.length - 1;
    else if (e.key === 'Delete') {
      e.preventDefault();
      this.close.emit(order[index]);
      return;
    }
    if (next === null) return;
    e.preventDefault();
    this.select.emit(order[next]);
    const buttons = (this.host.nativeElement as HTMLElement).querySelectorAll<HTMLButtonElement>(
      '[role="tab"]',
    );
    buttons[next]?.focus();
  }

  protected asIcon(name: string): IconName {
    return name as IconName;
  }

  protected onStripDragOver(e: DragEvent): void {
    if (this.draggingId() != null) e.preventDefault();
  }

  protected onStripDrop(e: DragEvent): void {
    const id = this.draggingId();
    if (!id) return;
    e.preventDefault();
    this.move.emit({ id, toIndex: this.paneState().order.length });
    this.dragEndTab.emit();
  }

  protected onDragStart(e: DragEvent, id: string): void {
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = 'move';
      e.dataTransfer.setData('text/plain', id);
    }
    this.dragStartTab.emit(id);
  }

  protected onTabDragOver(e: DragEvent): void {
    if (this.draggingId() != null) {
      e.preventDefault();
      e.stopPropagation();
    }
  }

  protected onTabDrop(e: DragEvent, index: number): void {
    const id = this.draggingId();
    if (!id) return;
    e.preventDefault();
    e.stopPropagation();
    this.move.emit({ id, toIndex: index });
    this.dragEndTab.emit();
  }

  protected onCloseClick(e: MouseEvent, id: string): void {
    e.stopPropagation();
    this.close.emit(id);
  }
}
