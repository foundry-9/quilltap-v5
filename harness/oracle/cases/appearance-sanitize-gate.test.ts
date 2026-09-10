/**
 * @jest-environment node
 *
 * Tier-3 ORACLE for v4 `lib/image-gen/appearance-resolution.ts`
 * `sanitizeAppearancesIfNeeded` — the five-rule gate (Rust
 * `quilltap_core::services::appearance_resolution::sanitize_appearances_if_needed`).
 *
 * [cc65d6bfc / bug 133] Until this family, NO harness family and no oracle case
 * drove that function directly (measured: zero hits for either spelling under
 * `crates/quilltap-harness/tests` and `harness/oracle/cases`). The story and
 * image-generation tier-3 families reach it only incidentally, and the
 * image-generation corpus keeps the Concierge OFF throughout, so its whole
 * fourth-parameter question was invisible. This drives v4's REAL function.
 *
 * Seams (the only things pinned):
 *   - `createLLMProvider` (`@/lib/llm`) → the recording-completion pattern: it
 *     answers the classify call and the sanitize task, and RECORDS the exact
 *     `provider|model|temperature|messages` key the Rust `CannedCompletionProvider`
 *     replays.
 *   - `moderationProviderRegistry.getDefaultProvider()` → null, so
 *     `classifyContent` always falls to the cheap LLM (the avatar oracle's shape).
 *   - `getApiKeyForCheapLLMSelection` → a constant (a host-side seam).
 *
 * No database: this family deliberately does NOT compare `llm_logs` (see the
 * corpus `$comment` — the DANGER_CLASSIFICATION projection is already diffed by
 * `danger_gatekeeper_tier3` and `story_background_job_tier3`, and un-mocking the
 * logger here would mean provisioning a whole instance for it).
 *
 * Emits one NDJSON line per RECORDED canned completion (`kind:"canned"`) and one
 * per case (`kind:"result"`, `{ label, appearances, completionCalls }`).
 *
 * Run (Node 24, from the v4 checkout; stage OUTSIDE any .claude path):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   cd ~/source/quilltap-server
 *   TMPO=/tmp/qt-appearance-gate-oracle; rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp $V5W/harness/oracle/cases/appearance-sanitize-gate.test.ts "$TMPO/cases/"
 *   cp $V5W/harness/oracle/fixtures/appearance-sanitize-gate.json "$TMPO/fixtures/"
 *   QT_ORACLE_OUT=/tmp/oracle-appearance-sanitize-gate.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- "appearance-sanitize-gate.test"
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

interface CaseSpec {
  label: string;
  token: string;
  mode: string;
  isDangerousChat: boolean;
  routesDangerousToUncensored: boolean;
  classification: 'safe' | 'dangerous';
  customClassificationPrompt?: string;
  sanitizeEchoes?: boolean;
  sanitizeJunk?: boolean;
}

interface Spec {
  userId: string;
  threshold: number;
  cheapLLMSelection: { provider: string; modelName: string; connectionProfileId: string };
  characters: Array<{ characterId: string; name: string }>;
  classifications: Record<string, string>;
  sanitizedText: Record<string, string>;
  cases: CaseSpec[];
}

/**
 * The appearance pair a case starts from. The TOKEN rides in the physical
 * description so every case's combined text — and therefore the classification
 * cache key, which is a process-global sha256 on both sides — is distinct.
 */
function appearancesFor(spec: Spec, c: CaseSpec) {
  return spec.characters.map((ch, i) => ({
    characterId: ch.characterId,
    characterName: ch.name,
    physicalDescription: `${ch.name} (${c.token}), ${i === 0 ? 'a woman with copper hair' : 'a tall woman with dark braided hair'}`,
    physicalDescriptionName: 'Portrait',
    clothingDescription: i === 0 ? 'wearing nothing at all' : 'in a sheer slip',
    clothingSource: 'stored' as const,
    wasSanitized: false,
  }));
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'appearance-sanitize-gate.json'), 'utf8')
  ) as Spec;

  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  process.env.LOG_LEVEL = 'error';

  const lines: string[] = [];
  const recorded = new Map<
    string,
    {
      provider: string;
      model: string;
      temperature: number | null;
      messages: Array<{ role: string; content: string }>;
      response: string;
    }
  >();
  // Per-case: which case is running, and how many completion calls it made.
  let current: CaseSpec | null = null;
  let calls = 0;

  jest.resetModules();

  jest.doMock('@/lib/llm', () => {
    const actual = jest.requireActual('@/lib/llm');
    return {
      __esModule: true,
      ...actual,
      createLLMProvider: async (provider: string, _baseUrl?: string) => ({
        sendMessage: async (
          params: {
            messages: Array<{ role: string; content: string }>;
            model: string;
            temperature?: number;
          },
          _apiKey: string
        ) => {
          const messages = params.messages.map((m) => ({ role: m.role, content: m.content }));
          const user = messages.find((m) => m.role === 'user')?.content ?? '';
          const system = messages.find((m) => m.role === 'system')?.content ?? '';
          const c = current!;
          calls += 1;
          let response: string;
          if (user.startsWith('Classify the following content:')) {
            response = spec.classifications[c.classification];
          } else if (
            system.startsWith('You are a content safety filter for image generation prompts.')
          ) {
            const items = JSON.parse(user) as Array<{
              characterId: string;
              appearanceText: string;
            }>;
            if (c.sanitizeJunk) {
              // v4's parser: a non-array answer falls back to the originals,
              // so nothing changes and `wasSanitized` stays false.
              response = JSON.stringify({ not: 'an array' });
            } else if (c.sanitizeEchoes) {
              // Same text back: v4's merge only rewrites when the text CHANGED.
              response = JSON.stringify(items);
            } else {
              response = JSON.stringify(
                items.map((it) => ({
                  characterId: it.characterId,
                  appearanceText: spec.sanitizedText[it.characterId],
                }))
              );
            }
          } else {
            throw new Error(`unexpected completion task: ${user.slice(0, 80)}`);
          }
          const key = `${provider}|${params.model}|${params.temperature ?? '-'}|${JSON.stringify(messages)}`;
          if (!recorded.has(key)) {
            recorded.set(key, {
              provider,
              model: params.model,
              temperature: params.temperature ?? null,
              messages,
              response,
            });
          }
          return {
            content: response,
            finishReason: 'stop',
            usage: { promptTokens: 10, completionTokens: 5, totalTokens: 15 },
          };
        },
      }),
    };
  });

  // No moderation provider → `classifyContent` always falls to the cheap LLM.
  jest.doMock('@/lib/plugins/moderation-provider-registry', () => ({
    __esModule: true,
    moderationProviderRegistry: {
      isInitialized: () => true,
      getAllProviders: () => [],
      getDefaultProvider: () => null,
    },
  }));

  jest.doMock('@/lib/services/api-key.service', () => {
    const actual = jest.requireActual('@/lib/services/api-key.service');
    return { __esModule: true, ...actual, getApiKeyForCheapLLMSelection: async () => 'test-key' };
  });

  const { sanitizeAppearancesIfNeeded } = await import('@/lib/image-gen/appearance-resolution');

  const selection = {
    provider: spec.cheapLLMSelection.provider,
    modelName: spec.cheapLLMSelection.modelName,
    connectionProfileId: spec.cheapLLMSelection.connectionProfileId,
    isLocal: false,
  };

  for (const c of spec.cases) {
    current = c;
    calls = 0;
    const dangerSettings = {
      mode: c.mode,
      threshold: spec.threshold,
      scanTextChat: true,
      scanImagePrompts: false,
      scanImageGeneration: false,
      displayMode: 'SHOW',
      showWarningBadges: true,
      ...(c.customClassificationPrompt
        ? { customClassificationPrompt: c.customClassificationPrompt }
        : {}),
    };
    const out = await sanitizeAppearancesIfNeeded(
      appearancesFor(spec, c) as never,
      dangerSettings as never,
      c.isDangerousChat,
      c.routesDangerousToUncensored,
      selection as never,
      spec.userId,
      `chat-${c.token}`
    );
    lines.push(
      JSON.stringify({
        kind: 'result',
        label: c.label,
        completionCalls: calls,
        appearances: out.map((a) => ({
          characterId: a.characterId,
          characterName: a.characterName,
          physicalDescription: a.physicalDescription,
          physicalDescriptionName: a.physicalDescriptionName,
          clothingDescription: a.clothingDescription,
          clothingSource: a.clothingSource,
          wasSanitized: a.wasSanitized,
        })),
      })
    );
  }
  current = null;

  for (const r of recorded.values()) lines.push(JSON.stringify({ kind: 'canned', ...r }));

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  // eslint-disable-next-line no-console
  console.log(
    `appearance-sanitize-gate oracle wrote ${outPath} (${spec.cases.length} cases, ${recorded.size} canned)`
  );
}

it('emits the appearance sanitize gate oracle', async () => {
  await main();
}, 120000);
