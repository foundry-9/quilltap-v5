/**
 * Draggable divider between the two workspace panes (port of v4
 * `WorkspaceDivider.tsx`, baseline `b8b12695`). Pointer-drag adjusts the left
 * pane's fraction of total width; arrow keys nudge it; double-click / Enter /
 * Home reset to an even split. Clamping to bounds happens in the reducer
 * (`SET_SPLIT_RATIO`).
 *
 * @module workspace/chrome/workspace-divider
 */

import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  inject,
  input,
  output,
} from '@angular/core';

/** Keyboard nudge step (fraction of width). */
const KEY_STEP = 0.02;

@Component({
  selector: 'qt-workspace-divider',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div
      class="qt-workspace-divider"
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize panes"
      [attr.aria-valuenow]="round100()"
      [attr.aria-valuemin]="0"
      [attr.aria-valuemax]="100"
      tabindex="0"
      (pointerdown)="onPointerDown($event)"
      (dblclick)="reset.emit()"
      (keydown)="onKeyDown($event)"
    >
      <div class="qt-workspace-divider-grip"></div>
    </div>
  `,
})
export class WorkspaceDivider {
  /** The pane-grid container whose rect maps clientX → ratio. */
  readonly containerEl = input.required<HTMLElement>();
  readonly ratio = input.required<number>();

  readonly ratioChange = output<number>();
  readonly reset = output<void>();

  private stop: () => void = () => undefined;

  constructor() {
    // Clean up any in-flight drag on destroy.
    inject(DestroyRef).onDestroy(() => this.stop());
  }

  protected round100(): number {
    return Math.round(this.ratio() * 100);
  }

  private handleMove(clientX: number): void {
    const el = this.containerEl();
    if (!el) return;
    const rect = el.getBoundingClientRect();
    if (rect.width <= 0) return;
    this.ratioChange.emit((clientX - rect.left) / rect.width);
  }

  protected onPointerDown(e: PointerEvent): void {
    e.preventDefault();
    const onPointerMove = (ev: PointerEvent) => this.handleMove(ev.clientX);
    const stop = () => {
      window.removeEventListener('pointermove', onPointerMove);
      window.removeEventListener('pointerup', stop);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
    };
    this.stop = stop;
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
    window.addEventListener('pointermove', onPointerMove);
    window.addEventListener('pointerup', stop);
  }

  protected onKeyDown(e: KeyboardEvent): void {
    if (e.key === 'ArrowLeft') {
      e.preventDefault();
      this.ratioChange.emit(this.ratio() - KEY_STEP);
    } else if (e.key === 'ArrowRight') {
      e.preventDefault();
      this.ratioChange.emit(this.ratio() + KEY_STEP);
    } else if (e.key === 'Home' || e.key === 'Enter') {
      e.preventDefault();
      this.reset.emit();
    }
  }
}
