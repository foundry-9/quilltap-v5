/**
 * Oracle case: the cheap-LLM SELECTION shape — `selectionFromProfile`, the
 * five-rung `getCheapLLMProvider` ladder, and
 * `resolveUncensoredCheapLLMSelection` (P4.D155, v4 `0506517d3` correction (a)).
 *
 * Drives the REAL code:
 *   selectionFromProfile, getCheapLLMProvider, resolveUncensoredCheapLLMSelection
 *     (lib/llm/cheap-llm.ts)
 *   deadlineFor                          (lib/memory/cheap-llm-tasks/core-execution.ts)
 *
 * ## Why this family exists
 *
 * `0506517d3` collapsed eight hand-built `CheapLLMSelection` literals onto one
 * `selectionFromProfile(profile, { localBaseUrlFallback })`, and two of them
 * disagreed with it: both priority-5 branches carried no `profileParameters`
 * (so a profile's provider params — DeepSeek `thinking`, Ollama `num_ctx` —
 * reached every rung of the ladder except the fallback one), and the
 * answer-confirmation / cheap-task selections hard-coded `isLocal: false`.
 *
 * Nothing in the harness drove the ladder: `cheap_llm_fallback_equivalence`
 * covers `buildCheapFallbackSelections`, and the tier-3 families that resolve a
 * selection record a canned key of `provider|model|temperature|messages`, which
 * carries neither corrected field. So the corrections were unmeasurable until
 * this family. It is tier-1 — the three functions are pure — which is also why
 * it can be a plain `tsx` case with no DB, no jest, and no mocks.
 *
 * ## `deadlineFor` rides along, and it is not decoration
 *
 * `isLocal` is otherwise a field nobody in a differential looks at. Its two
 * readers are the API-key resolver (`isLocal` ⇒ no key needed) and
 * `deadlineFor` (`isLocal` ⇒ the 180 s local budget rather than 90/45 s). The
 * second is pure, so every emitted selection carries its background deadline:
 * a port that got `isLocal` wrong changes a NUMBER here, not just a boolean.
 *
 * ## The plugin registry is empty under tsx, deliberately
 *
 * `getCheapestModel` asks `getCheapModelConfig(provider)` first and falls back
 * to `LEGACY_CHEAPEST_MODEL_MAP`. Nothing initializes the registry here, so
 * every priority-5 fallback resolves through the legacy map — which is what the
 * Rust side reproduces by passing `registry_cheapest_for_current: None`.
 *
 * ## The `[CheapLLM]` lines (P4.169, B1-6)
 *
 * Every row carries `lines`: v4's logger lines the call emitted, captured by
 * splicing the singleton logger's transports (`service` / `environment`
 * dropped). `LOG_LEVEL=debug` must be set in the ENVIRONMENT — the logger
 * reads it when `cheap-llm.ts` loads, before this module's body runs. The
 * `fromProfiles` section drives `selectCheapLLMFromProfiles` (the loaded-set
 * prologue v5 had duplicated, with neither DEBUG).
 *
 * Run from inside the server checkout (Node 24
 * `~/.nvm/versions/node/v24.13.1/bin`):
 *   cd ~/source/quilltap-server
 *   LOG_LEVEL=debug TZ=UTC npx tsx \
 *     <V5W>/harness/oracle/cases/cheap-llm-selection.ts \
 *     > /tmp/oracle-cheap-llm-selection.ndjson
 */

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { logger } from '@/lib/logger';
import { selectCheapLLMFromProfiles } from '@/lib/llm/cheap-llm-user-selection';
import {
  buildCheapLLMConfig,
  getCheapLLMProvider,
  resolveUncensoredCheapLLMSelection,
  selectionFromProfile,
  type CheapLLMConfig,
  type CheapLLMSelection,
} from '@/lib/llm/cheap-llm';
import { deadlineFor } from '@/lib/memory/cheap-llm-tasks/core-execution';
import type { ConnectionProfile } from '@/lib/schemas/types';
import { resolveConciergeSettings } from '@/lib/services/dangerous-content/resolver.service';

interface CorpusProfile {
  id: string;
  provider: string;
  modelName: string;
  baseUrl: string | null;
  isCheap: boolean;
  isDangerousCompatible: boolean;
  parameters: Record<string, unknown> | null;
  maxContext: number | null;
}

interface Corpus {
  profiles: Record<string, CorpusProfile>;
  fromProfile: Array<{ name: string; profile: string; localBaseUrlFallback: boolean }>;
  ladder: Array<{
    name: string;
    current: string;
    config: Partial<CheapLLMConfig> & { strategy: string; fallbackToLocal: boolean };
    available: string[];
    ollamaAvailable: boolean;
  }>;
  fromProfiles: Array<{
    name: string;
    profiles: string[];
    defaultKey: string | null;
    cheapLLMSettings: Record<string, unknown> | null;
  }>;
  uncensored: Array<{
    name: string;
    standard: string;
    isDangerousChat: boolean;
    /** v4 `3b463d6b1` (#76): the stored `conciergeSettings`; null = no policy. */
    concierge: Record<string, unknown> | null;
    chat: { conciergeMode: string } | null;
    available: string[];
  }>;
}

const here = dirname(fileURLToPath(import.meta.url));
const corpus = JSON.parse(
  readFileSync(join(here, '..', 'fixtures', 'cheap-llm-selection.json'), 'utf8'),
) as Corpus;

/** A corpus row as v4's `ConnectionProfile` sees it. */
function profile(key: string): ConnectionProfile {
  const p = corpus.profiles[key];
  if (!p) throw new Error(`no such corpus profile: ${key}`);
  return p as unknown as ConnectionProfile;
}

/**
 * The emitted shape. `profileParameters` is emitted as `null` when absent so an
 * omitted key and an explicit null cannot be told apart by JSON alone — the
 * Rust side compares the same normalization.
 */
if (process.env.LOG_LEVEL !== 'debug') {
  throw new Error('run with LOG_LEVEL=debug in the environment (see header)');
}
type Line = { level: string; message: string; fields: Record<string, unknown> };
let lines: Line[] = [];
const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
internal.transports.splice(0, internal.transports.length, {
  write: (d: unknown) => {
    const data = d as { level: string; message: string; context?: Record<string, unknown> };
    if (!data.message.startsWith('[CheapLLM]')) return;
    const { service: _s, environment: _e, ...fields } = (data.context ?? {}) as Record<string, unknown>;
    lines.push(JSON.parse(JSON.stringify({ level: data.level, message: data.message, fields })));
  },
});
/** The lines logged since the last call, then reset. */
function takeLines(): Line[] {
  const out = lines;
  lines = [];
  return out;
}

function emit(name: string, kind: string, selection: CheapLLMSelection): void {
  process.stdout.write(
    JSON.stringify({
      name,
      kind,
      lines: takeLines(),
      provider: selection.provider,
      modelName: selection.modelName,
      baseUrl: selection.baseUrl ?? null,
      connectionProfileId: selection.connectionProfileId ?? null,
      isLocal: selection.isLocal,
      profileParameters: selection.profileParameters ?? null,
      // The `isLocal` consequence, so the boolean is load-bearing here.
      deadlineMs: deadlineFor(selection),
    }) + '\n',
  );
}

for (const c of corpus.fromProfile) {
  emit(
    c.name,
    'fromProfile',
    selectionFromProfile(profile(c.profile), { localBaseUrlFallback: c.localBaseUrlFallback }),
  );
}

for (const c of corpus.ladder) {
  emit(
    c.name,
    'ladder',
    getCheapLLMProvider(
      profile(c.current),
      c.config as CheapLLMConfig,
      c.available.map(profile),
      c.ollamaAvailable,
    ),
  );
}

for (const c of corpus.fromProfiles) {
  const all = c.profiles.map(k => ({ ...profile(k), isDefault: k === c.defaultKey }));
  const picked = selectCheapLLMFromProfiles(
    all as unknown as ConnectionProfile[],
    buildCheapLLMConfig({ cheapLLMSettings: c.cheapLLMSettings as never }),
  );
  if (picked === null) {
    process.stdout.write(
      JSON.stringify({ name: c.name, kind: 'fromProfiles', lines: takeLines(), selection: null }) + '\n',
    );
  } else {
    emit(c.name, 'fromProfiles', picked.selection);
  }
}

for (const c of corpus.uncensored) {
  // The standard selection is built the plain way, exactly as every production
  // caller hands one in.
  const standard = selectionFromProfile(profile(c.standard));
  emit(
    c.name,
    'uncensored',
    resolveUncensoredCheapLLMSelection(
      standard,
      c.isDangerousChat,
      c.concierge === null
        ? undefined
        : resolveConciergeSettings({ conciergeSettings: c.concierge as never }, c.chat as never),
      c.available.map(profile),
    ),
  );
}
