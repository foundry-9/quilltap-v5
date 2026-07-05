/**
 * @jest-environment node
 *
 * W4.1g `tool_build_equivalence` ORACLE — drives v4's REAL `buildTools`
 * (`lib/services/chat-message/streaming.service.ts`) + the built-in half of
 * `buildToolsForProvider` (`lib/tools/plugin-tool-builder.ts`) over the flag
 * matrix in fixtures/tool-build.json, emitting the returned
 * `{ tools, modelSupportsNativeTools, useNativeWebSearch }` per case. The Rust
 * `services::tool_build::build_tools` matches byte-exact (tools compared as JSON
 * arrays; canonicalized/sorted by name).
 *
 * Injected/stubbed seams (mirroring the Rust `BuildToolsInput` fields):
 *   - `checkModelSupportsTools` (@/lib/tools) → the case's `modelSupportsNativeTools`;
 *   - `createLLMProvider` (@/lib/llm) → `{ supportsWebSearch }` (the provider
 *     capability). `getProvider` (provider-registry) is NOT mocked — the jest
 *     env registers no provider plugins, so it returns null and `buildTools`
 *     emits the canonical (universal/OpenAI) shape (the W4.7 provider-format
 *     reshape is deferred). `getImageProviderConstraints` is likewise null →
 *     the base image tool. `getRepositories()` is the jest.setup `jest.fn()`
 *     (returns undefined → the plugin-config read throws → caught → empty map),
 *     so no real DB is needed (plugin tools deferred, contribute nothing).
 *
 * The autonomous-room destructive-tool filter lives in the orchestrator (after
 * buildTools), not in buildTools itself; this oracle replicates that exact
 * post-filter for the `chatType === 'autonomous'` cases, matching the Rust
 * harness (which runs `tool_build::filter_destructive_tools`).
 *
 * Run from the v4 server checkout under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   V5=~/source/quilltap-v5
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-tool-build.ndjson \
 *     $N/npx jest --silent --watchman=false \
 *       --roots "$PWD" --roots "$V5/harness/oracle/cases" -- tool-build
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

// Mock only the leaf where `checkModelSupportsTools` lives (mocking the whole
// `@/lib/tools` barrel triggers a circular-init with plugin-tool-builder). The
// barrel re-exports it from this leaf, so buildTools() picks up the mock.
// `createLLMProvider` (@/lib/llm) is ALREADY a jest.fn() from jest.setup.
jest.mock('@/lib/tools/pseudo-tool-support', () => ({
  __esModule: true,
  ...jest.requireActual('@/lib/tools/pseudo-tool-support'),
  checkModelSupportsTools: jest.fn(),
}));

import { buildTools } from '@/lib/services/chat-message/streaming.service';
import { checkModelSupportsTools } from '@/lib/tools/pseudo-tool-support';
import { createLLMProvider } from '@/lib/llm';
import { DESTRUCTIVE_TOOL_NAMES } from '@/lib/tools/destructive-tools';

interface RawCase {
  name: string;
  provider?: string;
  modelName?: string;
  useNativeWebSearch?: boolean;
  allowToolUse?: boolean | null;
  allowWebSearch?: boolean;
  imageProfileId?: string | null;
  projectId?: string | null;
  requestFullContext?: boolean;
  disabledTools?: string[];
  disabledToolsUndefined?: boolean;
  disabledToolGroups?: string[];
  agentModeEnabled?: boolean;
  isMultiCharacter?: boolean;
  helpToolsEnabled?: boolean;
  canDressThemselves?: boolean;
  canCreateOutfits?: boolean;
  documentEditingEnabled?: boolean;
  askCarinaEnabled?: boolean;
  includeWorkspaceTools?: boolean;
  excludeMemorySearch?: boolean;
  sqlAccess?: boolean;
  modelSupportsNativeTools?: boolean;
  providerSupportsWebSearch?: boolean;
  chatType?: string;
  runDestructiveToolsAllowed?: number;
  destructiveToolPolicy?: string;
}

interface Spec {
  userId: string;
  cases: RawCase[];
}

function toolName(tool: unknown): string | undefined {
  const t = tool as { function?: { name?: string }; name?: string };
  return t.function?.name || t.name;
}

test('tool-build oracle', async () => {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'tool-build.json'), 'utf8')
  ) as Spec;
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const lines: string[] = [];

  for (const c of spec.cases) {
    const modelSupports = c.modelSupportsNativeTools ?? true;
    const providerSupportsWebSearch = c.providerSupportsWebSearch ?? false;
    jest.mocked(checkModelSupportsTools).mockResolvedValue(modelSupports);
    jest
      .mocked(createLLMProvider)
      .mockResolvedValue({ supportsWebSearch: providerSupportsWebSearch } as never);

    const connectionProfile = {
      provider: c.provider ?? 'OPENAI',
      modelName: c.modelName ?? 'gpt-4o',
      useNativeWebSearch: c.useNativeWebSearch ?? false,
      allowToolUse: c.allowToolUse === undefined ? undefined : c.allowToolUse,
      allowWebSearch: c.allowWebSearch ?? false,
      baseUrl: null,
    };
    const imageProfileId = c.imageProfileId ?? null;
    const disabledTools = c.disabledToolsUndefined ? undefined : c.disabledTools ?? [];

    const built = await buildTools(
      connectionProfile as never,
      imageProfileId,
      null, // imageProfile — null so getImageProviderConstraints is not consulted
      spec.userId,
      c.projectId ?? undefined,
      c.requestFullContext ?? false,
      disabledTools,
      c.disabledToolGroups ?? [],
      c.agentModeEnabled ?? false,
      c.isMultiCharacter ?? false,
      c.helpToolsEnabled ?? false,
      c.canDressThemselves ?? true,
      c.canCreateOutfits ?? true,
      c.documentEditingEnabled ?? false,
      c.askCarinaEnabled ?? false,
      c.includeWorkspaceTools ?? true,
      c.excludeMemorySearch ?? false,
      c.sqlAccess ?? false
    );

    let tools = built.tools as unknown[];

    // Autonomous-room destructive filter (orchestrator post-step).
    if ((c.chatType ?? 'chat') === 'autonomous') {
      const policy = c.destructiveToolPolicy ?? 'opt_in_per_room';
      const allowedAtRoom = (c.runDestructiveToolsAllowed ?? 0) === 1;
      const destructiveAllowed = policy !== 'always_refuse' && allowedAtRoom;
      if (!destructiveAllowed) {
        tools = tools.filter((t) => {
          const name = toolName(t);
          return !name || !DESTRUCTIVE_TOOL_NAMES.has(name);
        });
      }
    }

    lines.push(
      JSON.stringify({
        kind: 'case',
        name: c.name,
        tools,
        modelSupportsNativeTools: built.modelSupportsNativeTools,
        useNativeWebSearch: built.useNativeWebSearch,
      })
    );
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
});
