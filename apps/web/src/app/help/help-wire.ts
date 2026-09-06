/**
 * The help wire contract (§B of the `p4.9i2` round) + the dispatch client that
 * speaks it.
 *
 * The thirteen `help*` request DTOs were declared lane-locally and FOLDED into
 * `core-contract.ts` at the `p4.9i2` unification (§S.3), where the name-for-name
 * wire diff against `api/types.rs` ran clean (13/13); this module re-exports
 * them and adds the response DTOs (the Brahma precedent, `brahma-wire.ts`).
 *
 * Response DTOs mirror v4's route JSON name-for-name, derived from the v4
 * handlers at the anchors cited on each type. Lane P4.9I2A's tier-2 differential
 * is the arbiter of any disagreement — neither lane patches around it.
 *
 * @module help/help-wire
 */

import { Injectable, inject } from '@angular/core';

import { CoreClient } from '../core/core-client';

import type {
  HelpDocsListRequest,
  HelpDocsChatCountRequest,
  HelpDocsSearchRequest,
  HelpDocGetRequest,
  HelpChatListRequest,
  HelpChatEligibilityRequest,
  HelpChatCreateRequest,
  HelpChatGetRequest,
  HelpChatRenameRequest,
  HelpChatUpdateContextRequest,
  HelpChatDeleteRequest,
  HelpChatMessagesRequest,
  HelpChatSendRequest,
} from '../core/core-contract';

// ============================================================================
// Request DTOs (§B) — FOLDED into `core-contract.ts` at the unification (§S.3);
// re-exported here so the help family keeps one import site.
// ============================================================================

export type {
  HelpDocsListRequest,
  HelpDocsChatCountRequest,
  HelpDocsSearchRequest,
  HelpDocGetRequest,
  HelpChatListRequest,
  HelpChatEligibilityRequest,
  HelpChatCreateRequest,
  HelpChatGetRequest,
  HelpChatRenameRequest,
  HelpChatUpdateContextRequest,
  HelpChatDeleteRequest,
  HelpChatMessagesRequest,
  HelpChatSendRequest,
} from '../core/core-contract';

/** The thirteen-verb request union (each member becomes a `CoreRequest`). */
export type HelpRequest =
  | HelpDocsListRequest
  | HelpDocsChatCountRequest
  | HelpDocsSearchRequest
  | HelpDocGetRequest
  | HelpChatListRequest
  | HelpChatEligibilityRequest
  | HelpChatCreateRequest
  | HelpChatGetRequest
  | HelpChatRenameRequest
  | HelpChatUpdateContextRequest
  | HelpChatDeleteRequest
  | HelpChatMessagesRequest
  | HelpChatSendRequest;

// ============================================================================
// Response DTOs (mirror v4's route JSON name-for-name)
// ============================================================================

/** One row of the document INDEX — carries `slug`, unlike the single doc. */
export interface HelpDocIndexRow {
  id: string;
  slug: string;
  title: string;
  path: string;
  url: string;
}

/** One fetched document. v4's single-doc projection has NO `slug`. */
export interface HelpDocument {
  id: string;
  title: string;
  path: string;
  url: string;
  content: string;
}

/** One text-search hit (`help-docs/route.ts:38-115`). */
export interface HelpDocSearchMatch {
  slug: string;
  titleHit: boolean;
  snippet: string | null;
}

/** One eligible help character (v4 `HelpChatEligibleCharacter`). */
export interface HelpEligibleCharacter {
  id: string;
  name: string;
  avatarUrl: string | null;
  defaultHelpToolsEnabled: boolean;
  connectionProfileId: string | null;
  hasToolCapableProfile: boolean;
}

/** The eligibility payload (`help-chats/route.ts:78-119`). */
export interface HelpEligibility {
  eligible: boolean;
  characters: HelpEligibleCharacter[];
  reasons: string[];
}

/** A participant as the dialog's participant maps read it (defensive: v4's
 *  `buildParticipantMaps` accepts BOTH the nested and the flattened shape). */
export interface HelpChatParticipant {
  id: string;
  characterId?: string | null;
  name?: string | null;
  avatarUrl?: string | null;
  character?: { id?: string; name?: string; avatarUrl?: string | null } | null;
}

/** One row in the "Recent Help Chats" launcher (`help-chats/route.ts:40-73`). */
export interface HelpPastChat {
  id: string;
  title: string;
  updatedAt: string;
  participants: HelpChatParticipant[];
  messageCount: number;
  helpPageUrl: string | null;
}

/** A help-chat record. `create` echoes the stored chat; `get` the projection. */
export interface HelpChatRecord {
  id: string;
  title?: string;
  chatType?: string;
  participants?: HelpChatParticipant[];
  helpPageUrl?: string | null;
  messageCount?: number;
  createdAt?: string;
  updatedAt?: string;
  [k: string]: unknown;
}

/** One persisted transcript message. */
export interface HelpChatMessage {
  id: string;
  role: string;
  content: string;
  participantId?: string | null;
  createdAt: string;
  provider?: string | null;
  modelName?: string | null;
}

/**
 * The `send` dispatch reply — `{messageId}`, the id of the LAST persisted
 * assistant message, or `null` when none was produced. The SPA reconciles by
 * reloading the transcript, so nothing load-bearing is read off this body.
 */
export interface HelpSendResult {
  messageId?: string | null;
  [k: string]: unknown;
}

// ============================================================================
// The dispatch client
// ============================================================================

/**
 * Speaks the thirteen §B verbs over the ONE transport seam ({@link CoreClient}).
 * Every method routes through {@link dispatchHelp}.
 */
@Injectable({ providedIn: 'root' })
export class HelpApi {
  private readonly core = inject(CoreClient);

  /** The thirteen verbs ARE `CoreRequest` variants since the fold (§S.3). */
  private dispatchHelp(req: HelpRequest): Promise<Record<string, unknown>> {
    return this.core.dispatchData(req);
  }

  // --- help docs -----------------------------------------------------------

  /** The document index, keyed by slug downstream. */
  async docsList(): Promise<HelpDocIndexRow[]> {
    const data = await this.dispatchHelp({ type: 'helpDocsList' });
    return (data['documents'] as HelpDocIndexRow[] | undefined) ?? [];
  }

  /**
   * How many chats the operator has. The Guide's welcome card shows while this
   * is under 3; `null` means "not known", which v4 treats as "do not show".
   */
  async docsChatCount(): Promise<number | null> {
    const data = await this.dispatchHelp({ type: 'helpDocsChatCount' });
    const count = data['count'];
    return typeof count === 'number' ? count : null;
  }

  /** Full-text search over the documents' prose (server-side; see the Guide). */
  async docsSearch(q: string): Promise<HelpDocSearchMatch[]> {
    const data = await this.dispatchHelp({ type: 'helpDocsSearch', q });
    return (data['matches'] as HelpDocSearchMatch[] | undefined) ?? [];
  }

  /** One document by DB id or slug. */
  async docGet(id: string): Promise<HelpDocument | null> {
    const data = await this.dispatchHelp({ type: 'helpDocGet', id });
    return (data['document'] as HelpDocument | undefined) ?? null;
  }

  // --- help chats ----------------------------------------------------------

  /** The operator's past help chats (server-ordered). */
  async chatList(): Promise<HelpPastChat[]> {
    const data = await this.dispatchHelp({ type: 'helpChatList' });
    return (data['chats'] as HelpPastChat[] | undefined) ?? [];
  }

  /** Which characters can hold a help chat, and why not when none can. */
  async eligibility(): Promise<HelpEligibility> {
    const data = await this.dispatchHelp({ type: 'helpChatEligibility' });
    return {
      eligible: data['eligible'] === true,
      characters: (data['characters'] as HelpEligibleCharacter[] | undefined) ?? [],
      reasons: (data['reasons'] as string[] | undefined) ?? [],
    };
  }

  /** Open a help chat with the picked characters, anchored to a page URL. */
  async chatCreate(characterIds: string[], pageUrl: string): Promise<HelpChatRecord | null> {
    const data = await this.dispatchHelp({ type: 'helpChatCreate', characterIds, pageUrl });
    return (data['chat'] as HelpChatRecord | undefined) ?? null;
  }

  /** The chat detail record (the dialog reads its participants). */
  async chatGet(chatId: string): Promise<HelpChatRecord | null> {
    const data = await this.dispatchHelp({ type: 'helpChatGet', chatId });
    return (data['chat'] as HelpChatRecord | undefined) ?? null;
  }

  /** Rename a help chat. */
  async chatRename(chatId: string, title: string): Promise<HelpChatRecord | null> {
    const data = await this.dispatchHelp({ type: 'helpChatRename', chatId, title });
    return (data['chat'] as HelpChatRecord | undefined) ?? null;
  }

  /** Re-anchor an open help chat to the page the operator just walked to. */
  async chatUpdateContext(chatId: string, pageUrl: string): Promise<void> {
    await this.dispatchHelp({ type: 'helpChatUpdateContext', chatId, pageUrl });
  }

  /** Delete a help chat. */
  async chatDelete(chatId: string): Promise<void> {
    await this.dispatchHelp({ type: 'helpChatDelete', chatId });
  }

  /** Load a help chat's persisted transcript. */
  async chatMessages(chatId: string): Promise<HelpChatMessage[]> {
    const data = await this.dispatchHelp({ type: 'helpChatMessages', chatId });
    return (data['messages'] as HelpChatMessage[] | undefined) ?? [];
  }

  /**
   * Send one message. Resolves when the run COMPLETES server-side; the stream
   * frames arrive concurrently on {@link CoreClient.events$} scope-tagged by
   * `chatId` (the consumer subscribes + folds them independently).
   */
  async chatSend(chatId: string, content: string, fileIds?: string[]): Promise<HelpSendResult> {
    const data = await this.dispatchHelp({
      type: 'helpChatSend',
      chatId,
      content,
      ...(fileIds && fileIds.length ? { fileIds } : {}),
    });
    return data as HelpSendResult;
  }
}
