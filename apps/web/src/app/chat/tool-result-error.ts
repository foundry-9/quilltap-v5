/**
 * The human-readable failure sentence a `toolResult` frame carries — the client
 * twin of v4 `app/salon/[id]/hooks/useSSEStreaming.ts`'s
 * `resolveToolResultErrorText` (v4 `d9c98cf2`, Bug 84).
 *
 * The emitter (v4 `lib/services/chat-message/tool-execution.service.ts`; v5
 * `crate::services::tool_execution`) puts the text in `error`, a **sibling** of
 * `result`, because `result` itself is null on failure. Both clients used to
 * read `result?.error` — one level too deep — so every failure rendered the
 * caller's generic string instead of the provider's own sentence.
 *
 * Kept pure and standalone (v4 exports it from the hook for the same reason):
 * the defect gets a regression test that needs neither the SSE reader nor the
 * Salon component mounted.
 */

/** The two fields of a `toolResult` frame this resolution reads. */
export interface ToolResultErrorSource {
  result?: unknown;
  error?: string;
}

/**
 * Pull the failure sentence out of a `toolResult` frame.
 *
 * Prefers the sibling `error`; the nested `result.error` read is kept only as a
 * fallback in case a provider ever puts it there. The executor wraps the
 * sentence in its own `Error: ` prefix, which is stripped so display sites don't
 * read "Image generation failed: Error: ...". Anything empty resolves to
 * `undefined`, so the caller's generic string still fires.
 */
export function resolveToolResultErrorText(
  toolResult: ToolResultErrorSource | undefined,
): string | undefined {
  const raw =
    toolResult?.error || (toolResult?.result as { error?: string } | null | undefined)?.error;
  return raw?.replace(/^Error:\s*/, '').trim() || undefined;
}
