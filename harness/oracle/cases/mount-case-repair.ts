/**
 * Differential oracle — the mount-index case-collision repair (v4 `0a0419f5`).
 *
 * Drives v4's REAL `ensureFolderNocaseUniqueIndex` /
 * `ensureLinkNocaseUniqueIndex` / `repairMountPointNameCollisions`
 * (`@/lib/database/repositories/mount-index-case-repair`) over a real in-memory
 * better-sqlite3, one case per line of the shared spec
 * (`fixtures/mount-case-repair-spec.json`). Each case builds the tables + legacy
 * case-sensitive indexes exactly as v4's own `mount-index-case-repair.test.ts`
 * `freshDb()` does, plants the colliding rows, runs the named repair, and dumps
 * post-repair rows + the two tables' `sqlite_master` index rows.
 *
 * P4.D274 (v4 `ed8b15b50`, bug 186) extends it: the run `mountpoints-index`
 * drives v4's REAL `ensureMountPointNameUniqueIndex` (repair → validity → DROP
 * + CREATE the `idx_doc_mount_points_name_nocase` UNIQUE NOCASE index → INFO);
 * `presetNameIndex` plants the genuine index first (a no-op, no line),
 * `tamperNameIndex` a non-unique stand-in under its name (DROP + CREATE), and
 * `probeInsert` is inserted AFTER the run, its error text (or `null`) dumped as
 * `probeError`. The index dump now covers `doc_mount_points` too (the earlier
 * cases plant no index there, so their rows are unchanged), and EVERY case
 * dumps `lines` — the log lines v4 emitted, captured by splicing a transport
 * into the singleton logger: `{level, message, fields}` in v4's key order.
 *
 * The Rust diff (`crates/quilltap-harness/tests/mount_case_repair_equivalence.rs`)
 * reads the SAME spec, replays the identical setup with the ported repair, and
 * field-diffs against this NDJSON.
 *
 * Run from the v4 checkout (Node 24 for the native binding ABI):
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   $N/node --import tsx \
 *     ~/source/quilltap-v5/harness/oracle/cases/mount-case-repair.ts \
 *     > /tmp/oracle-mount-case-repair.ndjson
 */

import { join, dirname } from 'node:path';
import path from 'node:path';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import {
  ensureFolderNocaseUniqueIndex,
  ensureLinkNocaseUniqueIndex,
  ensureMountPointNameUniqueIndex,
  MOUNT_POINT_NAME_NOCASE_INDEX,
  repairMountPointNameCollisions,
} from '@/lib/database/repositories/mount-index-case-repair';
import { logger } from '@/lib/logger';
import { nextUniqueMountPointName } from '@/lib/mount-index/unique-mount-point-name';

// nextUniqueMountPointName — the naming leaf (case-insensitive + trimmed). Kept
// in lockstep with the Rust test's NAMING_CASES (same inputs, same order).
const NAMING_CASES: Array<{ taken: string[]; desired: string }> = [
  { taken: ['Lore'], desired: 'lore' },
  { taken: ['Lore', 'LORE (2)'], desired: 'lore' },
  { taken: ['Other'], desired: 'Lore' },
  { taken: ['  My Vault  '], desired: 'my vault' },
  { taken: [], desired: 'Fresh' },
];

// The root package aliases better-sqlite3-multiple-ciphers as better-sqlite3;
// require the real binding by absolute path (an in-memory DB needs no cipher).
const require = createRequire(import.meta.url);
const Database = require(join(process.cwd(), 'node_modules', 'better-sqlite3'));

const HERE = dirname(fileURLToPath(import.meta.url));
const SPEC = JSON.parse(
  readFileSync(join(HERE, '..', 'fixtures', 'mount-case-repair-spec.json'), 'utf8'),
) as Spec;

interface FolderSpec {
  id: string;
  parentId: string | null;
  name: string;
  path: string;
  createdAt: string;
}
interface LinkSpec {
  id: string;
  relativePath: string;
  folderId: string | null;
  createdAt: string;
}
interface MountPointSpec {
  id: string;
  name: string;
  createdAt: string;
}
interface Case {
  name: string;
  run: 'folder' | 'link' | 'mountpoints' | 'mountpoints-index';
  runTwice?: boolean;
  tamperFolderIndex?: boolean;
  presetNameIndex?: boolean;
  tamperNameIndex?: boolean;
  probeInsert?: MountPointSpec;
  folders?: FolderSpec[];
  links?: LinkSpec[];
  mountPoints?: MountPointSpec[];
}
interface Spec {
  mountPointId: string;
  cases: Case[];
}

const MP = SPEC.mountPointId;

// Every line v4 logs lands here (child loggers share the singleton's
// transports), so nothing reaches stdout; `runCase` drains it per case.
type Line = { level: string; message: string; fields: Record<string, unknown> };
let captured: Line[] = [];
const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
internal.transports.splice(0, internal.transports.length, {
  write: (d: unknown) => {
    const data = d as { level: string; message: string; context?: Record<string, unknown>; error?: { message: string } };
    const { service: _s, environment: _e, ...fields } = data.context ?? {};
    if (data.error) fields.error = data.error.message;
    captured.push({ level: data.level, message: data.message, fields });
  },
});
const FOLDER_NOCASE_INDEX = 'idx_doc_mount_folders_mp_parent_name_nocase';

function freshDb(): any {
  const db = new Database(':memory:');
  db.exec(`
    CREATE TABLE doc_mount_points (
      id TEXT PRIMARY KEY,
      name TEXT NOT NULL,
      createdAt TEXT NOT NULL
    );
    CREATE TABLE doc_mount_folders (
      id TEXT PRIMARY KEY,
      mountPointId TEXT NOT NULL,
      parentId TEXT,
      name TEXT NOT NULL,
      path TEXT NOT NULL,
      createdAt TEXT NOT NULL,
      updatedAt TEXT NOT NULL
    );
    CREATE TABLE doc_mount_file_links (
      id TEXT PRIMARY KEY,
      mountPointId TEXT NOT NULL,
      relativePath TEXT NOT NULL,
      fileName TEXT NOT NULL,
      folderId TEXT,
      createdAt TEXT NOT NULL
    );
  `);
  // The legacy case-sensitive indexes the repair must replace.
  db.exec(
    `CREATE UNIQUE INDEX "idx_doc_mount_folders_mp_parent_name" ` +
      `ON "doc_mount_folders" ("mountPointId", COALESCE("parentId", ''), "name")`,
  );
  db.exec(
    `CREATE UNIQUE INDEX "idx_doc_mount_file_links_mp_path" ` +
      `ON "doc_mount_file_links" ("mountPointId", "relativePath")`,
  );
  return db;
}

function runCase(c: Case): unknown {
  const db = freshDb();
  captured = [];
  try {
    if (c.presetNameIndex) {
      db.exec(
        `CREATE UNIQUE INDEX "${MOUNT_POINT_NAME_NOCASE_INDEX}" ON "doc_mount_points" ("name" COLLATE NOCASE)`,
      );
    }
    if (c.tamperNameIndex) {
      db.exec(`CREATE INDEX "${MOUNT_POINT_NAME_NOCASE_INDEX}" ON "doc_mount_points" ("name")`);
    }
    if (c.tamperFolderIndex) {
      // Simulate manual tampering: drop the legacy index and plant a non-unique
      // stand-in under the NOCASE index's name.
      db.exec(`DROP INDEX IF EXISTS "idx_doc_mount_folders_mp_parent_name"`);
      db.exec(`CREATE INDEX "${FOLDER_NOCASE_INDEX}" ON "doc_mount_folders" ("mountPointId")`);
    }
    for (const f of c.folders ?? []) {
      db.prepare(
        `INSERT INTO doc_mount_folders (id, mountPointId, parentId, name, path, createdAt, updatedAt)
         VALUES (?, ?, ?, ?, ?, ?, ?)`,
      ).run(f.id, MP, f.parentId, f.name, f.path, f.createdAt, f.createdAt);
    }
    for (const l of c.links ?? []) {
      db.prepare(
        `INSERT INTO doc_mount_file_links (id, mountPointId, relativePath, fileName, folderId, createdAt)
         VALUES (?, ?, ?, ?, ?, ?)`,
      ).run(l.id, MP, l.relativePath, path.posix.basename(l.relativePath), l.folderId, l.createdAt);
    }
    for (const m of c.mountPoints ?? []) {
      db.prepare(`INSERT INTO doc_mount_points (id, name, createdAt) VALUES (?, ?, ?)`).run(
        m.id,
        m.name,
        m.createdAt,
      );
    }

    const runOnce = () => {
      if (c.run === 'folder') ensureFolderNocaseUniqueIndex(db);
      else if (c.run === 'link') ensureLinkNocaseUniqueIndex(db);
      else if (c.run === 'mountpoints-index') ensureMountPointNameUniqueIndex(db);
      else repairMountPointNameCollisions(db);
    };
    runOnce();
    if (c.runTwice) runOnce();

    let probeError: string | null = null;
    if (c.probeInsert) {
      try {
        db.prepare(`INSERT INTO doc_mount_points (id, name, createdAt) VALUES (?, ?, ?)`).run(
          c.probeInsert.id,
          c.probeInsert.name,
          c.probeInsert.createdAt,
        );
      } catch (e) {
        probeError = (e as Error).message;
      }
    }

    const folders = db
      .prepare(`SELECT id, parentId, name, path FROM doc_mount_folders ORDER BY id`)
      .all();
    const links = db
      .prepare(`SELECT id, relativePath, fileName FROM doc_mount_file_links ORDER BY id`)
      .all();
    const mountPoints = db.prepare(`SELECT id, name FROM doc_mount_points ORDER BY id`).all();
    const indexes = db
      .prepare(
        `SELECT name, sql FROM sqlite_master WHERE type = 'index'
           AND tbl_name IN ('doc_mount_folders', 'doc_mount_file_links', 'doc_mount_points')
           AND sql IS NOT NULL ORDER BY name`,
      )
      .all();
    const out: Record<string, unknown> = { case: c.name, folders, links, mountPoints, indexes, lines: captured };
    if (c.probeInsert) out.probeError = probeError;
    return out;
  } finally {
    db.close();
  }
}

for (const c of SPEC.cases) {
  process.stdout.write(JSON.stringify(runCase(c)) + '\n');
}

process.stdout.write(
  JSON.stringify({
    case: 'naming',
    results: NAMING_CASES.map((c) => nextUniqueMountPointName(new Set(c.taken), c.desired)),
  }) + '\n',
);
