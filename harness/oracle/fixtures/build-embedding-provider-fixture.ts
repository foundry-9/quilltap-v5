/**
 * Tier-3 fixture builder — the seed state for v4's `generateEmbeddingForUser`
 * (P4.1a deliverable 4, the API-path EmbeddingProvider). Bakes, through v4's
 * REAL repositories, into the MAIN db (QT_FIXTURE_EMBED_MAIN):
 *   - the api_keys rows (minted ids; profiles reference them via apiKeyRef),
 *   - the embedding_profiles rows (PINNED ids — both sides pass them as the
 *     explicit profileId),
 *   - the fitted tfidf_vocabularies row for the BUILTIN default profile,
 *     produced by v4's REAL TfIdfVectorizer.fitCorpus over the spec corpus (so
 *     both sides load identical stored bytes — no ln at query time).
 * The mount-index db (QT_FIXTURE_EMBED_MOUNT) is a throwaway the initializer
 * needs; nothing in it is read.
 *
 * Run (Node 24, from the v4 checkout):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_EMBED_MAIN=/tmp/qt-embed-main.db \
 *   QT_FIXTURE_EMBED_MOUNT=/tmp/qt-embed-mount.db \
 *     $N/node --import tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-embedding-provider-fixture.ts
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface SpecApiKey {
  ref: string;
  user: string;
  provider: string;
  keyValue: string;
}

interface SpecProfile {
  id: string;
  user: string;
  name: string;
  provider: string;
  apiKeyRef?: string;
  apiKeyIdLiteral?: string;
  baseUrl: string | null;
  modelName: string;
  dimensions: number | null;
  truncateToDimensions: number | null;
  normalizeL2: boolean;
  isDefault: boolean;
  fitted?: boolean;
}

interface Spec {
  testPepperBase64: string;
  users: Record<string, string>;
  apiKeys: SpecApiKey[];
  profiles: SpecProfile[];
  tfidfCorpus: string[];
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    readFileSync(join(here, 'embedding-provider-tier3.json'), 'utf8')
  ) as Spec;

  const mainOut = process.env.QT_FIXTURE_EMBED_MAIN;
  const mountOut = process.env.QT_FIXTURE_EMBED_MOUNT;
  if (!mainOut || !mountOut) {
    throw new Error(
      'QT_FIXTURE_EMBED_MAIN and QT_FIXTURE_EMBED_MOUNT must both point at the .db files to write'
    );
  }
  for (const out of [mainOut, mountOut]) {
    for (const suffix of ['', '-journal', '-wal', '-shm']) {
      const p = out + suffix;
      if (existsSync(p)) rmSync(p);
    }
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-embed-fixture-build-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainOut;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountOut;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );

  await initializeDatabase();
  const repos = getRepositories();

  // API keys (minted ids — captured by ref for the profiles below).
  const keyIdByRef = new Map<string, string>();
  for (const k of spec.apiKeys) {
    const created = await repos.connections.createApiKey({
      userId: spec.users[k.user],
      label: `fixture ${k.ref}`,
      provider: k.provider,
      key_value: k.keyValue,
      isActive: true,
      lastUsed: null,
    } as never);
    keyIdByRef.set(k.ref, created.id);
  }

  // Embedding profiles (PINNED ids so both sides pass the same profileId).
  for (const p of spec.profiles) {
    const apiKeyId = p.apiKeyRef
      ? keyIdByRef.get(p.apiKeyRef) ?? null
      : p.apiKeyIdLiteral ?? null;
    if (p.apiKeyRef && !apiKeyId) throw new Error(`unknown apiKeyRef: ${p.apiKeyRef}`);
    await repos.embeddingProfiles.create(
      {
        userId: spec.users[p.user],
        name: p.name,
        provider: p.provider,
        apiKeyId,
        baseUrl: p.baseUrl,
        modelName: p.modelName,
        dimensions: p.dimensions,
        truncateToDimensions: p.truncateToDimensions,
        normalizeL2: p.normalizeL2,
        isDefault: p.isDefault,
        tags: [],
      } as never,
      {
        id: p.id,
        createdAt: '2020-01-01T00:00:00.000Z',
        updatedAt: '2020-01-01T00:00:00.000Z',
      }
    );
  }

  // Fit + persist the BUILTIN vocabulary through v4's REAL vectorizer, so both
  // sides load the identical stored state (vocabulary/idf bytes).
  const fitted = spec.profiles.filter((p) => p.provider === 'BUILTIN' && p.fitted);
  if (fitted.length > 0) {
    const plugin = await import('@/plugins/dist/qtap-plugin-builtin-embeddings');
    const TfIdfVectorizer = (plugin as { TfIdfVectorizer: new (b: boolean) => any })
      .TfIdfVectorizer;
    const vectorizer = new TfIdfVectorizer(true); // include bigrams (the refit default)
    vectorizer.fitCorpus(spec.tfidfCorpus);
    const state = vectorizer.getState();
    if (!state) throw new Error('TF-IDF fit produced no state');
    for (const p of fitted) {
      await repos.tfidfVocabularies.upsertByProfileId(p.id, {
        profileId: p.id,
        userId: spec.users[p.user],
        vocabulary: JSON.stringify(state.vocabulary),
        idf: JSON.stringify(state.idf),
        avgDocLength: state.avgDocLength,
        vocabularySize: state.vocabularySize,
        includeBigrams: state.includeBigrams,
        fittedAt: state.fittedAt,
      } as never);
    }
  }

  closeMountIndexSQLiteClient();
  await closeDatabase();

  process.stderr.write(
    `built embedding-provider fixture: main=${mainOut} mount=${mountOut} ` +
      `(${spec.apiKeys.length} api keys, ${spec.profiles.length} profiles, ` +
      `${fitted.length} fitted BUILTIN)\n`
  );
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`embedding-provider fixture build failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
