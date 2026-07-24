import { ChangeDetectionStrategy, Component, computed, input, signal } from '@angular/core';

import type { ChatDetail } from '../core/core-contract';
import { TerminalEmbed } from '../terminal/terminal-embed';
import { extractTerminalSessionId } from '../terminal/terminal-protocol';
import { Icon } from '../ui/icon';
import type { AnnouncementChip } from './chat-view-model';
import { MessageContent } from './message-content';
import { ToolMessage } from './tool-message';

/**
 * A packed row of collapsed Staff announcement chips (v4 `qt-chat-announcement-group`).
 * Each chip shows an importance dot, the sender name, its kind, and a time; a
 * click expands it into the full announcement body below the row.
 *
 * v5 groups every Staff announcement (bar Carina) into a chip, so an Ariel
 * `session-opened` message renders here, not in a `qt-message-row` — v4 collapses
 * it the same way and shows the terminal embed only when the chip is EXPANDED
 * (v4 renders the full `MessageRow`, incl. its embed, for an expanded
 * announcement). So the inline `qt-terminal-embed` rides the expanded body here,
 * gated on the same `session-opened` + `<!-- terminalSessionId:UUID -->` marker.
 */
@Component({
  selector: 'qt-announcement-group',
  changeDetection: ChangeDetectionStrategy.OnPush,
  imports: [Icon, MessageContent, TerminalEmbed, ToolMessage],
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
      @if (chip.message.role === 'TOOL') {
        <!-- A user-initiated Prospero tool run: the expanded chip shows the full
             tool card (v4 renders the standalone MessageRow → ToolMessage for an
             expanded TOOL announcement). -->
        <qt-tool-message [message]="chip.message" [chat]="chat()" />
      } @else {
        <div class="qt-chat-message-row qt-chat-message-row-assistant">
          <div class="qt-chat-message-body">
            <div class="qt-chat-message qt-chat-message-system">
              <qt-message-content [content]="chip.message.content" />
              @if (terminalSessionId(chip); as sid) {
                <div class="mt-2">
                  <qt-terminal-embed [sessionId]="sid" [chatId]="chatId()" />
                </div>
              }
            </div>
          </div>
        </div>
      }
    }
  `,
})
export class AnnouncementGroup {
  readonly chips = input.required<AnnouncementChip[]>();
  /** The parent chat id (the embed binds pop-out / kill against it). */
  readonly chatId = input.required<string>();
  /** The parent chat — threaded to an expanded TOOL chip's card for author resolution. */
  readonly chat = input.required<ChatDetail>();

  /**
   * The bound terminal session for an Ariel session-opened chip (v4 `MessageRow`
   * gate: `systemSender === 'ariel' && systemKind === 'session-opened'`, then the
   * marker) — or null.
   */
  protected terminalSessionId(chip: AnnouncementChip): string | null {
    const m = chip.message;
    if (m.systemSender !== 'ariel' || m.systemKind !== 'session-opened') return null;
    return extractTerminalSessionId(m.content);
  }

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
