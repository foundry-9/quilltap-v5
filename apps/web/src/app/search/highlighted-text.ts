import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import { highlightSegments } from './search.logic';

/**
 * v4 `HighlightedText` (`search-results.tsx`): the case-insensitive query
 * matches inside `text` wear `<mark class="qt-highlight">`.
 */
@Component({
  selector: 'qt-highlighted-text',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @for (segment of segments(); track $index) {
      @if (segment.match) {
        <mark class="qt-highlight">{{ segment.text }}</mark>
      } @else {
        {{ segment.text }}
      }
    }
  `,
})
export class HighlightedText {
  readonly text = input.required<string>();
  readonly query = input.required<string>();

  protected readonly segments = computed(() => highlightSegments(this.text(), this.query()));
}
