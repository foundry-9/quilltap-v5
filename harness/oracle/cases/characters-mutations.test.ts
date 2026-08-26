/**
 * @jest-environment node
 *
 * P4.6f (slice 4) characters MUTATIONS ORACLE: drives v4's REAL create /
 * quick-create (`characters/handlers/post.ts`) and update
 * (`characters/[id]/handlers/put.ts`) handlers over a FRESH copy of the committed
 * characters fixture per case, and emits each response body (the create/update
 * ECHO). The Rust ports (`api::characters::{character_create,
 * character_quick_create, character_update}`) are diffed against these echoes.
 *
 * The echo is a full OVERLAY re-read of the freshly-written vault
 * (v4 `create` reloads via `findById`; `update` returns
 * `applyDocumentStoreOverlayOne`), so the echo diff transitively proves the vault
 * round-trip in composition — every managed field (identity/description/
 * scenarios/systemPrompts/physicalDescription/…) is read back from the vault the
 * handler just wrote. The raw storage rows themselves are already byte-proven by
 * the standing `characters_create_tier2` / `vault_character_update` tier-2
 * differentials, so they are not re-dumped here.
 *
 * Minted ids + timestamps (the created character id, its mount-point FK, and the
 * per-array scenario/prompt/physical ids/timestamps) are position-blanked on both
 * sides (keys `id` / `createdAt` / `updatedAt` / `characterDocumentMountPointId`);
 * everything else is diffed byte-exact.
 *
 * Run (Node 24, from the v4 checkout — cp to a /tmp mirror; jest ignores .claude/):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<this worktree>
 *   TMPO=/tmp/qt-characters-oracle
 *   rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
 *   cp "$V5W/harness/oracle/cases/characters-mutations.test.ts" "$TMPO/cases/"
 *   cp "$V5W/harness/oracle/fixtures/characters.json"           "$TMPO/fixtures/"
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_CHARACTERS_MAIN=$V5W/crates/quilltap-web/tests/fixtures/characters-main.db \
 *   QT_FIXTURE_CHARACTERS_MOUNT=$V5W/crates/quilltap-web/tests/fixtures/characters-mount.db \
 *   QT_ORACLE_OUT=/tmp/oracle-characters-mutations.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=120000 \
 *       --roots "$PWD" --roots "$TMPO/cases" -- characters-mutations
 */

import * as fs from 'fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { mkdtempSync, mkdirSync, copyFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';

interface Spec {
  testPepperBase64: string;
  userId: string;
}

const ARIA = 'a1000000-0000-4000-8000-000000000001';
/** P4.D63: the ARCHIVED character (fixture extension) the write guard refuses. */
const FENN = 'a1000000-0000-4000-8000-000000000006';
const CONN = 'c0000001-0000-4000-8000-000000000001';
const ADVENTURE = '70000001-0000-4000-8000-000000000001';
const MYSTERY = '70000002-0000-4000-8000-000000000002';
/** Frozen instant for the photo-save case so the minted keptAt / relativePath /
 *  kept-image markdown are deterministic on both differential sides. */
const FIXED_KEPT_AT = '2026-04-01T12:00:00.000Z';

interface CaseSpec {
  name: string;
  kind:
    | 'create'
    | 'quick-create'
    | 'update'
    | 'update-seq'
    | 'wardrobe-create'
    | 'wardrobe-get'
    | 'wardrobe-update'
    | 'wardrobe-delete'
    | 'tag-list'
    | 'tag-get'
    | 'tag-create'
    | 'tag-update'
    | 'tag-delete'
    | 'depiction-get'
    | 'depiction-put'
    | 'st-import'
    | 'photo-remove'
    | 'photo-save'
    | 'photo-save-body'
    | 'character-delete'
    | 'archive-guard-repo'
    | 'archive-guard-wardrobe';
  id?: string;
  /** For wardrobe item ops: discover the baked item id by this title. */
  itemTitle?: string;
  /** For photo-remove: discover the vault link id by this relativePath. */
  photoRelativePath?: string;
  /** P4.60 (`photo-save-body`): POST this body VERBATIM to the photos route,
   *  substituting the discovered `photoRelativePath` link id wherever the body
   *  carries the string `<link>`. Records `{status, body}` plus the photos/
   *  link dump, so a refusal proves it wrote NOTHING. */
  rawBody?: unknown;
  body?: unknown;
  /** For `update-seq`: apply each PUT body in order over the same fresh DB; the
   *  FINAL PUT response (an overlay re-read) is the echo. Used to prove the
   *  explicit-null tri-state clear (set non-null, then null). */
  bodies?: unknown[];
}

/** Dump the mount-index photo GC tables + the character's defaultImageId — the
 *  DELETE-with-GC verification (baked ids identical on both sides → no remap). */
function dumpPhotoTables(
  mountDb: { prepare: (s: string) => { all: (...a: unknown[]) => unknown } },
  mainDb: { prepare: (s: string) => { get: (...a: unknown[]) => unknown } },
  mountPointId: string,
): unknown {
  return {
    links: mountDb
      .prepare('SELECT id, relativePath FROM doc_mount_file_links WHERE mountPointId = ? ORDER BY id')
      .all(mountPointId),
    files: mountDb.prepare('SELECT id, sha256 FROM doc_mount_files ORDER BY id').all(),
    blobs: mountDb.prepare('SELECT fileId FROM doc_mount_blobs ORDER BY fileId').all(),
    character: mainDb
      .prepare(`SELECT id, defaultImageId FROM characters WHERE id = '${ARIA}'`)
      .get(),
  };
}

const TAGGABLE_TABLES = [
  'characters',
  'chats',
  'connection_profiles',
  'image_profiles',
  'embedding_profiles',
  'files',
];

/** Dump each taggable table's (id, tags) + the tags table (id, name) — the
 *  DELETE fan-out verification. */
function dumpTagTables(getRawDatabase: () => { prepare: (s: string) => { all: () => unknown } }): unknown {
  const raw = getRawDatabase();
  const dump: Record<string, unknown> = {};
  for (const t of TAGGABLE_TABLES) {
    dump[t] = raw.prepare(`SELECT id, tags FROM ${t} ORDER BY id`).all();
  }
  dump.tags = raw.prepare('SELECT id, name FROM tags ORDER BY id').all();
  return dump;
}

/** Dump every cascade-touched table (baked ids identical on both sides → no
 *  remap): the character/chat/message/memory/plugin rows (MAIN) + the vault
 *  link/file/blob rows (MOUNT-INDEX). */
function dumpCascadeTables(
  mainDb: { prepare: (s: string) => { all: (...a: unknown[]) => unknown } },
  mountDb: { prepare: (s: string) => { all: (...a: unknown[]) => unknown } },
): unknown {
  return {
    characters: mainDb.prepare('SELECT id FROM characters ORDER BY id').all(),
    chats: mainDb.prepare('SELECT id FROM chats ORDER BY id').all(),
    messages: mainDb.prepare('SELECT id, chatId FROM chat_messages ORDER BY id').all(),
    memories: mainDb.prepare('SELECT id, characterId FROM memories ORDER BY id').all(),
    pluginData: mainDb
      .prepare('SELECT id, characterId FROM character_plugin_data ORDER BY id')
      .all(),
    links: mountDb
      .prepare('SELECT id, mountPointId, relativePath FROM doc_mount_file_links ORDER BY id')
      .all(),
    files: mountDb.prepare('SELECT id, sha256 FROM doc_mount_files ORDER BY id').all(),
    blobs: mountDb.prepare('SELECT fileId FROM doc_mount_blobs ORDER BY fileId').all(),
  };
}

function mockRequest(url: string, body: unknown): unknown {
  return {
    method: 'POST',
    url,
    nextUrl: new URL(url),
    headers: new Headers({ 'Content-Type': 'application/json' }),
    json: jest.fn().mockResolvedValue(body),
  };
}

async function runCase(
  spec: Spec,
  c: CaseSpec,
  scratch: string,
  fixtures: { main: string; mount: string },
): Promise<Record<string, unknown>> {
  jest.resetModules();

  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  // The update handler calls `revalidatePath('/')` (Next.js ISR cache), which
  // throws outside a Next request context (jest) — a harness artifact, not a v4
  // behavior. Stub it to a no-op so the handler completes normally.
  jest.doMock(
    'next/cache',
    () => ({
      __esModule: true,
      revalidatePath: () => {},
      revalidateTag: () => {},
    }),
    { virtual: true },
  );
  jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
  jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
  jest.doMock('@/lib/database/repositories', () =>
    jest.requireActual('@/lib/database/repositories'),
  );
  jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
  jest.doMock('@/lib/embedding/vector-store', () =>
    jest.requireActual('@/lib/embedding/vector-store'),
  );
  // Un-mock the character-vault-bridge (jest.setup stubs it) so the photo
  // gallery resolves the REAL vault mount point against the real DB.
  jest.doMock('@/lib/file-storage/character-vault-bridge', () =>
    jest.requireActual('@/lib/file-storage/character-vault-bridge'),
  );
  jest.doMock('@/lib/auth/session', () => ({
    __esModule: true,
    ...jest.requireActual('@/lib/auth/session'),
    getServerSession: async () => ({ user: { id: spec.userId } }),
  }));
  jest.doMock('@/lib/startup/startup-state', () => {
    const actual = jest.requireActual('@/lib/startup/startup-state');
    return {
      __esModule: true,
      ...actual,
      startupState: {
        ...actual.startupState,
        isReady: () => true,
        waitForReady: async () => true,
        isPepperResolved: () => true,
        getPepperState: () => 'resolved',
        getPhase: () => 'ready',
        isLockedMode: () => false,
      },
    };
  });

  const work = mkdtempSync(join(scratch, 'chars-'));
  const mainWork = join(work, 'main.db');
  const mountWork = join(work, 'mount.db');
  copyFileSync(fixtures.main, mainWork);
  copyFileSync(fixtures.mount, mountWork);
  process.env.SQLITE_PATH = mainWork;
  process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;

  const { initializeDatabase, closeDatabase } = await import('@/lib/database/manager');
  const { closeMountIndexSQLiteClient } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRepositories } = await import('@/lib/repositories/factory');

  await initializeDatabase();

  try {
    let status: number;
    let body: unknown;
    let tables: unknown;
    if (c.kind === 'archive-guard-repo') {
      // P4.D63 — the archive write guard at the REPOSITORY, which is where v4
      // puts it (`validateCharacterArchivePatch` at the top of `update()`).
      // The PUT route cannot reach it with an `archivedAt` key (its Zod schema
      // strips unknown keys), so both the refusal and the one sanctioned patch
      // are driven through the repo directly — the same level v5's port sits at.
      const { getRepositories } = await import('@/lib/repositories/factory');
      const repos = getRepositories();
      const outcomes: unknown[] = [];
      for (const patch of (c.bodies ?? []) as Array<Record<string, unknown>>) {
        try {
          const updated = await repos.characters.update(FENN, patch as never);
          outcomes.push({ ok: true, name: updated?.name ?? null, archivedAt: updated?.archivedAt ?? null });
        } catch (err) {
          outcomes.push({
            ok: false,
            name: err instanceof Error ? err.name : 'unknown',
            message: err instanceof Error ? err.message : String(err),
          });
        }
      }
      // Delete is v4's deliberate escape hatch: a tombstone can always be
      // destroyed, so this must succeed where every edit above was refused.
      const deleted = await repos.characters.delete(FENN);
      const afterDelete = await repos.characters.findByIdRaw(FENN);
      return {
        name: c.name,
        status: 200,
        body: { outcomes, deleted, gone: afterDelete === null },
      };
    }

    if (c.kind === 'archive-guard-wardrobe') {
      // P4.D63 — `resolveWardrobeMount` THROWS for an archived character; a
      // null return would fall through to the legacy DB write and silently
      // mutate the tombstone's wardrobe.
      const { getRepositories } = await import('@/lib/repositories/factory');
      const repos = getRepositories();
      let outcome: unknown;
      try {
        const created = await repos.wardrobe.create({
          characterId: FENN,
          title: 'Contraband Cloak',
          description: null,
          imagePrompt: null,
          types: ['outerwear'],
          componentItemIds: [],
          appropriateness: null,
          isDefault: false,
          replace: false,
          migratedFromClothingRecordId: null,
        } as never);
        outcome = { ok: true, id: created?.id ? '<minted>' : null };
      } catch (err) {
        outcome = {
          ok: false,
          name: err instanceof Error ? err.name : 'unknown',
          message: err instanceof Error ? err.message : String(err),
        };
      }
      const items = await repos.wardrobe.findByCharacterId(FENN);
      return {
        name: c.name,
        status: 200,
        body: { outcome, titles: (items ?? []).map((i: { title: string }) => i.title).sort() },
      };
    }

    if (c.kind === 'character-delete') {
      // DELETE /api/v1/characters/[id]?cascadeChats=true&cascadeImages=true.
      const url = `http://localhost/api/v1/characters/${ARIA}?cascadeChats=true&cascadeImages=true`;
      const { DELETE } = (await import('@/app/api/v1/characters/[id]/route')) as {
        DELETE: (...a: unknown[]) => Promise<unknown>;
      };
      const response = (await DELETE(mockRequest(url, undefined), {
        params: Promise.resolve({ id: ARIA }),
      })) as { status: number; json: () => Promise<unknown> };
      status = response.status;
      body = await response.json();
      const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
      const { getRawMountIndexDatabase } = await import(
        '@/lib/database/backends/sqlite/mount-index-client'
      );
      tables = dumpCascadeTables(getRawDatabase() as never, getRawMountIndexDatabase() as never);
      return { name: c.name, status, body, tables };
    }
    if (c.kind === 'photo-save') {
      // POST /api/v1/characters/[id]/photos {linkId}. Save the source vault link
      // (the avatar) into photos/. Date is frozen (see FIXED_KEPT_AT) so the
      // minted keptAt / relativePath / kept-image markdown are deterministic.
      const RealDate = Date;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      global.Date = class extends RealDate {
        constructor(...a: unknown[]) {
          if (a.length === 0) super(FIXED_KEPT_AT);
          // @ts-expect-error forward variadic args
          else super(...a);
        }
        static now(): number {
          return RealDate.parse(FIXED_KEPT_AT);
        }
      } as unknown as DateConstructor;
      try {
        const { getCharacterVaultStore } = await import(
          '@/lib/file-storage/character-vault-bridge'
        );
        const vault = await getCharacterVaultStore(ARIA);
        if (!vault) throw new Error('Aria vault did not resolve');
        const allLinks = await getRepositories().docMountFileLinks.findByMountPointId(
          vault.mountPointId,
        );
        const source = allLinks.find(
          (l: { relativePath: string }) => l.relativePath === c.photoRelativePath,
        );
        if (!source) throw new Error(`source link ${c.photoRelativePath} not found`);
        const url = `http://localhost/api/v1/characters/${ARIA}/photos`;
        const { POST } = (await import('@/app/api/v1/characters/[id]/photos/route')) as {
          POST: (...a: unknown[]) => Promise<unknown>;
        };
        const req = {
          method: 'POST',
          url,
          nextUrl: new URL(url),
          headers: new Headers({ 'content-type': 'application/json' }),
          json: jest.fn().mockResolvedValue({ linkId: (source as { id: string }).id }),
        };
        const response = (await POST(req, { params: Promise.resolve({ id: ARIA }) })) as {
          status: number;
          json: () => Promise<unknown>;
        };
        status = response.status;
        body = await response.json();
        // Dump the freshly-written photos/ link straight from the raw column
        // (deterministic under the frozen clock; the link id is not selected —
        // it's the only minted value).
        const { getRawMountIndexDatabase } = await import(
          '@/lib/database/backends/sqlite/mount-index-client'
        );
        const midb = getRawMountIndexDatabase() as unknown as {
          prepare: (s: string) => { all: (...a: unknown[]) => unknown };
        };
        const savedLinks = midb
          .prepare(
            "SELECT relativePath, fileId, originalMimeType, extractedText, description " +
              "FROM doc_mount_file_links WHERE mountPointId = ? AND relativePath LIKE 'photos/%' " +
              'ORDER BY relativePath',
          )
          .all(vault.mountPointId);
        tables = { savedLinks };
      } finally {
        global.Date = RealDate;
      }
      return { name: c.name, status, body, tables };
    }
    if (c.kind === 'photo-save-body') {
      // P4.60: POST an arbitrary body at the REAL photos route and record what
      // v4's `saveByIdSchema.safeParse` answers. The route joins its issue
      // messages with '; ' into `badRequest`, so these sentences are wire
      // payload, not just a status. The photos/ link dump rides along: a
      // refusal must write NOTHING.
      const RealDate = Date;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      global.Date = class extends RealDate {
        constructor(...a: unknown[]) {
          if (a.length === 0) super(FIXED_KEPT_AT);
          // @ts-expect-error forward variadic args
          else super(...a);
        }
        static now(): number {
          return RealDate.parse(FIXED_KEPT_AT);
        }
      } as unknown as DateConstructor;
      try {
        const { getCharacterVaultStore } = await import(
          '@/lib/file-storage/character-vault-bridge'
        );
        const vault = await getCharacterVaultStore(ARIA);
        if (!vault) throw new Error('Aria vault did not resolve');
        const allLinks = await getRepositories().docMountFileLinks.findByMountPointId(
          vault.mountPointId,
        );
        let payload = c.rawBody;
        if (c.photoRelativePath) {
          const source = allLinks.find(
            (l: { relativePath: string }) => l.relativePath === c.photoRelativePath,
          );
          if (!source) throw new Error(`source link ${c.photoRelativePath} not found`);
          payload = JSON.parse(
            JSON.stringify(c.rawBody).replace('"<link>"', JSON.stringify((source as { id: string }).id)),
          );
        }
        const url = `http://localhost/api/v1/characters/${ARIA}/photos`;
        const { POST } = (await import('@/app/api/v1/characters/[id]/photos/route')) as {
          POST: (...a: unknown[]) => Promise<unknown>;
        };
        const req = {
          method: 'POST',
          url,
          nextUrl: new URL(url),
          headers: new Headers({ 'content-type': 'application/json' }),
          json: jest.fn().mockResolvedValue(payload),
        };
        const response = (await POST(req, { params: Promise.resolve({ id: ARIA }) })) as {
          status: number;
          json: () => Promise<unknown>;
        };
        status = response.status;
        body = await response.json();
        const { getRawMountIndexDatabase } = await import(
          '@/lib/database/backends/sqlite/mount-index-client'
        );
        const midb = getRawMountIndexDatabase() as unknown as {
          prepare: (s: string) => { all: (...a: unknown[]) => unknown };
        };
        const savedLinks = midb
          .prepare(
            "SELECT relativePath, fileId, originalMimeType, extractedText, description " +
              "FROM doc_mount_file_links WHERE mountPointId = ? AND relativePath LIKE 'photos/%' " +
              'ORDER BY relativePath',
          )
          .all(vault.mountPointId);
        tables = { savedLinks };
      } finally {
        global.Date = RealDate;
      }
      return { name: c.name, status, body, tables };
    }
    if (c.kind === 'photo-remove') {
      // DELETE /api/v1/characters/[id]/photos/[linkId]. Discover the vault link
      // id by relativePath, call the route, dump the GC tables + defaultImageId.
      const { getCharacterVaultStore } = await import(
        '@/lib/file-storage/character-vault-bridge'
      );
      const vault = await getCharacterVaultStore(ARIA);
      if (!vault) throw new Error('Aria vault did not resolve');
      const links = await getRepositories().docMountFileLinks.findByMountPointId(vault.mountPointId);
      const target = links.find((l: { relativePath: string }) => l.relativePath === c.photoRelativePath);
      if (!target) throw new Error(`photo link ${c.photoRelativePath} not found`);
      const linkId = (target as { id: string }).id;
      const url = `http://localhost/api/v1/characters/${ARIA}/photos/${linkId}`;
      const { DELETE } = (await import('@/app/api/v1/characters/[id]/photos/[linkId]/route')) as {
        DELETE: (...a: unknown[]) => Promise<unknown>;
      };
      const response = (await DELETE(mockRequest(url, undefined), {
        params: Promise.resolve({ id: ARIA, linkId }),
      })) as { status: number; json: () => Promise<unknown> };
      status = response.status;
      body = await response.json();
      const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
      const { getRawMountIndexDatabase } = await import(
        '@/lib/database/backends/sqlite/mount-index-client'
      );
      tables = dumpPhotoTables(
        getRawMountIndexDatabase() as never,
        getRawDatabase() as never,
        vault.mountPointId,
      );
      return { name: c.name, status, body, tables };
    }
    if (c.kind === 'st-import') {
      // JSON leg of `POST /api/v1/characters?action=import`. Capture the slim
      // create echo, then read the created character back through the overlay
      // GET to prove the ST-derived vault fields (scenarios / systemPrompts /
      // firstMessage / exampleDialogues / sillyTavernData) round-tripped.
      const url = 'http://localhost/api/v1/characters?action=import';
      const { POST } = (await import('@/app/api/v1/characters/route')) as {
        POST: (...a: unknown[]) => Promise<unknown>;
      };
      const response = (await POST(mockRequest(url, c.body))) as {
        status: number;
        json: () => Promise<{ character?: { id?: string } }>;
      };
      status = response.status;
      body = await response.json();
      const createdId = (body as { character?: { id?: string } })?.character?.id;
      let readback: unknown = null;
      if (createdId) {
        const getUrl = `http://localhost/api/v1/characters/${createdId}`;
        const mod = (await import('@/app/api/v1/characters/[id]/route')) as {
          GET: (...a: unknown[]) => Promise<unknown>;
        };
        const rb = (await mod.GET(mockRequest(getUrl, undefined), {
          params: Promise.resolve({ id: createdId }),
        })) as { json: () => Promise<unknown> };
        readback = await rb.json();
      }
      return { name: c.name, status, body, readback };
    }
    if (c.kind.startsWith('depiction')) {
      const url = `http://localhost/api/v1/characters/${ARIA}?action=depiction-guidelines`;
      const mod = (await import('@/app/api/v1/characters/[id]/route')) as {
        GET: (...a: unknown[]) => Promise<unknown>;
        PUT: (...a: unknown[]) => Promise<unknown>;
      };
      const params = { params: Promise.resolve({ id: ARIA }) };
      const fn = c.kind === 'depiction-get' ? mod.GET : mod.PUT;
      const response = (await fn(mockRequest(url, c.body), params)) as {
        status: number;
        json: () => Promise<unknown>;
      };
      status = response.status;
      body = await response.json();
      let readback: unknown;
      if (c.kind === 'depiction-put') {
        // Read the file back through the GET path to prove the write landed.
        const rb = (await mod.GET(mockRequest(url, undefined), params)) as {
          json: () => Promise<{ content?: string }>;
        };
        readback = (await rb.json()).content ?? '';
      }
      return { name: c.name, status, body, ...(readback !== undefined ? { readback } : {}) };
    }
    if (c.kind.startsWith('tag')) {
      if (c.kind === 'tag-list' || c.kind === 'tag-create') {
        const url = 'http://localhost/api/v1/tags';
        const mod = (await import('@/app/api/v1/tags/route')) as {
          GET: (...a: unknown[]) => Promise<unknown>;
          POST: (...a: unknown[]) => Promise<unknown>;
        };
        const fn = c.kind === 'tag-list' ? mod.GET : mod.POST;
        const response = (await fn(mockRequest(url, c.body))) as {
          status: number;
          json: () => Promise<unknown>;
        };
        status = response.status;
        body = await response.json();
      } else {
        const url = `http://localhost/api/v1/tags/${c.id}`;
        const mod = (await import('@/app/api/v1/tags/[id]/route')) as {
          GET: (...a: unknown[]) => Promise<unknown>;
          PUT: (...a: unknown[]) => Promise<unknown>;
          DELETE: (...a: unknown[]) => Promise<unknown>;
        };
        const fn =
          c.kind === 'tag-get' ? mod.GET : c.kind === 'tag-update' ? mod.PUT : mod.DELETE;
        const response = (await fn(mockRequest(url, c.body), {
          params: Promise.resolve({ id: c.id }),
        })) as { status: number; json: () => Promise<unknown> };
        status = response.status;
        body = await response.json();
        if (c.kind === 'tag-delete') {
          const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
          tables = dumpTagTables(getRawDatabase as never);
        }
      }
      return { name: c.name, status, body, ...(tables !== undefined ? { tables } : {}) };
    }
    if (c.kind.startsWith('wardrobe')) {
      // Discover the baked item id (minted at fixture build) by title.
      let itemId = '';
      if (c.itemTitle) {
        const items = await getRepositories().wardrobe.findByCharacterId(ARIA);
        const found = items.find((i: { title: string }) => i.title === c.itemTitle) as
          | { id: string }
          | undefined;
        if (!found) throw new Error(`wardrobe item titled ${c.itemTitle} not found`);
        itemId = found.id;
      }
      if (c.kind === 'wardrobe-create') {
        const url = `http://localhost/api/v1/characters/${ARIA}/wardrobe`;
        const { POST } = (await import('@/app/api/v1/characters/[id]/wardrobe/route')) as {
          POST: (...a: unknown[]) => Promise<unknown>;
        };
        const response = (await POST(mockRequest(url, c.body), {
          params: Promise.resolve({ id: ARIA }),
        })) as { status: number; json: () => Promise<unknown> };
        status = response.status;
        body = await response.json();
      } else {
        const url = `http://localhost/api/v1/characters/${ARIA}/wardrobe/${itemId}`;
        const mod = (await import('@/app/api/v1/characters/[id]/wardrobe/[itemId]/route')) as {
          GET: (...a: unknown[]) => Promise<unknown>;
          PUT: (...a: unknown[]) => Promise<unknown>;
          DELETE: (...a: unknown[]) => Promise<unknown>;
        };
        const fn =
          c.kind === 'wardrobe-get' ? mod.GET : c.kind === 'wardrobe-update' ? mod.PUT : mod.DELETE;
        const response = (await fn(mockRequest(url, c.body), {
          params: Promise.resolve({ id: ARIA, itemId }),
        })) as { status: number; json: () => Promise<unknown> };
        status = response.status;
        body = await response.json();
      }
    } else if (c.kind === 'update' || c.kind === 'update-seq') {
      const url = `http://localhost/api/v1/characters/${c.id}`;
      const { PUT } = (await import('@/app/api/v1/characters/[id]/route')) as {
        PUT: (...a: unknown[]) => Promise<unknown>;
      };
      // `update` = one PUT; `update-seq` = each body in order over the same DB
      // (the last PUT's overlay-reload response is the echo).
      const putBodies = c.kind === 'update-seq' ? (c.bodies ?? []) : [c.body];
      let response: { status: number; json: () => Promise<unknown> } | undefined;
      for (const b of putBodies) {
        response = (await PUT(mockRequest(url, b), {
          params: Promise.resolve({ id: c.id }),
        })) as { status: number; json: () => Promise<unknown> };
      }
      if (!response) throw new Error(`${c.name}: no PUT bodies`);
      status = response.status;
      body = await response.json();
      if (c.kind === 'update-seq') {
        // The PUT echo is v4's merged+validated object, which carries an
        // explicitly-nulled key as present-`null`; v5's echo is a pure overlay
        // re-read (null columns omitted). To prove the CLEAR persisted without
        // tripping that echo-shape artifact, read the character back through the
        // GET path — both sides omit a null column there.
        const getUrl = `http://localhost/api/v1/characters/${c.id}`;
        const { GET } = (await import('@/app/api/v1/characters/[id]/route')) as {
          GET: (...a: unknown[]) => Promise<unknown>;
        };
        const rb = (await GET(mockRequest(getUrl, undefined), {
          params: Promise.resolve({ id: c.id }),
        })) as { json: () => Promise<unknown> };
        return { name: c.name, status, body, readback: await rb.json() };
      }
    } else {
      const action = c.kind === 'quick-create' ? '?action=quick-create' : '';
      const url = `http://localhost/api/v1/characters${action}`;
      const { POST } = (await import('@/app/api/v1/characters/route')) as {
        POST: (...a: unknown[]) => Promise<unknown>;
      };
      const response = (await POST(mockRequest(url, c.body))) as {
        status: number;
        json: () => Promise<unknown>;
      };
      status = response.status;
      body = await response.json();
    }
    return { name: c.name, status, body };
  } finally {
    await closeDatabase();
    closeMountIndexSQLiteClient();
    rmSync(work, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(
    fs.readFileSync(join(here, '..', 'fixtures', 'characters.json'), 'utf8'),
  ) as Spec;

  const fixtures = {
    main: process.env.QT_FIXTURE_CHARACTERS_MAIN ?? '',
    mount: process.env.QT_FIXTURE_CHARACTERS_MOUNT ?? '',
  };
  for (const [k, v] of Object.entries(fixtures)) {
    if (!v || !existsSync(v)) throw new Error(`fixture ${k} missing: ${v}`);
  }
  const outPath = process.env.QT_ORACLE_OUT;
  if (!outPath) throw new Error('QT_ORACLE_OUT must point at the NDJSON file to write');

  const scratch = mkdtempSync(join(tmpdir(), 'qt-characters-mutations-'));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
  process.env.QUILLTAP_DATA_DIR = scratch;
  delete process.env.SQLITE_WAL_MODE;
  process.env.LOG_LEVEL = 'error';

  const cases: CaseSpec[] = [
    {
      name: 'create_full',
      kind: 'create',
      body: {
        name: 'Fenwick',
        title: 'The Cartographer',
        identity: 'A wandering mapmaker.',
        description: 'Ink-stained and curious.',
        manifesto: 'Every coast deserves a name.',
        personality: 'Meticulous, wry.',
        firstMessage: 'Ah — a fellow traveller.',
        exampleDialogues: 'User: Where to?\nFenwick: North, always north.',
        controlledBy: 'llm',
        npc: false,
        defaultConnectionProfileId: CONN,
        scenarios: [
          { title: 'The Harbor', content: 'Fog rolls over the docks.' },
          { title: 'The Summit', content: 'The last ridge before the map ends.' },
        ],
        systemPrompts: [
          {
            id: 'd0000001-0000-4000-8000-000000000001',
            name: 'Terse',
            content: 'Answer in few words.',
            isDefault: true,
            createdAt: '2020-01-01T00:00:00.000Z',
            updatedAt: '2020-01-01T00:00:00.000Z',
          },
        ],
        physicalDescription: {
          id: 'e0000001-0000-4000-8000-000000000001',
          name: 'Fenwick body',
          shortPrompt: 'weathered coat',
          mediumPrompt: 'a weathered coat and a brass spyglass',
          fullDescription: 'Tall, lean, ink on the fingers.',
          createdAt: '2020-01-01T00:00:00.000Z',
          updatedAt: '2020-01-01T00:00:00.000Z',
        },
      },
    },
    {
      // The guard's whole truth table, in one fresh DB: three refused patches
      // (a plain edit, a MANAGED-field edit that would route to the vault, and
      // the old archive-finalization patch that nulls the vault pointer), then
      // the ONE sanctioned single-key unarchive, then a second plain edit that
      // now succeeds because the tombstone is gone — and finally delete, the
      // escape hatch that is unguarded even while archived.
      name: 'archive_guard_truth_table',
      kind: 'archive-guard-repo',
      bodies: [
        { name: 'Renamed While Archived' },
        { description: 'A vault-managed edit.' },
        { characterDocumentMountPointId: null },
        { archivedAt: null, name: 'Two keys is not the sanctioned patch' },
        { archivedAt: null },
        { name: 'Renamed After Rehydrate' },
      ],
    },
    {
      name: 'archive_guard_wardrobe_write',
      kind: 'archive-guard-wardrobe',
    },
    { name: 'create_minimal', kind: 'create', body: { name: 'Mimsy' } },
    { name: 'quick_create', kind: 'quick-create', body: { name: 'Quill' } },
    {
      name: 'update_managed',
      kind: 'update',
      id: ARIA,
      body: {
        title: 'Renamed Title',
        identity: 'A rewritten identity.',
        description: 'A rewritten description.',
        personality: 'Newly bold.',
        firstMessage: 'A fresh greeting.',
        scenarios: [{ title: 'Only Scenario', content: 'The reprojected scene.' }],
        physicalDescription: {
          name: 'Aria body v2',
          shortPrompt: 'silver cloak',
          longPrompt: 'a flowing silver cloak with embroidered stars',
        },
      },
    },
    {
      name: 'update_slim',
      kind: 'update',
      id: ARIA,
      body: {
        name: 'Aria Reforged',
        controlledBy: 'user',
        npc: true,
        canBeCarina: false,
      },
    },
    {
      // The fact sheet: a named schema key survives the Zod strip and is routed to
      // metadata.json (whole-object replace), so the reloaded echo carries it. The
      // unknown `notAField` key is stripped before the write overlay ever sees it —
      // pinning that `metadata` is NOT dropped the same way an unnamed key is.
      name: 'update_metadata',
      kind: 'update',
      id: ARIA,
      body: {
        metadata: { hasAnsibleAccess: true, clearanceLevel: 3, faction: 'Ordo Aurum' },
        notAField: 'stripped by the schema',
      },
    },
    // P4.6bh: `canChooseOutfit` PUT → vault properties.json round-trip. The echo is
    // an overlay re-read, so `character.canChooseOutfit === true` proves the schema
    // allowlist named it, the write overlay routed it to the vault RMW, and the read
    // overlay hydrated it back. (Aria's vault seeds `canChooseOutfit: false`.)
    {
      name: 'update_can_choose_outfit',
      kind: 'update',
      id: ARIA,
      body: { canChooseOutfit: true },
    },
    // P4.6bh (blissful-einstein): the wardrobe-permission tri-states persist through
    // PUT (previously stripped before update()). Echo reflects both DB columns.
    {
      name: 'update_wardrobe_permissions',
      kind: 'update',
      id: ARIA,
      body: { canDressThemselves: false, canCreateOutfits: true },
    },
    // P4.6bh: the explicit-`null` (inherit-global) tri-state is PRESERVED, not
    // stripped. First set both non-null, then PUT `{canDressThemselves: null}`:
    // canDressThemselves clears back to null while canCreateOutfits (untouched by
    // the second PUT) stays true — mirrors v4's put.route.test.ts null-case intent
    // at the route level (both columns seed null in the fixture).
    {
      name: 'update_clear_wardrobe_permission',
      kind: 'update-seq',
      id: ARIA,
      bodies: [
        { canDressThemselves: true, canCreateOutfits: true },
        { canDressThemselves: null },
      ],
    },
    {
      name: 'wardrobe_create',
      kind: 'wardrobe-create',
      body: {
        title: 'Storm Cloak',
        description: 'An oilskin cloak for rough weather.',
        imagePrompt: 'heavy grey oilskin storm cloak',
        types: ['top', 'accessories'],
        isDefault: false,
      },
    },
    { name: 'wardrobe_get', kind: 'wardrobe-get', itemTitle: 'Flight Jacket' },
    {
      name: 'wardrobe_update',
      kind: 'wardrobe-update',
      itemTitle: 'Flight Jacket',
      body: { title: 'Weathered Flight Jacket', description: null, isDefault: false },
    },
    // ── P4.D120 / v4 `d25dacc1` — the character item route's `archived` ──
    {
      name: 'wardrobe_update_archives',
      kind: 'wardrobe-update',
      itemTitle: 'Flight Jacket',
      body: { archived: true },
    },
    {
      name: 'wardrobe_update_restore_of_an_active_item_is_a_noop',
      kind: 'wardrobe-update',
      itemTitle: 'Flight Jacket',
      body: { archived: false },
    },
    {
      name: 'wardrobe_update_archived_null_is_a_validation_error',
      kind: 'wardrobe-update',
      itemTitle: 'Flight Jacket',
      body: { archived: null },
    },
    { name: 'wardrobe_delete', kind: 'wardrobe-delete', itemTitle: 'Goggles' },
    { name: 'tag_list', kind: 'tag-list' },
    { name: 'tag_get', kind: 'tag-get', id: ADVENTURE },
    { name: 'tag_create_new', kind: 'tag-create', body: { name: 'Voyage' } },
    { name: 'tag_create_dedup', kind: 'tag-create', body: { name: 'adventure' } },
    {
      name: 'tag_update',
      kind: 'tag-update',
      id: MYSTERY,
      body: { name: 'Enigma', quickHide: true },
    },
    { name: 'tag_delete', kind: 'tag-delete', id: ADVENTURE },
    { name: 'depiction_get_empty', kind: 'depiction-get' },
    {
      name: 'depiction_put_write',
      kind: 'depiction-put',
      body: { content: 'Keep depictions tasteful and in-period.' },
    },
    { name: 'depiction_put_clear', kind: 'depiction-put', body: { content: '   ' } },
    // P4.6i: ST import (JSON leg) — a chara_card_v2 card exercising the card
    // unwrap, scenario/system_prompt → single-item arrays, mes_example string,
    // title, and sillyTavernData passthrough.
    {
      name: 'st_import_card',
      kind: 'st-import',
      body: {
        spec: 'chara_card_v2',
        spec_version: '2.0',
        data: {
          name: 'Fable',
          description: 'A traveling storyteller.',
          personality: 'Whimsical and wry.',
          scenario: 'The caravan halts at dusk.',
          first_mes: 'Gather round, friends.',
          mes_example: 'User: A tale?\nFable: Always a tale.',
          system_prompt: 'You are Fable, a wandering storyteller.',
          creator_notes: 'imported for the differential',
          tags: ['bard'],
          creator: 'TestSuite',
          character_version: '2.1',
          extensions: { origin: 'sillytavern' },
          title: 'The Wandering Bard',
        },
      },
    },
    // P4.6i: save Aria's avatar link into the photos/ folder (the linkId save-by-id
    // leg — bytes from the source link's mount-blob). Frozen clock → deterministic
    // path + markdown.
    { name: 'photo_save_link', kind: 'photo-save', photoRelativePath: 'images/avatar.webp' },
    // -----------------------------------------------------------------------
    // P4.60 — the `saveByIdSchema` wrong-type-collapse arms. v5's edge used to
    // read fileId/linkId/caption/tags with `and_then(Value::as_str)`/`as_array`,
    // so a wrong-typed caption or tag list was DROPPED and the photo saved 201.
    // The refine's own quirk is measured here too: an `invalid_type` issue
    // suppresses it, a `too_small` issue does not.
    // -----------------------------------------------------------------------
    { name: 'photo_body_file_id_number', kind: 'photo-save-body', rawBody: { fileId: 123 } },
    { name: 'photo_body_file_id_empty', kind: 'photo-save-body', rawBody: { fileId: '' } },
    { name: 'photo_body_link_id_null', kind: 'photo-save-body', rawBody: { linkId: null } },
    { name: 'photo_body_neither', kind: 'photo-save-body', rawBody: {} },
    { name: 'photo_body_both', kind: 'photo-save-body', rawBody: { fileId: 'x', linkId: 'y' } },
    { name: 'photo_body_not_object', kind: 'photo-save-body', rawBody: 42 },
    {
      name: 'photo_body_caption_number',
      kind: 'photo-save-body',
      photoRelativePath: 'images/avatar.webp',
      rawBody: { linkId: '<link>', caption: 5 },
    },
    {
      name: 'photo_body_tags_string',
      kind: 'photo-save-body',
      photoRelativePath: 'images/avatar.webp',
      rawBody: { linkId: '<link>', tags: 'airship' },
    },
    {
      name: 'photo_body_tags_element',
      kind: 'photo-save-body',
      photoRelativePath: 'images/avatar.webp',
      rawBody: { linkId: '<link>', tags: ['ok', 2] },
    },
    // Two failing keys at once: issue ORDER is shape order, joined with '; '.
    {
      name: 'photo_body_two_issues',
      kind: 'photo-save-body',
      rawBody: { fileId: '', caption: 5 },
    },
    // The passing poles: an explicit null caption (nullable) and a tag list the
    // JSON leg does NOT filter (unlike the multipart leg's empty-tag drop).
    {
      name: 'photo_body_caption_null_ok',
      kind: 'photo-save-body',
      photoRelativePath: 'images/avatar.webp',
      rawBody: { linkId: '<link>', caption: null, tags: ['brass', ''] },
    },
    // P4.6i: remove Aria's avatar from the gallery — exercises the
    // defaultImageId-pointer clear + the GC-safe deleteWithGC (last link → the
    // file + blob are reclaimed).
    { name: 'photo_remove_avatar', kind: 'photo-remove', photoRelativePath: 'images/avatar.webp' },
    // P4.6i: cascade-delete Aria with cascadeChats + cascadeImages — the whole
    // fan-out (exclusive chat + messages, vault-link avatar via the gallery
    // remove, memories via the unlink batch, plugin data, the slim row).
    { name: 'character_delete_cascade', kind: 'character-delete' },
  ];

  const outLines: string[] = [];
  for (const c of cases) {
    const payload = await runCase(spec, c, scratch, fixtures);
    outLines.push(JSON.stringify(payload));
  }
  fs.writeFileSync(outPath, outLines.join('\n') + '\n');
  process.stderr.write(
    `characters-mutations oracle wrote ${outPath} (${outLines.length} cases)\n`,
  );
}

test('characters-mutations oracle', async () => {
  await main();
});
