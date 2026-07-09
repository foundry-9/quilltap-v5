import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

/** v4 `components/ui/LoadingState.tsx` — spinner / text / dots loading feedback. */
@Component({
  selector: 'qt-loading-state',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div [class]="wrapperClass()">
      @switch (variant()) {
        @case ('text') {
          <p class="text-sm qt-text-secondary">{{ message() }}</p>
        }
        @case ('dots') {
          <div class="flex items-center gap-2">
            <p class="text-sm qt-text-secondary">{{ message() }}</p>
            <span class="flex gap-1">
              <span class="w-1.5 h-1.5 rounded-full bg-current animate-bounce" style="animation-delay: 0ms"></span>
              <span class="w-1.5 h-1.5 rounded-full bg-current animate-bounce" style="animation-delay: 150ms"></span>
              <span class="w-1.5 h-1.5 rounded-full bg-current animate-bounce" style="animation-delay: 300ms"></span>
            </span>
          </div>
        }
        @default {
          <div class="flex flex-col items-center gap-3">
            <div class="qt-spinner text-primary"></div>
            @if (message()) {
              <p class="text-sm qt-text-secondary">{{ message() }}</p>
            }
          </div>
        }
      }
    </div>
  `,
})
export class LoadingState {
  readonly message = input<string>('Loading...');
  readonly variant = input<'spinner' | 'text' | 'dots'>('spinner');
  readonly klass = input<string>('', { alias: 'class' });
  protected readonly wrapperClass = computed(() => `flex items-center justify-center py-8 ${this.klass()}`.trim());
}
