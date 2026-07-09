import { ChangeDetectionStrategy, Component, computed, input, output } from '@angular/core';

/**
 * v4 `components/ui/EmptyState.tsx` — an empty-list placeholder with an optional
 * action. (The React version takes an `action.onClick` callback; here the action
 * label is an input and the click is an `(action)` output.)
 */
@Component({
  selector: 'qt-empty-state',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div [class]="containerClass()">
      @if (icon()) {
        <div class="qt-empty-state-icon"><ng-content select="[qt-empty-icon]"></ng-content></div>
      }
      <h3 class="qt-empty-state-title">{{ title() }}</h3>
      @if (description()) {
        <p class="qt-empty-state-description">{{ description() }}</p>
      }
      @if (actionLabel()) {
        <div class="qt-empty-state-action">
          <button type="button" class="qt-button-primary qt-button-sm" (click)="action.emit()">
            {{ actionLabel() }}
          </button>
        </div>
      }
    </div>
  `,
})
export class EmptyState {
  readonly title = input.required<string>();
  readonly description = input<string | undefined>(undefined);
  readonly actionLabel = input<string | undefined>(undefined);
  /** Set true and project `[qt-empty-icon]` content to render the icon slot. */
  readonly icon = input(false);
  readonly variant = input<'default' | 'dashed' | 'muted'>('default');
  readonly action = output<void>();

  protected readonly containerClass = computed(() => {
    const base = 'qt-empty-state rounded-lg';
    switch (this.variant()) {
      case 'dashed':
        return `${base} border border-dashed qt-border-default bg-transparent`;
      case 'muted':
        return `${base} qt-bg-muted/50`;
      default:
        return `${base} qt-bg-muted border qt-border-default`;
    }
  });
}
