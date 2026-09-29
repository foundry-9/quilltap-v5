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
 * per case (`kind:"result"`, `{ label, appearances, completionCalls,
 * concealPromptCalls, logLines }`).
 *
 * [97b25fc53] P4.D239 — the drape. Each case may pass `mode` ('redress' |
 * 'conceal') as the 8th argument (absent = v4's default, `redress`). ⚠ BOTH
 * sanitizer prompts open with the same sentence, so the sanitize branch below
 * still matches on `startsWith`; the SECOND probe (`includes('Do NOT invent
 * clothing')`, the conceal prompt only) counts `concealPromptCalls` — the
 * comparand that says which prompt the mode picked. A case's `undressed` map
 * is spliced into the sanitize answer per character (whatever the mode — the
 * redress row with a stray `true` proves v4 drops it); `sanitizedTextOverride`
 * replaces a character's answer text; `sanitizeThrows` makes the sanitize call
 * throw (v4's failure WARN arm — the previously unrecorded N3). The
 * projection carries `needsConcealment` (v4 only ever writes `true`, so
 * `JSON.stringify` drops the key otherwise). `logLines` records v4's REAL
 * logger calls (a spy that delegates) for the `[AppearanceResolution]` lines
 * and the new `[CheapLLM] Sanitizing appearances` DEBUG, keys in v4's order.
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
  /** v4 `3b463d6b1` (#76): the stored `conciergeSettings` + the chat the policy resolves WITH. */
  concierge: Record<string, unknown>;
  chat: Record<string, unknown>;
  isDangerousChat: boolean;
  routesDangerousToUncensored: boolean;
  classification: 'safe' | 'dangerous';
  sanitizeEchoes?: boolean;
  sanitizeJunk?: boolean;
  /** [97b25fc53] the 8th argument; absent = v4's default (`redress`). */
  mode?: 'redress' | 'conceal';
  /** [97b25fc53] per-character `undressed` value spliced into the sanitize answer. */
  undressed?: Record<string, unknown>;
  /** per-character sanitize answer text, overriding `spec.sanitizedText`. */
  sanitizedTextOverride?: Record<string, string>;
  /** the sanitize call throws (v4's `Sanitization failed` WARN arm). */
  sanitizeThrows?: boolean;
}

interface Spec {
  userId: string;
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
  let concealPromptCalls = 0;

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
            // The second probe: only the conceal prompt says this.
            if (system.includes('Do NOT invent clothing')) concealPromptCalls += 1;
            if (c.sanitizeThrows) {
              // Nothing recorded: the Rust side's canned MISS is its twin.
              throw new Error('sanitize provider unavailable');
            }
            const items = JSON.parse(user) as Array<{
              characterId: string;
              appearanceText: string;
            }>;
            const withUndressed = (it: { characterId: string }, o: Record<string, unknown>) =>
              c.undressed && Object.prototype.hasOwnProperty.call(c.undressed, it.characterId)
                ? { ...o, undressed: c.undressed[it.characterId] }
                : o;
            if (c.sanitizeJunk) {
              // v4's parser: a non-array answer falls back to the originals,
              // so nothing changes and `wasSanitized` stays false.
              response = JSON.stringify({ not: 'an array' });
            } else if (c.sanitizeEchoes) {
              // Same text back: v4's merge only rewrites when the text CHANGED
              // (or, [97b25fc53], when the character comes back undressed).
              response = JSON.stringify(items.map((it) => withUndressed(it, { ...it })));
            } else {
              response = JSON.stringify(
                items.map((it) =>
                  withUndressed(it, {
                    characterId: it.characterId,
                    appearanceText:
                      c.sanitizedTextOverride?.[it.characterId] ?? spec.sanitizedText[it.characterId],
                  })
                )
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

  // v4's REAL logger calls, recorded by a delegating spy (the
  // character-optimizer oracle's shape). Only the gate's own lines and the
  // sanitizer's pre-call DEBUG — the Gatekeeper's lines belong to its family.
  let logLines: Array<{ level: string; message: string; context: unknown }> = [];
  const loggerModule = (await import('@/lib/logger')) as {
    logger: Record<string, (...a: unknown[]) => unknown>;
  };
  for (const level of ['info', 'warn', 'error', 'debug']) {
    const original = loggerModule.logger[level];
    jest.spyOn(loggerModule.logger as never, level as never).mockImplementation(((
      message: string,
      context?: unknown,
      err?: unknown
    ) => {
      if (
        typeof message === 'string' &&
        (message.startsWith('[AppearanceResolution]') ||
          message === '[CheapLLM] Sanitizing appearances')
      ) {
        // Round-trip through JSON: v4's transports serialize the context, and
        // an `undefined` value (an absent `chatId`) drops its key.
        logLines.push({ level, message, context: context === undefined ? null : JSON.parse(JSON.stringify(context)) });
      }
      return original.call(loggerModule.logger, message, context, err);
    }) as never);
  }
  const { resolveConciergeSettings } = await import('@/lib/services/dangerous-content/resolver.service');

  const selection = {
    provider: spec.cheapLLMSelection.provider,
    modelName: spec.cheapLLMSelection.modelName,
    connectionProfileId: spec.cheapLLMSelection.connectionProfileId,
    isLocal: false,
  };

  for (const c of spec.cases) {
    current = c;
    calls = 0;
    concealPromptCalls = 0;
    logLines = [];
    const conciergePolicy = resolveConciergeSettings(
      { conciergeSettings: c.concierge as never },
      c.chat as never,
    );
    const out = await sanitizeAppearancesIfNeeded(
      appearancesFor(spec, c) as never,
      conciergePolicy,
      c.isDangerousChat,
      c.routesDangerousToUncensored,
      selection as never,
      spec.userId,
      `chat-${c.token}`,
      // Absent → `undefined` → v4's own default parameter (`'redress'`) fires.
      c.mode
    );
    lines.push(
      JSON.stringify({
        kind: 'result',
        label: c.label,
        completionCalls: calls,
        concealPromptCalls,
        logLines,
        appearances: out.map((a) => ({
          characterId: a.characterId,
          characterName: a.characterName,
          physicalDescription: a.physicalDescription,
          physicalDescriptionName: a.physicalDescriptionName,
          clothingDescription: a.clothingDescription,
          clothingSource: a.clothingSource,
          wasSanitized: a.wasSanitized,
          needsConcealment: (a as { needsConcealment?: boolean }).needsConcealment,
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
