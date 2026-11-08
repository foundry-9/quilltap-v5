import { ChangeDetectionStrategy, Component, input, output } from '@angular/core';

import type { ConversationMatch } from '../chat/conversation-search';
import { activateOnKey } from '../ui/activate-on-key';
import { HighlightedText } from './highlighted-text';

/**
 * v4 `formatWhen` (`conversation-matches.tsx`): the operator's own locale,
 * `''` for an unparseable date.
 */
export function formatWhen(createdAt: string): string {
  const date = new Date(createdAt);
  if (Number.isNaN(date.getTime())) return '';
  return date.toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  });
}

/**
 * The "This conversation" group of the search bar (v4
 * `components/search/conversation-matches.tsx`, `4f5047241`): matches inside
 * the open Salon chat, newest first. Picking one scrolls the transcript to it;
 * it does not navigate, so these are buttons rather than links.
 */
@Component({
  selector: 'qt-conversation-matches',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [HighlightedText],
  template: `
    <div class="py-2" role="group" aria-label="Matches in this conversation">
      <div class="px-3 py-1 qt-text-xs font-semibold uppercase tracking-wider">
        <span aria-hidden="true">🔖</span> This conversation ({{ countLabel() }})
      </div>
      @if (matches().length === 0) {
        <p class="px-3 py-2 qt-text-small">Not a whisper of "{{ query() }}" in this conversation</p>
      } @else {
        <div class="space-y-1">
          @for (match of matches(); track match.messageId) {
            <!--
              role="button" on a div, not a <button>: the row is prose, and two
              bundled themes set every <button> in tracked capitals.
            -->
            <div
              role="button"
              tabindex="0"
              (click)="select.emit(match.messageId)"
              (keydown)="activateOnKey($event)"
              class="block w-full text-left p-3 qt-hover-accent rounded-lg transition-colors cursor-pointer"
            >
              <div class="flex items-center gap-2 flex-wrap">
                <span class="qt-text-primary truncate">{{ match.speaker ?? 'Unknown speaker' }}</span>
                @if (match.isAnnouncement) {
                  <span class="text-xs px-1.5 py-0.5 rounded qt-badge-info">Announcement</span>
                }
                @if (match.occurrences > 1) {
                  <span class="qt-text-xs">×{{ match.occurrences }}</span>
                }
                <span class="qt-text-xs ml-auto">{{ formatWhen(match.createdAt) }}</span>
              </div>
              <p class="qt-text-small mt-1 line-clamp-2">
                <qt-highlighted-text [text]="match.snippet" [query]="query().trim()" />
              </p>
            </div>
          }
        </div>
      }
    </div>
  `,
})
export class ConversationMatches {
  readonly matches = input.required<ConversationMatch[]>();
  /** Every matching message, including those past the shown list. */
  readonly total = input.required<number>();
  readonly query = input.required<string>();
  readonly select = output<string>();

  protected readonly activateOnKey = activateOnKey;
  protected readonly formatWhen = formatWhen;

  protected countLabel(): string {
    const shown = this.matches().length;
    const total = this.total();
    return shown < total ? `${shown}/${total}` : `${total}`;
  }
}
