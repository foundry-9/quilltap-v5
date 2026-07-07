/**
 * Tier-1 oracle case (W4.7d): the LLM provider error taxonomy + normalizer +
 * formatter, plus regression rows for the already-ported predicates.
 *
 * Drives the REAL exports of v4's `lib/llm/errors.ts`:
 *   - `handleProviderError` + `getUserFriendlyError` (the ported W4.7d half);
 *   - the error-class constructors (default messages, `retryAfter`, token/content
 *     values) via `getUserFriendlyError`;
 *   - `isTokenLimitError` / `isContentLimitError` / `isToolUnsupportedError` /
 *     `isRecoverableRequestError` / `parseTokenLimitError` /
 *     `parseContentLimitError` (regression rows for the primary_stream port).
 *
 * Side-effect-free; no injection. Run from the v4 checkout:
 *   cd ~/source/quilltap-server
 *   npx tsx ~/source/quilltap-v5/harness/oracle/cases/llm-errors.ts \
 *     > /tmp/oracle-llm-errors.ndjson
 */

import {
  handleProviderError,
  getUserFriendlyError,
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
// 1. handleProviderError + getUserFriendlyError over a message corpus.
//    (One row per branch + precedence-collision rows.)
// --------------------------------------------------------------------------
const handleCases: Array<[string, string, string]> = [
  // id, provider, message
  ['apikey-unauthorized', 'OPENAI', 'Request failed: unauthorized'],
  ['apikey-invalid', 'ANTHROPIC', 'Invalid API key provided'],
  ['apikey-authentication', 'GOOGLE', 'authentication error'],
  ['apikey-401', 'OLLAMA', 'HTTP 401'],
  ['ratelimit-phrase', 'OPENAI', 'You hit the rate limit'],
  ['ratelimit-toomany', 'DEEPSEEK', 'too many requests, slow down'],
  ['ratelimit-429', 'GROK', 'server said 429'],
  ['network-network', 'OPENAI', 'a network problem'],
  ['network-econnrefused', 'OLLAMA', 'connect ECONNREFUSED 127.0.0.1:11434'],
  ['network-timeout', 'ANTHROPIC', 'Request timeout after 600s'],
  ['network-enotfound', 'Z_AI', 'getaddrinfo ENOTFOUND api.z.ai'],
  ['model-notfound', 'OPENAI', 'The model gpt-9 was not found'],
  ['model-doesnotexist', 'OPENROUTER', 'model does not exist'],
  ['token-prompt-too-long', 'ANTHROPIC', 'prompt is too long: 210311 tokens > 200000 maximum'],
  ['token-context-length', 'OPENAI', 'context_length_exceeded for this request'],
  ['token-maximum-context', 'OPENAI', 'This maximum context length is 128000 tokens for the model'],
  ['invalid-word', 'GOOGLE', 'invalid argument supplied'],
  ['invalid-400', 'OLLAMA', 'server returned 400'],
  ['generic', 'OLLAMA', 'some weird failure with no keywords'],
  // Precedence collisions (match ORDER is precedence-bearing):
  ['prec-401-and-429', 'OPENAI', 'HTTP 401 and also 429 too many requests'],
  ['prec-429-and-invalid', 'OPENAI', '429 invalid request body'],
  ['prec-network-and-invalid', 'OPENAI', 'network timeout on an invalid 400 route'],
  ['prec-authentication-and-network', 'OPENAI', 'authentication failed due to network'],
  // A bare "> maximum" that does NOT match a token-limit PATTERN → generic.
  ['token-bare-gt-maximum', 'ANTHROPIC', '210311 tokens > 200000 maximum'],
];

for (const [id, provider, message] of handleCases) {
  const err = handleProviderError(provider, new Error(message)) as LLMProviderError;
  const tokenErr = err instanceof TokenLimitError ? err : null;
  emit({
    kind: 'handle',
    id,
    provider,
    input: message,
    name: err.name,
    message: err.message,
    requestedTokens: tokenErr?.requestedTokens ?? null,
    maxTokens: tokenErr?.maxTokens ?? null,
    userFriendly: getUserFriendlyError(err),
  });
}

// --------------------------------------------------------------------------
// 2. Direct constructions (default messages / retryAfter / token+content values)
//    → getUserFriendlyError. The Rust side builds from the SAME (ctor, args) and
//    compares name / message / userFriendly.
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
    userFriendly: getUserFriendlyError(err),
  });
}

// --------------------------------------------------------------------------
// 3. Predicate regression rows (the primary_stream port).
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
