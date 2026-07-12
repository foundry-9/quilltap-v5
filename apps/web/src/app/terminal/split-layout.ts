import { NgTemplateOutlet } from '@angular/common';
import {
  ChangeDetectionStrategy,
  Component,
  computed,
  DestroyRef,
  ElementRef,
  inject,
  input,
  output,
  signal,
  TemplateRef,
  viewChild,
} from '@angular/core';

import { RightPaneVerticalSplit } from './right-pane-vertical-split';
import { clampDividerPosition } from './split-clamp';

type LayoutMode = 'normal' | 'split' | 'focus';

/**
 * `qt-split-layout` — the resizable two-pane Salon layout (v4 `SplitLayout`).
 * `normal`: chat fills the width. `split`: chat on the left, the right pane on
 * the right with a draggable divider. `focus`: the right pane fills the width,
 * the chat hidden. The right pane hosts the document, the terminal, or both
 * (stacked via {@link RightPaneVerticalSplit}). Only the terminal mounts this
 * round; the `documentContent` slot is ported for the next Salon slice.
 *
 * The chat pane stays mounted across every mode change (v4's note: unmounting it
 * destroys the composer's in-flight editor state), so only its width/visibility
 * varies. Content is passed as `TemplateRef`s (v4's `ReactNode` props).
 */
@Component({
  selector: 'qt-split-layout',
  // Stretch inside .qt-chat-main so .qt-doc-split-layout (h-full) gets a BOUNDED
  // height — an unstyled Angular host breaks the flex chain and the message
  // list's scroll container loses its bound (dogfood finding #3a; v4/React has
  // no host wrapper here).
  host: { class: 'flex flex-col flex-1 min-h-0 min-w-0' },
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [NgTemplateOutlet, RightPaneVerticalSplit],
  template: `
    <div
      #container
      class="qt-doc-split-layout"
      [class.qt-doc-focus-mode]="layoutMode() === 'focus'"
    >
      <div class="qt-doc-chat-pane" [style]="chatPaneStyle()">
        <ng-container [ngTemplateOutlet]="chatContent()" />
      </div>

      @if (layoutMode() === 'split') {
        <div
          class="qt-doc-divider"
          [class.qt-doc-divider-active]="isDragging()"
          role="separator"
          aria-label="Resize chat and right pane"
          aria-orientation="vertical"
          [attr.aria-valuenow]="splitPos()"
          [attr.aria-valuemin]="20"
          [attr.aria-valuemax]="80"
          tabindex="0"
          (mousedown)="onMouseDown($event)"
          (keydown)="onKeyDown($event)"
        >
          <div class="qt-doc-divider-grip"><span></span><span></span><span></span></div>
        </div>

        <div class="qt-doc-pane" [style.width.%]="100 - splitPos()">
          <ng-container [ngTemplateOutlet]="rightPaneTemplate()!" />
        </div>
      } @else if (layoutMode() === 'focus') {
        <div class="flex flex-col h-full overflow-hidden flex-1 min-w-0">
          <ng-container [ngTemplateOutlet]="rightPaneTemplate()!" />
        </div>
      }
    </div>

    <!-- Right-pane composition (a single template chosen by which panes are open). -->
    <ng-template #both>
      <qt-right-pane-vertical-split
        [position]="rightPaneVerticalSplit()"
        [topContent]="documentContent()!"
        [bottomContent]="terminalContent()!"
        (positionChange)="rightPaneVerticalSplitChange.emit($event)"
      />
    </ng-template>
  `,
})
export class SplitLayout {
  /** Combined mode derived from documentMode + terminalMode. */
  readonly mode = input.required<LayoutMode>();
  readonly dividerPosition = input.required<number>();
  readonly rightPaneVerticalSplit = input.required<number>();
  readonly chatContent = input.required<TemplateRef<unknown>>();
  /** Document pane content. Null when Document Mode is off (always null this round). */
  readonly documentContent = input<TemplateRef<unknown> | null>(null);
  /** Terminal pane content. Null when Terminal Mode is off. */
  readonly terminalContent = input<TemplateRef<unknown> | null>(null);

  readonly dividerPositionChange = output<number>();
  readonly rightPaneVerticalSplitChange = output<number>();

  private readonly container = viewChild.required<ElementRef<HTMLDivElement>>('container');
  private readonly both = viewChild.required<TemplateRef<unknown>>('both');
  private readonly destroyRef = inject(DestroyRef);

  protected readonly isDragging = signal(false);
  private readonly currentPosition = signal(0);
  private positionRef = 0;

  /** The single right-pane template: both stacked, else whichever one is open. */
  protected readonly rightPaneTemplate = computed<TemplateRef<unknown> | null>(() => {
    const doc = this.documentContent();
    const term = this.terminalContent();
    if (doc && term) return this.both();
    return doc ?? term ?? null;
  });

  /** With no right pane there's nothing to split/focus — the chat fills the width. */
  protected readonly layoutMode = computed<LayoutMode>(() =>
    this.rightPaneTemplate() ? this.mode() : 'normal',
  );

  protected readonly splitPos = () =>
    this.isDragging() ? this.currentPosition() : this.dividerPosition();

  protected readonly chatPaneStyle = computed<Record<string, string>>(() => {
    const style: Record<string, string> = {};
    switch (this.layoutMode()) {
      case 'split':
        style['width'] = `${this.splitPos()}%`;
        break;
      case 'focus':
        style['display'] = 'none';
        break;
      default:
        style['flex'] = '1';
        style['min-width'] = '0';
    }
    return style;
  });

  protected onMouseDown(e: MouseEvent): void {
    e.preventDefault();
    this.positionRef = this.dividerPosition();
    this.currentPosition.set(this.dividerPosition());
    this.isDragging.set(true);

    const rect = this.container().nativeElement.getBoundingClientRect();
    const width = rect.width;

    const onMove = (move: MouseEvent) => {
      const relativeX = move.clientX - rect.left;
      const percentage = (relativeX / width) * 100;
      this.positionRef = clampDividerPosition(percentage, width);
      this.currentPosition.set(this.positionRef);
    };
    const onUp = () => {
      document.removeEventListener('mousemove', onMove);
      document.removeEventListener('mouseup', onUp);
      document.body.style.cursor = '';
      document.body.style.userSelect = '';
      this.isDragging.set(false);
      this.dividerPositionChange.emit(this.positionRef);
    };
    document.addEventListener('mousemove', onMove);
    document.addEventListener('mouseup', onUp);
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
    this.destroyRef.onDestroy(() => {
      document.removeEventListener('mousemove', onMove);
      document.removeEventListener('mouseup', onUp);
    });
  }

  protected onKeyDown(event: KeyboardEvent): void {
    if (this.layoutMode() !== 'split') return;
    const width = this.container().nativeElement.getBoundingClientRect().width;
    const base = this.isDragging() ? this.currentPosition() : this.dividerPosition();
    let next: number | null = null;
    switch (event.key) {
      case 'ArrowLeft':
        next = base - 5;
        break;
      case 'ArrowRight':
        next = base + 5;
        break;
      case 'Home':
        next = 20;
        break;
      case 'End':
        next = 80;
        break;
      default:
        return;
    }
    event.preventDefault();
    const clamped = clampDividerPosition(next, width);
    this.positionRef = clamped;
    this.currentPosition.set(clamped);
    this.dividerPositionChange.emit(clamped);
  }
}
