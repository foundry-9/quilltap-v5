/**
 * Oracle case: tool-call message threading (chat-orchestration wave 4 / W4.1b.2).
 *
 * Drives the REAL pure functions from v4's
 *   lib/services/chat-message/tool-call-threading.ts:
 *     buildAssistantToolCallMessage, buildToolResultMessages
 *
 * Corpus covers: mixed callId / no-callId batches, empty prose, whitespace-only
 * prose, reasoning/thoughtSignature forwarding, an empty toolCalls list,
 * results-without-callId formatting, argument-stringify insertion order, and
 * number rendering.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/tool-call-threading.ts \
 *     > /tmp/oracle-tool-call-threading.ndjson
 */

import {
  buildAssistantToolCallMessage,
  buildToolResultMessages,
  type DetectedToolCall,
} from '@/lib/services/chat-message/tool-call-threading';
import type { ToolMessage } from '@/lib/services/chat-message/types';

type Row =
  | {
      id: string;
      kind: 'assistant';
      toolCalls: DetectedToolCall[];
      currentResponse: string;
      opts?: { reasoningContent?: string; thoughtSignature?: string };
      out: unknown;
    }
  | { id: string; kind: 'result'; toolMessages: ToolMessage[]; out: unknown };

const rows: Row[] = [];

function assistant(
  id: string,
  toolCalls: DetectedToolCall[],
  currentResponse: string,
  opts?: { reasoningContent?: string; thoughtSignature?: string },
): void {
  rows.push({
    id,
    kind: 'assistant',
    toolCalls,
    currentResponse,
    opts,
    out: buildAssistantToolCallMessage(toolCalls, currentResponse, opts),
  });
}

function result(id: string, toolMessages: ToolMessage[]): void {
  rows.push({
    id,
    kind: 'result',
    toolMessages,
    out: buildToolResultMessages(toolMessages),
  });
}

// ---- assistant turn: no call IDs → content-only ----
assistant('assistant-no-callid-plain', [{ name: 'roll', arguments: { sides: 6 } }], 'rolling the dice');
assistant('assistant-no-callid-empty-prose', [{ name: 'roll', arguments: {} }], '');
assistant('assistant-no-callid-whitespace-prose', [{ name: 'roll', arguments: {} }], '   \n\t  ');
assistant('assistant-empty-toolcalls-list', [], 'no tools here');

// ---- assistant turn: with call IDs → toolCalls attached ----
assistant('assistant-single-callid', [{ name: 'search', arguments: { q: 'wessex' }, callId: 'call_1' }], 'let me search');
assistant(
  'assistant-mixed-callid-filters-out-bare',
  [
    { name: 'a', arguments: { x: 1 }, callId: 'call_a' },
    { name: 'b', arguments: { y: 2 } },
    { name: 'c', arguments: { z: 3 }, callId: 'call_c' },
  ],
  'multi',
);
// A single bare (empty-string) callId is falsy → treated as no-callid batch.
assistant('assistant-empty-string-callid-falsy', [{ name: 'roll', arguments: { sides: 20 }, callId: '' }], 'text');

// ---- argument stringify: insertion order + shapes + numbers ----
assistant(
  'assistant-arg-insertion-order',
  [{ name: 't', arguments: { zebra: 1, apple: 2, mango: 3 }, callId: 'c' }],
  '',
);
assistant(
  'assistant-arg-nested-and-array',
  [{ name: 't', arguments: { outer: { inner: 'v' }, list: [1, 'two', true, null] }, callId: 'c' }],
  '',
);
assistant(
  'assistant-arg-fractional-number',
  [{ name: 't', arguments: { temp: 0.7, count: 3 }, callId: 'c' }],
  '',
);
assistant(
  'assistant-arg-string-with-escapes',
  [{ name: 't', arguments: { s: 'line1\nline2 "quoted" \\ backslash', unicode: 'café ☕' }, callId: 'c' }],
  '',
);
assistant('assistant-arg-empty-object', [{ name: 't', arguments: {}, callId: 'c' }], '');

// ---- reasoning / thoughtSignature forwarding ----
assistant('assistant-reasoning-forwarded', [{ name: 'x', arguments: {}, callId: 'c' }], 'prose', {
  reasoningContent: 'because reasons',
});
assistant('assistant-thoughtsig-forwarded', [{ name: 'x', arguments: {}, callId: 'c' }], 'prose', {
  thoughtSignature: 'sig-abc',
});
assistant('assistant-both-forwarded', [{ name: 'x', arguments: {}, callId: 'c' }], 'prose', {
  reasoningContent: 'r',
  thoughtSignature: 's',
});
assistant('assistant-reasoning-forwarded-no-callid', [{ name: 'x', arguments: {} }], 'prose', {
  reasoningContent: 'r',
});

// ---- tool result messages ----
result('result-empty', []);
result('result-single-callid', [{ toolName: 'search', success: true, content: 'found it', callId: 'call_1' }]);
result('result-single-no-callid', [{ toolName: 'roll', success: true, content: '6' }]);
result('result-empty-string-callid-falls-back', [
  { toolName: 'roll', success: true, content: '6', callId: '' },
]);
result('result-mixed', [
  { toolName: 'search', success: true, content: 'first', callId: 'c1' },
  { toolName: 'calc', success: false, content: 'error: bad input' },
  { toolName: 'read', success: true, content: 'body', callId: 'c2' },
]);
result('result-multiline-content-no-callid', [
  { toolName: 'grep', success: true, content: 'match 1\nmatch 2\nmatch 3' },
]);

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
