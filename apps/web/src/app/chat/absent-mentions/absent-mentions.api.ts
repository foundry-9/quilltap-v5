/**
 * The absent-character name chips' data layer — a port of v4
 * `hooks/useAbsentMentions.ts` (`9a88f862f`, at `236c04e35`): ONE read per
 * chat (`chatAbsentMentions`, contract C5 — P4.D347's server arm), split per
 * message on the client.
 *
 * Realtime rides the EXISTING `chats` topic (its row returns
 * `chatKeys.detail(id)`, the prefix of {@link chatKeys.absentMentions}); the
 * 60 s interval is the offline fallback only, gated by
 * {@link RealtimeService.refetchInterval}.
 *
 * @module chat/absent-mentions/absent-mentions.api
 */

import { Injectable, computed, inject, signal, type Signal } from '@angular/core';
import { injectQuery } from '@tanstack/angular-query-experimental';

import { CoreClient } from '../../core/core-client';
import type { AbsentMention, ChatAbsentMentionsResult } from '../../core/core-contract';
import { RealtimeService } from '../../core/realtime.service';
import { chatKeys } from '../chat-keys';

/** v4 `FALLBACK_POLL_MS`. */
const FALLBACK_POLL_MS = 60_000;

/** v4's shared empty answer — every message without a chip gets the same reference. */
const NONE: readonly AbsentMention[] = Object.freeze([]);

export interface AbsentMentions {
  /** Every mention the chat's read answered (message order). */
  readonly all: Signal<readonly AbsentMention[]>;
  /** One message's mentions — a memoised signal per id, {@link NONE} when it has none. */
  mentionsFor(messageId: string): Signal<readonly AbsentMention[]>;
}

/**
 * Read one chat's absent mentions. **Must be called from an injection context.**
 * A falsy chat id disables the read (v4 `enabled: !!chatId`).
 */
export function injectAbsentMentions(chatId: Signal<string | null | undefined>): AbsentMentions {
  const core = inject(CoreClient);
  const realtime = inject(RealtimeService);

  const query = injectQuery(() => {
    const id = chatId();
    return {
      queryKey: chatKeys.absentMentions(id ?? ''),
      queryFn: async (): Promise<AbsentMention[]> => {
        const data = (await core.dispatchData({
          type: 'chatAbsentMentions',
          chatId: id!,
        })) as unknown as Partial<ChatAbsentMentionsResult>;
        return data.mentions ?? [];
      },
      enabled: !!id,
      staleTime: 30_000,
      refetchInterval: realtime.refetchInterval(FALLBACK_POLL_MS),
    };
  });

  const all = computed<readonly AbsentMention[]>(() => query.data() ?? NONE);
  const byMessage = new Map<string, Signal<readonly AbsentMention[]>>();
  return {
    all,
    mentionsFor(messageId) {
      let signal = byMessage.get(messageId);
      if (!signal) {
        signal = computed(
          () => {
            const mine = all().filter((m) => m.messageId === messageId);
            return mine.length > 0 ? mine : NONE;
          },
          { equal: sameMentions },
        );
        byMessage.set(messageId, signal);
      }
      return signal;
    },
  };
}

/** Same mentions in the same order — a refetch with nothing new re-renders nothing. */
function sameMentions(a: readonly AbsentMention[], b: readonly AbsentMention[]): boolean {
  return (
    a === b ||
    (a.length === b.length && a.every((m, i) => JSON.stringify(m) === JSON.stringify(b[i])))
  );
}

/**
 * Where a Salon hands its {@link AbsentMentions} to the rows below it. The
 * Salon provides it (`providers: [AbsentMentionsSlot]`) and fills it once in
 * its constructor (`inject(AbsentMentionsSlot).set(injectAbsentMentions(…))`
 * — P4.D344's file, this lane's HANDOFF); each `qt-message-row` reads it
 * OPTIONALLY, so every surface that renders rows without a Salon (the specs,
 * the Brahma console) carries no query at all.
 */
@Injectable()
export class AbsentMentionsSlot {
  private readonly current = signal<AbsentMentions | null>(null);
  set(mentions: AbsentMentions): void {
    this.current.set(mentions);
  }
  /** One message's mentions, or null when no Salon has filled the slot. */
  mentionsFor(messageId: string): readonly AbsentMention[] | null {
    return this.current()?.mentionsFor(messageId)() ?? null;
  }
}

/**
 * The Salon's one call: read the chat's absent mentions AND hand them to the
 * rows through the {@link AbsentMentionsSlot} the Salon provides. A field
 * initializer (`private readonly absentMentions = bindAbsentMentions(this.chatId);`).
 */
export function bindAbsentMentions(chatId: Signal<string | null | undefined>): AbsentMentions {
  const mentions = injectAbsentMentions(chatId);
  inject(AbsentMentionsSlot).set(mentions);
  return mentions;
}
