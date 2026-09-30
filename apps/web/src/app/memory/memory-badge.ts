import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

import { Icon } from '../ui/icon';
import { Tooltip } from '../ui/tooltip';

/**
 * The chat card's memory badge — v4 `ChatCard.tsx:267-283`, the ONE badge both
 * of its hosts (the Salon list and the character Conversations tab) render: a
 * button that asks to delete the chat's memories and re-extract them.
 *
 * The card is a link, so the click stops the navigation before it emits; the
 * host owns the confirm, the dispatch and its own refresh
 * (`confirmAndReextractMemories`). Host `display: contents` so the card's
 * flex-wrap title row is unchanged.
 *
 * @module memory/memory-badge
 */
@Component({
  selector: 'qt-memory-badge',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, Tooltip],
  host: { style: 'display: contents' },
  template: `
    <qt-tooltip content="Memories — click to delete and re-extract">
      <button
        type="button"
        class="chat-card__badge inline-flex items-center gap-1 rounded-full qt-bg-primary/10 px-2.5 py-0.5 qt-body-sm font-semibold flex-shrink-0 hover:qt-bg-primary/20 transition-colors cursor-pointer"
        [attr.aria-label]="count() + ' memories — delete and re-extract'"
        (click)="onClick($event)"
      >
        <qt-icon name="book" class="w-3 h-3" />{{ count() }}
      </button>
    </qt-tooltip>
  `,
})
export class MemoryBadge {
  readonly chatId = input.required<string>();
  readonly count = input.required<number>();
  readonly reextract = output<string>();

  protected onClick(event: Event): void {
    event.preventDefault();
    event.stopPropagation();
    this.reextract.emit(this.chatId());
  }
}
