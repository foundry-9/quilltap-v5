/**
 * Pure transforms turning a `ChatDetail` into a renderable Salon view model:
 * swipe-group collapsing (v4 `useChatData.fetchChat`), per-message author/avatar
 * resolution (v4 `getMessageAvatar`), and the render-item list that groups runs
 * of Staff announcements into chips (v4 `announcement-render-items`).
 */

import type { ChatDetail, MessageDto } from '../core/core-contract';
import { normalizeAvatarSrc } from '../ui/avatar-stack';
import { groupToolMessagesIntoAssistants } from './group-tool-messages';
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

/**
 * The Brahma Console's pseudo-answerer id (v4
 * `lib/services/carina/brahma-answerer.ts`, mirrored in Rust as
 * `services::carina_query::BRAHMA_CARINA_ANSWERER_ID`). It names no character
 * record, so a Carina row carrying it is resolved by hand.
 */
const BRAHMA_CARINA_ANSWERER_ID = 'b4a4c0de-0000-4000-8000-000000000001';

/**
 * The portrait and prose name behind each Staff sender (v4 `getMessageAvatar`
 * :1097-1128, transcribed sender for sender; the webp assets are v4's own,
 * copied byte-identical into `public/images/avatars/`). Carina is deliberately
 * absent — her answers wear the answerer character's portrait instead.
 *
 * Pascal's `title` is v4's: the only Staff member the table gives one.
 */
const STAFF_AVATARS: Partial<
  Record<NonNullable<MessageDto['systemSender']>, { name: string; title: string | null; avatarUrl: string }>
> = {
  lantern: { name: 'The Lantern', title: null, avatarUrl: '/images/avatars/lantern-avatar.webp' },
  aurora: { name: 'Aurora', title: null, avatarUrl: '/images/avatars/aurora-avatar.webp' },
  librarian: {
    name: 'The Librarian',
    title: null,
    avatarUrl: '/images/avatars/librarian-avatar.webp',
  },
  concierge: {
    name: 'The Concierge',
    title: null,
    avatarUrl: '/images/avatars/concierge-avatar.webp',
  },
  prospero: { name: 'Prospero', title: null, avatarUrl: '/images/avatars/prospero-avatar.webp' },
  host: { name: 'The Host', title: null, avatarUrl: '/images/avatars/host-avatar.webp' },
  commonplaceBook: {
    name: 'The Commonplace Book',
    title: null,
    avatarUrl: '/images/avatars/commonplace-book-avatar.webp',
  },
  ariel: { name: 'Ariel', title: null, avatarUrl: '/images/avatars/ariel-avatar.webp' },
  suparna: { name: 'Suparṇā', title: null, avatarUrl: '/images/avatars/suparna-avatar.webp' },
  pascal: {
    name: 'Pascal',
    title: 'the Croupier',
    avatarUrl: '/images/avatars/pascal-avatar.webp',
  },
};

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
  // An ad-hoc announcement bubble (Insert Announcement). `customAnnouncer` takes
  // precedence over `systemSender` by construction — the writer sets one or the
  // other, never both (v4 `SalonView.tsx:1064-1097`).
  //
  // The character arm matters even though the bubble carries no `participantId`:
  // an announcement can be posted AS a character who is off-scene, and without
  // it the row falls through to the role fallback below and is attributed to
  // whichever character happens to sort first — the wrong speaker entirely.
  if (message.customAnnouncer?.kind === 'character' && message.customAnnouncer.characterId) {
    const charId = message.customAnnouncer.characterId;
    const participant = chat.participants.find((cp) => cp.character?.id === charId);
    if (participant?.character) {
      return {
        name: participant.character.name,
        title: participant.character.title,
        avatarUrl: normalizeAvatarSrc(participant.character.avatarUrl),
        isUser: false,
      };
    }
    // Not in the cast — the server ships these alongside the messages precisely
    // so the bubble can name them (v4 `get.ts:452-465`).
    const offScene = chat.offSceneCharacters?.find((c) => c.id === charId);
    if (offScene) {
      return {
        name: offScene.name,
        title: offScene.title,
        avatarUrl: normalizeAvatarSrc(offScene.avatarUrl),
        isUser: false,
      };
    }
    // The character was deleted; keep the bubble legible rather than silently
    // borrowing someone else's identity (v4 :1086).
    return { name: 'Off-scene character', title: null, avatarUrl: null, isUser: false };
  }

  // Announcement with a custom label.
  if (message.customAnnouncer?.kind === 'custom') {
    return {
      name: message.customAnnouncer.displayName || 'Announcement',
      title: null,
      avatarUrl: null,
      isUser: false,
    };
  }

  // A Staff sender. Every personified writer has a name and a portrait of its
  // own (v4 `getMessageAvatar` :1097-1128) — reached in v5 by the Staff rows
  // that render as FULL rows rather than chips: a Carina reference answer, a
  // Suparṇā letter, a Pascal roll. Without this arm they fell through to the
  // role fallback below and were attributed to whichever cast member sorted
  // first — the wrong speaker entirely, the same shape as dogfood finding #31
  // (P4.26).
  if (message.systemSender === 'carina') {
    // A reference answer renders under the ANSWERER character's own name and
    // avatar — there is no dedicated Carina portrait (v4 :1130-1156).
    const answererId = message.carinaMeta?.answererId;
    // The Brahma Console pseudocharacter has no character record.
    if (answererId === BRAHMA_CARINA_ANSWERER_ID) {
      return {
        name: 'Brahma',
        title: null,
        avatarUrl: '/images/avatars/brahma-avatar.webp',
        isUser: false,
      };
    }
    if (answererId) {
      const participant = chat.participants.find((cp) => cp.character?.id === answererId);
      if (participant?.character) {
        return {
          name: participant.character.name,
          title: participant.character.title,
          avatarUrl: normalizeAvatarSrc(participant.character.avatarUrl),
          isUser: false,
        };
      }
      const offScene = chat.offSceneCharacters?.find((c) => c.id === answererId);
      if (offScene) {
        return {
          name: offScene.name,
          title: offScene.title,
          avatarUrl: normalizeAvatarSrc(offScene.avatarUrl),
          isUser: false,
        };
      }
    }
    return { name: 'Carina', title: null, avatarUrl: null, isUser: false };
  }
  if (message.systemSender) {
    const staff = STAFF_AVATARS[message.systemSender];
    if (staff) {
      return { ...staff, isUser: false };
    }
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

/**
 * One rendered item: a normal message row, a standalone tool-result card, or a
 * run of Staff chips.
 */
export type RenderItem =
  | { type: 'message'; message: MessageDto }
  | { type: 'tool'; message: MessageDto }
  | { type: 'announcement-group'; chips: AnnouncementChip[] };

/**
 * A Staff-authored announcement collapses to a chip — a port of v4
 * `announcement-render-items.ts`'s `isCollapsedAnnouncement` (`ff12f491`),
 * exemption for exemption:
 *
 *  - **Carina** reference answers are real answers, rendered as a full row with
 *    the ANSWERER character's own avatar and name.
 *  - **Suparṇā `mail-delivery`** — a letter the operator can see is one
 *    addressed to their own character (the visibility filter only shows a
 *    targeted whisper when it targets a user-controlled participant), and those
 *    "are significant enough to read in full rather than pack into a chip".
 *  - **Pascal `custom-tool-result`** — the table's binding verdict on the scene,
 *    "legible in full rather than reduced to a chip the reader has to unpack".
 *
 * Both of the latter are KIND-scoped in v4 and were sender-scoped here, so v5
 * chipped every letter and spread the roll's exemption to Pascal's other kinds
 * (P4.26). v4's expanded/collapsed term is absent: v5 keeps the chip mounted and
 * renders the body beneath it (see `AnnouncementGroup`), so a chip never leaves
 * its group.
 */
export function isAnnouncementChip(message: MessageDto): boolean {
  if (message.systemSender == null) return false;
  if (message.systemSender === 'carina') return false;
  if (message.systemSender === 'suparna' && message.systemKind === 'mail-delivery') return false;
  if (message.systemSender === 'pascal' && message.systemKind === 'custom-tool-result') return false;
  return true;
}

/**
 * Resolve the author for a standalone TOOL row by borrowing the nearest
 * preceding character-assistant's participant when the row carries none (v4
 * `VirtualizedMessageList.tsx:228-247`). Historical TOOL rows persisted before
 * character attribution was added are identifiable by position only; the walk
 * stops at a USER boundary so it never reaches back into a prior turn.
 */
function resolveToolAvatar(message: MessageDto, grouped: MessageDto[], i: number): MessageDto {
  if (message.systemSender || message.participantId) return message;
  for (let k = i - 1; k >= 0; k--) {
    const prev = grouped[k];
    if (prev.role === 'ASSISTANT' && prev.participantId) {
      return { ...message, participantId: prev.participantId };
    }
    if (prev.role === 'USER') break;
  }
  return message;
}

/**
 * Build the render-item list. First fold character-initiated TOOL rows into
 * their host assistant (v4 `groupToolMessagesIntoAssistants` — they render
 * embedded inside the character's bubble); then, over the folded flow, pack
 * consecutive announcement chips into one flex-wrapping group (v4
 * `announcement-render-items`).
 *
 * Standalone TOOL rows that survive the fold split two ways, matching v4:
 *  - a `systemSender` TOOL row (a user-initiated Prospero run) is a collapsed
 *    announcement chip like any Staff row (v4 `isCollapsedAnnouncement` catches
 *    every `systemSender`) — it expands to the tool card in `AnnouncementGroup`;
 *  - a TOOL row with no `systemSender` (an orphan character run with no host in
 *    its turn) becomes a standalone `tool` item, checked BEFORE the chip test so
 *    it is never swept into an announcement group.
 */
export function buildRenderItems(messages: MessageDto[]): RenderItem[] {
  const grouped = groupToolMessagesIntoAssistants(messages);
  const items: RenderItem[] = [];
  let run: AnnouncementChip[] = [];

  const flush = () => {
    if (run.length > 0) {
      items.push({ type: 'announcement-group', chips: run });
      run = [];
    }
  };

  for (let i = 0; i < grouped.length; i++) {
    const message = grouped[i];
    if (message.role === 'TOOL' && !message.systemSender) {
      flush();
      items.push({ type: 'tool', message: resolveToolAvatar(message, grouped, i) });
    } else if (isAnnouncementChip(message)) {
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
