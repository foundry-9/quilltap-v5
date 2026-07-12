import { NgTemplateOutlet } from '@angular/common';
import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  ElementRef,
  inject,
  input,
  output,
  signal,
  TemplateRef,
  viewChild,
} from '@angular/core';

import { clampVerticalPosition } from './split-clamp';

const ABSOLUTE_MIN_PCT = 20;
const ABSOLUTE_MAX_PCT = 80;

/**
 * `qt-right-pane-vertical-split` — the right pane when BOTH Document Mode and
 * Terminal Mode are active: top = document, bottom = terminal, with a draggable
 * horizontal divider (v4 `RightPaneVerticalSplit`). Ported generically this
 * round even though only the terminal ever mounts it — Document Mode will supply
 * the top pane in a later Salon slice. Mirrors {@link SplitLayout}'s drag /
 * keyboard logic on the height axis.
 */
@Component({
  selector: 'qt-right-pane-vertical-split',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [NgTemplateOutlet],
  template: `
    <div #container class="qt-doc-vertical-split-layout">
      <div class="qt-doc-vertical-pane qt-doc-vertical-pane-top" [style.height.%]="splitPos()">
        <ng-container [ngTemplateOutlet]="topContent()" />
      </div>

      <div
        class="qt-doc-vertical-divider"
        [class.qt-doc-vertical-divider-active]="isDragging()"
        role="separator"
        aria-label="Resize document and terminal panes"
        aria-orientation="horizontal"
        [attr.aria-valuenow]="splitPos()"
        [attr.aria-valuemin]="20"
        [attr.aria-valuemax]="80"
        tabindex="0"
        (mousedown)="onMouseDown($event)"
        (keydown)="onKeyDown($event)"
      >
        <div class="qt-doc-vertical-divider-grip"><span></span><span></span><span></span></div>
      </div>

      <div
        class="qt-doc-vertical-pane qt-doc-vertical-pane-bottom"
        [style.height.%]="100 - splitPos()"
      >
        <ng-container [ngTemplateOutlet]="bottomContent()" />
      </div>
    </div>
  `,
})
export class RightPaneVerticalSplit {
  /** Top pane height as a percentage of the right-pane height (20–80). */
  readonly position = input.required<number>();
  readonly topContent = input.required<TemplateRef<unknown>>();
  readonly bottomContent = input.required<TemplateRef<unknown>>();
  readonly positionChange = output<number>();

  private readonly container = viewChild.required<ElementRef<HTMLDivElement>>('container');
  private readonly destroyRef = inject(DestroyRef);

  protected readonly isDragging = signal(false);
  private readonly currentPosition = signal(0);
  private positionRef = 0;

  protected readonly splitPos = () =>
    this.isDragging() ? this.currentPosition() : this.position();

  protected onMouseDown(e: MouseEvent): void {
    e.preventDefault();
    this.positionRef = this.position();
    this.currentPosition.set(this.position());
    this.isDragging.set(true);

    const rect = this.container().nativeElement.getBoundingClientRect();
    const height = rect.height;

    const onMove = (move: MouseEvent) => {
      const relativeY = move.clientY - rect.top;
      const percentage = (relativeY / height) * 100;
      this.positionRef = clampVerticalPosition(percentage, height);
      this.currentPosition.set(this.positionRef);
    };
    const onUp = () => {
      document.removeEventListener('mousemove', onMove);
      document.removeEventListener('mouseup', onUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      this.isDragging.set(false);
      this.positionChange.emit(this.positionRef);
    };
    document.addEventListener('mousemove', onMove);
    document.addEventListener('mouseup', onUp);
    document.body.style.cursor = 'row-resize';
    document.body.style.userSelect = 'none';
    this.destroyRef.onDestroy(() => {
      document.removeEventListener('mousemove', onMove);
      document.removeEventListener('mouseup', onUp);
    });
  }

  protected onKeyDown(event: KeyboardEvent): void {
    const height = this.container().nativeElement.getBoundingClientRect().height;
    const base = this.isDragging() ? this.currentPosition() : this.position();
    let next: number | null = null;
    switch (event.key) {
      case 'ArrowUp':
        next = base - 5;
        break;
      case 'ArrowDown':
        next = base + 5;
        break;
      case 'Home':
        next = ABSOLUTE_MIN_PCT;
        break;
      case 'End':
        next = ABSOLUTE_MAX_PCT;
        break;
      default:
        return;
    }
    event.preventDefault();
    const clamped = clampVerticalPosition(next, height);
    this.positionRef = clamped;
    this.currentPosition.set(clamped);
    this.positionChange.emit(clamped);
  }
}
