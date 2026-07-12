/**
 * The terminal WebSocket + REST protocol types — pinned FROM the frozen Rust
 * source this round (D5): `quilltap_host::terminal::protocol`
 * (`crates/quilltap-host/src/terminal/protocol.rs`) and the REST route surface
 * (`crates/quilltap-web/src/terminal_routes.rs`). Those Rust types were in turn
 * ported byte-verbatim from v4 `lib/schemas/terminal.types.ts`
 * (`WsClientMessageSchema` / `WsServerMessageSchema` / `PtySessionMetaSchema`),
 * so these mirror v4's union exactly. The server side is frozen — this file is
 * the SPA's single copy of the wire vocabulary.
 */

/**
 * Wire-level session metadata (Rust `PtySessionMeta` — the `terminal_sessions`
 * row minus the repository-managed `createdAt`/`updatedAt`). Nullable meta fields
 * serialize as explicit `null`.
 */
export interface PtySessionMeta {
  id: string;
  chatId: string;
  label?: string | null;
  shell: string;
  cwd: string;
  startedAt: string;
  exitedAt?: string | null;
  exitCode?: number | null;
  transcriptPath?: string | null;
}

/** Client-to-server WebSocket message (Rust `WsClientMessage`). */
export type WsClientMessage =
  | { type: 'input'; data: string }
  | { type: 'resize'; cols: number; rows: number }
  | { type: 'ping' };

/** The `chat-update` reason enum (Rust `ChatUpdateReason`; v4's single value). */
export type ChatUpdateReason = 'ariel-terminal-output';

/** Server-to-client WebSocket message (Rust `WsServerMessage`). */
export type WsServerMessage =
  | { type: 'output'; data: string }
  | { type: 'exit'; code: number | null; signal: string | null }
  | { type: 'meta'; meta: PtySessionMeta }
  | { type: 'pong' }
  | { type: 'chat-update'; chatId: string; reason?: ChatUpdateReason };

/**
 * The chat⟷embed binding: v4's content marker `<!-- terminalSessionId:UUID -->`
 * in a message body (regex byte-for-byte from v4 `MessageRow.tsx:25`). Kept as a
 * literal here so the marker-extraction spec pins it against drift.
 */
export const TERMINAL_SESSION_ID_RE = /<!--\s*terminalSessionId:([0-9a-f-]+)\s*-->/i;

/**
 * Extract the bound terminal session id from a message body, or `null` (v4
 * `extractTerminalSessionId`). Case-insensitive; matches anywhere in the content.
 */
export function extractTerminalSessionId(content: string | null | undefined): string | null {
  if (!content) return null;
  const m = content.match(TERMINAL_SESSION_ID_RE);
  return m ? m[1] : null;
}
