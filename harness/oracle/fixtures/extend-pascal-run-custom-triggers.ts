/**
 * P4.D305 (v4 `362426c8f`) — widen the COMMITTED `pascal-run-custom-{main,mount}.db`
 * pair IN PLACE with the trigger-phrase room, through v4's REAL repositories.
 *
 * ## Why a widen and not a rebuild
 *
 * The committed pair has been widened in place by later rounds (P4.D155's
 * columns migrator, P4.D288's span migrations, the refusal-ledger / transcript
 * columns), and none of those reached the builder. A fresh
 * `build-pascal-run-custom-fixture.ts` run at the pin therefore comes out a
 * generateDDL vintage BEHIND the committed bytes (measured 2026-10-16: no
 * `migrations_state` / `migrations_metadata` / `memory_extraction_cursors`, no
 * `chats."transcriptVersion"` / `"moderationRefusalCount"` /
 * `"lastModerationRefusalAt"`, no `chat_settings."dangerousContentSettings"`)
 * AND re-mints every character-vault id, which moves every family that reads
 * the pair. The memory note `rebuilt-fixture-pair-vintage` records the class.
 * The room is additive, so it is added the way the pair has always been
 * widened: on the committed file, through v4's own code. The builder carries
 * the same room (through the shared `pascal-run-custom-triggers.ts`) so a
 * future full rebuild keeps it.
 *
 * Also: CHAR_A's `metadata.json` gains `blessed: true` (appended — read, spread,
 * written back through `storeMountFile`), the sheet the `seer` gate reads.
 *
 * Idempotent: a pair that already carries the trigger room is left alone.
 *
 * Run (Node 24) from a worktree pinned at the round's pin, on COPIES staged
 * under the lane's scratch, then copy the two files back:
 *   cd /tmp/qt-v4-pin-<lane>-25b8fab6a
 *   QT_FIXTURE_CI_MAIN=<staged>/pascal-run-custom-main.db \
 *   QT_FIXTURE_CI_MOUNT=<staged>/pascal-run-custom-mount.db \
 *     npx tsx <V5W>/harness/oracle/fixtures/extend-pascal-run-custom-triggers.ts
 */

import { readFileSync, existsSync, rmSync, mkdtempSync, mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { tmpdir } from 'node:os';

import { addTriggerRoom, CHAT_TRIGGERS } from './pascal-run-custom-triggers';

const CHAR_A = 'a1000000-0000-4000-8000-00000000000a';

interface Spec {
  testPepperBase64: string;
  userId: string;
  seedTimestamp: string;
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, 'pascal-run-custom.json'), 'utf8')) as Spec;
  const TS = spec.seedTimestamp;

  const mainPath = process.env.QT_FIXTURE_CI_MAIN;
  const mountPath = process.env.QT_FIXTURE_CI_MOUNT;
  if (!mainPath || !mountPath || !existsSync(mainPath) || !existsSync(mountPath)) {
    throw new Error('QT_FIXTURE_CI_MAIN and QT_FIXTURE_CI_MOUNT must point at the staged .db files');
  }

  const scratch = mkdtempSync(join(tmpdir(), 'qt-pascal-extend-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.SQLITE_PATH = mainPath;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountPath;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { getRepositories } = await import('@/lib/repositories/factory');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { storeMountFile } = await import('@/lib/mount-index/store-file');

  await initializeDatabase();
  const repos = getRepositories();

  if (await repos.chats.findById(CHAT_TRIGGERS)) {
    process.stderr.write('pascal-run-custom pair already carries the trigger room — unchanged\n');
  } else {
    const writeVaultFile = async (mountPointId: string, relativePath: string, body: unknown) => {
      await storeMountFile({
        mountPointId,
        relativePath,
        data: Buffer.from(JSON.stringify(body, null, 2), 'utf8'),
        originalMimeType: 'application/json',
        transcodeImages: false,
        extractText: false,
        enqueueEmbedding: false,
        force: true,
      } as never);
    };
    const mkParticipant = (id: string, characterId: string, controlledBy: string) => ({
      id,
      type: 'CHARACTER',
      characterId,
      controlledBy,
      status: 'active',
      connectionProfileId: null,
      createdAt: TS,
      updatedAt: TS,
    });

    const charA = (await repos.characters.findById(CHAR_A)) as {
      metadata?: Record<string, unknown>;
      characterDocumentMountPointId?: string | null;
    } | null;
    if (!charA?.characterDocumentMountPointId) throw new Error('CHAR_A vault missing');
    await writeVaultFile(charA.characterDocumentMountPointId, 'metadata.json', {
      ...(charA.metadata ?? {}),
      blessed: true,
    });

    await addTriggerRoom({ repos, writeVaultFile, userId: spec.userId, ts: TS, mkParticipant });
    process.stderr.write(`widened pascal-run-custom pair with the trigger room: main=${mainPath}\n`);
  }

  // Let v4's un-awaited post-write tails (the mount point's `refreshStats`)
  // land BEFORE the connections close (memory note
  // `oracle-async-tails-and-jest-mocks`).
  await new Promise((r) => setTimeout(r, 500));
  closeMountIndexSQLiteClient();
  await closeDatabase();
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`pascal-run-custom trigger widen failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
