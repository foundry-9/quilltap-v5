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

import { computed, inject, type Signal } from '@angular/core';
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
