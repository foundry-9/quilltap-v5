import {
  ChangeDetectionStrategy,
  Component,
  computed,
  effect,
  input,
  signal,
  untracked,
} from '@angular/core';

/**
 * Reply Announcer — v4 `components/chat/ReplyAnnouncer.tsx` (`10e09b7c3`), whole.
 *
 * A visually hidden polite live region that tells a screen reader when a
 * character's reply lands in the Salon (WCAG 4.1.3). The message list itself
 * is virtualized — rows mount and unmount as you scroll — so marking it
 * `role="log"` would re-announce old messages; this region speaks only for
 * replies that arrive after the chat has loaded.
 *
 * Mounted by the Salon as the second child of `.qt-chat-main`, after the
 * `sr-only` `<h1>` (v4 `SalonView.tsx:1582-1583`) — that mount is P4.D344's
 * file (`salon-conversation.ts`), a HANDOFF from P4.D358.
 */
export const MAX_SPOKEN_CHARS = 300;

export interface AnnounceableMessage {
  id: string;
  role: string;
  content: string;
  participantId?: string | null;
  systemSender?: string | null;
}

export function spokenText(content: string): string {
  const flat = content.replace(/\s+/g, ' ').trim();
  return flat.length > MAX_SPOKEN_CHARS ? `${flat.slice(0, MAX_SPOKEN_CHARS)}…` : flat;
}

@Component({
  selector: 'qt-reply-announcer',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `<div class="sr-only" role="status" aria-live="polite" aria-atomic="true">{{ announcement() }}</div>`,
})
export class ReplyAnnouncer {
  readonly messages = input.required<readonly AnnounceableMessage[]>();
  /** Display name for a participant id, or undefined when unknown. */
  readonly nameFor = input.required<(participantId: string | null | undefined) => string | undefined>();

  protected readonly announcement = signal('');

  /** The last ASSISTANT row that is not a system sender's (v4 `:44-51`). */
  private readonly latest = computed<AnnounceableMessage | undefined>(() => {
    const messages = this.messages();
    for (let i = messages.length - 1; i >= 0; i--) {
      const m = messages[i];
      if (m.role === 'ASSISTANT' && !m.systemSender) return m;
    }
    return undefined;
  });

  constructor() {
    // undefined until the first check, so the history that is already there
    // when the chat opens is never read out (v4 `lastSeenIdRef`).
    let lastSeenId: string | null | undefined = undefined;
    effect(() => {
      const latest = this.latest();
      const nameFor = this.nameFor();
      untracked(() => {
        const latestId = latest?.id ?? null;
        if (lastSeenId === undefined) {
          lastSeenId = latestId;
          return;
        }
        if (!latest || latestId === lastSeenId) return;
        lastSeenId = latestId;
        const name = nameFor(latest.participantId) ?? 'A character';
        this.announcement.set(`${name} replied: ${spokenText(latest.content)}`);
      });
    });
  }
}
