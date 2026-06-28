/**
 * Oracle case #25 (Wave 5 / B16): chat-task artifact strippers.
 *
 * Drives the REAL helpers:
 *   stripToolArtifacts, extractVisibleConversation
 *     (lib/memory/cheap-llm-tasks/chat-tasks.ts)
 *   getCharacterChatPreview — exercised via the public
 *     transformCharacterChatToCardData(...).previewText (lib/chat-utils.ts)
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/chat-tasks.ts \
 *     > /tmp/oracle-chat-tasks.ndjson
 */

import {
  stripToolArtifacts,
  extractVisibleConversation,
} from '@/lib/memory/cheap-llm-tasks/chat-tasks';
import { transformCharacterChatToCardData, type CharacterChatShape } from '@/lib/chat-utils';

const rows: unknown[] = [];

// ---- stripToolArtifacts ---------------------------------------------------
const stripCases: Array<[string, string]> = [
  ['plain-long', 'This is a normal conversational message with no tools.'],
  ['plain-short', 'Hello there'],
  ['tool-call-marker', 'Sure thing![Tool call made] Here is the actual helpful response text.'],
  ['tool-result-block', '[Tool Result: lookup {"id":5}] The weather today is sunny and warm outside.'],
  [
    'leading-json-and-kv-lines',
    '{\n  "toolName": "search",\n  "query": "cats"\n}\nHere is my real reply to you now.',
  ],
  ['only-artifacts-null', '[Tool call made]\n[Tool Result: x {}]'],
  ['json-start-colon-line', 'This conversational line is plenty long here.\n: orphan colon line\n{"k": "v"}'],
  ['short-after-strip-null', 'Hi[Tool call made] there'],
  ['whitespace-lines-dropped', 'A sufficiently long conversational sentence here.\n   \n\t\n[Tool call made]'],
];
for (const [id, content] of stripCases) {
  rows.push({ kind: 'strip', id, content, out: stripToolArtifacts(content) });
}

// ---- extractVisibleConversation -------------------------------------------
type RawMsg = { type?: string; role?: string; content?: string };
const visibleCases: Array<[string, RawMsg[]]> = [
  ['basic', [
    { role: 'USER', content: 'Hi' },
    { role: 'ASSISTANT', content: 'Hello there friend, how can I help you today?' },
  ]],
  ['filters-system-and-tool', [
    { role: 'SYSTEM', content: 'system text' },
    { role: 'TOOL', content: 'tool text' },
    { role: 'USER', content: 'hey there' },
  ]],
  ['type-event-skipped', [
    { type: 'context-summary', role: 'ASSISTANT', content: 'a summary' },
    { type: 'message', role: 'USER', content: 'hi again' },
  ]],
  ['empty-content-skipped', [
    { role: 'USER', content: '' },
    { role: 'USER', content: 'real content' },
  ]],
  ['assistant-stripped-kept', [
    { role: 'ASSISTANT', content: 'Done![Tool call made] Here is the full helpful answer text now.' },
  ]],
  ['assistant-fully-stripped-dropped', [
    { role: 'ASSISTANT', content: '[Tool call made]' },
    { role: 'USER', content: 'hello' },
  ]],
  ['lowercase-roles', [
    { role: 'user', content: 'lower user' },
    { role: 'assistant', content: 'A reasonably long assistant reply without any tool stuff.' },
  ]],
  ['no-role-skipped', [{ content: 'no role here' }]],
  ['no-type-field-kept', [{ role: 'USER', content: 'no type field' }]],
  ['assistant-short-no-artifact-kept', [{ role: 'ASSISTANT', content: 'Hi' }]],
];
for (const [id, messages] of visibleCases) {
  rows.push({ kind: 'visible', id, messages, out: extractVisibleConversation(messages) });
}

// ---- getCharacterChatPreview (via transformCharacterChatToCardData) -------
const previewCases: Array<[string, string[]]> = [
  ['short', ['Short reply']],
  ['newlines-collapsed', ['Line one\nLine two\nLine three']],
  ['long-truncated', ['a'.repeat(150)]],
  ['empty-messages', []],
  ['trimmed', ['  padded text  \n ']],
  ['uses-last-message', ['first message', 'the actual last message']],
];
for (const [id, contents] of previewCases) {
  const chat = {
    id: 'chat1',
    title: null,
    updatedAt: '2024-01-01T00:00:00Z',
    messages: contents.map((content, i) => ({
      id: `m${i}`,
      role: 'ASSISTANT',
      content,
      createdAt: '2024-01-01T00:00:00Z',
    })),
  } as unknown as CharacterChatShape;
  const card = transformCharacterChatToCardData(chat);
  rows.push({ kind: 'preview', id, contents, out: card.previewText ?? null });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
