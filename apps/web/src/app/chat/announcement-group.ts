import { ChangeDetectionStrategy, Component, computed, input, signal } from '@angular/core';

import { Icon } from '../ui/icon';
import type { AnnouncementChip } from './chat-view-model';
import { MessageContent } from './message-content';

/**
 * A packed row of collapsed Staff announcement chips (v4 `qt-chat-announcement-group`).
 * Each chip shows an importance dot, the sender name, its kind, and a time; a
 * click expands it into the full announcement body below the row.
 */
@Component({
  selector: 'qt-announcement-group',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, MessageContent],
  template: `
    <div class="qt-chat-announcement-group">
      @for (chip of chips(); track chip.id) {
        <button
          type="button"
          class="qt-chat-announcement-chip"
          [title]="chip.sender + ' · ' + chip.kind"
          (click)="toggle(chip.id)"
        >
          <span class="qt-chat-announcement-dot" [class]="dotClass(chip)"></span>
          <span class="qt-chat-system-bar-sender">{{ chip.sender }}</span>
          <span class="qt-chat-system-bar-kind">{{ chip.kind }}</span>
          <span class="qt-chat-system-bar-time">{{ time(chip.createdAt) }}</span>
          <qt-icon name="chevron-down" class="qt-chat-system-bar-chevron" />
        </button>
      }
    </div>

    @for (chip of expandedChips(); track chip.id) {
      <div class="qt-chat-message-row qt-chat-message-row-assistant">
        <div class="qt-chat-message-body">
          <div class="qt-chat-message qt-chat-message-system">
            <qt-message-content [content]="chip.message.content" />
          </div>
        </div>
      </div>
    }
  `,
})
export class AnnouncementGroup {
  readonly chips = input.required<AnnouncementChip[]>();

  private readonly expanded = signal<Set<string>>(new Set());

  protected readonly expandedChips = computed(() =>
    this.chips().filter((c) => this.expanded().has(c.id)),
  );

  protected toggle(id: string): void {
    const next = new Set(this.expanded());
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    this.expanded.set(next);
  }

  protected dotClass(chip: AnnouncementChip): string {
    return `qt-chat-announcement-dot-${chip.importance}`;
  }

  protected time(iso: string): string {
    const d = new Date(iso);
    return Number.isNaN(d.getTime())
      ? ''
      : d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  }
}
