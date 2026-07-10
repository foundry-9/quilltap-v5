/**
 * Pure transforms turning a `ChatDetail` into a renderable Salon view model:
 * swipe-group collapsing (v4 `useChatData.fetchChat`), per-message author/avatar
 * resolution (v4 `getMessageAvatar`), and the render-item list that groups runs
 * of Staff announcements into chips (v4 `announcement-render-items`).
 */

import type { ChatDetail, MessageDto } from '../core/core-contract';
import { normalizeAvatarSrc } from '../ui/avatar-stack';
import {
  getAnnouncementImportance,
  getSystemKindDisplayLabel,
  getSystemSenderDisplayName,
  type AnnouncementImportance,
} from './system-message-labels';

/** Per-swipe-group state (v4 `SwipeState`), keyed by `swipeGroupId`. */
export interface SwipeState {
  current: number;
  total: number;
  messages: MessageDto[];
}

export interface SplitResult {
  messages: MessageDto[];
  swipeStates: Record<string, SwipeState>;
}

/**
 * Collapse swipe groups to their default (highest-`swipeIndex`) variant and sort
 * the visible flow by `createdAt` — a verbatim port of v4 `fetchChat`.
 */
export function splitSwipeGroups(all: MessageDto[]): SplitResult {
  const allMessages = all.filter((m) => m.role !== 'SYSTEM');

  const swipeGroups: Record<string, MessageDto[]> = {};
  const displayMessages: MessageDto[] = [];

  for (const m of allMessages) {
    if (m.swipeGroupId) {
      (swipeGroups[m.swipeGroupId] ??= []).push(m);
    } else {
      displayMessages.push(m);
    }
  }

  const swipeStates: Record<string, SwipeState> = {};
  for (const [groupId, groupMessages] of Object.entries(swipeGroups)) {
    const sorted = [...groupMessages].sort((a, b) => (a.swipeIndex || 0) - (b.swipeIndex || 0));
    const latestIndex = sorted.length - 1;
    displayMessages.push(sorted[latestIndex]);
    swipeStates[groupId] = { current: latestIndex, total: sorted.length, messages: sorted };
  }

  displayMessages.sort((a, b) => new Date(a.createdAt).getTime() - new Date(b.createdAt).getTime());

  return { messages: displayMessages, swipeStates };
}

/** The resolved author display for a message row. */
export interface MessageAuthor {
  name: string;
  title: string | null;
  avatarUrl: string | null;
  isUser: boolean;
}

/**
 * Resolve a message's author name / avatar (v4 `getMessageAvatar`, the branches
 * the read-only Salon needs: participant lookup + role fallback). Staff and
 * off-scene resolution beyond this is handled by the announcement chips, which
 * key off the sender label rather than an avatar.
 */
export function resolveMessageAuthor(message: MessageDto, chat: ChatDetail): MessageAuthor {
  // Announcement with a custom label.
  if (message.customAnnouncer?.kind === 'custom') {
    return {
      name: message.customAnnouncer.displayName || 'Announcement',
      title: null,
      avatarUrl: null,
      isUser: false,
    };
  }

  // A named participant (character).
  if (message.participantId) {
    const p = chat.participants.find((cp) => cp.id === message.participantId);
    if (p?.type === 'CHARACTER' && p.character) {
      return {
        name: p.character.name,
        title: p.character.title,
        avatarUrl: normalizeAvatarSrc(p.character.avatarUrl),
        isUser: p.controlledBy === 'user',
      };
    }
  }

  // Role fallback.
  if (message.role === 'USER') {
    const userChar = chat.participants.find(
      (cp) => cp.type === 'CHARACTER' && cp.controlledBy === 'user' && cp.character,
    );
    if (userChar?.character) {
      return {
        name: userChar.character.name,
        title: userChar.character.title,
        avatarUrl: normalizeAvatarSrc(userChar.character.avatarUrl),
        isUser: true,
      };
    }
    return {
      name: chat.user.name || 'User',
      title: null,
      avatarUrl: normalizeAvatarSrc(chat.user.image),
      isUser: true,
    };
  }

  const firstChar = chat.participants.find((cp) => cp.type === 'CHARACTER' && cp.character);
  return {
    name: firstChar?.character?.name ?? 'Assistant',
    title: firstChar?.character?.title ?? null,
    avatarUrl: normalizeAvatarSrc(firstChar?.character?.avatarUrl),
    isUser: false,
  };
}

/** A collapsed Staff announcement chip. */
export interface AnnouncementChip {
  id: string;
  sender: string;
  kind: string;
  importance: AnnouncementImportance;
  createdAt: string;
  message: MessageDto;
}

/** One rendered item: a normal message row, or a run of Staff chips. */
export type RenderItem =
  | { type: 'message'; message: MessageDto }
  | { type: 'announcement-group'; chips: AnnouncementChip[] };

/**
 * A Staff-authored announcement collapses to a chip — every `systemSender`
 * EXCEPT Carina, whose reference answers render as their own full row (v4).
 */
export function isAnnouncementChip(message: MessageDto): boolean {
  return message.systemSender != null && message.systemSender !== 'carina';
}

/**
 * Build the render-item list, packing consecutive announcement chips into one
 * flex-wrapping group (v4 `announcement-render-items`).
 */
export function buildRenderItems(messages: MessageDto[]): RenderItem[] {
  const items: RenderItem[] = [];
  let run: AnnouncementChip[] = [];

  const flush = () => {
    if (run.length > 0) {
      items.push({ type: 'announcement-group', chips: run });
      run = [];
    }
  };

  for (const message of messages) {
    if (isAnnouncementChip(message)) {
      run.push({
        id: message.id,
        sender: getSystemSenderDisplayName(message.systemSender),
        kind: getSystemKindDisplayLabel(message),
        importance: getAnnouncementImportance(message),
        createdAt: message.createdAt,
        message,
      });
    } else {
      flush();
      items.push({ type: 'message', message });
    }
  }
  flush();

  return items;
}
