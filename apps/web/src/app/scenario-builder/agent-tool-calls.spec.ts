import {
  applyAgentStreamEvent,
  EMPTY_AGENT_TOOL_CALL_STATE,
  type AgentToolCallState,
} from './agent-tool-calls';
import { SCENARIO_BUILDER_ORACLE } from './oracle-corpus.testing';

/**
 * The twin of v4's `applyAgentStreamEvent` (`parse-agent-stream.ts` at
 * `d1c06cd9d`). Two layers: v4's own `parse-agent-stream.test.ts` cases,
 * transcribed by name; then the recorded corpus from v4's REAL function,
 * replayed step by step with `toStrictEqual` (so an own `error: undefined` key
 * is distinguished from an absent one) and the same-reference rule checked on
 * every step.
 */
describe('applyAgentStreamEvent — v4 parse-agent-stream.test.ts', () => {
  it('starts from EMPTY_AGENT_TOOL_CALL_STATE with no tool calls', () => {
    expect(EMPTY_AGENT_TOOL_CALL_STATE).toEqual({ toolCalls: [], batchBase: 0 });
  });

  it('returns the same state object when the event carries nothing tool-related', () => {
    const state = EMPTY_AGENT_TOOL_CALL_STATE;
    expect(applyAgentStreamEvent(state, { reasoning: 'thinking…' })).toBe(state);
  });

  it('adds pending entries for a toolsDetected batch', () => {
    const next = applyAgentStreamEvent(EMPTY_AGENT_TOOL_CALL_STATE, {
      toolsDetected: 2,
      toolNames: ['search_web', 'curl'],
      toolArguments: [{ query: 'Gare du Nord' }, { url: 'https://example.com' }],
    });
    expect(next.toolCalls).toEqual([
      { name: 'search_web', arguments: { query: 'Gare du Nord' }, pending: true },
      { name: 'curl', arguments: { url: 'https://example.com' }, pending: true },
    ]);
    expect(next.batchBase).toBe(0);
  });

  it('falls back to "unknown" name and {} arguments for a malformed entry', () => {
    const next = applyAgentStreamEvent(EMPTY_AGENT_TOOL_CALL_STATE, {
      toolsDetected: 1,
      toolNames: [42],
      toolArguments: ['not an object'],
    });
    expect(next.toolCalls).toEqual([{ name: 'unknown', arguments: {}, pending: true }]);
  });

  it('settles a pending call by index on a matching toolResult', () => {
    let state = applyAgentStreamEvent(EMPTY_AGENT_TOOL_CALL_STATE, {
      toolsDetected: 1,
      toolNames: ['search'],
      toolArguments: [{ query: 'inn' }],
    });
    state = applyAgentStreamEvent(state, {
      toolResult: { index: 0, success: true, result: { hits: 3 } },
    });
    expect(state.toolCalls).toEqual([
      {
        name: 'search',
        arguments: { query: 'inn' },
        pending: false,
        success: true,
        result: { hits: 3 },
        error: undefined,
      },
    ]);
  });

  it('records a string error on a failed toolResult', () => {
    let state = applyAgentStreamEvent(EMPTY_AGENT_TOOL_CALL_STATE, {
      toolsDetected: 1,
      toolNames: ['curl'],
      toolArguments: [{ url: 'https://example.com' }],
    });
    state = applyAgentStreamEvent(state, {
      toolResult: { index: 0, success: false, error: 'timed out' },
    });
    expect(state.toolCalls[0]).toMatchObject({
      success: false,
      error: 'timed out',
      pending: false,
    });
  });

  it('ignores a toolResult with no matching pending entry', () => {
    const state: AgentToolCallState = { toolCalls: [], batchBase: 0 };
    expect(applyAgentStreamEvent(state, { toolResult: { index: 5, success: true } })).toBe(state);
  });

  it('batches tool-call indices across two detection batches from separate agent turns', () => {
    let state = applyAgentStreamEvent(EMPTY_AGENT_TOOL_CALL_STATE, {
      toolsDetected: 2,
      toolNames: ['search', 'doc_read_file'],
      toolArguments: [{ query: 'lore' }, { path: 'Knowledge/history.md' }],
    });
    expect(state.batchBase).toBe(0);
    state = applyAgentStreamEvent(state, { toolResult: { index: 0, success: true } });
    state = applyAgentStreamEvent(state, { toolResult: { index: 1, success: true } });
    state = applyAgentStreamEvent(state, {
      toolsDetected: 1,
      toolNames: ['curl'],
      toolArguments: [{ url: 'https://example.com' }],
    });
    expect(state.batchBase).toBe(2);
    expect(state.toolCalls).toHaveLength(3);
    state = applyAgentStreamEvent(state, {
      toolResult: { index: 0, success: false, error: 'blocked' },
    });
    expect(state.toolCalls[2]).toMatchObject({
      name: 'curl',
      success: false,
      error: 'blocked',
      pending: false,
    });
    expect(state.toolCalls[0]).toMatchObject({ success: true, pending: false });
    expect(state.toolCalls[1]).toMatchObject({ success: true, pending: false });
  });
});

describe('applyAgentStreamEvent — v4 oracle corpus (d1c06cd9d)', () => {
  const { toolCallCases, empty } = SCENARIO_BUILDER_ORACLE;

  it('carries the whole corpus (the empty-file guard)', () => {
    expect(toolCallCases).toHaveLength(29);
    expect(empty).toStrictEqual(EMPTY_AGENT_TOOL_CALL_STATE);
  });

  for (const c of toolCallCases) {
    it(c.name, () => {
      expect(c.steps).toHaveLength(c.events.length);
      let state: AgentToolCallState = EMPTY_AGENT_TOOL_CALL_STATE;
      c.events.forEach((event, i) => {
        const next = applyAgentStreamEvent(state, event);
        expect(next === state, `step ${i}: same-reference`).toBe(c.steps[i].sameReference);
        expect(next, `step ${i}: state`).toStrictEqual(c.steps[i].state);
        state = next;
      });
    });
  }
});
