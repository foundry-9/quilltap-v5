/**
 * Oracle case (chat-orchestration wave 1.6): the message-formatter anti-hijack
 * cleanups + provider name/prefix formatting.
 *
 * Drives the REAL pure functions from v4's lib/llm/message-formatter.ts:
 *   stripCharacterNamePrefix, truncateAtForeignSpeaker,
 *   normalizeContentBlockFormat, formatParticipantName, formatDisplayName,
 *   getProviderNameSupport, supportsNameField, formatMessagesForProvider.
 *
 * P4.D157 (v4 d4138b96b, the 4.9 dead-code sweep) DELETED
 * buildMultiCharacterContextSection; its `context` rows left this case with the
 * v5 twin. The twin's only callers were the two Host roster builders in
 * `services/host_notifications.rs`, which v4 deleted in the same commit and
 * which nothing in v5 called either (see post-office-host.ts). Do not re-add
 * the name: a named import of a deleted export makes this whole case fail to
 * LINK, emitting a ZERO-byte NDJSON.
 *
 * As of W4.7a the built-in provider MANIFESTS carry `messageFormat`, and v4's
 * `getProviderNameSupport` consults the registry before the legacy fallback. So
 * this oracle now INITIALIZES the real provider registry (the runtime way — load
 * the built plugins/dist bundles + `initializeProviderRegistry`) before driving
 * the getters, matching the Rust port's manifest-backed
 * `message_formatter::get_provider_name_support`. Provider list includes DEEPSEEK
 * / Z_AI (whose manifests advertise name-field support via the registry path,
 * where the legacy table alone would say no).
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/message-formatter.ts \
 *     > /path/to/oracle-message-formatter.ndjson
 */

import { createRequire } from 'node:module';
import { join } from 'node:path';
import {
  stripCharacterNamePrefix,
  truncateAtForeignSpeaker,
  normalizeContentBlockFormat,
  formatParticipantName,
  formatDisplayName,
  getProviderNameSupport,
  supportsNameField,
  formatMessagesForProvider,
  type MultiCharacterMessage,
} from '@/lib/llm/message-formatter';
import { initializeProviderRegistry } from '@/lib/plugins/provider-registry';
import type { Provider } from '@/lib/schemas/types';

const nodeRequire = createRequire(join(process.cwd(), 'noop.js'));
const PLUGIN_DIRS = [
  'anthropic', 'openai', 'google', 'grok', 'deepseek',
  'z-ai', 'openrouter', 'ollama', 'openai-compatible',
];

// Initialize the real provider registry (top-level await; tsx runs this as ESM)
// before any getProviderNameSupport call so the manifest messageFormat path is
// live — matching the Rust port.
await initializeProviderRegistry(
  PLUGIN_DIRS.map((d) => {
    const m = nodeRequire(join(process.cwd(), 'plugins', 'dist', `qtap-plugin-${d}`, 'index.js'));
    return m.plugin || m.default?.plugin || m.default;
  }),
);

type Row =
  | { kind: 'strip'; id: string; content: string; characterName?: string; aliases?: string[]; out: string }
  | { kind: 'truncate'; id: string; content: string; foreignNames: string[]; out: { text: string; truncatedAt: number | null } }
  | { kind: 'normalize'; id: string; content: string; out: string }
  | { kind: 'participant'; id: string; name: string; maxLength?: number; out: string }
  | { kind: 'display'; id: string; name: string; out: string }
  | { kind: 'nameSupport'; id: string; provider: string; out: { supportsNameField: boolean; supportedRoles: string[]; maxNameLength: number | null } }
  | { kind: 'supportsRole'; id: string; provider: string; role: 'user' | 'assistant'; out: boolean }
  | { kind: 'format'; id: string; provider: string; respondingCharacterName: string; messages: MultiCharacterMessage[]; out: Array<{ role: string; content: string; name: string | null; thoughtSignature: string | null }> };

const rows: Row[] = [];

// ---- stripCharacterNamePrefix --------------------------------------------
const strips: Array<[string, string, string | undefined, string[] | undefined]> = [
  ['empty', '', 'Alice', undefined],
  ['bracket-exact', '[Alice] hello there', 'Alice', undefined],
  ['colon-exact', 'Alice: hello there', 'Alice', undefined],
  ['bracket-ci', '[ALICE] shouting', 'Alice', undefined],
  ['leading-ws', '   [Alice]   hi', 'Alice', undefined],
  ['repeated', '[Alice]\n[Alice]\n[Alice]\n*content*', 'Alice', undefined],
  ['action-tag-kept', '[*sighs*] I am tired', 'Alice', undefined],
  ['status-kept', '[Whisper sent.] Done', 'Alice', undefined],
  ['no-prefix', 'Just prose here', 'Alice', undefined],
  ['bold-wrapper', '**[Alice]** hi', 'Alice', undefined],
  ['alias-bracket', '[Al] hello', 'Alice', ['Al', 'Ali']],
  ['alias-colon', 'Ali: hey', 'Alice', ['Al', 'Ali']],
  ['multiword-name', '[John Smith] hi', 'John Smith', undefined],
  ['multiword-partial', '[John] hi', 'John Smith', undefined],
  ['nonascii-name', '[José] hola', 'José', undefined],
  ['nonascii-colon', 'José: hola', 'José', undefined],
  ['regex-special-name', '[A.B*C] x', 'A.B*C', undefined],
  ['no-name-general', '[Someone] hello', undefined, undefined],
  ['no-name-action-kept', '[*grins*] hello', undefined, undefined],
  ['no-name-sentence-kept', '[This is a whole sentence here.] hi', undefined, undefined],
  ['no-name-long-over40', '[' + 'a'.repeat(41) + '] hi', undefined, undefined],
  ['no-name-apostrophe', "[O'Brien] hi", undefined, undefined],
  ['empty-name-string', '[] hi', '', undefined],
  ['tab-after-bracket', '[Alice]\thi', 'Alice', undefined],
  ['mixed-strip-then-stop', '[Alice] [Bob] hi', 'Alice', undefined],
  ['colon-then-bracket', 'Alice:\n[Alice] hi', 'Alice', undefined],
];
for (const [id, content, characterName, aliases] of strips) {
  rows.push({ kind: 'strip', id, content, characterName, aliases, out: stripCharacterNamePrefix(content, characterName, aliases) });
}

// ---- truncateAtForeignSpeaker --------------------------------------------
const truncs: Array<[string, string, string[]]> = [
  ['empty', '', ['Bob']],
  ['no-names', 'I nod.\n[Bob] Hey', []],
  ['no-match', 'I nod and smile warmly.', ['Bob']],
  ['start-bracket', '[Bob] Hey there', ['Bob']],
  ['start-colon', 'Bob: Hey there', ['Bob']],
  ['middle-bracket', 'I nod.\n[Bob] Hey there', ['Bob']],
  ['middle-colon', 'I nod.\nBob: Hey there', ['Bob']],
  ['trailing-ws-trimmed', 'I nod.  \n\n[Bob] Hey', ['Bob']],
  ['leading-space-tag', 'I speak.\n  [Bob] hi', ['Bob']],
  ['tab-before-tag', 'I speak.\n\t[Bob] hi', ['Bob']],
  ['multi-names-first-wins', 'Text.\n[Carol] a\n[Bob] b', ['Bob', 'Carol']],
  ['prose-mention-kept', 'I told Charlie about it.', ['Charlie']],
  ['own-name-not-here', 'I nod.\n[Alice] mine', ['Bob']],
  ['nonascii-name', 'Hola.\n[José] adiós', ['José']],
  ['regex-special', 'Hi.\n[A.B] x', ['A.B']],
  ['blank-names-filtered', 'Hi.\n[Bob] x', ['   ', 'Bob']],
  ['name-with-colon-no-space', 'Hi.\nBob:x', ['Bob']],
  ['ci-match', 'Hi.\n[BOB] x', ['Bob']],
  ['multiline-before', 'Line one.\nLine two.\n[Bob] hi', ['Bob']],
];
for (const [id, content, foreignNames] of truncs) {
  rows.push({ kind: 'truncate', id, content, foreignNames, out: truncateAtForeignSpeaker(content, foreignNames) });
}

// ---- normalizeContentBlockFormat -----------------------------------------
const norms: Array<[string, string]> = [
  ['empty', ''],
  ['plain', 'just some text'],
  ['python-block', `[{'type': 'text', 'text': "hi there"}]`],
  ['json-block', `[{"type": "text", "text": "json path"}]`],
  ['python-multiline', `[{'type': 'text', 'text': "line1\nline2"}]`],
  ['not-a-block', `[1, 2, 3]`],
  ['bracket-no-type', `[{"foo": "bar"}]`],
  ['type-not-text', `[{"type": "image", "text": "x"}]`],
  ['leading-ws', `   [{"type": "text", "text": "trimmed"}]   `],
  ['python-with-escapes', `[{'type': 'text', 'text': "quote \\" inside"}]`],
  ['json-non-string-text', `[{"type": "text", "text": 5}]`],
  ['empty-array', `[]`],
  ['starts-bracket-no-type-word', `[some prose]`],
];
for (const [id, content] of norms) {
  rows.push({ kind: 'normalize', id, content, out: normalizeContentBlockFormat(content) });
}

// ---- formatParticipantName -----------------------------------------------
const parts: Array<[string, string, number | undefined]> = [
  ['simple', 'Alice', undefined],
  ['spaces', 'John Smith', undefined],
  ['special-chars', 'A.B*C!@#', undefined],
  ['nonascii', 'José García', undefined],
  ['empty', '', undefined],
  ['only-special', '!@#$', undefined],
  ['leading-trailing-ws', '  Bob  ', undefined],
  ['truncate', 'ABCDEFGHIJ', 5],
  ['multi-space-run', 'a   b', undefined],
  ['unicode-only', 'こんにちは', undefined],
  ['keep-underscore-hyphen', 'a_b-c', undefined],
];
for (const [id, name, maxLength] of parts) {
  rows.push({ kind: 'participant', id, name, maxLength, out: maxLength === undefined ? formatParticipantName(name) : formatParticipantName(name, maxLength) });
}

// ---- formatDisplayName ---------------------------------------------------
const displays: Array<[string, string]> = [
  ['simple', 'Alice'],
  ['ws', '  Bob  '],
  ['empty', ''],
  ['only-ws', '   '],
  ['nonascii', 'José'],
];
for (const [id, name] of displays) {
  rows.push({ kind: 'display', id, name, out: formatDisplayName(name) });
}

// ---- getProviderNameSupport ----------------------------------------------
const providers = ['OPENAI', 'openai', 'ANTHROPIC', 'GOOGLE', 'GROK', 'OLLAMA', 'OPENROUTER', 'DEEPSEEK', 'Z_AI', 'openai-compatible', 'OPENAI_COMPATIBLE', 'unknown-provider'];
for (const p of providers) {
  const s = getProviderNameSupport(p as Provider);
  rows.push({ kind: 'nameSupport', id: p, provider: p, out: { supportsNameField: s.supportsNameField, supportedRoles: s.supportedRoles, maxNameLength: s.maxNameLength ?? null } });
}

// ---- supportsNameField ---------------------------------------------------
const roleChecks: Array<[string, string, 'user' | 'assistant']> = [
  ['openai-user', 'OPENAI', 'user'],
  ['openai-assistant', 'OPENAI', 'assistant'],
  ['anthropic-user', 'ANTHROPIC', 'user'],
  ['grok-assistant', 'GROK', 'assistant'],
  ['deepseek-user', 'DEEPSEEK', 'user'],
  ['zai-assistant', 'Z_AI', 'assistant'],
  ['unknown-user', 'nope', 'user'],
];
for (const [id, provider, role] of roleChecks) {
  rows.push({ kind: 'supportsRole', id, provider, role, out: supportsNameField(provider as Provider, role) });
}

// ---- formatMessagesForProvider -------------------------------------------
const mkMsg = (m: Partial<MultiCharacterMessage> & { role: MultiCharacterMessage['role']; content: string }): MultiCharacterMessage => m as MultiCharacterMessage;
const fmtCases: Array<[string, string, string, MultiCharacterMessage[]]> = [
  ['openai-user-named', 'OPENAI', 'Alice', [mkMsg({ role: 'user', content: 'hi', name: 'Bob' })]],
  ['openai-assistant-named', 'OPENAI', 'Alice', [mkMsg({ role: 'assistant', content: 'hi', name: 'Alice' })]],
  ['anthropic-user-prefix', 'ANTHROPIC', 'Alice', [mkMsg({ role: 'user', content: 'hi', name: 'Bob' })]],
  ['anthropic-assistant-prefix', 'ANTHROPIC', 'Alice', [mkMsg({ role: 'assistant', content: 'hi', name: 'Alice' })]],
  ['system-no-name', 'OPENAI', 'Alice', [mkMsg({ role: 'system', content: 'sys', name: 'X' })]],
  ['no-name-passthrough', 'OPENAI', 'Alice', [mkMsg({ role: 'user', content: 'anon' })]],
  ['empty-name-passthrough', 'OPENAI', 'Alice', [mkMsg({ role: 'user', content: 'anon', name: '' })]],
  ['already-prefixed', 'ANTHROPIC', 'Alice', [mkMsg({ role: 'user', content: '[Bob] existing', name: 'Bob' })]],
  ['already-prefixed-ci', 'ANTHROPIC', 'Alice', [mkMsg({ role: 'user', content: '[bob] existing', name: 'Bob' })]],
  ['thought-sig', 'OPENAI', 'Alice', [mkMsg({ role: 'assistant', content: 'hi', name: 'Alice', thoughtSignature: 'SIG' })]],
  ['thought-sig-null', 'OPENAI', 'Alice', [mkMsg({ role: 'assistant', content: 'hi', name: 'Alice', thoughtSignature: null })]],
  ['assistant-anthropic-prefix-name-with-spaces', 'ANTHROPIC', 'Alice', [mkMsg({ role: 'assistant', content: 'hi', name: 'John Smith' })]],
  ['nonascii-name-openai', 'OPENAI', 'Alice', [mkMsg({ role: 'user', content: 'hola', name: 'José' })]],
  // DEEPSEEK now supports the name field via the registry manifest (W4.7a) — the
  // legacy table has no DEEPSEEK row, so pre-W4.7a this would have used a prefix.
  ['deepseek-user-named', 'DEEPSEEK', 'Alice', [mkMsg({ role: 'user', content: 'hi', name: 'Bob' })]],
  ['deepseek-assistant-named', 'DEEPSEEK', 'Alice', [mkMsg({ role: 'assistant', content: 'hi', name: 'Alice' })]],
  ['mixed-batch', 'ANTHROPIC', 'Alice', [
    mkMsg({ role: 'system', content: 'sys' }),
    mkMsg({ role: 'user', content: 'hi', name: 'Bob' }),
    mkMsg({ role: 'assistant', content: 'yo', name: 'Alice' }),
  ]],
];
for (const [id, provider, resp, messages] of fmtCases) {
  const out = formatMessagesForProvider(messages, provider as Provider, resp).map((m) => ({
    role: m.role,
    content: m.content,
    name: m.name ?? null,
    thoughtSignature: m.thoughtSignature ?? null,
  }));
  rows.push({ kind: 'format', id, provider, respondingCharacterName: resp, messages, out });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
