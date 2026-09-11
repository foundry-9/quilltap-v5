import { ChangeDetectionStrategy, Component, computed, input, output, signal } from '@angular/core';

export type MemoryCascadeAction = 'DELETE_MEMORIES' | 'KEEP_MEMORIES' | 'REGENERATE_MEMORIES';

/** What the dialog answers with (v4 `onConfirm(action, rememberChoice)`). */
export interface MemoryCascadeChoice {
  action: MemoryCascadeAction;
  remember: boolean;
}

/**
 * The delete-message memory-cascade dialog (v4 `components/ui/MemoryCascadeDialog
 * .tsx`). Shown when a delete would orphan associated memories; the user picks
 * what to do with them. Microcopy is v4-verbatim.
 *
 * The "Remember this choice" checkbox rides the `confirm` payload (v4
 * `onConfirm(action, rememberChoice)`), because the host is what writes the
 * preference: the Salon PUTs `memoryCascadePreferences` and — since v4's bug-134
 * fix, the write side — invalidates the shared settings key, or "don't ask me
 * again" holds for this dialog and is forgotten by the next one until something
 * else refetches.
 */
@Component({
  selector: 'qt-memory-cascade-dialog',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="qt-dialog-overlay" (click)="cancel.emit()">
      <div class="qt-dialog-content max-w-md" (click)="$event.stopPropagation()">
        <h3 class="qt-heading-sm mb-4">Delete Message</h3>
        <p class="qt-body-sm mb-2">{{ bodyLine() }}</p>
        <p class="qt-body-sm font-medium mb-3">What would you like to do with the memories?</p>

        <div class="space-y-2 mb-4">
          @for (opt of options; track opt.value) {
            <label class="flex items-start gap-2 cursor-pointer">
              <input
                type="radio"
                name="memoryAction"
                class="mt-1"
                [value]="opt.value"
                [checked]="selected() === opt.value"
                (change)="selected.set(opt.value)"
              />
              <span>
                <span class="qt-text-primary font-medium">{{ opt.label }}</span>
                <span class="qt-text-secondary qt-body-sm block">{{ opt.sub }}</span>
              </span>
            </label>
          }
        </div>

        <label class="flex items-center gap-2 cursor-pointer mb-4">
          <input
            type="checkbox"
            class="qt-checkbox"
            [checked]="remember()"
            (change)="remember.set($any($event.target).checked)"
          />
          <span class="qt-text-xs qt-text-secondary">
            Remember this choice (can be changed in Settings)
          </span>
        </label>

        <div class="flex justify-end gap-2">
          <button type="button" class="qt-button-secondary qt-button-sm" (click)="cancel.emit()">
            Cancel
          </button>
          <button
            type="button"
            class="qt-button-destructive qt-button-sm"
            (click)="confirm.emit({ action: selected(), remember: remember() })"
          >
            Delete Message
          </button>
        </div>
      </div>
    </div>
  `,
})
export class MemoryCascadeDialog {
  readonly memoryCount = input.required<number>();
  readonly isSwipeGroup = input(false);

  readonly confirm = output<MemoryCascadeChoice>();
  readonly cancel = output<void>();

  protected readonly selected = signal<MemoryCascadeAction>('DELETE_MEMORIES');
  /** v4's `rememberChoice` state, unchecked on every open (`:39`). */
  protected readonly remember = signal(false);

  protected readonly options: { value: MemoryCascadeAction; label: string; sub: string }[] = [
    { value: 'DELETE_MEMORIES', label: 'Delete memories too', sub: 'Permanently remove associated memories' },
    {
      value: 'KEEP_MEMORIES',
      label: 'Keep memories',
      sub: 'Memories will be orphaned (no link to source message)',
    },
    {
      value: 'REGENERATE_MEMORIES',
      label: 'Delete and regenerate',
      sub: 'Delete old memories and extract new ones from conversation context',
    },
  ];

  protected readonly bodyLine = computed(() => {
    const n = this.memoryCount();
    const noun = n === 1 ? 'memory' : 'memories';
    return `This message has ${n} associated ${noun}.`;
  });
}
