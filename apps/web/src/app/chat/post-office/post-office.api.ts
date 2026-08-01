/**
 * The Post Office data layer: TanStack query keys + thin `CoreClient` dispatch
 * helpers for the in-chat announcement + mail dialogs (v4
 * `InsertAnnouncementDialog.tsx` / `ComposeMailDialog.tsx`).
 *
 * Every op reads through {@link CoreClient.dispatchData} (raw `data`, throwing a
 * `CoreDispatchError` carrying the server's own message on the error envelope) —
 * the §1 contract pins the response BODIES, not a narrowed Rust `Response` type,
 * so the response `type` string is not load-bearing here (the characters-vertical
 * precedent).
 *
 * The four verbs are P4.9E2A's; they do not exist on main while this lane runs,
 * so every call here answers a dispatch error until that lane lands. The dialogs
 * degrade the way v4's own queries do (an errored list is an empty list) rather
 * than pretending the server is there.
 */

import type { CoreClient } from '../../core/core-client';
import type {
  AnnouncerSenderWire,
  ConnectionProfileDto,
  StaffSenderWire,
} from '../../core/core-contract';

export type { AnnouncerSenderWire, StaffSenderWire };

/**
 * v4 `InsertAnnouncementDialog.tsx:21-32` — the staff roster IN v4's display
 * order (which is NOT the enum's order), with v4's exact labels down to the
 * Suparṇā diacritic.
 */
export const STAFF_OPTIONS: ReadonlyArray<{ id: StaffSenderWire; label: string }> = [
  { id: 'host', label: 'The Host' },
  { id: 'librarian', label: 'The Librarian' },
  { id: 'lantern', label: 'The Lantern' },
  { id: 'aurora', label: 'Aurora' },
  { id: 'concierge', label: 'The Concierge' },
  { id: 'prospero', label: 'Prospero' },
  { id: 'commonplaceBook', label: 'The Commonplace Book' },
  { id: 'ariel', label: 'Ariel' },
  { id: 'suparna', label: 'Suparṇā' },
  { id: 'pascal', label: 'Pascal the Croupier' },
];

/** One letter in a player-character's postbox (the §1 `ChatMailboxList` body). */
export interface MailboxLetter {
  path: string;
  from: string;
  sentAt: string;
}

/** The announcement dialog's profile projection (v4 `ProfileCard`, `:51-57`). */
export interface AnnouncementProfile {
  id: string;
  name: string;
  provider: string;
  modelName: string;
  isDefault: boolean;
}

/**
 * A current chat participant, offered as an optional whisper target (v4
 * `AudienceCandidate`, `InsertAnnouncementDialog.tsx:62-69`). `participantId`
 * is the CHAT PARTICIPANT id — what gets persisted as a whisper target, not the
 * workspace character id.
 */
export interface AudienceCandidate {
  participantId: string;
  name: string;
  controlledBy: 'llm' | 'user';
  avatarUrl?: string | null;
  status?: 'active' | 'silent' | 'absent' | 'removed';
}

/** v4 `queryKeys.mailbox.byCharacter(chatId, characterId)` (`ComposeMailDialog:120`). */
export const mailboxKeys = {
  all: ['mailbox'] as const,
  byCharacter: (chatId: string, characterId: string) =>
    ['mailbox', chatId, characterId] as const,
};

/** The announcement dialog's own profile-list key (v4 refetches once per open). */
export const announcementKeys = {
  profiles: ['post-office', 'connection-profiles'] as const,
};

/**
 * The connection profiles the in-character rewrite can run through (v4 loads
 * `/api/v1/connection-profiles` and maps five fields, `:121-132`).
 *
 * `fetchConnectionProfiles` in the characters vertical drops `isDefault`, which
 * this dialog's default-profile resolution needs — so this is its own mapping,
 * field-for-field with v4's, including v4's `String(...)`/`Boolean(...)`
 * coercions.
 */
export async function fetchAnnouncementProfiles(
  core: CoreClient,
): Promise<AnnouncementProfile[]> {
  const data = await core.dispatchData({ type: 'connectionProfileList' });
  const profiles = (data['profiles'] as ConnectionProfileDto[]) ?? [];
  return profiles.map((p) => ({
    id: String(p.id),
    name: String(p.name ?? ''),
    provider: String(p.provider ?? ''),
    modelName: String(p.modelName ?? ''),
    isDefault: Boolean(p.isDefault),
  }));
}

/**
 * §1 `ChatAnnouncementPost` — post the approved bubble (v4 `:232-239`).
 *
 * `targetParticipantIds`: the caller passes `audience.length > 0 ? audience :
 * null` (v4's own normalization, `:236`) — an empty array is never sent, only
 * `null` or a populated list.
 */
export async function postAnnouncement(
  core: CoreClient,
  args: {
    chatId: string;
    contentMarkdown: string;
    sender: AnnouncerSenderWire;
    targetParticipantIds?: string[] | null;
  },
): Promise<void> {
  await core.dispatchData({
    type: 'chatAnnouncementPost',
    chatId: args.chatId,
    contentMarkdown: args.contentMarkdown,
    sender: args.sender,
    targetParticipantIds: args.targetParticipantIds ?? null,
  });
}

/**
 * §1 `ChatAnnouncementPreview` — the in-character rewrite (v4 `:264-273`).
 * Persists nothing; the operator approves (or edits, or regenerates) before any
 * bubble is posted.
 *
 * v4 sends `systemPromptId: systemPromptId || undefined`, i.e. an absent key
 * rather than a null, so the empty-string case is dropped here too.
 * `targetParticipantIds` follows `postAnnouncement`'s empty→null normalization.
 */
export async function previewAnnouncement(
  core: CoreClient,
  args: {
    chatId: string;
    seedMarkdown: string;
    characterId: string;
    connectionProfileId: string;
    systemPromptId?: string | null;
    targetParticipantIds?: string[] | null;
  },
): Promise<string> {
  const data = await core.dispatchData({
    type: 'chatAnnouncementPreview',
    chatId: args.chatId,
    seedMarkdown: args.seedMarkdown,
    characterId: args.characterId,
    connectionProfileId: args.connectionProfileId,
    ...(args.systemPromptId ? { systemPromptId: args.systemPromptId } : {}),
    targetParticipantIds: args.targetParticipantIds ?? null,
  });
  // v4 `:284` — trims, and treats a blank rewrite as a failure at the call site.
  return String(data['proposedMarkdown'] ?? '').trim();
}

/** §1 `ChatMailboxList` — the "In reply to" options (v4 `:119-128`). */
export async function fetchMailbox(
  core: CoreClient,
  chatId: string,
  characterId: string,
): Promise<MailboxLetter[]> {
  const data = await core.dispatchData({ type: 'chatMailboxList', chatId, characterId });
  return (data['letters'] as MailboxLetter[]) ?? [];
}

/** §1 `ChatSendMail` — hand the letter to Suparṇā (v4 `:130-141`). */
export async function sendMail(
  core: CoreClient,
  args: {
    chatId: string;
    fromCharacterId: string;
    toCharacterId: string;
    bodyMarkdown: string;
    inReplyToPath: string | null;
  },
): Promise<string> {
  const data = await core.dispatchData({
    type: 'chatSendMail',
    chatId: args.chatId,
    fromCharacterId: args.fromCharacterId,
    toCharacterId: args.toCharacterId,
    bodyMarkdown: args.bodyMarkdown,
    inReplyToPath: args.inReplyToPath,
  });
  return String(data['path'] ?? '');
}
