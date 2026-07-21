/**
 * The Brahma Console wire contract (§B of the P4.9I1 round) + the dispatch
 * client that speaks it.
 *
 * **Lane-local, folded at unify.** The eight `brahmaConsole*` verbs live in lane
 * P4.9I1A's Rust `Request` union; they do NOT exist on the v5 core until that
 * lane merges. So in-lane this file declares the request/response DTOs and the
 * client casts each request to {@link CoreRequest} at the ONE dispatch seam
 * ({@link dispatchBrahma}). The UNIFIER folds these types into `core-contract.ts`
 * and runs the name-for-name wire diff against `api/types.rs` (§W.3); the cast
 * becomes a no-op once the union carries the variants.
 *
 * Response DTOs mirror v4's route JSON name-for-name, derived from the v4
 * handlers at the anchors cited on each type (the tier-2 differential in lane A
 * is the arbiter of any disagreement — neither lane patches around it).
 *
 * @module brahma/brahma-wire
 */

import { Injectable, inject } from '@angular/core';

import { CoreClient } from '../core/core-client';
import type { CoreRequest } from '../core/core-contract';

// ============================================================================
// Request DTOs (§B — verbatim-identical in P4.9I1A and P4.9I1B)
// ============================================================================

/** `GET /api/v1/brahma-console` — list the operator's Console chats. */
export interface BrahmaConsoleListRequest {
  type: 'brahmaConsoleList';
}

/**
 * `POST /api/v1/brahma-console` — create a Console chat. The dispatch field is
 * `consoleConnectionProfileId` (the DB column); omit it to start on the user's
 * default profile (v4 `route.ts:73-85`).
 */
export interface BrahmaConsoleCreateRequest {
  type: 'brahmaConsoleCreate';
  consoleConnectionProfileId?: string;
}

/** `GET /api/v1/brahma-console/{id}` — the chat detail record. */
export interface BrahmaConsoleGetRequest {
  type: 'brahmaConsoleGet';
  chatId: string;
}

/** `PATCH /api/v1/brahma-console/{id}` — rename (sets `isManuallyRenamed`). */
export interface BrahmaConsoleRenameRequest {
  type: 'brahmaConsoleRename';
  chatId: string;
  title: string;
}

/**
 * `PATCH /api/v1/brahma-console/{id}?action=set-model` — switch the model; the
 * same chat continues. The body field v4 reads is `connectionProfileId`
 * (`[id]/route.ts:100`, `setModelSchema`).
 */
export interface BrahmaConsoleSetModelRequest {
  type: 'brahmaConsoleSetModel';
  chatId: string;
  connectionProfileId: string;
}

/** `DELETE /api/v1/brahma-console/{id}`. */
export interface BrahmaConsoleDeleteRequest {
  type: 'brahmaConsoleDelete';
  chatId: string;
}

/** `GET /api/v1/brahma-console/{id}/messages` — the persisted transcript. */
export interface BrahmaConsoleMessagesRequest {
  type: 'brahmaConsoleMessages';
  chatId: string;
}

/**
 * `POST /api/v1/brahma-console/{id}/messages` — send one message. The dispatch
 * reply is the typed result of the run; the seven stream frames ride the global
 * Event channel scope-tagged by `chatId` (v4 `BrahmaConsoleSendOptions`).
 */
export interface BrahmaConsoleSendRequest {
  type: 'brahmaConsoleSend';
  chatId: string;
  content: string;
  fileIds?: string[];
}

/** The lane-local union the UNIFIER folds into {@link CoreRequest}. */
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

  /** The ONE cast seam: a lane-local {@link BrahmaRequest} onto the transport. */
  private dispatchBrahma(req: BrahmaRequest): Promise<Record<string, unknown>> {
    return this.core.dispatchData(req as unknown as CoreRequest);
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
