import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

/** v4 `components/ui/ErrorAlert.tsx` — an error banner with an optional Retry. */
@Component({
  selector: 'qt-error-alert',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div [class]="cssClass()">
      <div class="flex items-center justify-between gap-4">
        <div class="flex-1"><p class="qt-label">{{ message() }}</p></div>
        @if (retryable()) {
          <button type="button" class="qt-button-ghost qt-button-sm flex-shrink-0" (click)="retry.emit()">Retry</button>
        }
      </div>
    </div>
  `,
})
export class ErrorAlert {
  readonly message = input.required<string>();
  readonly retryable = input(false);
  readonly klass = input<string>('', { alias: 'class' });
  readonly retry = output<void>();
  protected readonly cssClass = computed(() => `qt-alert-error ${this.klass()}`.trim());
}
