import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

/**
 * v4 `components/ui/FormActions.tsx` — a right-aligned Cancel/Submit pair with a
 * spinner while loading.
 */
@Component({
  selector: 'qt-form-actions',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="flex gap-2 justify-end">
      <button type="button" class="qt-button-secondary" [disabled]="isLoading()" (click)="cancel.emit()">
        {{ cancelLabel() }}
      </button>
      @if (showSubmit()) {
        <button
          [type]="type()"
          class="qt-button-primary"
          [disabled]="isDisabled() || isLoading()"
          (click)="submit.emit()"
        >
          @if (isLoading()) {
            <span class="qt-spinner-sm"></span>
          }
          {{ submitLabel() }}
        </button>
      }
    </div>
  `,
})
export class FormActions {
  readonly submitLabel = input<string>('Save');
  readonly cancelLabel = input<string>('Cancel');
  readonly isLoading = input(false);
  readonly isDisabled = input(false);
  readonly type = input<'button' | 'submit'>('button');
  readonly showSubmit = input(true);
  readonly cancel = output<void>();
  readonly submit = output<void>();
}
