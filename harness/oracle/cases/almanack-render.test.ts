/**
 * @jest-environment node
 *
 * P4.37 tier-1 ALMANACK RENDER oracle: drives v4's REAL
 * `renderAlmanackMarkdown` (`lib/tools/almanack/render.ts`) over v4's own
 * committed `AlmanackReportData` fixture plus a family of deliberate variants,
 * and emits BOTH the input data (as JSON) and the rendered markdown per case.
 * The Rust port (`almanack::render::render_almanack_markdown`) deserializes the
 * SAME JSON and byte-compares its output.
 *
 * ── WHY VARIANTS ─────────────────────────────────────────────────────────────
 * v4's fixture populates every field distinctively, which proves the renderer
 * reaches every section — but it takes only ONE side of each branch. A section
 * whose empty arm never renders is structurally invisible to the diff (the
 * P4.D31 lesson). Each variant below flips a named set of branches: empty
 * collections, an unreachable Scriptorium, approximate attribution, pipes and
 * newlines in cells (the `cell()` escape), and the numeric/`null` edges
 * (fractional counts, null latencies, `retentionDays: 0`, `-0` rounding).
 *
 * ── TZ ───────────────────────────────────────────────────────────────────────
 * `formatDate`/`formatDateTime` call `toLocaleDateString()`/`toLocaleString()`
 * with NO arguments, so they read the ambient locale and zone. v4's
 * jest.config.ts pins `process.env.TZ = 'UTC'` before ICU resolves; the recipe
 * below pins it again explicitly (a `beforeAll` pin would be too late).
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-almanack-render-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases"
 *   cp "$V5W/harness/oracle/cases/almanack-render.test.ts" "$TMPO/cases/"
 *   cd ~/source/quilltap-server
 *   TZ=UTC QT_ORACLE_OUT=/tmp/oracle-almanack-render.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- almanack-render
 */

import * as fs from 'fs';

import { makeAlmanackFixture } from '@/__tests__/helpers/almanack/fixture';
import { renderAlmanackMarkdown } from '@/lib/tools/almanack/render';
import type { AlmanackReportData } from '@/lib/tools/almanack/types';

/** Deep clone through JSON — the fixture is plain data by construction. */
function clone(data: AlmanackReportData): AlmanackReportData {
  return JSON.parse(JSON.stringify(data)) as AlmanackReportData;
}

interface Variant {
  name: string;
  build: () => AlmanackReportData;
}

function variants(): Variant[] {
  return [
    // 1. v4's fixture verbatim — every section populated.
    { name: 'base', build: () => makeAlmanackFixture() },

    // 2. Every collection empty and every count zero: the `*None*` /
    //    `*No X*` / `countList` empty arms, plus the conditional blocks that
    //    must NOT render (electron shell, shell capabilities, highest app
    //    version, unknown icons, load errors, themes table, capability table,
    //    histogram, failed jobs, stored dimensions, status rows, folders,
    //    generated images, blob MIME rows, parse failures, by-store).
    {
      name: 'all_empty',
      build: () => {
        const d = clone(makeAlmanackFixture());
        d.runtimeEnvironment.electronShellVersion = null;
        d.runtimeEnvironment.shellCapabilities = [];
        d.databaseSecurity.databases = [];
        d.databaseSecurity.highestAppVersion = null;
        d.backupStatus = [];
        d.migrationState = {
          appliedCount: 0,
          lastMigrationId: null,
          lastMigrationAt: null,
          lastMigrationVersion: null,
        };
        d.plugins = {
          enabled: [],
          disabled: [],
          byCapability: [],
          npmInstalled: 0,
          bundled: 0,
          pluginConfigRows: 0,
          characterPluginDataRows: 0,
        };
        d.apiKeyTypes = [];
        d.providers = [];
        d.modelsByProvider = [];
        d.providerModelCache = [];
        d.apiKeyUsage = [];
        d.cheapLLM = {};
        d.imagePromptLLM = {};
        d.embeddingProvider = {};
        d.imageProviders = [];
        d.embeddingProviders = [];
        d.mcpServers = {
          configured: 0,
          enabled: 0,
          serverNames: [],
          autoReconnect: true,
          maxReconnectAttempts: 5,
        };
        d.themeInfo = {
          activeThemeId: null,
          colorMode: 'system',
          stats: { total: 0, withDarkMode: 0, withCssOverrides: 0, errors: 0 },
          themes: [],
          themesWithIcons: 0,
          totalIconOverrides: 0,
          totalUnknownIconOverrides: 0,
        };
        d.chatBreakdown.byType = [];
        d.chatBreakdown.participantHistogram = [];
        d.autonomousRooms.total = 0;
        d.memoryBreakdown.byKind = [];
        d.memoryBreakdown.bySource = [];
        d.memoryBreakdown.byWitnessedContext = [];
        d.characterBreakdown.vaultless = 0;
        d.instanceSettings.lastMaintenanceSweepAt = null;
        d.backgroundJobs = {
          byStatus: [],
          byType: [],
          failed: [],
          attemptsExhausted: 0,
          oldestPendingScheduledAt: null,
        };
        d.embeddingPipeline.statusByEntityType = [];
        d.embeddingPipeline.storedDimensions = [];
        d.embeddingPipeline.activeProfileDimensions = null;
        d.embeddingPipeline.dimensionMismatch = false;
        d.terminal.distinctShells = [];
        d.storageStats.folders = [];
        d.storageStats.generatedImagesByModel = [];
        d.scriptorium.mountPoints.byKind = [];
        d.scriptorium.mountPoints.scanStatuses = [];
        d.scriptorium.mountPoints.conversionStatuses = [];
        d.scriptorium.mountPoints.wellKnown = [];
        d.scriptorium.content.blobsByMimeType = [];
        d.scriptorium.characterVaults.missingKeystone = 0;
        d.scriptorium.wardrobe = [];
        d.scriptorium.customTools.byStore = [];
        d.scriptorium.customTools.parseFailureDetail = [];
        d.scriptorium.scenarios = [];
        d.personae.topCharacters = [];
        d.personae.projects = [];
        d.personae.groups = [];
        d.wireRecords.byType = [];
        d.wireRecords.connectionProfileLifetime = [];
        d.wireRecords.connectionProfileWindow = [];
        d.wireRecords.imageProfileWindow = [];
        d.wireRecords.cacheByProvider = [];
        d.wireRecords.cacheByProfile = [];
        return d;
      },
    },

    // 3. The mount index could not be opened — the whole Scriptorium section
    //    collapses to its unavailable paragraph.
    {
      name: 'scriptorium_unavailable',
      build: () => {
        const d = clone(makeAlmanackFixture());
        d.scriptorium.available = false;
        return d;
      },
    },

    // 4. Pre-4.9 attribution: the "Attribution is approximate" block renders
    //    and the profile labels are `provider/model` keys.
    {
      name: 'approximate_attribution',
      build: () => {
        const d = clone(makeAlmanackFixture());
        d.wireRecords.exactProfileAttribution = false;
        d.wireRecords.retentionDays = 0; // the "forever" arm
        d.wireRecords.loggingEnabled = false;
        d.wireRecords.connectionProfileWindow = d.wireRecords.connectionProfileWindow.map(r => ({
          ...r,
          label: `${r.provider}/${r.modelName}`,
        }));
        return d;
      },
    },

    // 5. The `cell()` escape: pipes and newlines in every cell-rendered field.
    {
      name: 'pipes_and_newlines',
      build: () => {
        const d = clone(makeAlmanackFixture());
        const dirty = (s: string) => `a|b\nc ${s}`;
        d.databaseSecurity.databases = d.databaseSecurity.databases.map(x => ({
          ...x,
          label: dirty(x.label),
        }));
        d.backupStatus = d.backupStatus.map(x => ({ ...x, label: dirty(x.label) }));
        d.plugins.enabled = d.plugins.enabled.map(p => ({
          ...p,
          title: dirty(p.title),
          version: dirty(p.version),
          capabilities: p.capabilities.map(dirty),
        }));
        d.plugins.byCapability = d.plugins.byCapability.map(c => ({
          ...c,
          capability: dirty(c.capability),
        }));
        d.providers = d.providers.map(p => ({ ...p, displayName: dirty(p.displayName) }));
        d.providerModelCache = d.providerModelCache.map(p => ({
          ...p,
          provider: dirty(p.provider),
        }));
        d.apiKeyUsage = d.apiKeyUsage.map(p => ({ ...p, provider: dirty(p.provider) }));
        d.imageProviders = d.imageProviders.map(p => ({
          ...p,
          displayName: dirty(p.displayName),
          models: p.models.map(dirty),
        }));
        d.embeddingProviders = d.embeddingProviders.map(p => ({
          ...p,
          displayName: dirty(p.displayName),
        }));
        d.themeInfo.themes = d.themeInfo.themes.map(t => ({
          ...t,
          name: dirty(t.name),
          version: dirty(t.version),
          source: dirty(t.source),
        }));
        d.chatBreakdown.byType = d.chatBreakdown.byType.map(r => ({
          ...r,
          chatType: dirty(r.chatType),
        }));
        d.backgroundJobs.failed = d.backgroundJobs.failed.map(r => ({
          ...r,
          type: dirty(r.type),
          lastError: dirty(r.lastError ?? 'x'),
        }));
        d.embeddingPipeline.storedDimensions = d.embeddingPipeline.storedDimensions.map(r => ({
          ...r,
          table: dirty(r.table),
        }));
        d.storageStats.folders = d.storageStats.folders.map(f => ({ ...f, path: dirty(f.path) }));
        d.storageStats.generatedImagesByModel = d.storageStats.generatedImagesByModel.map(r => ({
          ...r,
          generationModel: dirty(r.generationModel),
        }));
        d.scriptorium.mountPoints.byKind = d.scriptorium.mountPoints.byKind.map(r => ({
          ...r,
          mountType: dirty(r.mountType),
          storeType: dirty(r.storeType),
        }));
        d.scriptorium.mountPoints.wellKnown = d.scriptorium.mountPoints.wellKnown.map(r => ({
          ...r,
          label: dirty(r.label),
          settingKey: dirty(r.settingKey),
          name: r.name === null ? null : dirty(r.name),
        }));
        d.scriptorium.content.blobsByMimeType = d.scriptorium.content.blobsByMimeType.map(r => ({
          ...r,
          storedMimeType: dirty(r.storedMimeType),
        }));
        d.scriptorium.wardrobe = d.scriptorium.wardrobe.map(r => ({ ...r, tier: dirty(r.tier) }));
        d.scriptorium.customTools.byStore = d.scriptorium.customTools.byStore.map(r => ({
          ...r,
          store: dirty(r.store),
        }));
        d.scriptorium.customTools.parseFailureDetail =
          d.scriptorium.customTools.parseFailureDetail.map(r => ({
            store: dirty(r.store),
            path: dirty(r.path),
            reason: dirty(r.reason),
          }));
        d.scriptorium.scenarios = d.scriptorium.scenarios.map(r => ({ ...r, tier: dirty(r.tier) }));
        d.personae.topCharacters = d.personae.topCharacters.map(r => ({
          ...r,
          name: dirty(r.name),
        }));
        d.personae.projects = d.personae.projects.map(r => ({ ...r, name: dirty(r.name) }));
        d.personae.groups = d.personae.groups.map(r => ({
          ...r,
          name: dirty(r.name),
          members: r.members.map(dirty),
        }));
        d.wireRecords.byType = d.wireRecords.byType.map(r => ({ ...r, type: dirty(r.type) }));
        d.wireRecords.connectionProfileLifetime = d.wireRecords.connectionProfileLifetime.map(r => ({
          ...r,
          name: dirty(r.name),
          provider: dirty(r.provider),
          modelName: dirty(r.modelName),
        }));
        d.wireRecords.cacheByProvider = d.wireRecords.cacheByProvider.map(r => ({
          ...r,
          label: dirty(r.label),
        }));
        return d;
      },
    },

    // 6. The numeric and `null` edges: fractional counts through
    //    `toLocaleString`, `Math.round` at .5, null/zero latencies, a
    //    dimension mismatch, unknown icons, theme load errors, a missing
    //    keystone, vaultless characters, a store-less group, a model-fetch
    //    error, and a designated profile whose provider is set but whose model
    //    is not (v4 renders the literal `undefined`).
    {
      name: 'numeric_edges',
      build: () => {
        const d = clone(makeAlmanackFixture());
        d.memoryBreakdown.total = 1234.5678;
        d.memoryBreakdown.withOccurredAt = 0.0005;
        d.memoryBreakdown.withNarrativeTime = 12345.6785;
        d.chatStats.totalMessages = 1e21;
        d.chatStats.totalEstimatedCostUSD = 12.3456789;
        d.scriptorium.links.dedupRatio = 1.005;
        d.themeInfo.totalUnknownIconOverrides = 3;
        d.themeInfo.stats.errors = 2;
        d.embeddingPipeline.dimensionMismatch = true;
        d.embeddingPipeline.activeProfileDimensions = 1536;
        d.scriptorium.characterVaults.missingKeystone = 4;
        d.characterBreakdown.vaultless = 7;
        d.personae.countsRemovedParticipants = false;
        d.personae.groups = [
          ...d.personae.groups.map(g => ({ ...g, hasOfficialStore: false, members: [] })),
        ];
        d.personae.projects = d.personae.projects.map(p => ({ ...p, hasState: false }));
        d.modelsByProvider = [
          { provider: 'openai', models: [], error: 'connect ECONNREFUSED' },
          { provider: 'ollama', models: [] },
        ];
        d.wireRecords.byType = d.wireRecords.byType.map((r, i) => ({
          ...r,
          avgDurationMs: i === 0 ? null : 0.5,
        }));
        d.wireRecords.connectionProfileWindow = d.wireRecords.connectionProfileWindow.map(r => ({
          ...r,
          avgDurationMs: null,
          medianDurationMs: 999.4,
        }));
        d.wireRecords.cacheByProvider = d.wireRecords.cacheByProvider.map(r => ({
          ...r,
          rowsWithCacheUsage: 0,
          rowsWithCacheRead: 0,
          requestHitRatio: 0,
          tokenHitRatio: 1 / 3,
        }));
        d.cheapLLM = { provider: 'openai' };
        d.embeddingProvider = { provider: 'openai', model: 'text-embedding-3-small' };
        d.imagePromptLLM = { provider: 'openai', model: 'gpt-5-mini', profileName: 'Prompts' };
        d.imageProviders = d.imageProviders.map(p => ({ ...p, models: [] }));
        d.embeddingProviders = d.embeddingProviders.map(p => ({ ...p, models: [] }));
        d.runtimeEnvironment.uptimeSeconds = 0;
        d.runtimeEnvironment.electronShellVersion = '38.0.0';
        return d;
      },
    },

    // 7. Every optional date null — the `'N/A'` arm of both date formatters —
    //    plus a wardrobe and a scenario table that are present but all-zero
    //    (the `.every(...)` collapse), and an unparseable stamp.
    {
      name: 'null_dates_and_zero_tables',
      build: () => {
        const d = clone(makeAlmanackFixture());
        d.backupStatus = d.backupStatus.map(b => ({ ...b, newestDate: null, oldestDate: null }));
        d.migrationState.lastMigrationAt = null;
        d.migrationState.lastMigrationVersion = null;
        d.providerModelCache = d.providerModelCache.map(p => ({ ...p, newestUpdatedAt: null }));
        d.apiKeyUsage = d.apiKeyUsage.map(p => ({ ...p, lastUsed: null }));
        d.instanceSettings.lastMaintenanceSweepAt = 'not-a-date';
        d.backgroundJobs.oldestPendingScheduledAt = null;
        d.backgroundJobs.failed = d.backgroundJobs.failed.map(r => ({ ...r, lastError: null }));
        d.wireRecords.connectionProfileLifetime = d.wireRecords.connectionProfileLifetime.map(
          r => ({ ...r, lastTouchedAt: null }),
        );
        d.scriptorium.wardrobe = d.scriptorium.wardrobe.map(w => ({ ...w, items: 0, archived: 0 }));
        d.scriptorium.scenarios = d.scriptorium.scenarios.map(s => ({ ...s, count: 0 }));
        d.scriptorium.mountPoints.wellKnown = [
          { label: 'Resolved', settingKey: 'a', mountPointId: 'm1', name: 'Named', resolved: true },
          { label: 'Unnamed', settingKey: 'b', mountPointId: 'm2', name: null, resolved: true },
          { label: 'Missing', settingKey: 'c', mountPointId: 'm3', name: null, resolved: false },
          { label: 'Absent', settingKey: 'd', mountPointId: null, name: null, resolved: false },
        ];
        return d;
      },
    },

    // 8. The V8 legacy space-form stamps (the SQLite `datetime('now')` vintage,
    //    present in real instances): v4's `new Date(...)` parses them as LOCAL
    //    time (UTC under this oracle's pinned TZ) where a strict ISO parser
    //    would render 'N/A'. Both formatters exercised, with and without
    //    fractional seconds (the P4.37 §3 review finding).
    {
      name: 'space_form_date_stamps',
      build: () => {
        const d = clone(makeAlmanackFixture());
        d.generatedAt = '2026-08-05 09:07:03';
        d.migrationState.lastMigrationAt = '2026-08-04 23:59:59.500';
        d.instanceSettings.lastMaintenanceSweepAt = '2026-08-01 00:00:00';
        d.apiKeyUsage = d.apiKeyUsage.map(p => ({ ...p, lastUsed: '2026-07-31 12:30:45' }));
        return d;
      },
    },
  ];
}

async function main(): Promise<void> {
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');
  if (process.env.TZ !== 'UTC') {
    throw new Error(`TZ must be UTC for this oracle (saw ${process.env.TZ ?? '<unset>'})`);
  }

  const lines: string[] = [];
  for (const v of variants()) {
    const data = v.build();
    const markdown = renderAlmanackMarkdown(data);
    lines.push(JSON.stringify({ name: v.name, data, markdown, baseline: 'f7f1a956' }));
  }
  fs.writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`almanack-render oracle wrote ${outPath} (${lines.length} cases)\n`);
}

test('almanack-render oracle', async () => {
  await main();
});
