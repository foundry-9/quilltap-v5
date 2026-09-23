/**
 * The Scenario Builder oracle corpus, decoded — spec support only.
 *
 * `__fixtures__/scenario-builder-oracle.json` is written by
 * `apps/web/oracle/scenario-builder.recorder.ts` from v4's REAL
 * `applyAgentStreamEvent` and `describeHostActivity` at `d1c06cd9d`. JSON can
 * carry neither `undefined` nor `NaN`, and both matter (v4's settle writes
 * `error: undefined` as an OWN key; `toolsDetected: NaN` is a number to
 * `typeof`), so the recorder tags them `{"$undefined":true}` / `{"$nan":true}`
 * and this decodes the tags back before a spec replays anything.
 *
 * @module scenario-builder/oracle-corpus.testing
 */

import raw from './__fixtures__/scenario-builder-oracle.json';
import type { AgentStreamEvent, AgentToolCallState } from './agent-tool-calls';

export function decodeOracle(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(decodeOracle);
  if (value && typeof value === 'object') {
    const obj = value as Record<string, unknown>;
    if (obj['$undefined'] === true && Object.keys(obj).length === 1) return undefined;
    if (obj['$nan'] === true && Object.keys(obj).length === 1) return Number.NaN;
    const out: Record<string, unknown> = {};
    for (const key of Object.keys(obj)) out[key] = decodeOracle(obj[key]);
    return out;
  }
  return value;
}

export interface OracleToolCallCase {
  name: string;
  events: AgentStreamEvent[];
  steps: Array<{ sameReference: boolean; state: AgentToolCallState }>;
}

export interface OracleActivityCase {
  name: string;
  arguments: Record<string, unknown> | undefined;
  line: string;
}

export interface ScenarioBuilderOracle {
  source: string;
  empty: AgentToolCallState;
  toolCallCases: OracleToolCallCase[];
  activityCases: OracleActivityCase[];
}

export const SCENARIO_BUILDER_ORACLE = decodeOracle(raw) as ScenarioBuilderOracle;
