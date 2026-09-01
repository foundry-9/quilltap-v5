/**
 * P4.D135 — the pure fallback engine (v4 `65f5021c8`, `lib/llm/fallback/`).
 *
 * Drives v4's REAL modules:
 *   classifyFallbackTrigger / buildFallbackChain / recordAttempt /
 *   summarizeFallbackAttempts   (lib/llm/fallback/engine.ts)
 *   pickTierCandidate / tierMatches                (lib/llm/fallback/tier-picker.ts)
 *
 * Tier-1 exact. The corpus is v4's own two test files' shapes (568 lines
 * between them) plus the arms they do not reach — every trigger class, both
 * halves of the vision filter, the ranking's three keys, and the empty/single
 * /multi summary sentences.
 *
 * ## Two things this case does that v4's own tests deliberately do NOT
 *
 * 1. **The provider registry is INITIALIZED with the ten real dist plugins**
 *    (the `image-transport.ts` idiom), and neither capability predicate is
 *    mocked. v4's own tests mock `providerCanTransportImages`, so they never
 *    compare the real answer — and an UNINITIALIZED registry is worse than a
 *    mock here, because it silently changes the verdict: `getConfigRequirements`
 *    returns undefined, `requiresApiKey` defaults to `true` "for safety", and a
 *    keyless OLLAMA candidate is then skipped for want of a key it never needed.
 *    v5 reads its baked manifests, which know better. Initialising is what makes
 *    the two comparands the same PRODUCTION question.
 *
 *    Both capability answers are also emitted as their own rows — `transport`
 *    for `providerCanTransportImages`, `apiKeyCapability` for the
 *    `acceptsApiKey`/`requiresApiKey` pair — and the Rust side asserts its own
 *    answers against them. A capability divergence becomes a named failure
 *    instead of a mysteriously wrong pick three cases later.
 * 2. **Errors are emitted as the `(name, message)` pair the classifier actually
 *    reads**, not as a constructor label, plus an explicit `isNullish` flag. v5
 *    has no error-class hierarchy at the stream seam, so its `FallbackError`
 *    names `kind` / `name` / `message` explicitly; feeding it v4's OBSERVED
 *    name+message is what makes the two classifiers comparable rather than
 *    merely similar. `isNullish` is what separates `throw null` (v4's
 *    `String(error ?? 'unknown error')` arm) from `throw ''`, which
 *    `String(error ?? '')` alone renders identically.
 *
 * ⚠ `LOG_LEVEL=error` is LOAD-BEARING, not tidiness. The engine and the tier
 * picker log at debug/info/warn through v4's real logger, which writes JSON
 * lines to STDOUT — straight into the NDJSON this script is producing. Without
 * the env var the first line of the oracle is a log record, and the differential
 * dies on a row with no `id` rather than on anything it measures. (`error` is
 * v4's lowest level and the engine never logs at it.)
 *
 * Run from the v4 checkout (pin a detached worktree via
 * `recipe_sweep.py --v4` when v4 HEAD has moved past the baseline):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   cd ~/source/quilltap-server
 *   LOG_LEVEL=error \
 *     $N/npx tsx $V5W/harness/oracle/cases/fallback-engine.ts \
 *     > /tmp/oracle-fallback-engine.ndjson
 */

import { createRequire } from 'node:module';
import { join } from 'node:path';

import {
  APIKeyError,
  ContentLimitError,
  LLMProviderError,
  ModelNotFoundError,
  NetworkError,
  RateLimitError,
  TokenLimitError,
} from '@/lib/llm/errors';
import { providerCanTransportImages } from '@/lib/llm/image-transport';
import { acceptsApiKey, requiresApiKey } from '@/lib/plugins/provider-validation';
import {
  buildFallbackChain,
  classifyFallbackTrigger,
  pickTierCandidate,
  recordAttempt,
  summarizeFallbackAttempts,
  tierMatches,
  type FallbackContext,
} from '@/lib/llm/fallback';
import type { ConnectionProfile } from '@/lib/schemas/types';

const nodeRequire = createRequire(import.meta.url);

const rows: unknown[] = [];

// ── Profiles ────────────────────────────────────────────────────────────────

function makeProfile(overrides: Partial<ConnectionProfile> = {}): ConnectionProfile {
  return {
    id: 'p-primary',
    userId: 'u1',
    name: 'Primary',
    provider: 'ANTHROPIC',
    transport: 'api',
    courierDeltaMode: true,
    apiKeyId: 'k1',
    baseUrl: null,
    modelName: 'claude-sonnet',
    parameters: {},
    isDefault: false,
    isCheap: false,
    allowWebSearch: false,
    useNativeWebSearch: false,
    allowToolUse: true,
    pseudoToolMode: 'auto',
    multiCharacterPrefill: null,
    fallbackProfileId: null,
    allowTierFallback: false,
    modelClass: 'Extended',
    maxContext: null,
    maxTokens: null,
    isDangerousCompatible: false,
    supportsImageUpload: false,
    tags: [],
    sortIndex: 0,
    totalTokens: 0,
    totalPromptTokens: 0,
    totalCompletionTokens: 0,
    messageCount: 0,
    createdAt: '2026-01-01T00:00:00.000Z',
    updatedAt: '2026-01-01T00:00:00.000Z',
    ...overrides,
  } as ConnectionProfile;
}

function makeContext(overrides: Partial<FallbackContext> = {}): FallbackContext {
  return {
    userId: 'u1',
    purpose: 'chat',
    dangerous: false,
    needsVision: false,
    needsTools: false,
    alreadyTried: [],
    ...overrides,
  };
}

function makeRepos(profiles: ConnectionProfile[]) {
  return {
    connections: {
      findById: async (id: string) => profiles.find((p) => p.id === id) ?? null,
      findByUserId: async (userId: string) => profiles.filter((p) => p.userId === userId),
    },
  };
}

// ── 0. Initialize the registry with the real dist plugins ───────────────────
//
// Load-bearing: see the header. Every capability answer below — and every
// vision/credential filter the chain and picker apply — is the production one
// only once this has run.

const PLUGIN_DIRS = [
  'anthropic',
  'openai',
  'google',
  'grok',
  'deepseek',
  'z-ai',
  'openrouter',
  'ollama',
  'openai-compatible',
  'nanogpt',
];
const { initializeProviderRegistry } = await import('@/lib/plugins/provider-registry');
await initializeProviderRegistry(
  PLUGIN_DIRS.map((d) => {
    const m = nodeRequire(join(process.cwd(), 'plugins', 'dist', `qtap-plugin-${d}`, 'index.js'));
    return m.plugin || m.default?.plugin || m.default;
  }),
);

// ── 1. The shared capability answers ────────────────────────────────────────
//
// Every provider name the corpus uses, and what v4 says about it. The Rust side
// asserts its own answers match, so a registry-vs-mirror disagreement is caught
// here rather than showing up as a mystery chain.

const PROVIDERS_USED = [
  'ANTHROPIC',
  'OPENAI',
  'GOOGLE',
  'GROK',
  'OPENROUTER',
  'DEEPSEEK',
  'NANOGPT',
  'OPENAI_COMPATIBLE',
  'OLLAMA',
  'Z_AI',
  'anthropic',
  'A_PROVIDER_NOBODY_KNOWS',
];
for (const provider of PROVIDERS_USED) {
  rows.push({
    kind: 'transport',
    id: `transport/${provider}`,
    provider,
    canTransport: providerCanTransportImages(provider),
  });
  rows.push({
    kind: 'apiKeyCapability',
    id: `apiKey/${provider}`,
    provider,
    accepts: acceptsApiKey(provider),
    requires: requiresApiKey(provider),
  });
}

// ── 2. classifyFallbackTrigger ──────────────────────────────────────────────

function named(name: string, message: string): Error {
  const e = new Error(message);
  e.name = name;
  return e;
}

const CLASSIFY: Array<[string, unknown]> = [
  // The typed provider errors.
  ['api-key', new APIKeyError('OPENAI')],
  ['rate-limit', new RateLimitError('OPENAI')],
  ['rate-limit-retry-after', new RateLimitError('OPENAI', 30)],
  ['network', new NetworkError('OPENAI')],
  ['model-not-found', new ModelNotFoundError('OPENAI', 'gpt-9')],
  ['untyped-provider-error', new LLMProviderError('GROK', 'something went wrong')],
  // The non-triggers.
  ['token-limit-typed', new TokenLimitError('ANTHROPIC', 210311, 200000)],
  ['content-limit-typed', new ContentLimitError('ANTHROPIC', 'pdf_pages')],
  ['token-limit-message', new Error('prompt is too long')],
  ['content-limit-message', new Error('image is too large')],
  ['tool-unsupported', new Error('Function calling is not supported')],
  ['zod', named('ZodError', 'Invalid input')],
  // Ladder ORDER: the non-trigger checks run BEFORE the typed ladder (v4's
  // own comment — several non-triggers arrive AS LLMProviderError subclasses),
  // so a typed error whose MESSAGE matches a non-trigger pattern is a
  // non-trigger. Without these rows a mutation hoisting the typed arms above
  // the non-trigger checks stays green (round-1 unification, 2026-09-01).
  ['non-trigger-beats-typed-network', (() => { const e = new NetworkError('OPENAI'); e.message = 'prompt is too long'; return e; })()],
  ['non-trigger-beats-typed-rate', (() => { const e = new RateLimitError('OPENAI'); e.message = 'Function calling is not supported'; return e; })()],
  ['zod-beats-network-message', named('ZodError', 'fetch failed')],
  ['unattributed-4xx', new Error('400 Bad Request: unknown field')],
  ['unattributed-403', new Error('403 Forbidden')],
  // The cheap path's own deadline.
  ['cheap-deadline', named('CheapLLMTimeoutError', 'Cheap LLM task exceeded its 45000ms budget')],
  // Message-pattern arms, in the order the classifier tries them.
  ['bare-503', new Error('503 Service Unavailable')],
  ['overloaded', new Error('upstream is overloaded')],
  ['internal-server-error', new Error('Internal Server Error')],
  ['bad-gateway', new Error('Bad Gateway')],
  ['gateway-timeout', new Error('Gateway Timeout')],
  ['server-had-an-error', new Error('The server had an error while processing your request')],
  ['econnreset', new Error('read ECONNRESET')],
  ['econnrefused', new Error('connect ECONNREFUSED 127.0.0.1:11434')],
  ['enotfound', new Error('getaddrinfo ENOTFOUND api.example.com')],
  ['socket-hang-up', new Error('socket hang up')],
  ['fetch-failed', new Error('fetch failed')],
  ['timed-out', new Error('The operation timed out')],
  ['aborted', new Error('The operation was aborted')],
  // NETWORK is tried BEFORE PROVIDER, so a message matching both is network.
  ['network-beats-provider', new Error('504 gateway timeout')],
  ['auth-401', new Error('401 Unauthorized')],
  ['auth-invalid-key', new Error('Invalid API key provided')],
  ['auth-authentication', new Error('authentication failed')],
  ['rate-429', new Error('429 Too Many Requests')],
  ['rate-words', new Error('You have hit the rate limit for this model')],
  ['model-missing-words', new Error('The model gpt-9 does not exist')],
  ['model-missing-unknown', new Error('model unknown')],
  // The tail.
  ['bare-vendor-message', new Error('Upstream said no')],
  ['empty-message', new Error('')],
  ['non-error-string', 'a thrown string'],
  ['non-error-null', null],
  ['non-error-undefined', undefined],
  ['non-error-number', 42],
];

for (const [id, error] of CLASSIFY) {
  const trigger = classifyFallbackTrigger(error);
  rows.push({
    kind: 'classify',
    id: `classify/${id}`,
    // What the classifier actually reads. v5's FallbackError names the same
    // three inputs, so the Rust side reconstructs from these rather than from
    // a constructor label.
    errName: error instanceof Error ? error.name : null,
    errMessage: error instanceof Error ? error.message : String(error ?? ''),
    isError: error instanceof Error,
    isNullish: error === null || error === undefined,
    trigger,
  });
}

// ── 3. tierMatches ──────────────────────────────────────────────────────────

const CLASSES: Array<string | null> = ['Compact', 'Standard', 'Extended', 'Deep', null, 'Bogus'];
for (const candidate of CLASSES) {
  for (const failed of CLASSES) {
    rows.push({
      kind: 'tierMatches',
      id: `tier/${candidate ?? 'null'}-vs-${failed ?? 'null'}`,
      candidateClass: candidate,
      failedClass: failed,
      result: tierMatches(
        makeProfile({ id: 'cand', modelClass: candidate }),
        makeProfile({ id: 'failed', modelClass: failed }),
      ),
    });
  }
}

// ── 4. pickTierCandidate ────────────────────────────────────────────────────

interface PickCase {
  id: string;
  failed: Partial<ConnectionProfile>;
  candidates: Array<Partial<ConnectionProfile>>;
  context?: Partial<FallbackContext>;
}

const FAILED = { id: 'failed', provider: 'ANTHROPIC', modelClass: 'Standard' };

const PICKS: PickCase[] = [
  { id: 'nobody-qualifies', failed: FAILED, candidates: [FAILED] },
  { id: 'never-the-failed-twin', failed: FAILED, candidates: [{ id: 'failed', provider: 'OPENAI' }] },
  {
    id: 'never-already-tried',
    failed: FAILED,
    candidates: [{ id: 'spare', provider: 'OPENAI' }],
    context: { alreadyTried: ['spare'] },
  },
  {
    id: 'never-courier',
    failed: FAILED,
    candidates: [{ id: 'courier', provider: 'OPENAI', transport: 'courier' }],
  },
  {
    id: 'no-usable-key',
    failed: FAILED,
    candidates: [{ id: 'keyless', provider: 'OPENAI', apiKeyId: null }],
  },
  {
    id: 'keyless-provider-passes',
    failed: FAILED,
    candidates: [{ id: 'ollama', provider: 'OLLAMA', apiKeyId: null }],
  },
  {
    id: 'dangerous-requires-cleared',
    failed: FAILED,
    candidates: [
      { id: 'mainstream', provider: 'OPENAI', isDangerousCompatible: false },
      { id: 'cleared', provider: 'GOOGLE', isDangerousCompatible: true },
    ],
    context: { dangerous: true },
  },
  {
    id: 'vision-needs-the-flag',
    failed: FAILED,
    candidates: [{ id: 'no-flag', provider: 'OPENAI', supportsImageUpload: false }],
    context: { needsVision: true },
  },
  {
    id: 'vision-needs-a-transporting-plugin',
    failed: FAILED,
    candidates: [
      { id: 'flagged-openai', provider: 'OPENAI', supportsImageUpload: true },
      { id: 'flagged-deepseek', provider: 'DEEPSEEK', supportsImageUpload: true },
    ],
    context: { needsVision: true },
  },
  {
    id: 'tools-off-is-skipped',
    failed: FAILED,
    candidates: [{ id: 'no-tools', provider: 'OPENAI', allowToolUse: false }],
    context: { needsTools: true },
  },
  {
    id: 'tools-off-is-fine-without-tools',
    failed: FAILED,
    candidates: [{ id: 'no-tools', provider: 'OPENAI', allowToolUse: false }],
  },
  {
    id: 'lower-class-rejected',
    failed: FAILED,
    candidates: [{ id: 'worse', provider: 'OPENAI', modelClass: 'Compact' }],
  },
  {
    id: 'different-provider-beats-better-tier',
    failed: FAILED,
    candidates: [
      { id: 'same', provider: 'ANTHROPIC', modelClass: 'Deep' },
      { id: 'different', provider: 'OPENAI', modelClass: 'Standard' },
    ],
  },
  {
    id: 'provider-compare-is-case-insensitive',
    failed: FAILED,
    candidates: [{ id: 'same-lower', provider: 'anthropic' }],
  },
  {
    id: 'case-folded-sibling-still-loses-to-a-different-provider',
    failed: FAILED,
    candidates: [
      { id: 'same-lower', provider: 'anthropic' },
      { id: 'different', provider: 'GOOGLE' },
    ],
  },
  {
    id: 'quality-breaks-the-tie',
    failed: FAILED,
    candidates: [
      { id: 'lower', provider: 'OPENAI', modelClass: 'Standard', sortIndex: 0 },
      { id: 'higher', provider: 'GOOGLE', modelClass: 'Deep', sortIndex: 5 },
    ],
  },
  {
    id: 'sort-index-breaks-the-remaining-tie',
    failed: FAILED,
    candidates: [
      { id: 'second', provider: 'GOOGLE', modelClass: 'Deep', sortIndex: 9 },
      { id: 'first', provider: 'OPENAI', modelClass: 'Deep', sortIndex: 1 },
    ],
  },
  {
    id: 'unclassified-failed-takes-only-unclassified',
    failed: { id: 'failed', provider: 'ANTHROPIC', modelClass: null },
    candidates: [
      { id: 'classified', provider: 'OPENAI', modelClass: 'Deep' },
      { id: 'unclassified', provider: 'GOOGLE', modelClass: null },
    ],
  },
  {
    id: 'a-full-tie-keeps-input-order',
    failed: FAILED,
    candidates: [
      { id: 'b-first-in-input', provider: 'GOOGLE', modelClass: 'Standard', sortIndex: 3 },
      { id: 'a-second-in-input', provider: 'OPENROUTER', modelClass: 'Standard', sortIndex: 3 },
    ],
  },
];

for (const c of PICKS) {
  const failed = makeProfile(c.failed);
  const candidates = c.candidates.map((p) => makeProfile(p));
  const context = makeContext(c.context);
  const pick = pickTierCandidate(failed, candidates, context);
  rows.push({
    kind: 'pick',
    id: `pick/${c.id}`,
    failed,
    candidates,
    context,
    pickedId: pick ? pick.id : null,
  });
}

// ── 5. buildFallbackChain ───────────────────────────────────────────────────

interface ChainCase {
  id: string;
  primary: Partial<ConnectionProfile>;
  others: Array<Partial<ConnectionProfile>>;
  context?: Partial<FallbackContext>;
}

const UNDERSTUDY = { id: 'p-understudy', name: 'Understudy', provider: 'OPENAI' };

const CHAINS: ChainCase[] = [
  {
    id: 'primary-then-understudy',
    primary: { fallbackProfileId: 'p-understudy' },
    others: [UNDERSTUDY],
  },
  {
    id: 'no-recursion',
    primary: { fallbackProfileId: 'p-understudy' },
    others: [
      { ...UNDERSTUDY, fallbackProfileId: 'p-third' },
      { id: 'p-third', name: 'Third', provider: 'GOOGLE' },
    ],
  },
  {
    id: 'cycle-is-harmless',
    primary: { fallbackProfileId: 'p-understudy' },
    others: [{ ...UNDERSTUDY, fallbackProfileId: 'p-primary' }],
  },
  { id: 'self-reference-ignored', primary: { fallbackProfileId: 'p-primary' }, others: [] },
  { id: 'deleted-understudy-dropped', primary: { fallbackProfileId: 'p-understudy' }, others: [] },
  {
    id: 'courier-understudy-dropped',
    primary: { fallbackProfileId: 'p-understudy' },
    others: [{ ...UNDERSTUDY, transport: 'courier' }],
  },
  {
    id: 'blind-understudy-dropped-on-a-vision-turn',
    primary: { fallbackProfileId: 'p-understudy', supportsImageUpload: true },
    others: [{ ...UNDERSTUDY, supportsImageUpload: false }],
    context: { needsVision: true },
  },
  {
    id: 'seeing-understudy-kept-on-a-vision-turn',
    primary: { fallbackProfileId: 'p-understudy', supportsImageUpload: true },
    others: [{ ...UNDERSTUDY, supportsImageUpload: true }],
    context: { needsVision: true },
  },
  {
    id: 'non-transporting-plugin-dropped-on-a-vision-turn',
    primary: { fallbackProfileId: 'p-understudy', supportsImageUpload: true },
    others: [{ ...UNDERSTUDY, provider: 'DEEPSEEK', supportsImageUpload: true }],
    context: { needsVision: true },
  },
  {
    id: 'blind-understudy-kept-on-a-text-turn',
    primary: { fallbackProfileId: 'p-understudy' },
    others: [{ ...UNDERSTUDY, supportsImageUpload: false }],
    context: { needsVision: false },
  },
  {
    id: 'dangerous-incompatible-understudy-honoured',
    primary: { fallbackProfileId: 'p-understudy' },
    others: [{ ...UNDERSTUDY, isDangerousCompatible: false }],
    context: { dangerous: true },
  },
  {
    id: 'tools-off-understudy-honoured',
    primary: { fallbackProfileId: 'p-understudy' },
    others: [{ ...UNDERSTUDY, allowToolUse: false }],
    context: { needsTools: true },
  },
  {
    id: 'already-tried-understudy-skipped',
    primary: { fallbackProfileId: 'p-understudy' },
    others: [UNDERSTUDY],
    context: { alreadyTried: ['p-understudy'] },
  },
  {
    id: 'already-tried-primary-is-not-led-with',
    primary: { fallbackProfileId: 'p-understudy' },
    others: [UNDERSTUDY],
    context: { alreadyTried: ['p-primary'] },
  },
  {
    id: 'no-tier-pick-without-opt-in',
    primary: { fallbackProfileId: null },
    others: [{ id: 'p-spare', name: 'Spare', provider: 'GOOGLE' }],
  },
  {
    id: 'exactly-one-tier-pick',
    primary: { fallbackProfileId: 'p-understudy', allowTierFallback: true },
    others: [
      UNDERSTUDY,
      { id: 'p-a', name: 'Spare A', provider: 'GOOGLE' },
      { id: 'p-b', name: 'Spare B', provider: 'GROK' },
    ],
  },
  {
    id: 'never-the-same-profile-twice',
    primary: { fallbackProfileId: 'p-understudy', allowTierFallback: true },
    others: [UNDERSTUDY],
  },
  {
    id: 'tier-pick-alone',
    primary: { fallbackProfileId: null, allowTierFallback: true },
    others: [{ id: 'p-spare', name: 'Spare', provider: 'GOOGLE' }],
  },
  {
    id: 'tier-pick-respects-dangerous',
    primary: { fallbackProfileId: null, allowTierFallback: true },
    others: [
      { id: 'p-mainstream', name: 'Mainstream', provider: 'GOOGLE', isDangerousCompatible: false },
      { id: 'p-cleared', name: 'Cleared', provider: 'GROK', isDangerousCompatible: true },
    ],
    context: { dangerous: true },
  },
  {
    id: 'tier-pick-never-offers-another-users-profile',
    primary: { fallbackProfileId: null, allowTierFallback: true },
    others: [{ id: 'p-other-user', name: 'Someone Else', provider: 'GOOGLE', userId: 'u2' }],
  },
];

for (const c of CHAINS) {
  const primary = makeProfile(c.primary);
  const others = c.others.map((p) => makeProfile(p));
  const context = makeContext(c.context);
  const chain = await buildFallbackChain(primary, makeRepos([primary, ...others]), context);
  rows.push({
    kind: 'chain',
    id: `chain/${c.id}`,
    primary,
    others,
    context,
    chain: chain.map((x) => ({ id: x.profile.id, kind: x.kind })),
  });
}

// ── 6. recordAttempt + summarizeFallbackAttempts ────────────────────────────

const ATTEMPT_INPUTS: Array<[string, Partial<ConnectionProfile>, string, unknown]> = [
  ['plain', { name: 'Claude Sonnet' }, 'rate-limit', new Error('429')],
  ['network', { name: 'Kimi', provider: 'OPENROUTER', modelName: 'kimi-k2' }, 'network', new Error('ECONNRESET')],
  ['string-throw', { name: 'Odd One' }, 'provider-error', 'a thrown string'],
  ['null-throw', { name: 'Nulled' }, 'provider-error', null],
  ['empty-message', { name: 'Silent' }, 'empty-response', new Error('')],
  ['moderation', { name: 'Refuser' }, 'moderation-refusal', new Error('content filtered')],
];

const recorded = ATTEMPT_INPUTS.map(([id, profile, trigger, err]) => {
  const attempt = recordAttempt(makeProfile(profile), trigger as never, err);
  rows.push({
    kind: 'record',
    id: `record/${id}`,
    profile: makeProfile(profile),
    trigger,
    errName: err instanceof Error ? err.name : null,
    errMessage: err instanceof Error ? err.message : String(err ?? ''),
    isError: err instanceof Error,
    isNullish: err === null || err === undefined,
    attempt,
  });
  return attempt;
});

const SUMMARIES: Array<[string, number[], boolean]> = [
  ['empty', [], false],
  ['empty-offered', [], true],
  ['single', [0], false],
  ['single-offered', [0], true],
  ['pair-offered', [0, 1], true],
  ['pair-not-offered', [0, 1], false],
  ['triple-not-offered', [0, 1, 2], false],
  ['odd-messages', [2, 3, 4], true],
];

for (const [id, idx, offered] of SUMMARIES) {
  const attempts = idx.map((i) => recorded[i]);
  rows.push({
    kind: 'summarize',
    id: `summarize/${id}`,
    attempts,
    tierPickWasOffered: offered,
    text: summarizeFallbackAttempts(attempts, offered),
  });
}

for (const r of rows) process.stdout.write(JSON.stringify(r) + '\n');
