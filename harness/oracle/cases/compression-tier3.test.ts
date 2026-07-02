/**
 * @jest-environment node
 *
 * Tier-3 (mocked-completion) ORACLE for the context-compression service half
 * (v4 `applyContextCompression`, lib/chat/context/compression.ts:191, driving
 * `compressConversationHistory`, lib/memory/cheap-llm-tasks/compression-tasks.ts).
 *
 * `applyContextCompression` makes NO DB writes, so the differential is
 * result-object equivalence: this oracle drives v4's REAL
 * `applyContextCompression` over the committed corpus
 * (harness/oracle/fixtures/compression-tier3.json) with ONLY the model/infra
 * seams stubbed, and emits each call's full `ContextCompressionResult` plus the
 * exact `provider|model|temperature|messages` canned keys the provider
 * answered. The Rust side replays exactly those recorded entries through
 * `CannedCompletionProvider`, so any prompt/selection/temperature divergence
 * surfaces as a canned-miss.
 *
 * Stubbed seams:
 *   - `createLLMProvider` — the returned provider's `sendMessage` resolves each
 *     call by matching the request messages against the current corpus case's
 *     completion rules (keyed on `provider|model`), RECORDS the exact canned key
 *     it answered (as `kind: "canned"` rows), and returns the rule's fixed
 *     content (empty or non-empty) or throws the rule's error. Already a
 *     `jest.fn()` from jest.setup — re-implemented per case here.
 *   - `getApiKeyForCheapLLMSelection` → a constant (key management is
 *     host-side; the Rust boundary starts at the provider call).
 *   - `logLLMCall` → already a no-op mock from jest.setup.
 *
 * Everything else — the split, the prompt building, `executeCheapLLMTask`'s
 * temperature/uncensored orchestration, the token estimates, the result
 * assembly — is v4's REAL code. No database is touched.
 *
 * Runs under v4's JEST (not tsx) because `createLLMProvider` is a module export
 * only `jest.mock` can replace. The file lives in the v5 harness tree; v4's jest
 * resolves it via an extra `--roots`, with `@/` mapped to v4.
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-compression.ndjson \
 *     $N/npx jest --silent --watchman=false \
 *       --roots "$PWD" --roots "$V5/harness/oracle/cases" -- compression-tier3
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

// Pin the API-key acquisition to a constant (host-side seam). Must be declared
// before importing the module under test so the mock is in place.
jest.mock('@/lib/services/api-key.service', () => ({
  __esModule: true,
  ...jest.requireActual('@/lib/services/api-key.service'),
  getApiKeyForCheapLLMSelection: jest.fn(async () => 'test-key'),
}));

import { applyContextCompression } from '@/lib/chat/context/compression';
import { createLLMProvider } from '@/lib/llm';

interface CompletionRule {
  /** Provider to match (v4 `selection.provider`). */
  provider: string;
  /** Model to match (v4 `selection.modelName`). */
  model: string;
  /** Fixed response content (may be empty to drive the fallback). */
  response?: string;
  /** Fixed error message to throw instead of responding. */
  error?: string;
  /** Optional usage returned with the response. */
  usage?: { promptTokens: number; completionTokens: number; totalTokens: number };
}

interface ProfileSpec {
  id: string;
  provider: string;
  modelName: string;
  baseUrl: string | null;
  isDangerousCompatible: boolean;
  parameters: Record<string, unknown> | null;
}

interface CallSpec {
  name: string;
  messages: Array<{ role: 'user' | 'assistant' | 'system'; content: string }>;
  systemPrompt: string;
  windowSize: number;
  compressionTargetTokens: number;
  characterName: string;
  userName: string;
  userId: string;
  /** The cheap-LLM selection passed to compression (v4 `CheapLLMSelection`). */
  selection: {
    provider: string;
    modelName: string;
    baseUrl?: string;
    connectionProfileId?: string;
    isLocal: boolean;
    profileParameters?: Record<string, unknown>;
  };
  dangerSettings: { mode: string; uncensoredTextProfileId?: string } | null;
  availableProfiles: ProfileSpec[] | null;
  completionRules: CompletionRule[];
}

interface Spec {
  calls: CallSpec[];
}

test('compression tier-3 oracle', async () => {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'compression-tier3.json'), 'utf8')
  ) as Spec;
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  // Recorded canned-completion entries, deduped by the exact call key. The key
  // format MUST match `quilltap_core::model::completion::canned_completion_key`.
  const cannedRecorded = new Map<
    string,
    {
      provider: string;
      model: string;
      temperature: number | null;
      messages: Array<{ role: string; content: string }>;
      response: string | null;
      failure: string | null;
      usage: CompletionRule['usage'] | null;
    }
  >();

  const lines: string[] = [];

  for (const call of spec.calls) {
    // Per-case provider: resolves by (provider, model) against the case rules,
    // records the answered canned key, returns content or throws.
    jest.mocked(createLLMProvider).mockImplementation(
      async (provider: string, _baseUrl?: string) =>
        ({
          sendMessage: async (
            params: {
              messages: Array<{ role: string; content: string }>;
              model: string;
              temperature?: number;
            },
            _apiKey: string
          ) => {
            const rule = call.completionRules.find(
              (r) => r.provider === provider && r.model === params.model
            );
            if (!rule) {
              throw new Error(
                `no completion rule for provider=${provider} model=${params.model}`
              );
            }
            // Record the exact canned key on EVERY answered call — both success
            // and failure — so the Rust side can replay it (a failure rule is
            // registered via `with_failure`, a response rule via
            // `with_response`). A prompt/selection/temperature divergence
            // therefore surfaces as a canned-miss on either path.
            const messages = params.messages.map((m) => ({ role: m.role, content: m.content }));
            const key = `${provider}|${params.model}|${params.temperature ?? '-'}|${JSON.stringify(messages)}`;
            if (!cannedRecorded.has(key)) {
              cannedRecorded.set(key, {
                provider,
                model: params.model,
                temperature: params.temperature ?? null,
                messages,
                response: rule.error !== undefined ? null : rule.response ?? '',
                failure: rule.error ?? null,
                usage: rule.error !== undefined ? null : rule.usage ?? null,
              });
            }
            if (rule.error !== undefined) {
              throw new Error(rule.error);
            }
            return {
              content: rule.response ?? '',
              finishReason: 'stop',
              usage: rule.usage,
            };
          },
        }) as never
    );

    // v4 builds `uncensoredFallback` from `options.dangerSettings &&
    // options.availableProfiles` — `isDangerousChat` is NOT part of
    // `ContextCompressionOptions`, so it is always undefined on this path.
    const result = await applyContextCompression(
      call.messages as never,
      call.systemPrompt,
      {
        enabled: true,
        windowSize: call.windowSize,
        compressionTargetTokens: call.compressionTargetTokens,
        systemPromptTargetTokens: 1500,
        selection: call.selection as never,
        userId: call.userId,
        characterName: call.characterName,
        userName: call.userName,
        ...(call.dangerSettings ? { dangerSettings: call.dangerSettings as never } : {}),
        ...(call.availableProfiles
          ? { availableProfiles: call.availableProfiles as never }
          : {}),
      } as never
    );

    lines.push(JSON.stringify({ kind: 'result', call: call.name, result }));
  }

  for (const entry of cannedRecorded.values()) {
    lines.push(JSON.stringify({ kind: 'canned', ...entry }));
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`compression oracle wrote ${outPath}\n`);
  expect(lines.length).toBeGreaterThanOrEqual(spec.calls.length);
});
