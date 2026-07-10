/**
 * Shared Settings fixture builder (P4.6d) — the test-pepper substrate for the
 * Settings-server-surface differential set (chat-settings, connection profiles,
 * API keys, provider models). SYNTHETIC keys only.
 *
 * Bakes, via v4's REAL repositories (ids/timestamps pinned so both differential
 * sides read identical rows; runtime mutations mint fresh values that the
 * harness normalizes):
 *   - userA (the main user: a chat_settings row, two connection profiles [an
 *     OPENAI default + an ANTHROPIC], two api keys they reference, a roleplay
 *     template, two cached OPENAI provider_models rows),
 *   - userB (a bare user with NO chat_settings row — the GET default-injection /
 *     fresh-row PUT cases).
 *
 * Regenerate (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_SETTINGS_MAIN=/tmp/qt-settings-fixture.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-settings-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

const TEST_PEPPER_BASE64 = 'dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=';
const TS = '2020-01-01T00:00:00.000Z';
const USER_A = '5e100000-0000-4000-8000-000000000001';
const USER_B = '5e100000-0000-4000-8000-000000000002';

// Emit the shared spec so both differential sides agree on ids/pepper.
const SPEC = {
  testPepperBase64: TEST_PEPPER_BASE64,
  userA: USER_A,
  userB: USER_B,
  settingsIdA: '5e200000-0000-4000-8000-000000000001',
  roleplayTemplateId: '5e500000-0000-4000-8000-000000000001',
  apiKeys: {
    openai: '5e300000-0000-4000-8000-000000000001',
    anthropic: '5e300000-0000-4000-8000-000000000002',
  },
  profiles: {
    gpt: '5e400000-0000-4000-8000-000000000001',
    claude: '5e400000-0000-4000-8000-000000000002',
  },
  providerModels: {
    a: '5e600000-0000-4000-8000-000000000001',
    b: '5e600000-0000-4000-8000-000000000002',
  },
};

async function main() {
  const out = process.env.QT_FIXTURE_SETTINGS_MAIN;
  if (!out) throw new Error('set QT_FIXTURE_SETTINGS_MAIN');

  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER_BASE64;
  process.env.LOG_LEVEL = 'error';

  const scratch = mkdtempSync(join(tmpdir(), 'qt-settings-build-'));
  const mainWork = join(scratch, 'main.db');
  process.env.SQLITE_PATH = mainWork;
  // A throwaway mount-index (the store-backed repos are unused here).
  process.env.SQLITE_MOUNT_INDEX_PATH = join(scratch, 'mount.db');

  const { initializeDatabase, ensureCollection, getCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();
  const repos = getRepositories();

  // Users.
  await repos.users.create(
    { username: 'usera', email: null, name: 'User A' } as never,
    { id: USER_A, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.users.create(
    { username: 'userb', email: null, name: 'User B' } as never,
    { id: USER_B, createdAt: TS, updatedAt: TS } as never,
  );

  // chat_settings for userA (userB stays bare).
  await repos.chatSettings.create(
    { userId: USER_A } as never,
    { id: SPEC.settingsIdA, createdAt: TS, updatedAt: TS } as never,
  );

  // Roleplay template (for the defaultRoleplayTemplateId existence check).
  const { RoleplayTemplateSchema } = await import('@/lib/schemas/types');
  await ensureCollection('roleplay_templates', RoleplayTemplateSchema);
  await repos.roleplayTemplates.create(
    { name: 'RP', systemPrompt: 'You are roleplaying.', isBuiltIn: false } as never,
    { id: SPEC.roleplayTemplateId, createdAt: TS, updatedAt: TS } as never,
  );

  // api_keys (pinned ids via direct collection insert), SYNTHETIC values.
  const { ApiKeySchema } = await import('@/lib/schemas/profile.types');
  await ensureCollection('api_keys', ApiKeySchema);
  const apiKeyCol = await getCollection('api_keys');
  await apiKeyCol.insertOne(
    ApiKeySchema.parse({
      id: SPEC.apiKeys.openai,
      userId: USER_A,
      label: 'OpenAI',
      provider: 'OPENAI',
      key_value: 'sk-openai-synthetic-000000000000',
      isActive: true,
      createdAt: TS,
      updatedAt: TS,
    }) as never,
  );
  await apiKeyCol.insertOne(
    ApiKeySchema.parse({
      id: SPEC.apiKeys.anthropic,
      userId: USER_A,
      label: 'Anthropic',
      provider: 'ANTHROPIC',
      key_value: 'sk-ant-synthetic-0000000000000000',
      isActive: true,
      createdAt: '2020-01-02T00:00:00.000Z', // newer → sorts first in the list
      updatedAt: '2020-01-02T00:00:00.000Z',
    }) as never,
  );

  // connection_profiles.
  await repos.connections.create(
    {
      userId: USER_A,
      name: 'GPT',
      provider: 'OPENAI',
      transport: 'api',
      apiKeyId: SPEC.apiKeys.openai,
      baseUrl: null,
      modelName: 'gpt-4o',
      parameters: {},
      isDefault: true,
      isCheap: false,
      sortIndex: 0,
      tags: [],
    } as never,
    { id: SPEC.profiles.gpt, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.connections.create(
    {
      userId: USER_A,
      name: 'Claude',
      provider: 'ANTHROPIC',
      transport: 'api',
      apiKeyId: SPEC.apiKeys.anthropic,
      baseUrl: null,
      modelName: 'claude-sonnet',
      parameters: {},
      isDefault: false,
      isCheap: true,
      sortIndex: 1,
      tags: [],
    } as never,
    { id: SPEC.profiles.claude, createdAt: TS, updatedAt: TS } as never,
  );

  // provider_models — cached OPENAI rows.
  await repos.providerModels.create(
    {
      provider: 'OPENAI',
      modelId: 'gpt-4o',
      modelType: 'chat',
      displayName: 'GPT-4o',
      contextWindow: 128000,
      maxOutputTokens: 16000,
      deprecated: false,
      experimental: false,
    } as never,
    { id: SPEC.providerModels.a, createdAt: TS, updatedAt: TS } as never,
  );
  await repos.providerModels.create(
    {
      provider: 'OPENAI',
      modelId: 'gpt-4o-mini',
      modelType: 'chat',
      displayName: 'GPT-4o mini',
      contextWindow: 128000,
      maxOutputTokens: 16000,
      deprecated: false,
      experimental: false,
    } as never,
    { id: SPEC.providerModels.b, createdAt: TS, updatedAt: TS } as never,
  );

  await closeDatabase();

  // Copy the baked main DB to the requested output.
  const { copyFileSync } = await import('node:fs');
  copyFileSync(mainWork, out);
  rmSync(scratch, { recursive: true, force: true });

  // Emit the spec next to the fixture.
  const __dirname = dirname(fileURLToPath(import.meta.url));
  const { writeFileSync } = await import('node:fs');
  writeFileSync(join(__dirname, 'settings.json'), JSON.stringify(SPEC, null, 2) + '\n');
  process.stderr.write(`built settings fixture: ${out}\n`);
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`settings fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
