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
 * P4.D343 (v4 `b15fe9404` + `a8214b253` at the pin `236c04e35`): the
 * re-aim of an IMPORT-DEAD + GARBLED case. `b15fe9404` deleted
 * `@/lib/tools/destructive-tools` (this file's import died at load), and
 * `a8214b253` turned `buildTools`' positional list into ONE options object — the
 * old positional call still compiled under tsx and ran with `options =
 * connectionProfile` (every case a garbled comparison). The case now passes
 * `BuildToolsOptions` whole. The autonomous-room destructive filter is no
 * longer a post-step: the turn's EFFECT CEILING is computed by v4's REAL
 * `resolveTurnEffectCeiling` (`turn-tools.service.ts`) from the case's
 * `chatType` / `runDestructiveToolsAllowed` / `destructiveToolPolicy` and its
 * `storytellerCeiling`, and passed INTO `buildTools`, which applies it last.
 * Each row also carries `effectCeiling` and the two returned effect maps as
 * sorted entry arrays.
 *
 * Run from the v4 server checkout under Node 24 (jest ignores `.claude/`, so
 * the case + spec are mirrored first):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   TMPO=/tmp/qt-oracle-w41g; rm -rf $TMPO; mkdir -p $TMPO/cases $TMPO/fixtures
 *   cp $V5W/harness/oracle/cases/tool-build.test.ts $TMPO/cases/
 *   cp $V5W/harness/oracle/fixtures/tool-build.json $TMPO/fixtures/
 *   cd ~/source/quilltap-server
 *   QT_ORACLE_OUT=/tmp/oracle-tool-build.ndjson \
 *     $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$TMPO/cases" \
 *       --testTimeout=180000 --runTestsByPath "$TMPO/cases/tool-build.test.ts"
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
import { resolveTurnEffectCeiling } from '@/lib/services/chat-message/turn-tools.service';

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
  /** P4.D328 (v4 `e1872c94f`): the positional `canManageInventory` at index 12.
   * Absent → `true` (v4's orchestrator default); `null` reaches v4 as `null`
   * (`inventoryToolFlags(null)` reads ON). */
  canManageInventory?: boolean | null;
  documentEditingEnabled?: boolean;
  /** P4.D216 (v4 `d1c06cd9d`): the positional `docToolsMode` at index 13. Absent
   * → derived from `documentEditingEnabled` the way every v4 caller maps it
   * (`true → 'full'`, else `'off'`). */
  docToolsMode?: 'off' | 'read' | 'full';
  /** P4.D216 (v4 `d1c06cd9d`): `buildTools`' trailing `extras` bag. */
  extras?: { pluginToolAllowlist?: string[]; documentsOnlySearch?: boolean; webSearch?: boolean };
  askCarinaEnabled?: boolean;
  includeWorkspaceTools?: boolean;
  excludeMemorySearch?: boolean;
  sqlAccess?: boolean;
  /** P4.D332 (v4 `ea2cb60ab`): a non-null `customToolContext` — the roster SWITCH.
   * The jest env has no repositories, so the roster resolve fails soft to "no
   * tools": `run_custom` stays withheld while `edit_custom_tool` (which rides
   * the switch, not the roster's contents) is offered. */
  customToolContext?: boolean;
  modelSupportsNativeTools?: boolean;
  providerSupportsWebSearch?: boolean;
  chatType?: string;
  runDestructiveToolsAllowed?: number;
  destructiveToolPolicy?: string;
  /** P4.D343: a storyteller turn's ceiling (its bid's tier), fed through
   * `resolveTurnEffectCeiling` like the orchestrator does. */
  storytellerCeiling?: 'none' | 'read' | 'write' | 'destroy';
}

interface Spec {
  userId: string;
  cases: RawCase[];
}

const sortedEntries = (m: Map<string, string>) => [...m.entries()].sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));

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
    const docToolsMode: 'off' | 'read' | 'full' =
      c.docToolsMode ?? (c.documentEditingEnabled ? 'full' : 'off');

    const effectCeiling = resolveTurnEffectCeiling(
      { id: c.name, chatType: c.chatType ?? 'chat', runDestructiveToolsAllowed: c.runDestructiveToolsAllowed ?? 0 } as never,
      { autonomousRoomSettings: { destructiveToolPolicy: c.destructiveToolPolicy ?? 'opt_in_per_room' } } as never,
      c.storytellerCeiling
    );

    const built = await buildTools({
      connectionProfile: connectionProfile as never,
      userId: spec.userId,
      imageProfileId,
      imageProfile: null, // null so getImageProviderConstraints is not consulted
      projectId: c.projectId ?? undefined,
      requestFullContext: c.requestFullContext ?? false,
      disabledTools,
      disabledToolGroups: c.disabledToolGroups ?? [],
      agentModeEnabled: c.agentModeEnabled ?? false,
      isMultiCharacter: c.isMultiCharacter ?? false,
      helpToolsEnabled: c.helpToolsEnabled ?? false,
      canDressThemselves: c.canDressThemselves ?? true,
      canCreateOutfits: c.canCreateOutfits ?? true,
      canManageInventory: (c.canManageInventory === undefined ? true : c.canManageInventory) as never,
      docToolsMode,
      askCarinaEnabled: c.askCarinaEnabled ?? false,
      includeWorkspaceTools: c.includeWorkspaceTools ?? true,
      excludeMemorySearch: c.excludeMemorySearch ?? false,
      sqlAccess: c.sqlAccess ?? false,
      // customToolContext — the corpus has no Pascal roster; a case may flip the switch.
      customToolContext: c.customToolContext ? ({ userId: spec.userId, chatId: 'chat-tb' } as never) : null,
      ...(c.extras ?? {}),
      effectCeiling,
    });

    const tools = built.tools as unknown[];

    lines.push(
      JSON.stringify({
        kind: 'case',
        name: c.name,
        tools,
        modelSupportsNativeTools: built.modelSupportsNativeTools,
        useNativeWebSearch: built.useNativeWebSearch,
        effectCeiling: effectCeiling ?? null,
        customToolEffects: sortedEntries(built.customToolEffects),
        pluginToolEffects: sortedEntries(built.pluginToolEffects),
      })
    );
  }

  fs.writeFileSync(outPath, lines.join('\n') + '\n');
});
