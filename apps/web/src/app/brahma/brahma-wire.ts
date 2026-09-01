/**
 * The Brahma Console wire contract (§B of the P4.9I1 round) + the dispatch
 * client that speaks it.
 *
 * **Folded at the workspace-tabs remainder round's unification (§W.3).** The
 * eight `brahmaConsole*` request DTOs now live in `core-contract.ts` (diffed
 * name-for-name against `api/types.rs`) and are re-exported below; the
 * response DTOs stay here (the console reads them defensively).
 *
 * Response DTOs mirror v4's route JSON name-for-name, derived from the v4
 * handlers at the anchors cited on each type (the tier-2 differential in lane A
 * is the arbiter of any disagreement — neither lane patches around it).
 *
 * @module brahma/brahma-wire
 */

import { Injectable, inject } from '@angular/core';

import { CoreClient } from '../core/core-client';
import type {
  BrahmaConsoleListRequest,
  BrahmaConsoleCreateRequest,
  BrahmaConsoleGetRequest,
  BrahmaConsoleRenameRequest,
  BrahmaConsoleSetModelRequest,
  BrahmaConsoleDeleteRequest,
  BrahmaConsoleMessagesRequest,
  BrahmaConsoleSendRequest,
} from '../core/core-contract';

// ============================================================================
// Request DTOs (§B) — FOLDED into `core-contract.ts` at the workspace-tabs
// remainder round's unification (§W.3); re-exported here so the console's
// consumers and specs keep their import site.
// ============================================================================

export type {
  BrahmaConsoleListRequest,
  BrahmaConsoleCreateRequest,
  BrahmaConsoleGetRequest,
  BrahmaConsoleRenameRequest,
  BrahmaConsoleSetModelRequest,
  BrahmaConsoleDeleteRequest,
  BrahmaConsoleMessagesRequest,
  BrahmaConsoleSendRequest,
} from '../core/core-contract';

/** The eight-verb request union (each member is a `CoreRequest` variant). */
export type BrahmaRequest =
  | BrahmaConsoleListRequest
  | BrahmaConsoleCreateRequest
  | BrahmaConsoleGetRequest
  | BrahmaConsoleRenameRequest
  | BrahmaConsoleSetModelRequest
  | BrahmaConsoleDeleteRequest
  | BrahmaConsoleMessagesRequest
  | BrahmaConsoleSendRequest;

// ============================================================================
// Response DTOs (mirror v4's route JSON name-for-name)
// ============================================================================

/** One row in the past-chats launcher (v4 `route.ts:48-56`). */
export interface BrahmaPastChat {
  id: string;
  title: string;
  /**
   * v4 `735d9408c` added this to the route's projection, in this slot. The
   * launcher renders no date (only the message count), so nothing DISPLAYS it —
   * it is carried for wire fidelity with v4's row.
   */
  createdAt: string;
  updatedAt: string;
  lastMessageAt: string | null;
  messageCount: number;
  consoleConnectionProfileId: string | null;
}

/**
 * A Console chat record. `create`/`rename`/`set-model` echo the full stored
 * ChatMetadata (v4 `created({ chat })` / `successResponse({ chat: updated })`);
 * `get` returns the projected detail (v4 `[id]/route.ts:56-66`). The SPA reads
 * only `id` + `consoleConnectionProfileId`, so the rest stays permissive.
 */
export interface BrahmaChatRecord {
  id: string;
  title?: string;
  chatType?: string;
  consoleConnectionProfileId?: string | null;
  messageCount?: number;
  createdAt?: string;
  updatedAt?: string;
  lastMessageAt?: string | null;
  [k: string]: unknown;
}

/** One persisted transcript message (v4 `getMessages` rows). */
export interface BrahmaConsoleMessage {
  id: string;
  role: string;
  content: string;
  createdAt: string;
  provider?: string | null;
  modelName?: string | null;
  /** Reasoning ("thinking") for the turn — DISPLAY ONLY (v4 `reasoningContent`). */
  reasoningContent?: string | null;
}

/**
 * The `send` dispatch reply — the typed result of the run. Its authoritative
 * shape is lane P4.9I1A's; the SPA reconciles by reloading the transcript, so it
 * reads nothing load-bearing off this body (stays permissive in-lane).
 */
export interface BrahmaSendResult {
  messageId?: string | null;
  [k: string]: unknown;
}

// ============================================================================
// The dispatch client
// ============================================================================

/**
 * Speaks the eight §B verbs over the ONE transport seam ({@link CoreClient}).
 * Every method casts its lane-local request at {@link dispatchBrahma}; that cast
 * is the whole "inert-in-lane" surface the unifier retires.
 */
@Injectable({ providedIn: 'root' })
export class BrahmaConsoleApi {
  private readonly core = inject(CoreClient);

  /** The eight verbs are `CoreRequest` variants (folded at unification). */
  private dispatchBrahma(req: BrahmaRequest): Promise<Record<string, unknown>> {
    return this.core.dispatchData(req);
  }

  /** List the operator's Console chats (most-recent first, server-ordered). */
  async list(): Promise<BrahmaPastChat[]> {
    const data = await this.dispatchBrahma({ type: 'brahmaConsoleList' });
    return (data['chats'] as BrahmaPastChat[] | undefined) ?? [];
  }

  /** Create a Console chat; omit the profile to start on the user's default. */
  async create(consoleConnectionProfileId?: string): Promise<BrahmaChatRecord | null> {
    const data = await this.dispatchBrahma({
      type: 'brahmaConsoleCreate',
      ...(consoleConnectionProfileId ? { consoleConnectionProfileId } : {}),
    });
    return (data['chat'] as BrahmaChatRecord | undefined) ?? null;
  }

  /** The chat detail record (used to sync the active model on select). */
  async get(chatId: string): Promise<BrahmaChatRecord | null> {
    const data = await this.dispatchBrahma({ type: 'brahmaConsoleGet', chatId });
    return (data['chat'] as BrahmaChatRecord | undefined) ?? null;
  }

  /** Rename a chat. */
  async rename(chatId: string, title: string): Promise<BrahmaChatRecord | null> {
    const data = await this.dispatchBrahma({ type: 'brahmaConsoleRename', chatId, title });
    return (data['chat'] as BrahmaChatRecord | undefined) ?? null;
  }

  /** Switch the model for a chat (the same conversation continues). */
  async setModel(chatId: string, connectionProfileId: string): Promise<BrahmaChatRecord | null> {
    const data = await this.dispatchBrahma({
      type: 'brahmaConsoleSetModel',
      chatId,
      connectionProfileId,
    });
    return (data['chat'] as BrahmaChatRecord | undefined) ?? null;
  }

  /** Delete a chat. */
  async delete(chatId: string): Promise<void> {
    await this.dispatchBrahma({ type: 'brahmaConsoleDelete', chatId });
  }

  /** Load a chat's persisted transcript. */
  async messages(chatId: string): Promise<BrahmaConsoleMessage[]> {
    const data = await this.dispatchBrahma({ type: 'brahmaConsoleMessages', chatId });
    return (data['messages'] as BrahmaConsoleMessage[] | undefined) ?? [];
  }

  /**
   * Send one message. Resolves when the run COMPLETES server-side; the seven
   * stream frames arrive concurrently on {@link CoreClient.events$}
   * scope-tagged by `chatId` (the consumer subscribes+folds them independently).
   */
  async send(chatId: string, content: string, fileIds?: string[]): Promise<BrahmaSendResult> {
    const data = await this.dispatchBrahma({
      type: 'brahmaConsoleSend',
      chatId,
      content,
      ...(fileIds && fileIds.length ? { fileIds } : {}),
    });
    return data as BrahmaSendResult;
  }
}
