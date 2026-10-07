/**
 * Migration-created index dumper (P4.153, dogfood #149) — the SECOND committed
 * provisioning artifact, `services/provisioning/migration_indexes.json`.
 *
 * v4 has TWO index families. `dump-fresh-schema.ts` captures the generateDDL
 * one (`createdAt` DESC + `userId` per table, the mount-index nocase pair, …)
 * that the repositories' `ensureCollection` makes. The OTHER family —
 * `idx_chat_messages_chatId`, `idx_chats_projectId`, `idx_memories_characterId`,
 * the UNIQUE `idx_connection_profiles_userId_name`, … — is made by v4's
 * MIGRATIONS, which a real v4 first boot runs in PHASE 1 before any repository
 * is touched. A v5 instance provisioned from the generateDDL dump alone carried
 * none of it (a restore into one took 2 h 26 m against 9 m — the per-chat
 * message reads scanned).
 *
 * This script builds v4's instance the real-boot way (`migrations-first.ts`:
 * v4's REAL `MigrationRunner` over v4's REAL registry on an empty data dir,
 * then the repository pass) and dumps, per partition, every `type = 'index'`
 * row of `sqlite_master` that is NOT already in `fresh_schema.json`, the SQL
 * VERBATIM (SQLite keeps the statement as written, minus `IF NOT EXISTS`).
 *
 * A name `fresh_schema.json` already carries (generateDDL and a migration both
 * create it) keeps the generateDDL copy (the order's R-D) — EXCEPT where the
 * migration's text is UNIQUE and the generateDDL copy is not (the
 * write-semantics class; at `94fbb1ae3` exactly one,
 * `idx_doc_mount_folders_mp_path`). A real v4 boot keeps the migration's
 * UNIQUE index, so the dump carries it and the provisioner SKIPS
 * `fresh_schema.json`'s plain copy of any name this artifact carries (the
 * human's ruling, 2026-10-06). The other shared names differ only by
 * `("userId" ASC)` vs `("userId")` — the same index — and stay generateDDL.
 *
 * Two classes are left out, both counted on stderr and in the P4.153 record:
 *   - any other shared name (the generateDDL copy is kept);
 *   - an index on a table `fresh_schema.json` does not create (the legacy
 *     `wardrobe_items`, which only the initial-schema migration makes) — v5's
 *     TABLE surface stays the generateDDL one (R-A), so there is nothing to
 *     index.
 *
 * `QT_REAL_BOOT_MASTER` is REQUIRED (P4.160 R-F): the `sqlite_master` dump of
 * a REAL `tsx server.ts` first boot at the same pin (the recipe in
 * `docs/developer/porting/work-orders/surveys/2026-10-06-p4.153-v4-first-boot-
 * indexes.md`, "The recipe"). The dumper diffs its own family against the real
 * boot's by name and SQL, prints every difference as a FINDING (the real boot
 * wins), and EXITS NON-ZERO on any finding — after writing the artifact, so
 * the diff against the committed one is still there to read. Before P4.160 it
 * printed the findings and exited 0, and the cross-check was optional.
 *
 * Output: JSON `{ source, main, mountIndex, llmLogs }`, each partition's
 * statements ordered by index name. NEVER hand-edit it (D23).
 *
 * Run from a pinned v4 worktree under Node 24:
 *   N=~/.nvm/versions/node/v24.13.1/bin
 *   cd ~/source/quilltap-server
 *   V5W=${V5W:-$HOME/source/quilltap-v5}
 *   QT_FRESH_SCHEMA=$V5W/crates/quilltap-core/src/services/provisioning/fresh_schema.json \
 *   QT_MIGRATION_INDEXES_OUT=$V5W/crates/quilltap-core/src/services/provisioning/migration_indexes.json \
 *   QT_REAL_BOOT_MASTER=/tmp/real-boot-master.json \
 *     $N/npx tsx $V5W/harness/oracle/provision/dump-migration-indexes.ts
 * (`QT_REAL_BOOT_MASTER`: `{ main, mountIndex, llmLogs }`, each partition's
 * `SELECT type, name, tbl_name, sql FROM sqlite_master` rows from the real
 * boot's three `.db` files under the test pepper.)
 */

import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import {
  buildMigrationsFirst,
  indexRows,
  migrationsFirstEnv,
  type IndexRow,
} from './migrations-first';

type Partition = 'main' | 'mountIndex' | 'llmLogs';
const PARTITIONS: Partition[] = ['main', 'mountIndex', 'llmLogs'];

/** `CREATE [UNIQUE] (TABLE|INDEX) [IF NOT EXISTS] "name"` → [kind, name]. */
function ddlName(sql: string): [string, string] {
  const m = /^CREATE\s+(?:UNIQUE\s+)?(TABLE|INDEX)\s+(?:IF NOT EXISTS\s+)?"?([A-Za-z0-9_]+)"?/i.exec(
    sql,
  );
  if (!m) throw new Error(`unrecognized fresh_schema statement: ${sql.slice(0, 80)}`);
  return [m[1].toUpperCase(), m[2]];
}

const isUnique = (sql: string) => /^CREATE\s+UNIQUE\s/i.test(sql);

async function main(): Promise<void> {
  const out = process.env.QT_MIGRATION_INDEXES_OUT;
  const freshPath = process.env.QT_FRESH_SCHEMA;
  const realPath = process.env.QT_REAL_BOOT_MASTER;
  if (!out || !freshPath || !realPath) {
    throw new Error(
      'QT_MIGRATION_INDEXES_OUT, QT_FRESH_SCHEMA and QT_REAL_BOOT_MASTER must all be set ' +
        '(the real-boot cross-check is required — see the header)',
    );
  }
  const fresh = JSON.parse(readFileSync(freshPath, 'utf8')) as Record<Partition, string[]>;

  const scratch = mkdtempSync(join(tmpdir(), 'qt-migration-indexes-'));
  process.on('exit', () => rmSync(scratch, { recursive: true, force: true }));
  mkdirSync(join(scratch, 'data'), { recursive: true });
  migrationsFirstEnv(scratch);

  const report = await buildMigrationsFirst();

  const { getRawDatabase } = await import('@/lib/database/backends/sqlite/client');
  const { getRawMountIndexDatabase } = await import(
    '@/lib/database/backends/sqlite/mount-index-client'
  );
  const { getRawLLMLogsDatabase } = await import('@/lib/database/backends/sqlite/llm-logs-client');
  const { closeDatabase } = await import('@/lib/database/manager');
  const live: Record<Partition, IndexRow[]> = {
    main: indexRows(getRawDatabase()),
    mountIndex: indexRows(getRawMountIndexDatabase()),
    llmLogs: indexRows(getRawLLMLogsDatabase()),
  };

  const artifact: Record<Partition, string[]> = { main: [], mountIndex: [], llmLogs: [] };
  const family: Record<Partition, IndexRow[]> = { main: [], mountIndex: [], llmLogs: [] };
  const lines: string[] = [];
  for (const p of PARTITIONS) {
    const freshTables = new Set<string>();
    const freshIndexes = new Map<string, string>();
    for (const sql of fresh[p]) {
      const [kind, name] = ddlName(sql);
      if (kind === 'TABLE') freshTables.add(name);
      else freshIndexes.set(name, sql);
    }
    const shared: string[] = [];
    const noTable: string[] = [];
    const replaces: string[] = [];
    for (const row of live[p]) {
      const freshSql = freshIndexes.get(row.name);
      if (freshSql !== undefined && isUnique(row.sql) && !isUnique(freshSql)) {
        replaces.push(row.name);
        family[p].push(row);
      } else if (freshSql !== undefined) shared.push(row.name);
      else if (!freshTables.has(row.tbl_name)) noTable.push(`${row.name}(${row.tbl_name})`);
      else family[p].push(row);
    }
    artifact[p] = family[p].map((r) => r.sql);
    const unique = family[p].filter((r) => /^CREATE UNIQUE/i.test(r.sql)).map((r) => r.name);
    lines.push(
      `${p}: ${family[p].length} migration indexes (UNIQUE: ${unique.join(', ') || 'none'}); ` +
        `UNIQUE over fresh_schema's plain copy: [${replaces.join(', ')}]; ` +
        `left out ${shared.length} shared with fresh_schema, ${noTable.length} on tables v5 never ` +
        `creates [${noTable.join(', ')}]`,
    );
  }

  const v4Commit = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
  const v4Version = JSON.parse(readFileSync('package.json', 'utf8')).version as string;
  writeFileSync(
    out,
    JSON.stringify(
      {
        source: {
          dumper: 'harness/oracle/provision/dump-migration-indexes.ts',
          v4Commit,
          v4Version,
          note: 'Generated - never hand-edit (D23). Re-dump from a pinned v4 worktree.',
        },
        ...artifact,
      },
      null,
      2,
    ) + '\n',
  );

  // The real-boot cross-check (R-B: the real boot wins) — required (P4.160 R-F).
  let findings = 0;
  {
    const real = JSON.parse(readFileSync(realPath, 'utf8')) as Record<
      Partition,
      { type: string; name: string; tbl_name: string; sql: string | null }[]
    >;
    for (const p of PARTITIONS) {
      const mine = new Map(family[p].map((r) => [r.name, r.sql]));
      const freshPlain = new Map(
        fresh[p].filter((sql) => ddlName(sql)[0] === 'INDEX').map((sql) => [ddlName(sql)[1], sql]),
      );
      const freshTables = new Set(fresh[p].map(ddlName).filter(([k]) => k === 'TABLE').map(([, n]) => n));
      const theirs = new Map(
        real[p]
          .filter(
            (r) =>
              r.type === 'index' &&
              r.sql &&
              freshTables.has(r.tbl_name) &&
              (!freshPlain.has(r.name) ||
                (isUnique(r.sql) && !isUnique(freshPlain.get(r.name) as string))),
          )
          .map((r) => [r.name, r.sql as string]),
      );
      for (const [name, sql] of theirs) {
        if (!mine.has(name)) {
          findings++;
          lines.push(`FINDING ${p}: the real boot has ${name}, the runner-driven build does not`);
        } else if (mine.get(name) !== sql) {
          findings++;
          lines.push(`FINDING ${p}: ${name} differs — real ${sql} | runner ${mine.get(name)}`);
        }
      }
      for (const name of mine.keys()) {
        if (!theirs.has(name)) {
          findings++;
          lines.push(`FINDING ${p}: the runner-driven build has ${name}, the real boot does not`);
        }
      }
    }
    lines.push(`real-boot cross-check: ${findings} finding(s)`);
  }

  await closeDatabase();
  process.stderr.write(
    `migrations: run=${report.migrationsRun} skipped=${report.migrationsSkipped} ` +
      `deferred=[${report.deferred.join(', ')}] failed=[${report.failed.join(', ')}]\n` +
      lines.join('\n') +
      `\nwrote ${out} (v4 ${v4Commit} ${v4Version})\n`,
  );
  if (findings > 0) {
    process.stderr.write(
      `migration-index dump: ${findings} real-boot FINDING(s) — the runner-driven build ` +
        `differs from a real first boot; do not commit this dump until they are resolved\n`,
    );
    process.exit(1);
  }
  process.exit(0);
}

main().catch((err) => {
  process.stderr.write(`migration-index dump failed: ${err?.stack ?? err}\n`);
  process.exit(1);
});
