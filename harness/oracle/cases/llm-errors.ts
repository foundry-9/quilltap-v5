/**
 * Tier-1 oracle case (W4.7d): the LLM provider error taxonomy + normalizer +
 * formatter, plus regression rows for the already-ported predicates.
 *
 * Drives the REAL exports of v4's `lib/llm/errors.ts`:
 *   - the error-class constructors (default messages, `retryAfter`, token/content
 *     values), compared on `name` + `message`;
 *   - `isTokenLimitError` / `isContentLimitError` / `isToolUnsupportedError` /
 *     `isRecoverableRequestError` / `parseTokenLimitError` /
 *     `parseContentLimitError` (regression rows for the primary_stream port).
 *
 * P4.D157 (v4 d4138b96b, the 4.9 dead-code sweep) DELETED handleProviderError
 * and getUserFriendlyError. Their rows left this case with the v5 twins (v5's
 * only callers of either were inside `llm_errors.rs`'s own `#[cfg(test)]`
 * module): the whole `handle` section is gone, and the `construct` rows dropped
 * their `userFriendly` field — the ONE field change in this split; every other
 * field of every surviving row is byte-identical. Do not re-add the deleted
 * names: a named import of a deleted export makes this whole case fail to LINK,
 * emitting a ZERO-byte NDJSON.
 *
 * Side-effect-free; no injection. Run from the v4 checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/llm-errors.ts \
 *     > /tmp/oracle-llm-errors.ndjson
 */

import {
  APIKeyError,
  RateLimitError,
  NetworkError,
  ModelNotFoundError,
  InvalidRequestError,
  TokenLimitError,
  ContentLimitError,
  LLMProviderError,
  isTokenLimitError,
  isContentLimitError,
  isToolUnsupportedError,
  isRecoverableRequestError,
  parseTokenLimitError,
  parseContentLimitError,
  type ContentLimitType,
} from '@/lib/llm/errors';

const out: string[] = [];
const emit = (row: Record<string, unknown>) => out.push(JSON.stringify(row));

// --------------------------------------------------------------------------
// 1. Direct constructions (default messages / retryAfter / token+content values).
//    The Rust side builds from the SAME (ctor, args) and compares name / message.
// --------------------------------------------------------------------------
type Construct =
  | { id: string; ctor: 'apiKey'; provider: string }
  | { id: string; ctor: 'rateLimit'; provider: string; retryAfter?: number }
  | { id: string; ctor: 'network'; provider: string; message?: string }
  | { id: string; ctor: 'modelNotFound'; provider: string; model: string }
  | { id: string; ctor: 'invalidRequest'; provider: string; message: string }
  | {
      id: string;
      ctor: 'tokenLimit';
      provider: string;
      requestedTokens?: number;
      maxTokens?: number;
      message?: string;
    }
  | {
      id: string;
      ctor: 'contentLimit';
      provider: string;
      limitType: ContentLimitType;
      limitValue?: number;
      maxValue?: number;
      message?: string;
    }
  | { id: string; ctor: 'base'; provider: string; message: string };

const constructs: Construct[] = [
  { id: 'ctor-apikey', ctor: 'apiKey', provider: 'OPENAI' },
  { id: 'ctor-ratelimit-noretry', ctor: 'rateLimit', provider: 'ANTHROPIC' },
  { id: 'ctor-ratelimit-retry', ctor: 'rateLimit', provider: 'ANTHROPIC', retryAfter: 30 },
  { id: 'ctor-ratelimit-retry-zero', ctor: 'rateLimit', provider: 'ANTHROPIC', retryAfter: 0 },
  { id: 'ctor-network-default', ctor: 'network', provider: 'OLLAMA' },
  { id: 'ctor-network-msg', ctor: 'network', provider: 'OLLAMA', message: 'boom' },
  { id: 'ctor-model', ctor: 'modelNotFound', provider: 'OPENAI', model: 'gpt-x' },
  { id: 'ctor-invalid', ctor: 'invalidRequest', provider: 'GOOGLE', message: 'bad field' },
  { id: 'ctor-token-both', ctor: 'tokenLimit', provider: 'ANTHROPIC', requestedTokens: 210311, maxTokens: 200000 },
  { id: 'ctor-token-none', ctor: 'tokenLimit', provider: 'ANTHROPIC' },
  { id: 'ctor-token-zero', ctor: 'tokenLimit', provider: 'ANTHROPIC', requestedTokens: 0, maxTokens: 200000 },
  { id: 'ctor-content-both', ctor: 'contentLimit', provider: 'X', limitType: 'pdf_pages', limitValue: 120, maxValue: 100 },
  { id: 'ctor-content-max', ctor: 'contentLimit', provider: 'X', limitType: 'image_size', maxValue: 5242880 },
  { id: 'ctor-content-desc', ctor: 'contentLimit', provider: 'X', limitType: 'file_size' },
  { id: 'ctor-content-token', ctor: 'contentLimit', provider: 'X', limitType: 'token', limitValue: 0, maxValue: 4096 },
  { id: 'ctor-content-unknown', ctor: 'contentLimit', provider: 'X', limitType: 'unknown' },
  { id: 'ctor-base', ctor: 'base', provider: 'OLLAMA', message: 'raw failure' },
];

for (const c of constructs) {
  let err: LLMProviderError;
  switch (c.ctor) {
    case 'apiKey':
      err = new APIKeyError(c.provider);
      break;
    case 'rateLimit':
      err = new RateLimitError(c.provider, c.retryAfter);
      break;
    case 'network':
      err = c.message !== undefined ? new NetworkError(c.provider, c.message) : new NetworkError(c.provider);
      break;
    case 'modelNotFound':
      err = new ModelNotFoundError(c.provider, c.model);
      break;
    case 'invalidRequest':
      err = new InvalidRequestError(c.provider, c.message);
      break;
    case 'tokenLimit':
      err = new TokenLimitError(c.provider, c.requestedTokens, c.maxTokens, c.message);
      break;
    case 'contentLimit':
      err = new ContentLimitError(c.provider, c.limitType, c.limitValue, c.maxValue, c.message);
      break;
    case 'base':
      err = new LLMProviderError(c.provider, c.message);
      break;
  }
  emit({
    kind: 'construct',
    spec: c,
    id: c.id,
    name: err.name,
    message: err.message,
  });
}

// --------------------------------------------------------------------------
// 2. Predicate regression rows (the primary_stream port).
// --------------------------------------------------------------------------
const predicateInputs: Array<[string, string]> = [
  ['p-context-length', 'context_length_exceeded'],
  ['p-prompt-too-long', 'prompt is too long'],
  ['p-token-limit', 'this exceeds the token limit'],
  ['p-tokens-gt-max', '210311 tokens > 200000 maximum'],
  ['p-maximum-context', 'maximum context length is 128000 tokens'],
  ['p-pdf', 'maximum of 100 PDF pages allowed'],
  ['p-image-large', 'image is too large for this model'],
  ['p-file-exceeds', 'file exceeds the maximum'],
  ['p-content-too-long', 'content too long here'],
  ['p-tool-unsupported', 'tool use with function calling is unsupported'],
  ['p-func-not-supported', 'function calling is not supported'],
  ['p-tools-not-supported', 'tools are not supported by this model'],
  ['p-none', 'a totally benign message'],
];

for (const [id, input] of predicateInputs) {
  const err = new Error(input);
  const pt = parseTokenLimitError(err);
  const pc = parseContentLimitError(err);
  emit({
    kind: 'predicate',
    id,
    input,
    isToken: isTokenLimitError(err),
    isContent: isContentLimitError(err),
    isToolUnsupported: isToolUnsupportedError(err),
    isRecoverable: isRecoverableRequestError(err),
    parseTokenReq: pt.requestedTokens ?? null,
    parseTokenMax: pt.maxTokens ?? null,
    parseContentType: pc.type,
    parseContentMax: pc.maxValue ?? null,
    parseContentDesc: pc.description ?? null,
  });
}

process.stdout.write(out.join('\n') + '\n');
