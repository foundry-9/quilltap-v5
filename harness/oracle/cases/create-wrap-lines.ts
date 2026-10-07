/**
 * Oracle case (P4.163, contract C1 item 3): the per-repository CREATE wrap
 * lines v4 logs when a row is refused at `create` — measured through a
 * `Logger.prototype` spy on v4's REAL repositories, over a freshly
 * initialized test-pepper instance (no fixture).
 *
 * Every repository below wraps the base `_create` in its OWN 3-argument
 * (rethrow) `safeQuery` (`'Error creating <kind>'`, `{userId, …}`), and
 * `_create` is itself a rethrowing `safeQuery` whose `validate` logs `Data
 * validation failed` first — so a schema refusal logs THREE lines, in order:
 * `Data validation failed {collection, error}`, the `_create` line (`Error
 * creating entity`, or the characters repository's `createErrorMessage()`
 * override `Error creating character entity`), and the wrap. `chats.
 * addMessage` is a STANDALONE `safeQuery` (no `collection` injected) whose
 * `ChatEventSchema.parse` throws with no validation line of its own.
 *
 * Each op runs twice: plain, and inside v4's REAL
 * `withStrictRepositoryFailures` (the import runs strict, the restore does
 * not), so `strictFailures: true` is measured where v4 appends it.
 *
 * Per op it emits `{label, strict, threw, logs: [{level, message, fields}]}`
 * with `fields` in v4's own key order. The Rust side
 * (`crates/quilltap-harness/tests/create_wrap_lines_equivalence.rs`) calls
 * v5's `db::fallback` home for the same kind with the same context and the
 * oracle's own `error` text, and compares the whole rendered lines.
 *
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   rm -f /tmp/oracle-create-wrap-lines.ndjson
 *   cd ~/source/quilltap-server
 *   PATH=$N:$PATH npx tsx $V5W/harness/oracle/cases/create-wrap-lines.ts \
 *     > /tmp/oracle-create-wrap-lines.ndjson
 */

import { mkdtempSync, mkdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const TEST_PEPPER = '3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=';
const USER = 'a1000000-0000-4000-8000-000000000001';
const CHAT = 'c1000000-0000-4000-8000-0000000000e7';

type Line = { level: string; message: string; fields: Array<[string, unknown]> };

async function main(): Promise<void> {
  const scratch = mkdtempSync(join(tmpdir(), 'qt-create-wrap-oracle-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });

  process.env.ENCRYPTION_MASTER_PEPPER = TEST_PEPPER;
  process.env.SQLITE_PATH = join(scratch, 'quilltap.db');
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { withStrictRepositoryFailures } = await import(
    '@/lib/database/repositories/strict-failures'
  );
  const { CharactersRepository } = await import('@/lib/database/repositories/characters.repository');
  const { ConnectionProfilesRepository } = await import(
    '@/lib/database/repositories/connection-profiles.repository'
  );
  const { ImageProfilesRepository } = await import(
    '@/lib/database/repositories/image-profiles.repository'
  );
  const { EmbeddingProfilesRepository } = await import(
    '@/lib/database/repositories/embedding-profiles.repository'
  );
  const { FilesRepository } = await import('@/lib/database/repositories/files.repository');
  const { FoldersRepository } = await import('@/lib/database/repositories/folders.repository');
  const { TagsRepository } = await import('@/lib/database/repositories/tags.repository');
  const { RoleplayTemplatesRepository } = await import(
    '@/lib/database/repositories/roleplay-templates.repository'
  );
  const { PromptTemplatesRepository } = await import(
    '@/lib/database/repositories/prompt-templates.repository'
  );
  const { ChatsRepository } = await import('@/lib/database/repositories/chats.repository');

  await initializeDatabase();

  let opLogs: Line[] | null = null;
  const { Logger } = await import('@/lib/logger');
  for (const level of ['error', 'warn'] as const) {
    const original = Logger.prototype[level];
    Logger.prototype[level] = function (
      this: unknown,
      message: string,
      context?: Record<string, unknown>,
      ...rest: unknown[]
    ) {
      // winston drops an `undefined` field, so the line v4 WRITES omits it.
      opLogs?.push({
        level,
        message,
        fields: Object.entries(context ?? {}).filter(([, v]) => v !== undefined),
      });
      return (original as (...a: unknown[]) => void).call(this, message, context, ...rest);
    } as never;
  }

  // Each payload carries the wrap's context fields and FAILS its schema on a
  // field the context does not name (a non-string where a string is required,
  // or a missing required key), so the refusal is Zod's, raised in `validate`.
  const ops: Array<{ label: string; run: () => Promise<unknown> }> = [
    {
      label: 'character',
      run: () => new CharactersRepository().create({ userId: USER, name: 'Abigail', title: 7 } as never),
    },
    {
      label: 'connection_profile',
      run: () =>
        new ConnectionProfilesRepository().create({
          userId: USER,
          name: 'Primary',
          provider: 'OPENAI',
          modelName: 7,
        } as never),
    },
    {
      // `name` absent: winston drops an `undefined` field — the key is OMITTED.
      label: 'connection_profile_nameless',
      run: () =>
        new ConnectionProfilesRepository().create({
          userId: USER,
          provider: 'OPENAI',
          modelName: 7,
        } as never),
    },
    {
      label: 'image_profile',
      run: () =>
        new ImageProfilesRepository().create({
          userId: USER,
          name: 'Painter',
          provider: 'OPENAI',
          modelName: 7,
        } as never),
    },
    {
      label: 'embedding_profile',
      run: () =>
        new EmbeddingProfilesRepository().create({
          userId: USER,
          name: 'Indexer',
          provider: 'OPENAI',
          modelName: 7,
        } as never),
    },
    {
      label: 'file',
      run: () =>
        new FilesRepository().create({
          userId: USER,
          originalFilename: 'notes.md',
          sha256: 7,
        } as never),
    },
    {
      label: 'folder',
      run: () => new FoldersRepository().create({ userId: USER, path: '/Notes/', name: 7 } as never),
    },
    {
      label: 'tag',
      run: () => new TagsRepository().create({ userId: USER, name: 'Lore', visualStyle: 7 } as never),
    },
    {
      label: 'roleplay_template',
      run: () =>
        new RoleplayTemplatesRepository().create({
          userId: USER,
          name: 'Stage',
          systemPrompt: 7,
        } as never),
    },
    {
      label: 'prompt_template',
      run: () =>
        new PromptTemplatesRepository().create({
          userId: USER,
          name: 'Muse',
          content: 7,
        } as never),
    },
    {
      label: 'chat_message_add',
      run: () => new ChatsRepository().addMessage(CHAT, { type: 'message', id: 7 } as never),
    },
  ];

  for (const op of ops) {
    for (const strict of [false, true]) {
      opLogs = [];
      let threw: string | null = null;
      try {
        if (strict) {
          await withStrictRepositoryFailures(op.run);
        } else {
          await op.run();
        }
      } catch (error) {
        threw = error instanceof Error ? error.message : String(error);
      }
      const logs = opLogs;
      opLogs = null;
      process.stdout.write(
        JSON.stringify({ label: op.label, strict, threw: threw !== null, logs }) + '\n',
      );
    }
  }

  await closeDatabase();
}

main().catch((error) => {
  process.stderr.write(`create-wrap-lines oracle failed: ${error?.stack ?? error}\n`);
  process.exit(1);
});
