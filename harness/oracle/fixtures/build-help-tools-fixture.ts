/**
 * Fixture builder (W4.1d batch 1) — the shared starting DB for the help_settings
 * tool read-differential. Seeds FIVE user-scoped main-DB tables that
 * `help_settings` reads (chat_settings + connection_profiles +
 * embedding_profiles + image_profiles + roleplay_templates) from the committed
 * spec (`help-tools-tier2.json`), each via v4's REAL repository `.create(data,
 * { id, createdAt, updatedAt })` (ids + timestamps pinned → fully
 * deterministic). Each table is created by v4's own `ensureCollection(name,
 * Schema)`, so the DDL is identical to production.
 *
 * Both the oracle (cases/help-tools.ts) and the Rust port
 * (help_tools_equivalence.rs) READ a COPY of this same baked file, so every cell
 * is identical on both sides and the help_settings reads compare with ZERO
 * normalization.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_HELP=/tmp/qt-help.db \
 *     $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-help-tools-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Row {
  id: string;
  createdAt: string;
  updatedAt: string;
  [k: string]: unknown;
}
interface Spec {
  testPepperBase64: string;
  chatSettings: Row[];
  connectionProfiles: Row[];
  embeddingProfiles: Row[];
  imageProfiles: Row[];
  roleplayTemplates: Row[];
}

/** Strip the `$comment`/id/createdAt/updatedAt keys and return {data, options}. */
function split(row: Row): { data: Record<string, unknown>; options: { id: string; createdAt: string; updatedAt: string } } {
  const { id, createdAt, updatedAt, $comment: _c, ...data } = row as Row & { $comment?: unknown };
  return { data, options: { id, createdAt, updatedAt } };
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const specPath = join(here, 'help-tools-tier2.json');
  const spec = JSON.parse(readFileSync(specPath, 'utf8')) as Spec;

  const out = process.env.QT_FIXTURE_HELP;
  if (!out) {
    throw new Error('QT_FIXTURE_HELP must point at the fixture .db to write');
  }
  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = out + suffix;
    if (existsSync(p)) rmSync(p);
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-help-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = out;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, ensureCollection, closeDatabase } = await import(
    '@/lib/database/manager'
  );
  const { ChatSettingsRepository } = await import(
    '@/lib/database/repositories/chat-settings.repository'
  );
  const { ConnectionProfilesRepository } = await import(
    '@/lib/database/repositories/connection-profiles.repository'
  );
  const { EmbeddingProfilesRepository } = await import(
    '@/lib/database/repositories/embedding-profiles.repository'
  );
  const { ImageProfilesRepository } = await import(
    '@/lib/database/repositories/image-profiles.repository'
  );
  const { RoleplayTemplatesRepository } = await import(
    '@/lib/database/repositories/roleplay-templates.repository'
  );
  const { ChatSettingsSchema, EmbeddingProfileSchema, ImageProfileSchema } = await import(
    '@/lib/schemas/types'
  );
  const { ConnectionProfileSchema } = await import('@/lib/schemas/profile.types');
  const { RoleplayTemplateSchema } = await import('@/lib/schemas/template.types');

  await initializeDatabase();
  await ensureCollection('chat_settings', ChatSettingsSchema);
  await ensureCollection('connection_profiles', ConnectionProfileSchema);
  await ensureCollection('embedding_profiles', EmbeddingProfileSchema);
  await ensureCollection('image_profiles', ImageProfileSchema);
  await ensureCollection('roleplay_templates', RoleplayTemplateSchema);

  const chatSettings = new ChatSettingsRepository();
  const connections = new ConnectionProfilesRepository();
  const embeddingProfiles = new EmbeddingProfilesRepository();
  const imageProfiles = new ImageProfilesRepository();
  const roleplayTemplates = new RoleplayTemplatesRepository();

  for (const r of spec.chatSettings) {
    const { data, options } = split(r);
    await chatSettings.create(data as never, options);
  }
  for (const r of spec.connectionProfiles) {
    const { data, options } = split(r);
    await connections.create(data as never, options);
  }
  for (const r of spec.embeddingProfiles) {
    const { data, options } = split(r);
    await embeddingProfiles.create(data as never, options);
  }
  for (const r of spec.imageProfiles) {
    const { data, options } = split(r);
    await imageProfiles.create(data as never, options);
  }
  for (const r of spec.roleplayTemplates) {
    const { data, options } = split(r);
    await roleplayTemplates.create(data as never, options);
  }

  await closeDatabase();
  process.stderr.write(
    `built help-tools fixture: ${out} (${spec.chatSettings.length} settings, ` +
      `${spec.connectionProfiles.length} conn, ${spec.embeddingProfiles.length} emb, ` +
      `${spec.imageProfiles.length} img, ${spec.roleplayTemplates.length} rp)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`help-tools fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
