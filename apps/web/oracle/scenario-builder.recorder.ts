/**
 * The v4-side recorder behind the Scenario Builder's two pure twins (P4.D218):
 *
 *  - `src/app/scenario-builder/agent-tool-calls.ts` — v4's
 *    `applyAgentStreamEvent` (`components/agent-stream/parse-agent-stream.ts`
 *    at `d1c06cd9d`), the fold of `toolsDetected` / `toolResult` frames into
 *    the Host's activity list;
 *  - `src/app/scenario-builder/host-activity.ts` — v4's `describeHostActivity`
 *    (`components/scenario-builder/ScenarioBuilderDialog.tsx`), the one line
 *    per enquiry.
 *
 * Both are imported from v4's REAL modules (the dialog file imports cleanly
 * under tsx — measured, so nothing is extracted by hand). `parseAgentSseLine`
 * and `splitSseBuffer` are deliberately NOT recorded: v5 never reads SSE bytes
 * (the frames arrive already parsed on the ONE Event channel), so they have no
 * v5 counterpart to measure.
 *
 * This file lives OUTSIDE `src/` on purpose: it imports v4's `@/components/...`
 * and would not compile in the SPA's own tsconfig.
 *
 * Encoding: JSON cannot carry `undefined` or `NaN`, and both matter here (v4's
 * settle writes `error: undefined` as an OWN key; a `toolsDetected: NaN` frame
 * is a number to `typeof`). Both sides therefore use the same tagged encoding:
 * `{"$undefined":true}` and `{"$nan":true}` — the spec decodes it with
 * `decodeOracle` before replaying.
 *
 * Run it from a pinned v4 worktree (Node 24 at `~/.nvm/versions/node/v24.13.1/bin`):
 *
 * ```bash
 * PIN=/tmp/qt-v4-pin-p4d218-d1c06cd9d
 * git -C ~/source/quilltap-server worktree add --detach "$PIN" d1c06cd9d
 * ln -sfn ~/source/quilltap-server/node_modules "$PIN/node_modules"
 * cp <V5>/apps/web/oracle/scenario-builder.recorder.ts "$PIN/"
 * cd "$PIN" && npx tsx scenario-builder.recorder.ts \
 *   > <V5>/apps/web/src/app/scenario-builder/__fixtures__/scenario-builder-oracle.json
 * ```
 *
 * Expect a JSON object with `toolCallCases` (29) and `activityCases` (40); a
 * short or empty file means the recorder errored and the redirect already
 * truncated the old one (the empty-file trap) — the spec's length guards catch it.
 */

import {
  applyAgentStreamEvent,
  EMPTY_AGENT_TOOL_CALL_STATE,
  type AgentToolCallState,
} from '@/components/agent-stream/parse-agent-stream';
import { describeHostActivity } from '@/components/scenario-builder/ScenarioBuilderDialog';

const NAN = Number.NaN;

/** A named sequence of frames folded from the empty state. */
interface ToolCallCase {
  name: string;
  events: Array<Record<string, unknown>>;
}

const TOOL_CALL_CASES: ToolCallCase[] = [
  // --- v4's own test cases (parse-agent-stream.test.ts), by name -----------
  {
    name: 'returns the same state object when the event carries nothing tool-related',
    events: [{ reasoning: 'thinking…' }],
  },
  {
    name: 'adds pending entries for a toolsDetected batch',
    events: [
      {
        toolsDetected: 2,
        toolNames: ['search_web', 'curl'],
        toolArguments: [{ query: 'Gare du Nord' }, { url: 'https://example.com' }],
      },
    ],
  },
  {
    name: 'falls back to "unknown" name and {} arguments for a malformed entry',
    events: [{ toolsDetected: 1, toolNames: [42], toolArguments: ['not an object'] }],
  },
  {
    name: 'settles a pending call by index on a matching toolResult',
    events: [
      { toolsDetected: 1, toolNames: ['search'], toolArguments: [{ query: 'inn' }] },
      { toolResult: { index: 0, success: true, result: { hits: 3 } } },
    ],
  },
  {
    name: 'records a string error on a failed toolResult',
    events: [
      { toolsDetected: 1, toolNames: ['curl'], toolArguments: [{ url: 'https://example.com' }] },
      { toolResult: { index: 0, success: false, error: 'timed out' } },
    ],
  },
  {
    name: 'ignores a toolResult with no matching pending entry',
    events: [{ toolResult: { index: 5, success: true } }],
  },
  {
    name: 'batches tool-call indices across two detection batches from separate agent turns',
    events: [
      {
        toolsDetected: 2,
        toolNames: ['search', 'doc_read_file'],
        toolArguments: [{ query: 'lore' }, { path: 'Knowledge/history.md' }],
      },
      { toolResult: { index: 0, success: true } },
      { toolResult: { index: 1, success: true } },
      { toolsDetected: 1, toolNames: ['curl'], toolArguments: [{ url: 'https://example.com' }] },
      { toolResult: { index: 0, success: false, error: 'blocked' } },
    ],
  },

  // --- the edges v4's tests do not reach ------------------------------------
  // The count is `toolsDetected`, NOT the names array's length.
  {
    name: 'toolsDetected larger than toolNames pads with unknown/{}',
    events: [{ toolsDetected: 3, toolNames: ['search'], toolArguments: [{ query: 'a' }] }],
  },
  {
    name: 'toolsDetected smaller than toolNames truncates',
    events: [{ toolsDetected: 1, toolNames: ['search', 'curl'], toolArguments: [{}, {}] }],
  },
  // A zero-count batch still re-bases (a NEW object even though nothing was added).
  {
    name: 'toolsDetected 0 adds nothing but re-bases to the current length',
    events: [
      { toolsDetected: 1, toolNames: ['search'], toolArguments: [{}] },
      { toolsDetected: 0 },
      { toolResult: { index: 0, success: true } },
    ],
  },
  {
    name: 'toolsDetected as a string is ignored',
    events: [{ toolsDetected: '2', toolNames: ['a', 'b'] }],
  },
  { name: 'toolsDetected true is ignored', events: [{ toolsDetected: true, toolNames: ['a'] }] },
  {
    name: 'toolsDetected NaN is a number: re-bases, adds nothing',
    events: [{ toolsDetected: NAN, toolNames: ['a'] }],
  },
  {
    name: 'toolsDetected fractional loops to the ceiling',
    events: [{ toolsDetected: 1.5, toolNames: ['a', 'b', 'c'] }],
  },
  {
    name: 'toolsDetected negative adds nothing',
    events: [{ toolsDetected: -1, toolNames: ['a'] }],
  },
  {
    name: 'toolNames not an array reads as empty',
    events: [{ toolsDetected: 1, toolNames: 'search', toolArguments: { query: 'x' } }],
  },
  {
    name: 'toolArguments entry that is an array is accepted as an object',
    events: [{ toolsDetected: 2, toolNames: ['a', 'b'], toolArguments: [[1, 2], null] }],
  },
  // The index defaults to 0 — never a by-name match.
  {
    name: 'toolResult without an index settles batch index 0',
    events: [
      { toolsDetected: 2, toolNames: ['search', 'curl'], toolArguments: [{}, {}] },
      { toolResult: { name: 'curl', success: true } },
    ],
  },
  {
    name: 'toolResult with a string index settles batch index 0',
    events: [
      { toolsDetected: 2, toolNames: ['search', 'curl'], toolArguments: [{}, {}] },
      { toolResult: { index: '1', success: true } },
    ],
  },
  {
    name: 'toolResult with a fractional index matches nothing',
    events: [
      { toolsDetected: 2, toolNames: ['search', 'curl'], toolArguments: [{}, {}] },
      { toolResult: { index: 0.5, success: true } },
    ],
  },
  {
    name: 'toolResult with a negative index can reach the previous batch',
    events: [
      { toolsDetected: 1, toolNames: ['search'], toolArguments: [{}] },
      { toolsDetected: 1, toolNames: ['curl'], toolArguments: [{}] },
      { toolResult: { index: -1, success: false, error: 'reached back' } },
    ],
  },
  {
    // The result frame NAMES its tool (as the Salon's frames do), so a fold
    // that matched by name would settle the FIRST `search` — v4 settles index 1.
    // (Without the name the case cannot tell the two rules apart; the first
    // mutation run proved it.)
    name: 'duplicate tool names settle by index, not by name',
    events: [
      {
        toolsDetected: 2,
        toolNames: ['search', 'search'],
        toolArguments: [{ query: 'a' }, { query: 'b' }],
      },
      { toolResult: { index: 1, name: 'search', success: true, result: 'second' } },
    ],
  },
  {
    name: 'a non-string error is dropped to undefined',
    events: [
      { toolsDetected: 1, toolNames: ['curl'], toolArguments: [{}] },
      { toolResult: { index: 0, success: false, error: { message: 'nope' } } },
    ],
  },
  {
    name: 'success absent settles with success undefined',
    events: [
      { toolsDetected: 1, toolNames: ['search'], toolArguments: [{}] },
      { toolResult: { index: 0 } },
    ],
  },
  {
    name: 'a second toolResult for the same index overwrites the first',
    events: [
      { toolsDetected: 1, toolNames: ['search'], toolArguments: [{}] },
      { toolResult: { index: 0, success: false, error: 'first' } },
      { toolResult: { index: 0, success: true, result: 'second' } },
    ],
  },
  {
    name: 'toolResult as a string is ignored',
    events: [{ toolsDetected: 1, toolNames: ['a'] }, { toolResult: 'done' }],
  },
  {
    name: 'toolResult null is ignored',
    events: [{ toolsDetected: 1, toolNames: ['a'] }, { toolResult: null }],
  },
  {
    name: 'toolsDetected and toolResult in ONE frame settle against the new batch',
    events: [
      { toolsDetected: 1, toolNames: ['search'], toolArguments: [{}] },
      {
        toolsDetected: 1,
        toolNames: ['curl'],
        toolArguments: [{}],
        toolResult: { index: 0, success: true },
      },
    ],
  },
  {
    name: 'status, reasoning, done and error frames leave the state untouched',
    events: [
      { toolsDetected: 1, toolNames: ['search'], toolArguments: [{}] },
      { status: { kind: 'status', stage: 'tool_executing', message: 'x', toolName: 'search' } },
      { reasoning: 'hmm' },
      { done: true, scenario: 'A scene.' },
      { error: 'boom', errorType: 'scenario_builder_failed' },
    ],
  },
];

interface ActivityCase {
  name: string;
  arguments: Record<string, unknown> | undefined;
}

const ACTIVITY_CASES: ActivityCase[] = [
  // v4's own cases (ScenarioBuilderDialog.test.tsx).
  { name: 'search_web', arguments: { query: 'Gare du Nord' } },
  { name: 'curl', arguments: { url: 'https://example.com' } },
  { name: 'curl', arguments: {} },
  { name: 'search', arguments: { query: 'inn' } },
  { name: 'doc_read_file', arguments: { path: 'Knowledge/lore.md' } },
  { name: 'doc_read_file', arguments: {} },
  { name: 'mystery_tool', arguments: {} },
  // Every arm, with and without its argument.
  { name: 'search_web', arguments: {} },
  { name: 'search', arguments: {} },
  { name: 'doc_read_file', arguments: { uri: 'qtap://store/a.md' } },
  { name: 'doc_read_file', arguments: { path: '', uri: 'qtap://store/b.md' } },
  { name: 'doc_grep', arguments: { query: 'the siege' } },
  { name: 'doc_grep', arguments: {} },
  { name: 'doc_list_files', arguments: { folder: 'Knowledge' } },
  { name: 'doc_list_files', arguments: { folder: '' } },
  { name: 'doc_list_files', arguments: {} },
  { name: 'doc_read_frontmatter', arguments: { path: 'Scenarios/x.md' } },
  { name: 'doc_read_frontmatter', arguments: { uri: 'qtap://u' } },
  { name: 'doc_read_frontmatter', arguments: {} },
  { name: 'doc_read_heading', arguments: { path: 'Knowledge/y.md', heading: 'History' } },
  { name: 'doc_read_heading', arguments: {} },
  { name: 'submit_final_response', arguments: { response: 'ignored' } },
  // The quote strip: leading/trailing runs of straight and curly quotes, and
  // surrounding whitespace, go; inner quotes stay.
  { name: 'search_web', arguments: { query: '"Gare du Nord"' } },
  { name: 'search_web', arguments: { query: "  'Gare'  " } },
  { name: 'search_web', arguments: { query: '“‘Vey’s Crossing’”' } },
  { name: 'search', arguments: { query: 'the "Lantern" Inn' } },
  { name: 'doc_grep', arguments: { query: '""' } },
  { name: 'search', arguments: { query: '   ' } },
  // The trim runs BEFORE the strip, so an inner-space quote run survives.
  { name: 'search_web', arguments: { query: '" spaced "' } },
  // Non-string arguments read as absent.
  { name: 'search_web', arguments: { query: 42 } },
  { name: 'curl', arguments: { url: null } },
  { name: 'doc_read_file', arguments: { path: ['a'], uri: 7 } },
  { name: 'doc_list_files', arguments: { folder: true } },
  // `arguments` itself absent (the `?? {}`).
  { name: 'search_web', arguments: undefined },
  { name: 'doc_read_file', arguments: undefined },
  // The default arm carries the raw name — even an empty or odd one.
  { name: '', arguments: {} },
  { name: 'unknown', arguments: {} },
  { name: 'Search_Web', arguments: { query: 'case matters' } },
  // Non-latin text passes through the template untouched.
  { name: 'search_web', arguments: { query: 'Café de Flore ☕' } },
  { name: 'doc_read_file', arguments: { path: 'Knowledge/Ünïcode—dash.md' } },
];

/** The shared tagged encoding (see the header). */
function encode(value: unknown): unknown {
  if (value === undefined) return { $undefined: true };
  if (typeof value === 'number' && Number.isNaN(value)) return { $nan: true };
  if (Array.isArray(value)) return value.map(encode);
  if (value && typeof value === 'object') {
    const out: Record<string, unknown> = {};
    for (const key of Object.keys(value))
      out[key] = encode((value as Record<string, unknown>)[key]);
    return out;
  }
  return value;
}

const toolCallCases = TOOL_CALL_CASES.map((c) => {
  let state: AgentToolCallState = EMPTY_AGENT_TOOL_CALL_STATE;
  const steps = c.events.map((event) => {
    const next = applyAgentStreamEvent(state, event);
    const sameReference = next === state;
    state = next;
    return { sameReference, state: next };
  });
  return { name: c.name, events: c.events, steps };
});

const activityCases = ACTIVITY_CASES.map((c) => ({
  name: c.name,
  arguments: c.arguments,
  line: describeHostActivity({ name: c.name, arguments: c.arguments as Record<string, unknown> }),
}));

process.stdout.write(
  JSON.stringify(
    encode({
      source:
        'v4 d1c06cd9d — parse-agent-stream.ts applyAgentStreamEvent + ScenarioBuilderDialog.tsx describeHostActivity',
      empty: EMPTY_AGENT_TOOL_CALL_STATE,
      toolCallCases,
      activityCases,
    }),
    null,
    2,
  ) + '\n',
);
