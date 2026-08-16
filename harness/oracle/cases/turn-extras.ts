/**
 * Oracle case (P4.D82): turn extras — the parts of an outgoing payload that are
 * not context (v4 `f933ba9c`, bug 70).
 *
 * Drives the REAL functions from v4's lib/services/chat-message/turn-extras.ts:
 * extractToolNames, buildToolChangeNotice, collectTurnExtras. The last one also
 * pulls v4's real buildAgentModeInstructions and its real token estimator, so
 * the reservation arithmetic is compared end to end rather than re-derived.
 *
 * Strings are compared EXACTLY: the tool-change notice is a contractual
 * UI-adjacent sentence the model reads, and the agent-mode instructions are a
 * prompt block.
 *
 * The provider only selects the estimator's chars-per-token rate; every row uses
 * OPENAI, whose rate is the default 3.5 the Rust port injects. (An empty plugin
 * registry — this case initializes none — is exactly what makes the default
 * apply on the v4 side too.)
 *
 * The unserializable-definition arm of countToolSchemaTokens is NOT covered here:
 * its input is a circular object, so no NDJSON row can carry it. See the
 * token-estimation case header.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/turn-extras.ts \
 *     > /tmp/oracle-turn-extras.ndjson
 */

import {
  collectTurnExtras,
  buildToolChangeNotice,
  extractToolNames,
} from '@/lib/services/chat-message/turn-extras';

const OPENAI_STYLE_TOOL = {
  type: 'function',
  function: {
    name: 'doc_read',
    description: 'Read a document from the vault by path.',
    parameters: {
      type: 'object',
      properties: {
        path: { type: 'string', description: 'Vault-relative path to the file.' },
      },
      required: ['path'],
    },
  },
};

const ANTHROPIC_STYLE_TOOL = {
  name: 'doc_write',
  description: 'Write a document into the vault.',
  input_schema: {
    type: 'object',
    properties: { path: { type: 'string' }, content: { type: 'string' } },
  },
};

const NAMELESS_TOOL = { nonsense: true };
/** An empty `function.name` is falsy, so v4's `||` chain falls to `name`. */
const EMPTY_NAME_FALLS_THROUGH = { type: 'function', function: { name: '' }, name: 'fallback_name' };
/** Both candidates falsy → 'unknown' → filtered out of the roster entirely. */
const BOTH_EMPTY = { function: { name: '' }, name: '' };
const NULL_NAME = { name: null };

type Row =
  | { kind: 'names'; id: string; tools: unknown[]; out: string[] }
  | { kind: 'notice'; id: string; toolNames: string[]; out: string }
  | {
      kind: 'extras';
      id: string;
      tools: unknown[];
      agentModeEnabled: boolean;
      agentModeMaxTurns: number;
      toolSettingsChanged: boolean;
      out: {
        agentModeInstructions: string | null;
        toolChangeNotice: string | null;
        toolSchemaTokens: number;
        reservedTokens: number;
      };
    };

const rows: Row[] = [];

// extractToolNames(tools)
const nameCases: Array<[string, unknown[]]> = [
  ['empty', []],
  ['both-shapes', [OPENAI_STYLE_TOOL, ANTHROPIC_STYLE_TOOL]],
  ['drops-unrecognisable', [OPENAI_STYLE_TOOL, ANTHROPIC_STYLE_TOOL, NAMELESS_TOOL]],
  ['empty-function-name-falls-through', [EMPTY_NAME_FALLS_THROUGH]],
  ['both-empty-dropped', [BOTH_EMPTY]],
  ['null-name-dropped', [NULL_NAME]],
  ['order-preserved', [ANTHROPIC_STYLE_TOOL, OPENAI_STYLE_TOOL]],
];
for (const [id, tools] of nameCases) {
  rows.push({ kind: 'names', id, tools, out: extractToolNames(tools) });
}

// buildToolChangeNotice(toolNames)
const noticeCases: Array<[string, string[]]> = [
  ['none-left', []],
  ['one', ['doc_read']],
  ['two', ['doc_read', 'doc_write']],
  ['many', ['a', 'b', 'c', 'd', 'e']],
];
for (const [id, toolNames] of noticeCases) {
  rows.push({ kind: 'notice', id, toolNames, out: buildToolChangeNotice(toolNames) });
}

// collectTurnExtras(options)
const extrasCases: Array<[string, unknown[], boolean, number, boolean]> = [
  // tools, agent mode off, no change → the schemas are the whole reservation
  ['schemas-only', [OPENAI_STYLE_TOOL], false, 25, false],
  // nothing at all → nothing reserved
  ['nothing', [], false, 25, false],
  // agent mode alone (no tools) — the instructions still cost a system message
  ['agent-mode-no-tools', [], true, 25, false],
  ['agent-mode-with-tools', [OPENAI_STYLE_TOOL], true, 25, false],
  // the maxTurns figure is interpolated into the instructions, so it moves the count
  ['agent-mode-100-turns', [OPENAI_STYLE_TOOL], true, 100, false],
  // tool-change notice alone, naming the roster
  ['tool-change-with-tools', [OPENAI_STYLE_TOOL, ANTHROPIC_STYLE_TOOL], false, 25, true],
  // the "all disabled" notice fires on an EMPTY slate
  ['tool-change-all-disabled', [], false, 25, true],
  // both injections accumulate
  ['both-injections', [OPENAI_STYLE_TOOL, ANTHROPIC_STYLE_TOOL], true, 25, true],
  // an unrecognisable entry is still counted in the schemas but never named
  ['unnamed-tool-still-measured', [NAMELESS_TOOL], false, 25, true],
];
for (const [id, tools, agentModeEnabled, agentModeMaxTurns, toolSettingsChanged] of extrasCases) {
  rows.push({
    kind: 'extras',
    id,
    tools,
    agentModeEnabled,
    agentModeMaxTurns,
    toolSettingsChanged,
    out: collectTurnExtras({
      tools,
      agentMode: { enabled: agentModeEnabled, maxTurns: agentModeMaxTurns },
      toolSettingsChanged,
      provider: 'OPENAI' as never,
    }),
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
