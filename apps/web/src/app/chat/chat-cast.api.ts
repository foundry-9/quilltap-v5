/**
 * The chat-cast data layer: thin `CoreClient` dispatch helpers for the in-chat
 * cast dialogs (v4 `AddCharacterDialog` / `CreateNPCDialog` / `ParticipantCard`'s
 * inline controls), the per-chat avatar overrides, and the composer's RNG tool.
 *
 * Every op reads through {@link CoreClient.dispatchData} (raw `data`, throwing a
 * `CoreDispatchError` carrying the server's own message) — §1 pins the request
 * shapes, not a narrowed Rust `Response` type, so the response `type` string is
 * not load-bearing here (the Post Office / characters-vertical precedent).
 *
 * **The tri-state builders are the point of this module.** Four participant
 * fields are v4-`nullish` (absent ≠ explicit null ≠ value) and the server
 * branches on `!== undefined` (v4 `helpers.ts:159-160,180`). Rather than trust
 * every call site to remember, the update helper takes a patch whose keys are
 * OPTIONAL and spreads only the keys actually present — so "leave it alone" is
 * expressed by not passing the key at all, and `null` reaches the wire only when
 * a caller deliberately clears. `chat-cast.api.spec.ts` asserts that field by
 * field; it is this lane's stand-in for a Rust differential.
 *
 * The eight cast/avatar verbs are P4.9E1A's and the two tool verbs are P4.9E3A's;
 * none exists on `main` while this lane runs, so every call here answers a
 * dispatch error until those lanes land. Callers degrade the way v4's own
 * handlers do (an error toast, no optimistic state) rather than pretending the
 * server is there.
 */

import type { CoreClient } from '../core/core-client';
import type {
  ChatCreateOutfitSelectionInput,
  ChatUpdateParticipantRequest,
  ParticipantStatusWire,
} from '../core/core-contract';

/** The add-participant bag (v4 `AddCharacterDialog.handleAddCharacter:186-209`). */
export interface AddParticipantInput {
  characterId: string;
  connectionProfileId?: string;
  controlledBy?: 'llm' | 'user';
  hasHistoryAccess?: boolean;
  joinScenario?: string | null;
  imageProfileId?: string | null;
  displayOrder?: number;
  outfitSelection?: ChatCreateOutfitSelectionInput;
}

/**
 * §1 `ChatAddParticipant` — add a character to an existing chat.
 *
 * v4 builds its body key by key and includes `connectionProfileId` ONLY for
 * LLM control ("schema doesn't accept null", `:196-199`), `joinScenario` only
 * when non-blank (`:201-203`), and `outfitSelection` only when the selector
 * produced one (`:207-209`) — the server then defaults to `mode: 'default'`.
 * The same conditional shape is reproduced here.
 */
export async function addParticipant(
  core: CoreClient,
  chatId: string,
  input: AddParticipantInput,
): Promise<void> {
  await core.dispatchData({
    type: 'chatAddParticipant',
    chatId,
    characterId: input.characterId,
    ...(input.connectionProfileId !== undefined
      ? { connectionProfileId: input.connectionProfileId }
      : {}),
    ...(input.controlledBy !== undefined ? { controlledBy: input.controlledBy } : {}),
    ...(input.hasHistoryAccess !== undefined
      ? { hasHistoryAccess: input.hasHistoryAccess }
      : {}),
    ...(input.joinScenario !== undefined ? { joinScenario: input.joinScenario } : {}),
    ...(input.imageProfileId !== undefined ? { imageProfileId: input.imageProfileId } : {}),
    ...(input.displayOrder !== undefined ? { displayOrder: input.displayOrder } : {}),
    ...(input.outfitSelection !== undefined
      ? { outfitSelection: input.outfitSelection }
      : {}),
  });
}

/**
 * The update-participant patch. **Every key is optional and the optionality is
 * the wire contract** — a key you do not set is a key the server never sees, and
 * the four `| null` fields clear the stored value when you set them to `null`.
 */
export interface UpdateParticipantPatch {
  connectionProfileId?: string;
  imageProfileId?: string | null;
  selectedSystemPromptId?: string | null;
  displayOrder?: number;
  isActive?: boolean;
  status?: ParticipantStatusWire;
  controlledBy?: 'llm' | 'user';
  hasHistoryAccess?: boolean;
  joinScenario?: string | null;
  talkativeness?: number | null;
}

/**
 * §1 `ChatUpdateParticipant` — patch one participant (v4 `POST
 * …?action=update-participant`).
 *
 * Only the keys present on `patch` reach the wire. `Object.entries` skips
 * nothing but `undefined` values are filtered explicitly, because an
 * `{isActive: undefined}` written by a caller must behave like an absent key,
 * not like a `null`.
 */
export async function updateParticipant(
  core: CoreClient,
  chatId: string,
  participantId: string,
  patch: UpdateParticipantPatch,
): Promise<void> {
  const present: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(patch)) {
    if (value !== undefined) {
      present[key] = value;
    }
  }
  const request = {
    type: 'chatUpdateParticipant',
    chatId,
    participantId,
    ...present,
  } as ChatUpdateParticipantRequest;
  await core.dispatchData(request);
}

/** §1 `ChatRemoveParticipant` — soft-remove (v4 `useChatControls:466-471`). */
export async function removeParticipant(
  core: CoreClient,
  chatId: string,
  participantId: string,
): Promise<void> {
  await core.dispatchData({ type: 'chatRemoveParticipant', chatId, participantId });
}

/** §1 `ChatRebuildSystemPrompt` — force-recompile the identity stack. */
export async function rebuildSystemPrompt(
  core: CoreClient,
  chatId: string,
  participantId: string,
): Promise<void> {
  await core.dispatchData({ type: 'chatRebuildSystemPrompt', chatId, participantId });
}

/** One per-chat avatar override (v4 `actions/avatars.ts:18` `handleGetAvatars`). */
export interface ChatAvatarOverride {
  characterId: string;
  imageId: string;
  [key: string]: unknown;
}

/**
 * §1 `ChatGetAvatars` — the per-chat overrides.
 *
 * The response body is E1A's, so it is read structurally: v4 answers
 * `{ avatars: [...] }`, and an unexpected body degrades to an empty list rather
 * than throwing into the dialog.
 */
export async function fetchChatAvatars(
  core: CoreClient,
  chatId: string,
): Promise<ChatAvatarOverride[]> {
  const data = await core.dispatchData({ type: 'chatGetAvatars', chatId });
  const avatars = data['avatars'];
  return Array.isArray(avatars) ? (avatars as ChatAvatarOverride[]) : [];
}

/** §1 `ChatSetAvatar` — pin an image as this character's face in this chat. */
export async function setChatAvatar(
  core: CoreClient,
  chatId: string,
  characterId: string,
  imageId: string,
): Promise<void> {
  await core.dispatchData({ type: 'chatSetAvatar', chatId, characterId, imageId });
}

/** §1 `ChatRemoveAvatar` — drop the override, falling back to the default. */
export async function removeChatAvatar(
  core: CoreClient,
  chatId: string,
  characterId: string,
): Promise<void> {
  await core.dispatchData({ type: 'chatRemoveAvatar', chatId, characterId });
}

/**
 * §1 `ChatToggleAvatarGeneration` — flip the per-chat generation switch.
 * Returns the server's new value when it reports one (v4's handler echoes the
 * chat), else `null` so the caller refetches rather than guessing.
 */
export async function toggleAvatarGeneration(
  core: CoreClient,
  chatId: string,
): Promise<boolean | null> {
  const data = await core.dispatchData({ type: 'chatToggleAvatarGeneration', chatId });
  const enabled = data['autoGenerateAvatars'] ?? data['enabled'];
  return typeof enabled === 'boolean' ? enabled : null;
}

/** The die/coin/bottle a roll asks for (v4 `rngRequestSchema.type`). */
export type RngKind = number | 'flip_coin' | 'spin_the_bottle';

/**
 * The preview-mode roll v4 hands the composer as a pending chip
 * (`RngDropdown.tsx:31-40` `RngPendingResult`, filled from `data.result`).
 */
export interface RngPreviewResult {
  summary: string;
  formattedText: string;
  requestPrompt: string;
  /** v4's own `{type, rolls}` bag — passed through opaque, never re-keyed. */
  arguments: Record<string, unknown>;
}

/**
 * §1 `ChatRng` — roll from the composer gutter (v4 `RngDropdown.executeRng:93-99`).
 *
 * **`kind`, not `type`** (E3A §1). `preview: true` returns the roll without
 * writing a TOOL message; v4 then carries it in the composer until the next send
 * threads it as a `pendingToolResults` entry.
 *
 * The preview body is v4's (`actions/rng.ts:91-107`): `{success, preview: true,
 * result: {…, formattedText, summary, requestPrompt, arguments}}`. A response
 * that is not a preview (v4 returns `{message, result}` instead) yields `null`,
 * which the caller treats as "the roll already landed as a message".
 */
export async function rollRng(
  core: CoreClient,
  chatId: string,
  kind: RngKind,
  rolls: number,
  preview: boolean,
): Promise<RngPreviewResult | null> {
  const data = await core.dispatchData({ type: 'chatRng', chatId, kind, rolls, preview });
  if (!preview || data['preview'] !== true) {
    return null;
  }
  const result = (data['result'] ?? {}) as Record<string, unknown>;
  return {
    summary: String(result['summary'] ?? ''),
    formattedText: String(result['formattedText'] ?? ''),
    requestPrompt: String(result['requestPrompt'] ?? ''),
    arguments: (result['arguments'] as Record<string, unknown>) ?? {},
  };
}
