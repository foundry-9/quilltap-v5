import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

/**
 * Confirmation prompt shown when switching a bundle with components back to a
 * single garment: the components are discarded, and the user chooses whether to
 * keep the types they were computing or reset them
 * (v4 `components/wardrobe/wardrobe-item-editor/WardrobeModeChangePrompt.tsx`).
 *
 * Stacks above the editor dialog (v4 z-[90]/z-[100], `:25/:31`).
 */
@Component({
  selector: 'qt-wardrobe-mode-change-prompt',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <button
      class="qt-dialog-overlay !p-0 cursor-default border-none z-[90]"
      (click)="cancel.emit()"
      aria-label="Cancel"
      type="button"
    ></button>
    <div
      class="fixed top-1/2 left-1/2 transform -translate-x-1/2 -translate-y-1/2 z-[100] pointer-events-auto"
      style="width: min(420px, calc(100vw - 2rem))"
    >
      <div class="qt-dialog">
        <div class="qt-dialog-header">
          <h3 class="qt-dialog-title">Switch to a single garment?</h3>
        </div>
        <div class="qt-dialog-body">
          <p class="qt-text-small">
            This will discard the {{ componentCount() }} component{{ plural() }}. Keep types as
            they are now, or reset?
          </p>
        </div>
        <div class="qt-dialog-footer flex flex-wrap gap-2 justify-end">
          <button type="button" (click)="cancel.emit()" class="qt-button-ghost">Cancel</button>
          <button type="button" (click)="reset.emit()" class="qt-button-secondary">
            Reset types
          </button>
          <button type="button" (click)="keepTypes.emit()" class="qt-button-primary">
            Keep types
          </button>
        </div>
      </div>
    </div>
  `,
})
export class WardrobeModeChangePrompt {
  /** How many components will be discarded by switching to a single garment. */
  readonly componentCount = input.required<number>();
  readonly cancel = output<void>();
  readonly reset = output<void>();
  readonly keepTypes = output<void>();

  /** v4 `:41` — `componentCount === 1 ? '' : 's'`. */
  protected readonly plural = computed(() => (this.componentCount() === 1 ? '' : 's'));
}
