/**
 * The tool-call fold for an agent run watched live — the twin of v4's
 * `applyAgentStreamEvent` (`components/agent-stream/parse-agent-stream.ts` at
 * `d1c06cd9d`), which the Scenario Builder's run hook uses to turn the loop's
 * `toolsDetected` / `toolResult` frames into the Host's activity list.
 *
 * Pure and immutable: a `toolsDetected` frame appends one pending entry per
 * counted call and re-bases the batch; a `toolResult` frame settles the entry
 * at `batchBase + index`. A frame carrying nothing tool-related returns the
 * SAME state object, so a caller can skip a signal write.
 *
 * ## Why a twin and not `core/chat-stream.reducer.ts`
 *
 * The Salon reducer folds the same two frames, and measured against v4's real
 * function (the recorded corpus in `__fixtures__/`) it disagrees on four axes,
 * each of which the corpus exercises:
 *
 *  1. the COUNT — v4 loops `toolsDetected` times, padding missing names with
 *     `'unknown'` and missing arguments with `{}`; the reducer maps over the
 *     names array, so `toolsDetected: 3` with one name yields one entry;
 *  2. an ABSENT index — v4 defaults it to 0; the reducer falls back to a
 *     by-NAME match, which settles the wrong entry when two calls share a name;
 *  3. the addressing — v4 settles `batchBase + index` across the WHOLE run (a
 *     negative index can reach the previous batch); the reducer touches only
 *     the most recent batch;
 *  4. the same-reference rule — the reducer always returns a new state.
 *
 * So the reducer is kept for the Salon, where it is right, and this is v4's
 * function transcribed for the one surface that shares v4's.
 *
 * v4's two SSE helpers in the same file (`parseAgentSseLine`,
 * `splitSseBuffer`) are NOT ported: v5 never reads SSE bytes — the frames reach
 * the SPA already parsed, off the one Event channel (`scenarioBuilderProgress`
 * frames scoped by a client-minted run id; see `scenario-builder-run.state.ts`).
 *
 * @module scenario-builder/agent-tool-calls
 */

/**
 * A tool call observed live on the stream — built from a `toolsDetected` frame
 * (name + arguments) and completed by the matching `toolResult` frame (v4
 * `AgentStreamToolCall`).
 */
export interface AgentStreamToolCall {
  name: string;
  arguments: Record<string, unknown>;
  /** Result payload once it arrives (often null on failure). */
  result?: unknown;
  success?: boolean;
  /** Human-readable error text on failure. */
  error?: string;
  /** True until the matching toolResult frame fills this in. */
  pending: boolean;
}

/**
 * Tool calls accumulated across a whole agent run, plus the base offset of the
 * current detection batch — a `toolResult` is indexed within its batch, so the
 * base maps it back to the right entry across several agent turns.
 */
export interface AgentToolCallState {
  toolCalls: AgentStreamToolCall[];
  batchBase: number;
}

export const EMPTY_AGENT_TOOL_CALL_STATE: AgentToolCallState = { toolCalls: [], batchBase: 0 };

/** A frame: whatever JSON object the server sent. */
export type AgentStreamEvent = Record<string, unknown>;

/**
 * Fold one frame into the tool-call state. Returns the SAME object when the
 * frame carries nothing tool-related.
 */
export function applyAgentStreamEvent(
  state: AgentToolCallState,
  event: AgentStreamEvent,
): AgentToolCallState {
  let next = state;

  const detected = event['toolsDetected'];
  if (typeof detected === 'number') {
    const names: unknown[] = Array.isArray(event['toolNames']) ? event['toolNames'] : [];
    const argsArr: unknown[] = Array.isArray(event['toolArguments']) ? event['toolArguments'] : [];
    const added: AgentStreamToolCall[] = [];
    for (let i = 0; i < detected; i++) {
      const a = argsArr[i];
      added.push({
        name: typeof names[i] === 'string' ? (names[i] as string) : 'unknown',
        arguments: a && typeof a === 'object' ? (a as Record<string, unknown>) : {},
        pending: true,
      });
    }
    next = { toolCalls: [...next.toolCalls, ...added], batchBase: next.toolCalls.length };
  }

  const toolResult = event['toolResult'];
  if (toolResult && typeof toolResult === 'object') {
    const tr = toolResult as {
      index?: unknown;
      success?: boolean;
      result?: unknown;
      error?: unknown;
    };
    const gi = next.batchBase + (typeof tr.index === 'number' ? tr.index : 0);
    const entry = next.toolCalls[gi];
    if (entry) {
      const toolCalls = [...next.toolCalls];
      toolCalls[gi] = {
        ...entry,
        result: tr.result,
        success: tr.success,
        error: typeof tr.error === 'string' ? tr.error : undefined,
        pending: false,
      };
      next = { ...next, toolCalls };
    }
  }

  return next;
}
