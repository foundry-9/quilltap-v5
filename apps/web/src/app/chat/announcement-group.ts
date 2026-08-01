import { ChangeDetectionStrategy, Component, computed, input, signal } from '@angular/core';

import type { ChatDetail } from '../core/core-contract';
import { TerminalEmbed } from '../terminal/terminal-embed';
import { extractTerminalSessionId } from '../terminal/terminal-protocol';
import { Icon } from '../ui/icon';
import type { AnnouncementChip } from './chat-view-model';
import { MessageContent } from './message-content';
import {
  getAnnouncementAccentClasses,
  getAnnouncementOutcomeState,
  type PascalOutcomeState,
} from './system-message-labels';
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
          [class]="chipClasses(chip)"
          [id]="'message-' + chip.id"
          [attr.data-message-id]="chip.id"
          [attr.aria-expanded]="isExpanded(chip.id)"
          [attr.aria-label]="chipLabel(chip)"
          (click)="toggle(chip.id)"
        >
          <span class="qt-chat-announcement-dot" [class]="dotClass(chip)" aria-hidden="true"></span>
          <span class="qt-chat-system-bar-sender">{{ chip.sender }}</span>
          <!-- The state is carried by colour alone in the bar; name it for readers
               who can't see the accent (and for anyone skimming with a screen
               reader). -->
          @if (outcomeState(chip); as state) {
            <span class="sr-only">{{ state }}</span>
          }
          <!-- v4 AnnouncementBarContents renders the kind span only when there
               IS a kind label. An unconditional span costs a kindless row the
               chip's 0.5rem flex gap twice over, so the sender floats away from
               its timestamp. -->
          @if (chip.kind) {
            <span class="qt-chat-system-bar-kind">{{ chip.kind }}</span>
          }
          <!-- The whisper audience ("to Alice, Bob"), or nothing when public
               (v4 WhisperTag, a163862c). -->
          @if (whisperNames(chip); as names) {
            <span class="qt-chat-system-bar-whisper" [title]="'Whispered to ' + names">
              <span class="sr-only">whispered </span>
              to {{ names }}
            </span>
          }
          <span class="qt-chat-system-bar-time">{{ time(chip.createdAt) }}</span>
          <!-- class on qt-icon is an INPUT applied to the inner span, so the
               modifier has to be composed into that string; a per-class host
               binding would land on the wrong element. -->
          <qt-icon
            [name]="isExpanded(chip.id) ? 'chevron-down' : 'chevron-right'"
            [class]="chevronClasses(chip.id)"
          />
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
            <!-- v4 renders an EXPANDED announcement's body through the ordinary
                 message block — chat-message qt-chat-message-assistant
                 (MessageRow.tsx:294-300; a Staff row is written role ASSISTANT,
                 so the USER arm never applies). .qt-chat-message-system exists
                 in v4's stylesheet but NO v4 markup wears it — the only use in
                 the whole checkout is the theme storybook's catalogue page. v5
                 had reached for it here, so every expanded announcement came out
                 text-sm italic text-center py-2 on a muted slab while every
                 other message rendered as a normal bubble (P4.26). -->
            <div class="qt-chat-message qt-chat-message-assistant">
              <qt-message-content
                [content]="chip.message.content"
                [blobMountPointId]="blobMountPointId()"
              />
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
   * The chat's blob mount point, threaded to the expanded body's markdown img
   * rewrite exactly as `MessageRow` threads it to an ordinary bubble. Without it
   * a Librarian announcement carrying a relative image ref renders a broken
   * image where the same ref in a character's line resolves (P4.26).
   */
  protected readonly blobMountPointId = computed(() => this.chat().blobMountPointId ?? null);

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

  protected isExpanded(id: string): boolean {
    return this.expanded().has(id);
  }

  /** v4 appends its `-down` modifier only while the bar is open. */
  protected chevronClasses(id: string): string {
    return this.isExpanded(id)
      ? 'qt-chat-system-bar-chevron qt-chat-system-bar-chevron-down'
      : 'qt-chat-system-bar-chevron';
  }

  /**
   * v4 `AnnouncementChip`'s `aria-label` — "Expand <sender> <kind> message".
   * v4 hard-codes the "Expand" verb because its chip is only ever the collapsed
   * affordance; v5 keeps the same chip mounted while the body renders below it,
   * so the verb follows the state.
   */
  protected chipLabel(chip: AnnouncementChip): string {
    const verb = this.isExpanded(chip.id) ? 'Collapse' : 'Expand';
    return `${verb} ${chip.sender}${chip.kind ? ` ${chip.kind}` : ''} message`;
  }

  protected toggle(id: string): void {
    const next = new Set(this.expanded());
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    this.expanded.set(next);
  }

  /**
   * A Pascal roll outcome swaps the importance dot for the outcome's own state —
   * the reader wants to know whether the table dealt a success or a failure, not
   * that Pascal is generically important (v4 `AnnouncementBarContents`,
   * `231be14c`). Defensive here: v5 renders Pascal's results as their own full
   * row, never a collapsed chip.
   */
  protected outcomeState(chip: AnnouncementChip): PascalOutcomeState | null {
    return getAnnouncementOutcomeState(chip.message);
  }

  protected dotClass(chip: AnnouncementChip): string {
    const state = this.outcomeState(chip);
    return state
      ? `qt-chat-announcement-dot-outcome-${state}`
      : `qt-chat-announcement-dot-${chip.importance}`;
  }

  /**
   * The chip's modifier classes: the `qt-pascal-result` accent for a roll's
   * chip wrapper, plus `qt-chat-announcement-chip-whisper` when the
   * announcement is a whisper (v4 `AnnouncementChip.tsx`'s `isWhisper`,
   * `a163862c`) — the whisper border color, so a private aside is
   * distinguishable from a public one before it's ever expanded.
   */
  protected chipClasses(chip: AnnouncementChip): string {
    const accent = getAnnouncementAccentClasses(chip.message);
    const whisper = this.isWhisperChip(chip) ? 'qt-chat-announcement-chip-whisper' : '';
    return [accent, whisper].filter(Boolean).join(' ');
  }

  protected isWhisperChip(chip: AnnouncementChip): boolean {
    return !!chip.message.targetParticipantIds?.length;
  }

  /**
   * The whisper audience, "Alice, Bob" — or null when the announcement is
   * public (v4 `WhisperTag`, `a163862c`). Participant ids resolve against the
   * group's own `chat` input, the same source `message-row.ts`'s
   * `whisperTargets` reads.
   */
  protected whisperNames(chip: AnnouncementChip): string | null {
    const ids = chip.message.targetParticipantIds;
    if (!ids || ids.length === 0) return null;
    const participants = this.chat().participants;
    return ids
      .map((id) => participants.find((p) => p.id === id)?.character?.name || 'unknown')
      .join(', ');
  }

  protected time(iso: string): string {
    const d = new Date(iso);
    return Number.isNaN(d.getTime())
      ? ''
      : d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });
  }
}
