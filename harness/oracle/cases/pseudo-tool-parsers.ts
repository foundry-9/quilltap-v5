/**
 * Oracle case: pseudo-tool parsers / strippers / conversions / mode resolution
 * (chat-orchestration wave 4 / W4.1b.1 — parsers half).
 *
 * Drives the REAL pure functions from v4's:
 *   lib/tools/simple-json-parser.ts, lib/tools/legacy/text-block-parser.ts,
 *   lib/tools/pseudo-tool-support.ts, and the service wrapper
 *   lib/services/chat-message/pseudo-tool.service.ts (formatSimpleJsonToolResult).
 *
 * The prompt builders (buildSimpleJsonToolInstructions / buildTextBlockInstructions
 * / buildNativeToolInstructions) are exercised by a SEPARATE oracle
 * (pseudo-tool-prompts.ts) because the simple-json builder renders from the b.3
 * tool definitions.
 *
 * Run from inside the server checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/pseudo-tool-parsers.ts \
 *     > /tmp/oracle-pseudo-tool-parsers.ndjson
 */

import {
  parseSimpleJsonCalls,
  convertSimpleJsonToToolCallRequest,
  stripSimpleJsonMarkers,
  hasSimpleJsonMarkers,
  mapSimpleJsonToolName,
  escapeXmlAttribute,
  SIMPLE_JSON_STOP_SEQUENCES,
} from '@/lib/tools/simple-json-parser';
import {
  parseTextBlockCalls,
  convertTextBlockToToolCallRequest,
  stripTextBlockMarkers,
  hasTextBlockMarkers,
  mapTextBlockToolName,
} from '@/lib/tools/legacy/text-block-parser';
import {
  resolveToolMode,
  shouldUseTextBlockTools,
  type ToolMode,
} from '@/lib/tools/pseudo-tool-support';
import { formatSimpleJsonToolResult } from '@/lib/services/chat-message/pseudo-tool.service';

const rows: unknown[] = [];
function emit(row: unknown): void {
  rows.push(row);
}

// ---- simple-json parse (+ convert) ----
function sjParse(id: string, response: string): void {
  const parsed = parseSimpleJsonCalls(response);
  emit({
    fn: 'parse_simple_json',
    id,
    response,
    out: {
      parsed: parsed.map((p) => ({
        toolName: p.toolName,
        arguments: p.arguments,
        fullMatch: p.fullMatch,
        startIndex: p.startIndex,
        endIndex: p.endIndex,
        parserTier: p.parserTier,
      })),
      converted: parsed.map(convertSimpleJsonToToolCallRequest),
    },
  });
}

sjParse('sj-clean', '<tool_call>{"name":"search","arguments":{"query":"x"}}</tool_call>');
sjParse('sj-whitespace-newlines', '<tool_call>\n{"name": "search", "arguments": {"query": "x"}}\n</tool_call>');
sjParse('sj-prose-before', 'Let me look that up.\n\n<tool_call>{"name":"search","arguments":{"query":"x"}}</tool_call>');
sjParse('sj-second-block-dropped', '<tool_call>{"name":"search","arguments":{"query":"a"}}</tool_call><tool_call>{"name":"rng","arguments":{"type":"d20"}}</tool_call>');
sjParse('sj-args-empty', '<tool_call>{"name":"request_full_context","arguments":{}}</tool_call>');
sjParse('sj-unknown-top-keys', '<tool_call>{"name":"rng","arguments":{"type":"d6"},"extra":"ignored"}</tool_call>');
// alias tags
sjParse('sj-alias-toolcall', '<toolcall>{"name":"rng","arguments":{"type":"d20"}}</toolcall>');
sjParse('sj-alias-tool', '<tool>{"name":"rng","arguments":{"type":"d20"}}</tool>');
sjParse('sj-alias-call', '<call>{"name":"rng","arguments":{"type":"d20"}}</call>');
sjParse('sj-alias-function_call', '<function_call>{"name":"rng","arguments":{"type":"d20"}}</function_call>');
sjParse('sj-alias-uppercase', '<TOOL_CALL>{"name":"rng","arguments":{"type":"d20"}}</TOOL_CALL>');
sjParse('sj-alias-open-upper-close-lower', '<TOOL_CALL>{"name":"rng","arguments":{}}</tool_call>');
sjParse('sj-alias-whitespace-in-tag', '<tool_call >{"name":"rng","arguments":{}}</tool_call >');
// repaired tier
sjParse('sj-single-quotes', "<tool_call>{'name':'search','arguments':{'query':'x'}}</tool_call>");
sjParse('sj-trailing-commas', '<tool_call>{"name":"search","arguments":{"query":"x",},}</tool_call>');
sjParse('sj-unquoted-keys', '<tool_call>{name:"search",arguments:{query:"x"}}</tool_call>');
sjParse('sj-smart-quotes', '<tool_call>{“name”:“search”,“arguments”:{“query”:“x”}}</tool_call>');
sjParse('sj-repaired-combo', "<tool_call>{name:'search', arguments:{query:'x', limit:3,},}</tool_call>");
// brace tier
sjParse('sj-no-close-well-formed-strict', '<tool_call>{"name":"search","arguments":{"query":"x"}}');
sjParse('sj-brace-prose-bled', '<tool_call>{"name":"search","arguments":{"query":"x"}} and then some trailing prose');
sjParse('sj-brace-nested', '<tool_call>{"name":"foo","arguments":{"a":{"b":{"c":1}}}}');
sjParse('sj-brace-string-literal', '<tool_call>{"name":"foo","arguments":{"msg":"} not a closer "}}</tool_call>');
// failure modes
sjParse('sj-no-markers', 'plain prose only');
sjParse('sj-empty', '');
sjParse('sj-malformed-beyond-repair', '<tool_call>this is not json at all and never will be</tool_call>');
sjParse('sj-name-missing', '<tool_call>{"arguments":{"query":"x"}}</tool_call>');
sjParse('sj-name-nonstring', '<tool_call>{"name":42,"arguments":{}}</tool_call>');
sjParse('sj-name-only', '<tool_call>{"name":"request_full_context"}</tool_call>');
sjParse('sj-args-nonobject', '<tool_call>{"name":"foo","arguments":"bar"}</tool_call>');
sjParse('sj-alias-mapping-converted', '<tool_call>{"name":"dice","arguments":{"type":"d20"}}</tool_call>');
sjParse('sj-name-whitespace-trimmed', '<tool_call>{"name":"  search  ","arguments":{}}</tool_call>');
sjParse('sj-name-empty-whitespace', '<tool_call>{"name":"   ","arguments":{}}</tool_call>');

// ---- simple-json strip ----
function sjStrip(id: string, response: string): void {
  emit({ fn: 'strip_simple_json', id, response, out: stripSimpleJsonMarkers(response) });
}
sjStrip('strip-sj-wellformed', 'before <tool_call>{"name":"search","arguments":{}}</tool_call> after');
sjStrip('strip-sj-alias-toolcall', 'a <toolcall>{}</toolcall> b');
sjStrip('strip-sj-alias-function_call', 'a <function_call>{}</function_call> b');
sjStrip('strip-sj-dangling', 'prose <tool_call>{"name":"search","arguments":{"query":"x"}} trailing');
sjStrip('strip-sj-dangling-no-tail', 'prose <tool_call>{"name":"search","arguments":{"query":"x"}}');
sjStrip('strip-sj-collapse-blanks', 'a\n\n\n\n<tool_call>{}</tool_call>\n\n\nb');
sjStrip('strip-sj-none', 'plain prose');
sjStrip('strip-sj-empty', '');
// idempotency (strip twice)
{
  const once = stripSimpleJsonMarkers('before <tool_call>{"name":"x"}</tool_call> after');
  emit({ fn: 'strip_simple_json', id: 'strip-sj-idempotent', response: once, out: stripSimpleJsonMarkers(once) });
}

// ---- simple-json has ----
function sjHas(id: string, response: string): void {
  emit({ fn: 'has_simple_json', id, response, out: hasSimpleJsonMarkers(response) });
}
sjHas('has-sj-canonical', '<tool_call>{}</tool_call>');
sjHas('has-sj-alias-space', 'prose <toolcall>{}</toolcall>');
sjHas('has-sj-tool', '<tool>{}</tool>');
sjHas('has-sj-uppercase', '<TOOL_CALL>{}</TOOL_CALL>');
sjHas('has-sj-plain', 'just some prose');
sjHas('has-sj-empty', '');
sjHas('has-sj-similar-tools', '<tools>{}</tools>');
sjHas('has-sj-mention', 'mention of tool_call without tags');
sjHas('has-sj-attr-space', '<tool_call foo>{}</tool_call>');

// ---- name mapping ----
for (const n of ['search', 'rng', 'dice', 'memory', 'navigate', 'UNKNOWN_TOOL', '  DICE  ', 'closet', 'discard']) {
  emit({ fn: 'map_simple_json_name', id: `msj-${n.trim()}`, name: n, out: mapSimpleJsonToolName(n) });
}
for (const n of ['WHISPER', 'MEMORY', 'SEARCH_MEMORIES', 'IMAGE', 'web_search', 'unknown_tool', 'Wear', 'DISCARD']) {
  emit({ fn: 'map_text_block_name', id: `mtb-${n}`, name: n, out: mapTextBlockToolName(n) });
}

// ---- escapeXmlAttribute ----
for (const [id, v] of [
  ['xml-mixed', 'a"b<c>d&e\'f'],
  ['xml-plain', 'search'],
  ['xml-amp-first', '<&>'],
  ['xml-empty', ''],
] as Array<[string, string]>) {
  emit({ fn: 'escape_xml', id, value: v, out: escapeXmlAttribute(v) });
}

// ---- formatSimpleJsonToolResult ----
for (const [id, name, content] of [
  ['fmt-plain', 'search', 'result body'],
  ['fmt-escaped-name', 'a"b<c>', 'body'],
  ['fmt-multiline', 'rng', 'line1\nline2'],
] as Array<[string, string, string]>) {
  emit({ fn: 'format_simple_json_result', id, toolName: name, content, out: formatSimpleJsonToolResult(name, content) });
}

// ---- SIMPLE_JSON_STOP_SEQUENCES ----
emit({ fn: 'stop_sequences', id: 'stop', out: SIMPLE_JSON_STOP_SEQUENCES });

// ---- text-block parse (+ convert) ----
function tbParse(id: string, response: string): void {
  const parsed = parseTextBlockCalls(response);
  emit({
    fn: 'parse_text_block',
    id,
    response,
    out: {
      parsed: parsed.map((p) => ({
        toolName: p.toolName,
        params: p.params,
        content: p.content,
        fullMatch: p.fullMatch,
        startIndex: p.startIndex,
        endIndex: p.endIndex,
      })),
      converted: parsed.map(convertTextBlockToToolCallRequest),
    },
  });
}
tbParse('tb-content-basic', 'Hello [[WHISPER to="Elena"]]secret message[[/WHISPER]] world');
tbParse('tb-self-closing', 'Roll dice: [[RNG type="d20" /]]');
tbParse('tb-multi-param', '[[RNG type="d6" count="3" /]]');
tbParse('tb-multiline', '[[GENERATE_IMAGE]]\na cozy coffee shop\non a rainy day\nwarm lighting\n[[/GENERATE_IMAGE]]');
tbParse('tb-case-insensitive', '[[whisper to="Elena"]]hello[[/whisper]]');
tbParse('tb-case-insensitive-mixed-close', '[[Whisper to="Elena"]]hello[[/WHISPER]]');
tbParse('tb-multiple-blocks', 'Let me search for that.\n[[SEARCH_MEMORIES]]favorite food[[/SEARCH_MEMORIES]]\nAnd also check the web.\n[[SEARCH_WEB]]latest recipes[[/SEARCH_WEB]]');
tbParse('tb-no-match', 'Just a normal response with no tools.');
tbParse('tb-malformed-missing-close', '[[WHISPER to="Elena"]]hello');
tbParse('tb-malformed-mismatched', '[[WHISPER to="Elena"]]hello[[/SEARCH_WEB]]');
tbParse('tb-markdown-brackets', 'Check out [this link](http://example.com) and [another one](http://test.com)');
tbParse('tb-single-bracket', '[TOOL:memory]search query[/TOOL]');
tbParse('tb-single-quoted-param', "[[WHISPER to='Elena']]hello[[/WHISPER]]");
tbParse('tb-content-with-params', '[[SEARCH_MEMORIES limit="5"]]garden plans[[/SEARCH_MEMORIES]]');
tbParse('tb-indices', 'prefix [[RNG type="d20" /]] suffix');
tbParse('tb-escaped-quotes', '[[WEAR id=\\"item-uuid\\" /]]');
tbParse('tb-escaped-interior', '[[CREATE_NOTE title="a \\"quoted\\" word"]]body[[/CREATE_NOTE]]');
tbParse('tb-content-trimmed', '[[SEARCH]]   spaced query   [[/SEARCH]]');
tbParse('tb-numeric-conversion', '[[RNG type="d6" count="3" /]]');
tbParse('tb-boolean-conversion', '[[STATE operation="set" key="flag" value="true" /]]');
tbParse('tb-wardrobe-wear-wrap', '[[WEAR mode="wear" id="item-uuid" /]]');
tbParse('tb-wardrobe-take-off-wrap', '[[TAKE_OFF mode="clear_slot" slot="top" /]]');
tbParse('tb-self-close-after-content', '[[SEARCH]]query[[/SEARCH]] then [[RNG type="d6" /]]');
tbParse('tb-astral-content', '[[GENERATE_IMAGE]]emoji 😀 scene[[/GENERATE_IMAGE]] after');
tbParse('tb-query-not-numeric', '[[SEARCH]]12345[[/SEARCH]]');
tbParse('tb-decimal-number', '[[RNG type="number" min="1.5" max="9" /]]');

// ---- text-block strip ----
function tbStrip(id: string, response: string): void {
  emit({ fn: 'strip_text_block', id, response, out: stripTextBlockMarkers(response) });
}
tbStrip('strip-tb-content', 'before [[WHISPER to="X"]]hi[[/WHISPER]] after');
tbStrip('strip-tb-self-closing', 'a [[RNG type="d20" /]] b');
tbStrip('strip-tb-mismatched-close-any', '[[WHISPER to="X"]]hi[[/ANYTHING]] tail');
tbStrip('strip-tb-collapse-spaces', 'a  [[RNG type="d6" /]]  b   c');
tbStrip('strip-tb-collapse-newlines', 'a\n\n\n\n[[RNG type="d6" /]]\n\n\nb');
tbStrip('strip-tb-none', 'plain prose no markers');
tbStrip('strip-tb-empty', '');

// ---- text-block has ----
function tbHas(id: string, response: string): void {
  emit({ fn: 'has_text_block', id, response, out: hasTextBlockMarkers(response) });
}
tbHas('has-tb-content', '[[WHISPER to="X"]]hi[[/WHISPER]]');
tbHas('has-tb-self-closing', '[[RNG type="d20" /]]');
tbHas('has-tb-plain', 'no markers here');
tbHas('has-tb-markdown', '[link](url)');
tbHas('has-tb-single-bracket', '[TOOL:x]y[/TOOL]');
tbHas('has-tb-empty', '');

// ---- mode resolution ----
const overrides: Array<ToolMode | undefined | string> = [
  undefined,
  'auto',
  'native',
  'simple-json',
  'text-block',
  'garbage-unknown',
];
for (const supports of [true, false]) {
  for (const ov of overrides) {
    const id = `mode-${supports}-${ov ?? 'undef'}`;
    emit({
      fn: 'resolve_tool_mode',
      id,
      supportsNative: supports,
      override: ov ?? null,
      out: resolveToolMode(supports, ov as ToolMode | undefined),
    });
    emit({
      fn: 'should_use_text_block',
      id,
      supportsNative: supports,
      override: ov ?? null,
      out: shouldUseTextBlockTools(supports, ov as ToolMode | undefined),
    });
  }
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
