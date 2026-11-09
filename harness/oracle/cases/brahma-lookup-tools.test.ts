/**
 * @jest-environment node
 *
 * Oracle case (P4.D374, tier 2): the four Brahma lookup tools — v4's REAL
 * `executeLookupCharacterTool` / `executeLookupMemoriesTool` /
 * `executeLookupMessagesTool` / `executeReadMessagesTool`
 * (`lib/tools/handlers/brahma-lookup-handler.ts`, `c539c82e0`) and the
 * dispatcher gate in `executeToolCallWithContext` (`tool-executor.ts:1066-1088`).
 *
 * Each case runs on a FRESH copy of the three fixture DBs built by
 * `build-brahma-lookup-fixture.ts`; the row carries everything the Rust side
 * needs to repeat it: the tool, the args, and the exact plant SQL executed on
 * the copy (a throwing plant rebuilds ONE column as a VIRTUAL `json(...)` so
 * both sides' reads fail — memory note `throwing-plant-must-fail-both-reads`).
 *
 * Per row: `resultJson` (`JSON.stringify` of the handler result, or of the
 * executor's `{toolName, success, result, error}`), the captured `BrahmaLookup`
 * lines (`logs`, the root `service`/`environment` dropped, the child's
 * `service`/`module` bindings kept), and `unchanged` — whether the `memories`,
 * `chats` and `chat_messages` tables are byte-identical after the call (the
 * read-only proof; `lookup_memories` writes no access stamp).
 *
 * The model boundary alone is pinned: `generateEmbeddingForUser` answers the
 * corpus's canned vectors (unknown text → v4's `EmbeddingError`). The memory
 * calendar runs on the jest zone (`TZ=UTC`, no instance timezone planted).
 *
 * Run (Node 24, from the pinned v4 checkout; stage outside `.claude/`):
 *   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
 *   STAGE=/tmp/qt-bl-stage
 *   rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
 *   cp $V5W/harness/oracle/cases/brahma-lookup-tools.test.ts $STAGE/harness/oracle/cases/
 *   cp $V5W/harness/oracle/fixtures/brahma-lookup-tools.json $STAGE/harness/oracle/fixtures/
 *   cd ~/source/quilltap-server
 *   QT_FIXTURE_BL_MAIN=/tmp/qt-bl-main.db QT_FIXTURE_BL_MOUNT=/tmp/qt-bl-mount.db \
 *   QT_FIXTURE_BL_LLM=/tmp/qt-bl-llm.db \
 *     $N/node --import tsx $V5W/harness/oracle/fixtures/build-brahma-lookup-fixture.ts
 *   QT_FIXTURE_BL_MAIN=/tmp/qt-bl-main.db QT_FIXTURE_BL_MOUNT=/tmp/qt-bl-mount.db \
 *   QT_FIXTURE_BL_LLM=/tmp/qt-bl-llm.db QT_ORACLE_OUT=/tmp/oracle-brahma-lookup.ndjson \
 *     $N/npx jest --silent --watchman=false --testTimeout=180000 --roots "$PWD" \
 *       --roots "$STAGE/harness/oracle/cases" --runTestsByPath "$STAGE/harness/oracle/cases/brahma-lookup-tools.test.ts"
 */

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';

type Tool = 'lookup_character' | 'lookup_memories' | 'lookup_messages' | 'read_messages';
type Plant = { table: string; column: string; expr: string };
interface Case {
  label: string;
  tool: Tool;
  args: unknown;
  /** Run through `executeToolCallWithContext` with this `operatorSurface`. */
  executor?: boolean;
  /** A throwing plant: `column` rebuilt as `<expr>` VIRTUAL after the rows. */
  plant?: Plant;
}

const F = 'c7400000-0000-4000-8000-000000000001';
const ABI_ARCH = 'c7400000-0000-4000-8000-000000000004';
const CH1 = 'd7400000-0000-4000-8000-000000000001';
const CH5 = 'd7400000-0000-4000-8000-000000000005';
const msg = (n: number) => `b7400000-0000-4000-8000-${String(n).padStart(12, '0')}`;
const NAME_THROWS: Plant = { table: 'characters', column: 'name', expr: `json("identity")` };
const MEMCHAR_THROWS: Plant = { table: 'memories', column: 'characterId', expr: `json("content")` };
const CONTENT_THROWS: Plant = { table: 'chat_messages', column: 'content', expr: `json("role")` };
const CHATTYPE_THROWS: Plant = { table: 'chats', column: 'chatType', expr: `json('not json')` };
const MSG_ROLE_THROWS: Plant = { table: 'chat_messages', column: 'role', expr: `json("content")` };
const CH1_THROWS: Plant = {
  table: 'chats',
  column: 'chatType',
  expr: `CASE WHEN "id" = '${CH1}' THEN json('not json') ELSE 'salon' END`,
};

const CASES: Case[] = [
  // lookup_character
  { label: 'lc_by_id', tool: 'lookup_character', args: { character: F } },
  { label: 'lc_by_id_mixed_case', tool: 'lookup_character', args: { character: F.toUpperCase() } },
  { label: 'lc_by_name', tool: 'lookup_character', args: { character: '  friday ' } },
  { label: 'lc_by_alias', tool: 'lookup_character', args: { character: 'Fri' } },
  { label: 'lc_substring_unique', tool: 'lookup_character', args: { character: 'lau' } },
  { label: 'lc_ambiguous', tool: 'lookup_character', args: { character: 'an' } },
  { label: 'lc_live_over_archived', tool: 'lookup_character', args: { character: 'Abigail' } },
  { label: 'lc_archived_by_id', tool: 'lookup_character', args: { character: ABI_ARCH } },
  { label: 'lc_none', tool: 'lookup_character', args: { character: 'Nobody' } },
  { label: 'lc_blank_ref', tool: 'lookup_character', args: { character: '   ' } },
  { label: 'lc_conversations_sorted_ties', tool: 'lookup_character', args: { character: 'F' } },
  { label: 'lc_physical_fallback_chain', tool: 'lookup_character', args: { character: 'Zed' } },
  { label: 'lc_user_controlled', tool: 'lookup_character', args: { character: 'Laura' } },
  { label: 'lc_no_conversations_annabel', tool: 'lookup_character', args: { character: 'Annabel' } },
  { label: 'lc_extra_key_stripped', tool: 'lookup_character', args: { character: 'Friday', extra: 1 } },
  // MEASURED: v4's `characters.findAllRaw` is a FALLBACK read (`[]`), so a
  // broken `characters` table resolves no one — it never reaches the catch.
  { label: 'lc_fallback_characters', tool: 'lookup_character', args: { character: 'Friday' }, plant: NAME_THROWS },
  { label: 'lc_throw_probe_chats', tool: 'lookup_character', args: { character: F }, plant: CHATTYPE_THROWS },
  // lookup_memories
  { label: 'lm_all_holders', tool: 'lookup_memories', args: { query: 'birthday' } },
  { label: 'lm_one_holder', tool: 'lookup_memories', args: { query: 'birthday', character: 'Friday' } },
  { label: 'lm_about', tool: 'lookup_memories', args: { query: 'birthday', about: 'Friday' } },
  { label: 'lm_about_unresolved', tool: 'lookup_memories', args: { query: 'birthday', about: 'Nobody' } },
  { label: 'lm_character_ambiguous', tool: 'lookup_memories', args: { query: 'birthday', character: 'an' } },
  { label: 'lm_limit_30', tool: 'lookup_memories', args: { query: 'birthday', limit: 30 } },
  { label: 'lm_limit_string', tool: 'lookup_memories', args: { query: 'sail', limit: '2' } },
  { label: 'lm_cold_included', tool: 'lookup_memories', args: { query: 'birthday', character: 'Friday', limit: 30 } },
  { label: 'lm_no_stamp', tool: 'lookup_memories', args: { query: 'storm vigil' } },
  { label: 'lm_clip_astral', tool: 'lookup_memories', args: { query: 'astral', character: 'Zed', limit: 1 } },
  { label: 'lm_phrase_quoted', tool: 'lookup_memories', args: { query: '"birthday cake"' } },
  { label: 'lm_orthogonal_query', tool: 'lookup_memories', args: { query: 'nothing matches here' } },
  { label: 'lm_unembedded_query', tool: 'lookup_memories', args: { query: 'zebra' } },
  { label: 'lm_empty_character_is_all', tool: 'lookup_memories', args: { query: 'sail', character: '' } },
  { label: 'lm_window_dateonly', tool: 'lookup_memories', args: { query: 'birthday', since: '2026-03-10', until: '2026-03-11' } },
  { label: 'lm_window_iso', tool: 'lookup_memories', args: { query: 'birthday', since: '2026-03-10T00:00:00.000Z' } },
  { label: 'lm_window_open_since', tool: 'lookup_memories', args: { query: 'birthday', until: '2026-03-12' } },
  // MEASURED: `memories.countByCharacterId` falls back to 0 (no holders).
  { label: 'lm_fallback_count', tool: 'lookup_memories', args: { query: 'birthday' }, plant: MEMCHAR_THROWS },
  { label: 'lm_throw_probe_search', tool: 'lookup_memories', args: { query: 'birthday', character: F }, plant: MEMCHAR_THROWS },
  { label: 'lm_load_chats_skip', tool: 'lookup_memories', args: { query: 'birthday', character: 'Friday', limit: 30 }, plant: CH1_THROWS },
  // lookup_messages
  { label: 'lmsg_by_character', tool: 'lookup_messages', args: { query: 'birthday', character: 'Friday' } },
  { label: 'lmsg_all_chats', tool: 'lookup_messages', args: { query: 'birthday' } },
  { label: 'lmsg_speaker', tool: 'lookup_messages', args: { query: 'birthday', speaker: 'Abigail' } },
  { label: 'lmsg_speaker_friday', tool: 'lookup_messages', args: { query: 'birthday', speaker: 'Friday' } },
  { label: 'lmsg_speaker_unresolved', tool: 'lookup_messages', args: { query: 'birthday', speaker: 'Nobody' } },
  { label: 'lmsg_window_string_compare', tool: 'lookup_messages', args: { query: 'birthday', since: '2026-03-10T10:01:00.000Z' } },
  { label: 'lmsg_window_dateonly', tool: 'lookup_messages', args: { query: 'birthday', since: '2026-03-15' } },
  // A seatless USER line (no participantId on either side), so the window's
  // string compare is provable without H1: out of the window, then in it.
  { label: 'lmsg_window_unseated_out', tool: 'lookup_messages', args: { query: 'toast', until: '2026-03-15T23:59:59.999Z' } },
  { label: 'lmsg_window_unseated_in', tool: 'lookup_messages', args: { query: 'toast', since: '2026-03-16' } },
  { label: 'lmsg_no_chats', tool: 'lookup_messages', args: { query: 'birthday', character: 'Zed' } },
  { label: 'lmsg_no_hits_note', tool: 'lookup_messages', args: { query: 'xylophone' } },
  { label: 'lmsg_phrase_quoted', tool: 'lookup_messages', args: { query: '"birthday cake"' } },
  { label: 'lmsg_snippet_ellipses', tool: 'lookup_messages', args: { query: 'wave' } },
  { label: 'lmsg_limit_1', tool: 'lookup_messages', args: { query: 'birthday', limit: 1 } },
  { label: 'lmsg_fallback_characters', tool: 'lookup_messages', args: { query: 'birthday', character: 'Friday' }, plant: NAME_THROWS },
  { label: 'lmsg_throw_probe_chats_all', tool: 'lookup_messages', args: { query: 'birthday' }, plant: CHATTYPE_THROWS },
  { label: 'lmsg_throw_probe_chats_by_character', tool: 'lookup_messages', args: { query: 'birthday', character: F }, plant: CHATTYPE_THROWS },
  { label: 'lmsg_throw_probe_search', tool: 'lookup_messages', args: { query: 'birthday' }, plant: MSG_ROLE_THROWS },
  // read_messages
  { label: 'rm_unknown_chat', tool: 'read_messages', args: { conversation_id: 'd7400000-0000-4000-8000-000000000099' } },
  { label: 'rm_default', tool: 'read_messages', args: { conversation_id: CH1 } },
  { label: 'rm_around', tool: 'read_messages', args: { conversation_id: CH1, around_message_id: msg(2) } },
  { label: 'rm_around_before_after', tool: 'read_messages', args: { conversation_id: CH1, around_message_id: msg(7), before: 0, after: '1' } },
  { label: 'rm_around_not_spoken', tool: 'read_messages', args: { conversation_id: CH1, around_message_id: msg(3) } },
  { label: 'rm_established_fact_hidden', tool: 'read_messages', args: { conversation_id: CH1, around_message_id: msg(6) } },
  { label: 'rm_around_empty_is_offset', tool: 'read_messages', args: { conversation_id: CH1, around_message_id: '', count: 1 } },
  { label: 'rm_offset', tool: 'read_messages', args: { conversation_id: CH1, offset: 1, count: 2 } },
  { label: 'rm_negative_offset', tool: 'read_messages', args: { conversation_id: CH1, offset: -2 } },
  { label: 'rm_negative_beyond', tool: 'read_messages', args: { conversation_id: CH1, offset: -100 } },
  { label: 'rm_offset_past_end', tool: 'read_messages', args: { conversation_id: CH1, offset: 100 } },
  { label: 'rm_cast_dedupe', tool: 'read_messages', args: { conversation_id: CH1, count: 20 } },
  { label: 'rm_clip_2500', tool: 'read_messages', args: { conversation_id: CH1, around_message_id: msg(7), before: 0, after: 0 } },
  { label: 'rm_clip_astral', tool: 'read_messages', args: { conversation_id: CH5 } },
  { label: 'rm_removed_seat_chat', tool: 'read_messages', args: { conversation_id: 'd7400000-0000-4000-8000-000000000004' } },
  // MEASURED: `getMessages` falls back to [] (zero spoken lines).
  { label: 'rm_fallback_messages', tool: 'read_messages', args: { conversation_id: CH1 }, plant: CONTENT_THROWS },
  { label: 'rm_throw_probe_chat', tool: 'read_messages', args: { conversation_id: CH1 }, plant: CHATTYPE_THROWS },
  // invalid_* (the first Zod issue's bytes)
  { label: 'invalid_lc_nonobject', tool: 'lookup_character', args: 'Friday' },
  { label: 'invalid_lc_empty', tool: 'lookup_character', args: {} },
  { label: 'invalid_lc_empty_string', tool: 'lookup_character', args: { character: '' } },
  { label: 'invalid_lc_number', tool: 'lookup_character', args: { character: 5 } },
  { label: 'invalid_lm_query_empty', tool: 'lookup_memories', args: { query: '' } },
  { label: 'invalid_lm_limit_0', tool: 'lookup_memories', args: { query: 'a', limit: 0 } },
  { label: 'invalid_lm_limit_31', tool: 'lookup_memories', args: { query: 'a', limit: 31 } },
  { label: 'invalid_lm_limit_x', tool: 'lookup_memories', args: { query: 'a', limit: 'x' } },
  { label: 'invalid_lm_limit_float', tool: 'lookup_memories', args: { query: 'a', limit: 2.5 } },
  { label: 'invalid_lm_limit_null', tool: 'lookup_memories', args: { query: 'a', limit: null } },
  { label: 'invalid_lm_limit_blank_string', tool: 'lookup_memories', args: { query: 'a', limit: '  ' } },
  { label: 'invalid_lm_character_number', tool: 'lookup_memories', args: { query: 'a', character: 5 } },
  { label: 'invalid_lm_two_issues_first_wins', tool: 'lookup_memories', args: { query: 3, limit: 99 } },
  { label: 'invalid_lmsg_limit_neg', tool: 'lookup_messages', args: { query: 'a', limit: -1 } },
  { label: 'invalid_lmsg_speaker_null', tool: 'lookup_messages', args: { query: 'a', speaker: null } },
  { label: 'invalid_rm_missing', tool: 'read_messages', args: {} },
  { label: 'invalid_rm_before_11', tool: 'read_messages', args: { conversation_id: CH1, before: 11 } },
  { label: 'invalid_rm_after_neg', tool: 'read_messages', args: { conversation_id: CH1, after: -1 } },
  { label: 'invalid_rm_count_21', tool: 'read_messages', args: { conversation_id: CH1, count: 21 } },
  { label: 'invalid_rm_count_0', tool: 'read_messages', args: { conversation_id: CH1, count: 0 } },
  { label: 'invalid_rm_offset_float', tool: 'read_messages', args: { conversation_id: CH1, offset: 1.5 } },
  { label: 'invalid_rm_array', tool: 'read_messages', args: [] },
  // The dispatcher: the operator gate and the envelope.
  { label: 'gate_lookup_character', tool: 'lookup_character', args: { character: 'Friday' }, executor: false },
  { label: 'gate_lookup_memories', tool: 'lookup_memories', args: { query: 'birthday' }, executor: false },
  { label: 'gate_lookup_messages', tool: 'lookup_messages', args: { query: 'birthday' }, executor: false },
  { label: 'gate_read_messages', tool: 'read_messages', args: { conversation_id: CH1 }, executor: false },
  { label: 'exec_lookup_character_operator', tool: 'lookup_character', args: { character: 'Laura' }, executor: true },
  { label: 'exec_read_messages_failure_operator', tool: 'read_messages', args: { conversation_id: 'nope' }, executor: true },
];

/** The plant's SQL: drop what references the column, then re-add it VIRTUAL. */
async function plantSql(rawQuery: (sql: string, params?: unknown[]) => Promise<unknown>, p: Plant): Promise<string[]> {
  const out: string[] = [];
  const refs = (await rawQuery(
    `SELECT type, name FROM sqlite_master WHERE (type = 'index' OR type = 'trigger') AND tbl_name = ? AND sql LIKE ?`,
    [p.table, `%${p.column}%`],
  )) as Array<{ type: string; name: string }>;
  for (const r of refs) out.push(`DROP ${r.type.toUpperCase()} "${r.name}"`);
  // FTS triggers on chat_messages name `content` via `new.content` etc.
  if (p.table === 'chat_messages') {
    const trig = (await rawQuery(
      `SELECT name FROM sqlite_master WHERE type = 'trigger' AND tbl_name = 'chat_messages'`,
    )) as Array<{ name: string }>;
    for (const t of trig) {
      const stmt = `DROP TRIGGER IF EXISTS "${t.name}"`;
      if (!out.includes(`DROP TRIGGER "${t.name}"`)) out.push(stmt);
    }
  }
  out.push(`ALTER TABLE "${p.table}" DROP COLUMN "${p.column}"`);
  out.push(`ALTER TABLE "${p.table}" ADD COLUMN "${p.column}" TEXT AS (${p.expr}) VIRTUAL`);
  return out;
}

test('brahma-lookup-tools oracle', async () => {
  const here = dirname(fileURLToPath(import.meta.url));
  const spec = JSON.parse(readFileSync(join(here, '..', 'fixtures', 'brahma-lookup-tools.json'), 'utf8')) as {
    testPepperBase64: string;
    userId: string;
    cannedEmbeddings: Record<string, number[]>;
  };
  const mainFixture = process.env.QT_FIXTURE_BL_MAIN;
  const mountFixture = process.env.QT_FIXTURE_BL_MOUNT;
  const llmFixture = process.env.QT_FIXTURE_BL_LLM;
  const outPath = process.env.QT_ORACLE_OUT;
  if (!mainFixture || !mountFixture || !llmFixture || !outPath) {
    throw new Error('QT_FIXTURE_BL_MAIN / _MOUNT / _LLM and QT_ORACLE_OUT must be set');
  }
  const cipherDriverPath = require('node:path').join(
    process.cwd(),
    'packages/quilltap/node_modules/better-sqlite3-multiple-ciphers',
  );
  // Hold v4's job host off (memory note `v4-oracle-job-host-and-log-capture`).
  (globalThis as Record<string, unknown>).__quilltapJobHost = {
    child: null,
    spawning: false,
    shuttingDown: true,
    childCrashed: false,
    restartTimestamps: [],
    signalHandlersInstalled: false,
    invalidationListeners: new Set(),
  };

  const lines: string[] = [];
  for (const c of CASES) {
    const scratch = mkdtempSync(join(tmpdir(), 'qt-bl-oracle-'));
    mkdirSync(join(scratch, 'data'), { recursive: true });
    const mainWork = join(scratch, 'main-work.db');
    const mountWork = join(scratch, 'mount-work.db');
    const llmWork = join(scratch, 'llm-work.db');
    copyFileSync(mainFixture, mainWork);
    copyFileSync(mountFixture, mountWork);
    copyFileSync(llmFixture, llmWork);

    process.env.ENCRYPTION_MASTER_PEPPER = spec.testPepperBase64;
    process.env.SQLITE_PATH = mainWork;
    process.env.SQLITE_MOUNT_INDEX_PATH = mountWork;
    process.env.SQLITE_LLM_LOGS_PATH = llmWork;
    process.env.QUILLTAP_DATA_DIR = scratch;
    delete process.env.SQLITE_WAL_MODE;
    process.env.LOG_LEVEL = 'debug';

    jest.resetModules();
    jest.doMock('better-sqlite3', () => jest.requireActual(cipherDriverPath));
    jest.doMock('@/lib/database/manager', () => jest.requireActual('@/lib/database/manager'));
    jest.doMock('@/lib/database/repositories', () => jest.requireActual('@/lib/database/repositories'));
    jest.doMock('@/lib/repositories/factory', () => jest.requireActual('@/lib/repositories/factory'));
    jest.doMock('@/lib/database/backends/sqlite/client', () => jest.requireActual('@/lib/database/backends/sqlite/client'));
    jest.doMock('@/lib/database/backends/sqlite/mount-index-client', () =>
      jest.requireActual('@/lib/database/backends/sqlite/mount-index-client'),
    );
    jest.doMock('@/lib/database/backends/sqlite/llm-logs-client', () =>
      jest.requireActual('@/lib/database/backends/sqlite/llm-logs-client'),
    );
    jest.doMock('@/lib/embedding/vector-store', () => jest.requireActual('@/lib/embedding/vector-store'));
    jest.doMock('@/lib/mount-index/database-store', () => jest.requireActual('@/lib/mount-index/database-store'));
    jest.doMock('@/lib/embedding/embedding-service', () => {
      const actual = jest.requireActual('@/lib/embedding/embedding-service');
      const { EmbeddingError } = actual;
      return {
        __esModule: true,
        ...actual,
        generateEmbeddingForUser: async (text: string) => {
          const vec = spec.cannedEmbeddings[text];
          if (!vec) throw new EmbeddingError(`no canned embedding registered for input (${text.length} chars)`);
          return { embedding: new Float32Array(vec), model: 'canned', dimensions: vec.length, provider: 'canned' };
        },
      };
    });

    const { logger } = await import('@/lib/logger');
    const logs: Array<{ level: string; message: string; fields: Record<string, unknown> }> = [];
    const internal = logger as unknown as { transports: Array<{ write: (d: unknown) => void }> };
    internal.transports.splice(0, internal.transports.length, {
      write: (d: unknown) => {
        const data = d as { level: string; message: string; context?: Record<string, unknown>; error?: { message: string } };
        const ctx = (data.context ?? {}) as Record<string, unknown>;
        if (ctx.service !== 'BrahmaLookup') return;
        const { environment: _e, ...fields } = ctx;
        if (data.error) fields.error = data.error.message;
        logs.push(JSON.parse(JSON.stringify({ level: data.level, message: data.message, fields })));
      },
    });

    const { initializeDatabase, closeDatabase, rawQuery } = await import('@/lib/database/manager');
    await initializeDatabase();
    const plants = c.plant ? await plantSql(rawQuery as never, c.plant) : [];
    for (const sql of plants) await rawQuery(sql);
    (await import('@/lib/memory/index-store')).memoryIndex.reloadAll();

    const dump = async (table: string) => {
      try {
        return JSON.stringify(await rawQuery(`SELECT * FROM "${table}" ORDER BY rowid`));
      } catch (e) {
        return `throws: ${e instanceof Error ? e.message : String(e)}`;
      }
    };
    const before = [await dump('memories'), await dump('chats'), await dump('chat_messages')];

    let resultJson: string;
    if (c.executor !== undefined) {
      const { executeToolCallWithContext } = await import('@/lib/chat/tool-executor');
      const context: Record<string, unknown> = {
        chatId: CH1,
        userId: spec.userId,
        ...(await import('@/lib/wardrobe/turn-containers')).newWardrobeTurnContainers(),
      };
      if (c.executor) context.operatorSurface = true;
      const r = await executeToolCallWithContext({ name: c.tool, arguments: c.args as never }, context as never);
      resultJson = JSON.stringify({ toolName: r.toolName, success: r.success, result: r.result ?? null, error: r.error ?? null });
    } else {
      const h = await import('@/lib/tools/handlers/brahma-lookup-handler');
      const fn = {
        lookup_character: h.executeLookupCharacterTool,
        lookup_memories: h.executeLookupMemoriesTool,
        lookup_messages: h.executeLookupMessagesTool,
        read_messages: h.executeReadMessagesTool,
      }[c.tool];
      resultJson = JSON.stringify(await fn(c.args, { userId: spec.userId }));
    }
    const after = [await dump('memories'), await dump('chats'), await dump('chat_messages')];
    const unchanged = before.every((b, i) => b === after[i]);

    lines.push(
      JSON.stringify({
        label: c.label,
        tool: c.tool,
        args: c.args,
        executor: c.executor ?? null,
        plants,
        resultJson,
        logs,
        unchanged,
      }),
    );
    const { closeMountIndexSQLiteClient } = await import('@/lib/database/backends/sqlite/mount-index-client');
    closeMountIndexSQLiteClient();
    await closeDatabase();
    rmSync(scratch, { recursive: true, force: true });
  }
  writeFileSync(outPath, lines.join('\n') + '\n');
  process.stderr.write(`brahma-lookup oracle wrote ${outPath} (${lines.length} cases)\n`);
});
