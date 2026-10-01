# Survey: bug 173 (raw CLI SQL decodes compressed text) + the round's riders, for P4.D240

**Date:** 2026-09-30. **v4:** `ca363178d` (checkout clean AT it, so reading the
checkout reads the round target). **v5:** `main` at `735cf568e`. **Baseline:**
`97b25fc53`. Read-only: nothing built, nothing run beyond `git show`/`diff`/`md5`
and four pure-Node `console.table`/`zlib` probes (Node 24.13.1, no DB, no v4
tree touched).

**Headline findings**

1. **v5 already has the codec.** `quilltap_core::db::text_compression::{is_compressed_text_blob,
   decode_blob}` are `pub`, total (never fail), and mirror v4's CLI
   `isCompressedTextBlob` / `decodeText` arm for arm. The CLI already imports
   from that module (`dbopen.rs:134`). **No core change and no re-export needed.**
2. **Exactly ONE Tier R case reds at the new pin:** `db raw blob read`
   (`cli_differential.rs:2032-2036`). No other case reads a compressed cell
   through raw SQL, and no case has an embedding BLOB in a raw SELECT.
3. **The ledger's "both printers need the decode" premise is FALSE.** v5's
   `--repl` is a loud refusal (`db_cmd.rs:122-127`, "recognized but not yet
   available"), deferred since P4.3. So v4's `--repl` half has no v5 site; there
   is only the one raw-SQL reader branch.
4. **A pre-existing table-mode divergence becomes reachable.** Node's
   `console.table` renders a Buffer cell as `<Buffer eb 01 02 03>`. v5's
   `vtable::cell_text` renders it as compact JSON
   `{"type":"Buffer","data":[235,1,2,3]}`. `nodefmt.rs:9-10` documents this as
   a seam ("blobs stay out of table-rendered SQL results"). After the fix, a
   table-mode `SELECT *` on `chat_messages` prints decoded text next to an
   embedding Buffer, which is the most natural query an operator writes. The
   order must either port Node's Buffer inspect form or keep the "untouched"
   arm `--json`-only (v4's own test does it `--json`-only).
5. **`cell_to_js_value` has four other callers** (`db_characters.rs:198`,
   `docs_cmd.rs:387`, `sync_cmd.rs:201`, `db_cmd.rs:328`). v4 changed none of
   their paths, so the decode must go in the raw-SQL reader branch only. Putting
   it in the shared function would change three un-moved verbs.

---

## §A v4

### A.1 The `ddf942635` hunks (code)

`packages/quilltap/lib/text-codec.js`: one function added, exports widened (md5
at `ddf942635` = at `ca363178d` = `5842febf61bcd929cc84cb142ed7e28b`, 90 lines):

```js
function decodeCompressedTextInRows(rows) {          // :70
  let decoded = 0;
  if (!Array.isArray(rows)) return decoded;          // :72
  for (const row of rows) {
    if (!row || typeof row !== 'object') continue;
    for (const key of Object.keys(row)) {
      if (isCompressedTextBlob(row[key])) {
        row[key] = decodeText(row[key]);
        decoded++;
      }
    }
  }
  return decoded;                                    // :82
}
module.exports = { decodeText, decodeCompressedTextInRows, isCompressedTextBlob, registerTextCodecFunction }; // :85
```

`packages/quilltap/bin/quilltap.js`: `+require` at `:17`. The raw-SQL reader
branch gets `decodeCompressedTextInRows(rows);` at `:1047` (with a
`// Compressed text columns arrive as Buffers; print them as text (bug 173).`
comment at `:1046`). The `--repl` SQL reader branch gets the same call at
`:1116`. The return value is ignored at both sites. **The `bin/quilltap.js` md5
is identical at `ddf942635` and `ca363178d`** (`2afa4266…`). After `ddf942635`
the only `packages/quilltap/` change is `ca363178d`'s `package.json` stamp, so
Tier R at either pin compares the same CLI.

Pre-existing helpers (unchanged):

| fn | lines | behaviour |
|---|---|---|
| `isCompressedTextBlob(v)` | `:27-34` | `false` unless `Buffer.isBuffer(v) && v.length >= 3 && v[0]===0x51 && v[1]===0x01 && v[2]===0x01`. Non-Buffer, `null`, short buffer → `false` |
| `decodeText(v)` | `:37-49` | `null`/`undefined` → `null`; string → itself; non-Buffer → `String(v)`; Buffer without header → `toString('utf-8')`; header → `brotliDecompressSync(payload).toString('utf-8')`, **catch → `payload.toString('utf-8')`**. Documented "never throws" |

Through `decodeCompressedTextInRows`, only header-bearing Buffers reach
`decodeText`, so in practice only the last arm is used. Probed edge cases (Node
24.13.1):

- Header plus an empty payload: brotli throws `unexpected end of file`, so the
  catch returns `""`.
- Header plus a non-brotli payload: throws `Decompression failed`, so the catch
  returns the payload as lossy UTF-8.
- A truncated stream throws, and the catch returns the payload bytes.
- **Trailing junk after a complete stream is IGNORED.** Node returns the
  decoded text.

**The CLI never sees a throw from this path.**

### A.2 The printers at `ca363178d`

The raw-SQL branch is `bin/quilltap.js:1042-1060`:

```js
    } else if (sql) {                                   // :1042
      const stmt = db.prepare(sql);
      if (stmt.reader) {
        const rows = stmt.all();
        // Compressed text columns arrive as Buffers; print them as text (bug 173).
        decodeCompressedTextInRows(rows);               // :1047
        if (asJson) {
          console.log(JSON.stringify(rows, null, 2));   // :1049
        } else if (rows.length === 0) {
          console.log('(no results)');                  // :1051
        } else {
          console.table(rows);                          // :1053
        }
      } else {
        const info = stmt.run();                        // writers untouched
```

The `--repl` branch starts at `:1060`. Its SQL arm is at `:1112-1122`:
`const rows = stmt.all(); decodeCompressedTextInRows(rows); if (rows.length === 0) console.log('(no results)'); else console.table(rows);`.
`.tables/.schema/.cols/.find` are untouched.

- The decode runs **before** the `rows.length === 0` check (on empty rows it is
  a no-op, and `--json` prints `[]`). It **mutates in place**: `row[key] = …`
  reassigns an existing own key, so the **key order is preserved**.
- `stmt.reader` is true for `… RETURNING` writers too, so `--write "UPDATE …
  RETURNING content"` also decodes.
- **Value type in output:** a decoded cell becomes a JS string. `--json` now
  prints `"…"` where it printed `{"type":"Buffer","data":[…]}`. `console.table`
  now prints a quoted inspect string (`'…'`, `\n` escaped) where it printed
  `<Buffer 51 01 01 …(50 bytes)… ... N more bytes>`.

Node `console.table` facts the lane's twin must match (all probed):

| input | Node prints | v5 `vtable` today |
|---|---|---|
| a string with `\n` and `'` | `"line1\nline2 it's"` (escaped, quote-switched) | same (`inspect_quote`) |
| a string > 10,000 UTF-16 units | **truncated**: `'yyyy…'... 5 more characters` (`maxStringLength` 10000) | **NOT truncated**: DIVERGES |
| a string with C1 `\u0085` | `\x85` | raw (documented seam) |
| `'日本語'` | column width 8 (East-Asian wide = 2 cells) | width counted in `char`s: DIVERGES (documented seam) |
| Buffer `[eb 01 02 03]` | `<Buffer eb 01 02 03>` | `{"type":"Buffer","data":[235,1,2,3]}`: DIVERGES (documented seam) |
| Buffer, 0 / 50 / 51 bytes | `<Buffer >` / 50 hex pairs / `... 1 more byte` (singular) | n/a |

### A.3 The v4 integration test

The test is
`__tests__/unit/packages/quilltap/db-raw-sql-qt-text.integration.test.js`
(158 lines at `ca363178d`). It runs under `npm run test:integration`.

**Fixture.** The DB is PLAIN: the test writes no `.dbkey`, so `loadDbKey`
returns `null`. The DDL is
`chat_messages (id, chatId, content TEXT, embedding BLOB, updatedAt)`, plus the
FTS5 table, the map, and an AFTER UPDATE trigger that calls `qt_text`. It holds
two rows:

- `m-1`: `content = compressText(LONG_TEXT)` with
  `embedding = Buffer.from([0xeb,1,2,3])`.
- `m-2`: `content = SHORT_TEXT` with `embedding = NULL`.

`LONG_TEXT` is `'The Tuesday-night pie was, as ever, an act of considerable optimism. ' + 'x'.repeat(600)`.

Assertions:

- `:115-121` `--json 'SELECT id, content FROM chat_messages ORDER BY id'` →
  `expect(rows[0].content).toBe(LONG_TEXT); expect(rows[1].content).toBe(SHORT_TEXT);`
- `:123-127` table `SELECT content … WHERE id = 'm-1'` →
  `toContain(LONG_TEXT.slice(0, 40))`, `not.toContain('Buffer')`
- `:129-137` `--repl` with stdin `SELECT content … WHERE id = 'm-1'\n` → the same two
  assertions.
- `:139-143` `--json 'SELECT embedding … m-1'` →
  `expect(rows[0].embedding).toEqual({ type: 'Buffer', data: [0xeb, 1, 2, 3] });`
  (the "untouched" half is **`--json` only**).
- Bug-162 cases kept: `:105` the `qt_text` substr, `:145` the `--write`
  UPDATE under the trigger, `:154` `--count`/`--tables`.

### A.4 The docs and help hunks (verbatim)

`help/database-protection.md` (inserted after the `--data-dir … --tables`
fence, before `### Tidying Up the Premises`):

> A word on the longer messages: Quilltap keeps any message of 512 bytes or more folded up in compressed form, the way a sensible traveller rolls shirts rather than packing them flat. When a query *returns* such a column — `SELECT content FROM chat_messages`, say — the command unrolls it for you, so the text prints as text. When you need to work on the text *inside* the query (a `WHERE … LIKE`, a `substr`, a `json_extract`), wrap the column in `qt_text()` first: `SELECT id FROM chat_messages WHERE qt_text(content) LIKE '%pie%'`.

`packages/quilltap/README.md` (one line replaced):

```
-Compressed text columns (`chat_messages.content`, `llm_logs.request` / `response` and friends) are stored as BLOBs. Wrap them in `qt_text()` to read the text: `SELECT qt_text(content) …`.
+Compressed text columns (`chat_messages.content`, `llm_logs.request` / `response` and friends) are stored as BLOBs. A raw query's output decodes them for display, so `SELECT content …` prints text. Inside SQL — `WHERE`, `LIKE`, `substr`, `json_extract` — wrap the column in `qt_text()` to work on the text: `SELECT json_extract(qt_text(response), '$.error') …`.
```

Root `README.md`: the version badge `dev.100` → `dev.101` (NO-PORT).

The version stamps per commit (root = `packages/quilltap`): `97b25fc53`,
`aa92cf91c` and `a67a282c6` are `4.10.0-dev.100`; `ddf942635` is `-dev.101`;
`ca363178d` is **`-dev.105`**.

---

## §B v5 on `main`

### B.1 The CLI crate

**`crates/quilltap-cli/src/nodefmt.rs`** (261 lines):

- `:1-10` is the module doc. **`:9-10` is the seam** "blobs stay out of
  table-rendered SQL results".
- `js_num_string` `:18`; `js_number_to_json` `:29`.
- **`cell_to_js_value` `:50-67`**: Null → null, Integer/Real →
  `js_number_to_json`, Text → `from_utf8_lossy`, **Blob → `{"type":"Buffer","data":[…]}`**.
- `json_stringify_pretty` `:72`; `inspect_quote` `:81-113`; `node_join` `:118`;
  `node_normalize` `:125`; `node_resolve` `:154`; `utf16_len` `:165`;
  `slice_utf16` `:173`; `js_parse_int` `:180`; `format_bytes` `:195`;
  tests `:214-261`.

**The doc comment that is now false** (`:41-49`):

```
/// MEASURED at v4 `f45a517a9` (P4.D203): the Buffer form is CORRECT for the
/// raw-SQL path and must not be decoded. v4's CLI decodes compressed columns
/// in exactly three verbs — `cmdMessages`, `cmdMessage` and `cmdLog`
/// (`db-commands.js:484,570,616-617`), all three of which v5 does not ship —
/// while `quilltap db "<SQL>"` (`bin/quilltap.js:1049-1059`) does plain
/// `JSON.stringify(rows)` / `console.table(rows)` over whatever
/// better-sqlite3 returns. A compressed cell therefore prints as a Buffer on
/// BOTH sides. What makes the path usable is the `qt_text()` registration in
/// `db_cmd::open_encrypted`: the operator wraps the column in their own SQL.
```

`cell_to_js_value` callers: `db_cmd.rs:328` (the `--count` value) and
`db_cmd.rs:357` (**the raw-SQL reader, the ONLY bug-173 site**);
`db_characters.rs:198`; `docs_cmd.rs:387`; `sync_cmd.rs:201`. v4 moved none of
the last three paths.

**`crates/quilltap-cli/src/db_cmd.rs`** (1,010 lines):

- `:1-4` module doc; imports `:14-18` (`cell_to_js_value`, `console_table`).
- `--repl` parsed at `:99`. **`:122-127` refuses it:**
  `out::elog("Error: db --repl is recognized but not yet available in this build of the quilltap CLI."); out::exit(1);`.
  Recorded deferral: `status-log.md:142095`, `p4.d214…:295`,
  `p4.3-quilltap-cli.md:183`. **There is no v5 REPL to decode in.**
- Open: `open_encrypted(&db_path, pepper.as_deref(), …)` at `:192-208`. A
  missing `.dbkey` means `None`, which opens plain, as in v4.
- `dispatch` is at `:305-392`. The raw SQL path is `:341-387`: `prepare`
  `:342`; the reader test is `stmt.column_count() > 0` at `:343` (matches
  `stmt.reader`, RETURNING included); column names `:345-350`; the row loop
  `:352-361` builds an insertion-ordered `Map` with
  **`cell_to_js_value(row.get_ref(idx)…)` at `:357`**; `--json` `:362-365`
  (`json_stringify_pretty` of the array); `(no results)` `:366-367`; table
  `:368-369` (`console_table`). The writer arm is `:371-385`.

**`crates/quilltap-cli/src/vtable.rs`** is the `console.table` twin.
`cell_text` (`:21-31`) handles null, bool, number, `String → inspect_quote`, and
**`other → serde_json::to_string`** (`:29`, "documented seam"). That last arm is
why a Buffer renders as compact JSON. Widths are counted in `char`s (`:9-10`).
`console_table` is at `:35-118`.

**The core codec**, `crates/quilltap-core/src/db/text_compression.rs`
(`pub mod text_compression` at `db/mod.rs:126`; `pub mod db` at `lib.rs:174`):

| item | line | API |
|---|---|---|
| `TEXT_BLOB_MAGIC/VERSION/CODEC_BROTLI/HEADER_BYTES` | `:127-133` | `pub const` |
| `is_compressed_text_blob(value: &[u8]) -> bool` | `:187` | the three-byte check, `len >= 3` |
| `blob_to_text(ValueRef) -> Option<String>` | `:271` | v4 `blobToText`; total |
| **`decode_blob(bytes: &[u8]) -> String`** | `:287-302` | header absent → lossy UTF-8; brotli `Ok` → lossy UTF-8; **`Err(_)` → `from_utf8_lossy(payload)`**. No error type; total. Its doc already names "the CLI formatter" as a consumer |
| `register_qt_text` | `:379` | already used by `dbopen.rs:134` |

Equivalence with v4's CLI `decodeText`: arm for arm. Rust's `brotli`
`BrotliDecompress` (brotli-decompressor 5.0.3, `lib.rs:263`) **`break`s on
`ResultSuccess` and returns `Ok(())` with unread input left**, so trailing junk
is ignored as in Node (read from source, not run). `from_utf8_lossy`
replaces maximal subparts, the same practice as Node's WHATWG decoder.

**Implementation shape the lane can take with zero core edits:** in
`db_cmd.rs:357`, match `ValueRef::Blob(b) if is_compressed_text_blob(b) =>
Value::String(decode_blob(b))`, else `cell_to_js_value(...)`. Put it in a
named CLI helper (e.g. `nodefmt::raw_sql_cell_to_js_value`) so the four other
callers keep the Buffer form. Doing it per cell at build time is equivalent to
v4's in-place pass: same keys, same order, and every cell is either a
header-bearing Buffer or untouched.

### B.2 Tier R: `crates/quilltap-cli/tests/cli_differential.rs` (4,376 lines)

(The order's path `crates/quilltap-harness/tests/cli_differential.rs` does not
exist. The file lives in the CLI crate.)

- **The recipe header is at `:1-18`:**
  `QT_V4_CHECKOUT=~/source/quilltap-server QT_NODE=$N/node cargo test -p quilltap-cli --test cli_differential`.
  The gate is at `:1812-1823`: unset `QT_V4_CHECKOUT` means `SKIP:`. The v4
  binary is `<checkout>/packages/quilltap/bin/quilltap.js`, run with `QT_NODE`
  (default `node`).
- **There is no `cases.json`.** Cases are inline `ctx.case(...)` /
  `ctx.case_with(...)` calls in `fn cli_differential` (`:1811`). Each case
  resets `live/` from `master/` and runs v4 and then v5 (`:440-447`).
  `compare` (`:454+`) byte-diffs stdout, stderr and the exit code, with only
  the opt-in normalizers (heartbeat, recall, reach, bak). **Table output is
  compared raw, not normalized.** The total prints at `:4365`
  ("CLI differential: N cases"). No literal asserts 266.
- **The fixture is built by `build_master` (`:501+`), in the same file.**
  instA is encrypted: `dbkey::save_dbkey(&data_a, PEPPER, "")` with
  `Writer::open_writable`. The `chat_messages` grow (P4.D214) is at
  `:561-597`. It uses v4's bug-162 DDL **without `embedding`** (`:567-572`).
  Row `m-1` = `text_to_blob(long_text)` (the same `LONG_TEXT`), asserted
  `is_blob()` (`:589-590`). There is no `m-2`, and no embedding column.
- Raw-SQL cases (the `d(...)` = `db --data-dir <instA>` helper is at `:1836`):

| case | line | reads compressed? | at the new pin |
|---|---|---|---|
| `db select table` / `db select json` / `db select empty` / `db select empty json` | `:1878-1901` | no (`widgets`) | green |
| `db qt_text read` | `:2024-2031` | via `qt_text` (already text) | green |
| **`db raw blob read`** | **`:2032-2036`** (`--json "SELECT content FROM chat_messages LIMIT 1"`) | **YES, raw** | **RED** (v4 prints the string, v5 Buffer JSON) |
| `db write under fts trigger` | `:2037-2045` | writer | green |
| `db count chat_messages` / `db tables after grow` | `:2046-2055` | no | green |
| `--write "SELECT 1"` ×2 | `:2082`, `:2124` | no | green |

  **No `--repl` case exists** (v5 refuses it; v4 would block on stdin). **No
  case selects an embedding BLOB through raw SQL.** The docs verbs read
  `doc_mount_chunks.embedding` (`:956` plant), but through `docs_cmd`, which
  is unchanged.
- **Planting the new row:** grow `build_master`'s DDL to v4's
  `ca363178d` shape (add `embedding BLOB`, `:567-572`). Bind
  `vec![0xeb,1,2,3]` as `m-1`'s embedding and add v4's `m-2`
  (`SHORT_TEXT`, NULL embedding). No existing case selects `*` from
  `chat_messages`, so the grow is neutral to the other cases (verify: re-run
  all). Candidate cases mirroring v4 `:115-143`:
  - `--json` id+content ORDER BY id. This is the "both rows come back as
    strings" proof.
  - The table form `content WHERE id='m-1'`. It is ASCII, 669 chars, so it
    stays under Node's 10,000 cap and v5's `inspect_quote` is byte-identical.
  - `--json` embedding (the `{"type":"Buffer","data":[235,1,2,3]}` half).
  - The table form of the embedding **only if** the lane ports Node's
    `<Buffer …>` inspect into `vtable::cell_text`; otherwise it is a designed
    red.

  The existing `db raw blob read` turns green by itself. Rename or re-comment it
  rather than adding a duplicate.

### B.3 Other raw-row printers

`"Buffer"` appears outside tests only in `nodefmt.rs` and
`quilltap-core/src/services/backup/marshal.rs` (the backup marshal, not a
printer). No `quilltap-web` or Tauri SQL console renders raw rows.
`tools/run_sql.rs` (core) is v4's `lib/` `run_sql` tool, a different path
that `ddf942635` did not touch. **The CLI raw-SQL reader is the only site.**

---

## §C Riders

### C.1 `help/` (129 → 129)

The file count is 129 at `97b25fc53` and 129 at `ca363178d`, and v5's `help/`
also holds 129. `diff -rq help ~/source/quilltap-server/help` lists **exactly
these nine** and nothing else, so v5 equals `97b25fc53` everywhere else. No
adds and no deletes.

| page | md5 at `ca363178d` | v5 md5 now | moved by | heading added |
|---|---|---|---|---|
| `ai-character-import.md` | `8d5624e86de895e45baa8b7793b85cc9` | `14cff5d3…` | `ca363178d` | – |
| `character-creation.md` | `39a4d850b327a1a541a9f2199b5ad8e5` | `ed3f1c03…` | `ca363178d` | – |
| `character-external-prompt.md` | `8a746e1a1bb0d3079442458eb29603de` | `da138f50…` | `ca363178d` | – |
| `character-optimizer.md` | `9e871634aad1d6f779cea73552531ec1` | `38d7af42…` | `ca363178d` | `## Committees Are Corrected, Not Codified` |
| `chat-multi-character.md` | `a0a57cb52f4786f1415826475ff9dc13` | `dabeccac…` | `ca363178d` | `### Your Narration Stands` |
| `database-protection.md` | `1cb6b735188d470706d76180e8087604` | `8e5a64d8…` | `ddf942635` | – (one paragraph) |
| `episodic-memory.md` | `49253a5d31722452742f9c3f062da366` | `8d2df1db…` | `ca363178d` | – |
| `memory-playing-a-character.md` | `20442eaf24ed32c790157147f25aa5de` | `a0cd29d8…` | `ca363178d` | `## What Counts as an Agreement` |
| `prompts.md` | `017a0ee3bf14fbb462746ccfc0cb4b36` | `ac27eb0b…` | `ca363178d` | `#### Whose story it is` |

Diff size: 43 insertions and 1 deletion. **Four pages gain a heading**, so the
section-chunk counts move. `help_tree_equivalence` and
`help_section_size_equivalence` must be regenerated at the pin; a stale oracle
would red on them. Eight of the nine are `ca363178d`'s, documenting behaviour
the anti-committee lane ports (copying the text is harmless; the tree is this
lane's).

**Guards:**

| guard | where | role |
|---|---|---|
| `help_tree_embed_guard.rs` | `:76` `VENDORED_FILE_COUNT: usize = 129` (history comment `:40-75`, incl. `:46` naming `database-protection.md`) | embedded vs DISK only: **cannot see a stale re-vendor** |
| `host_help_docs_boot.rs` | `:105` `assert_eq!(expected, 129, "the vendored tree at v4 b0b6656b5")` | the second deliberate literal |
| `help_web_routes.rs` | `:102` derives `embedded_help_source_files().len()` | no literal (P4.D168) |
| `help_tree_equivalence.rs` | recipe `:20-33` (`QT_ORACLE_HELP_TREE`, jest `help-tree-sync`) | **the proof the tree is v4's** |
| `help_section_size_equivalence.rs` | reads the tree | re-run |

The four-literals-in-three-crates memory is now **two literals, two crates**
(harness + host), plus one derived site in web. `grep -rn '\b129\b' crates/`
finds no others. 129 stays 129, so none move. The other `help_*` families
(`help_doc_{chunking,ensure,slug,sync,sync_guards}`, `help_docs_{routes,tier2}`,
`help_snippet`, `help_system_prompt`, `help_tools`, `help_context_resolver`,
`help_chats_routes`, `help_chat_orchestrator_tier3`) do not read the tree
content (per P4.D238's measurement). The SPA's
`apps/web/src/app/help/__fixtures__/help-guide-tables.json` holds slugs only,
so it is unaffected.

**P4.D238's recipe** (`work-orders/p4.d238-…md:130-145`, §C.1):

> Copy the WHOLE tree from the `97b25fc53` pin (`diff -rq` EMPTY afterwards;
> md5s in the lane record); `help_tree_embed_guard.rs:76` and
> `host_help_docs_boot.rs` UNMOVED at 129 and RUN; `help_tree_equivalence` at
> the `97b25fc53` pin GREEN; the other 2 of 16 `help_*` families that read the
> TREE CONTENT re-run …

For this round, read it as "the whole tree from `ca363178d`".

### C.2 The `docs/v4/` mirror

The mirror lays out v4 `docs/` 1:1 under `docs/v4/`, plus one extra file:
`docs/v4/packages-quilltap-README.md` mirrors `packages/quilltap/README.md`.
Every lagging file below is byte-equal to its `97b25fc53` version, so the
mirror sits exactly at the baseline. `.DS_Store` differences are untracked
local noise.

| mirror path | v4 source | bytes at `ca363178d` | moved by |
|---|---|---|---|
| `docs/v4/CHANGELOG.md` | `docs/CHANGELOG.md` | 171,935 | all four |
| `docs/v4/developer/bugs.md` | same | 314,963 | `aa92cf91c`, `ddf942635` |
| `docs/v4/developer/bugs/fixed/bug-173-raw-sql-buffer-output.md` (**NEW**) | same | 6,477 | `aa92cf91c` (added in `bugs/`), `ddf942635` (moved to `fixed/`) |
| `docs/v4/developer/features/prompt-trust-and-anti-committee.md` (**NEW**) | same | 72,358 | `a67a282c6`, `ca363178d` |
| `docs/v4/developer/PROMPT_ARCHITECTURE.md` | same | 34,609 | `ca363178d` |
| `docs/v4/developer/SYSTEM_PROMPT_PLUGIN_DEVELOPMENT.md` | same | 29,638 | `ca363178d` |
| **`docs/v4/packages-quilltap-README.md`** (the ledger missed it) | `packages/quilltap/README.md` | 35,078 | `ddf942635` |

⚠ **Precedent:** P4.D238's §R.9 rule makes the mirror **the UNIFIER's**
("`docs/v4/**` are never written by a lane"). In that model a lane pre-lists
paths and byte counts, and the unifier copies. If P4.D240 is to own the
copy itself, the order must say so explicitly, or the lane should pre-list
only.

### C.3 The NO-PORT? commits (ratification evidence)

- **`aa92cf91c`** (2026-09-30 19:51 -0500, "File bug 173: …"):
  `M docs/CHANGELOG.md`, `M docs/developer/bugs.md`,
  `A docs/developer/bugs/bug-173-raw-sql-buffer-output.md`. 3 files,
  +103/−1.
- **`a67a282c6`** (2026-09-30 20:06 -0500, "Spec the prompt trust and
  anti-committee safeguards"): `M .claude/commands/update-documentation.md`,
  `M docs/CHANGELOG.md`,
  `A docs/developer/features/prompt-trust-and-anti-committee.md`. 3 files,
  +602.

Neither touches `lib/`, `app/`, `packages/`, `plugins/`, `components/`,
`help/` or any test. Both are NO-PORT, with mirror material only (C.2).
`.claude/commands/*` is the standing NO-PORT class.

### C.4 The version constants

The only `V4_APP_VERSION` in the tree is
`crates/quilltap-harness/tests/ai_import_tier3_equivalence.rs:115`
(`"4.10.0-dev.100"`, commented "v4 `97b25fc53` (the round target; moves with
the oracle baseline)"; used at `:668`). The target is **`4.10.0-dev.105`** at
`ca363178d`, or `-dev.101` at a `ddf942635` pin.

⚠ `ca363178d` changes `lib/services/ai-import.service.ts`, so the
**anti-committee lane re-records `ai_import_tier3`** and should own this
constant. P4.D240 should not touch it; record the handoff.

Other `dev.N` literals are fixture stamps, not pins:
`qtap_schema.rs:141` (`dev.1`, a schema example) and
`harness/oracle/fixtures/qtap-import-two-link-blob.qtap` (`dev.61`). v5's CLI
prints its own `CARGO_PKG_VERSION` (`main.rs:153,177`), and Tier R has no
`--version` case, so the v4 stamp never reaches Tier R.

---

## §D Traps and premises to measure in the lane

1. **The ledger premise "v5 has `--repl`, both printers need the decode" is
   REFUTED.** `--repl` is a refusal (`db_cmd.rs:122-127`). The lane ports one
   site. v4's `--repl` decode is banked by name for whoever ports the REPL.
   Implementing the REPL is out of scope (no stubs).
2. **Do not decode in the shared `cell_to_js_value`.** Its other callers are
   `db_characters`, `docs_cmd` and `sync_cmd`, and v4 left those paths alone. A
   mutation test should prove the decode is scoped: decode there, and the
   docs/characters families must not move.
3. **No throw exists to handle.** v4 `decodeText` and v5 `decode_blob` are both
   total. A corrupt or empty or truncated payload prints the payload as lossy
   UTF-8 (empty payload → `""`). If wanted, plant a header-plus-garbage blob in
   a Tier R row for byte parity of the fallback; the lossy rendering of
   non-UTF-8 must match Node's.
4. **Trailing bytes after a complete brotli stream** are ignored by both
   (Node probed; Rust read from source). Pin that only if a corpus row is cheap.
5. **`--json` key order** is preserved on both sides (v4 reassigns an existing
   key; v5 builds an insertion-ordered `Map` with `preserve_order`, already
   proven by `db select json`).
6. **Table mode, the decoded string:** the `inspect_quote` twin is already
   byte-exact for the ASCII `LONG_TEXT`. Three divergences now become
   operator-reachable, because decoded message text is long and often
   non-ASCII:
   - (a) Node truncates strings over 10,000 UTF-16 units with
     `'…'... N more characters`, and v5 does not.
   - (b) East-Asian wide width (v5 counts `char`s).
   - (c) C1 controls.

   Each is either ported or recorded as a named divergence. (a) is the likely
   one on real Friday messages. Keep the Tier R corpus inside the seams or port
   them.
7. **Table mode, the embedding Buffer:** Node prints `<Buffer eb 01 02 03>`
   (≤50 bytes, then ` ... N more byte(s)`, empty `<Buffer >`); v5 prints
   compact JSON (`vtable.rs:29`). Porting it is a recognizer in `cell_text` for
   the `{"type":"Buffer","data":[…]}` map that only `cell_to_js_value` emits.
   That would close the `nodefmt.rs:9-10` seam, but it touches `vtable.rs`,
   whose other callers (`db_characters`, `docs_cmd`) also gain the form. Measure
   whether any of their tables can carry a BLOB.
8. **Tier R reds at the new pin, unported: exactly 1** (`db raw blob read`).
   Measure it red-first at `ddf942635`/`ca363178d` before the port, and green at
   `97b25fc53`. That is the both-directions proof. The total grows by however
   many cases the lane adds (266 + N).
9. **Tier R compares table output byte-for-byte**, with no normalizer for SQL
   tables. Any table-form case is a full-byte claim.
10. **The pin.** The CLI is byte-identical at `ddf942635` and `ca363178d`, so
    the live checkout works as `QT_V4_CHECKOUT` while v4 HEAD stays
    `ca363178d`. Re-probe the HEAD before running. The ledger says PIN REQUIRED,
    so prefer a pinned worktree per the ledger's §2 recipe.
11. **Node ABI** (memory `oracle-node-abi-gotcha.md`): Tier R loads
    `packages/quilltap/node_modules/better-sqlite3-multiple-ciphers` under
    `QT_NODE`. The machine has Homebrew Node 26 (ABI 147) next to nvm 24.x
    (ABI 137). Verify with a real `new Database()` under the chosen `QT_NODE`
    before trusting a red. Use the lane-private rebuild if needed, and never
    `npm rebuild` the human's checkout.
12. **The `.dbkey`** (memory `quilltap-cli-needs-the-dbkey.md`): Tier R's instA
    has a sentinel `.dbkey`, which is fine. For a hand probe of a committed
    fixture, `ENCRYPTION_MASTER_PEPPER` alone gives the misleading "file is
    not a database". `--data-dir` is the instance root.
13. **Oracle before wording.** Updating the false `nodefmt.rs:41-49` comment
    and the `cli_differential.rs:2016-2022` P4.D214 block comment is the lane's
    job, and the case comments must stop claiming "Buffer on both sides". Change
    no user-facing CLI string (`db_help.txt`): v4's bin help did not move.
14. **The help re-vendor:** regenerate `help_tree_equivalence` and
    `help_section_size_equivalence` at the pin, because four pages gain a
    heading. The embed guard passes even on a stale tree.
15. **The README mirror** is one more lagging path than the ledger listed
    (C.2).

---

## §E Ownership (proposed for P4.D240)

**Edit:**

- `crates/quilltap-cli/src/db_cmd.rs`: the reader branch, `:341-370`.
- `crates/quilltap-cli/src/nodefmt.rs`: the new scoped helper; rewrite the
  `:37-49` doc; update the `:6-10` seam text.
- `crates/quilltap-cli/src/vtable.rs`: ONLY if §D.6/7 are ported (an order
  decision).
- `crates/quilltap-cli/tests/cli_differential.rs`: `build_master` `:561-597`
  (the DDL plus the planted rows) and the cases at `:2016-2055` plus new ones.
- `crates/quilltap-cli/Cargo.toml` (version line).
- `help/**`: the whole tree from `ca363178d`, with `diff -rq` empty.
- `docs/CHANGELOG.md` and `docs/developer/porting/status-log.md` (append only).
- This order's header.

**Run unmoved:**

- `crates/quilltap-harness/tests/help_tree_embed_guard.rs`
- `crates/quilltap-host/tests/host_help_docs_boot.rs`
- `crates/quilltap-web/tests/help_web_routes.rs`
- `help_tree_equivalence` and `help_section_size_equivalence` (regenerated at
  the pin)
- the CLI crate's unit tests
- `sync_report_equivalence` and `completion_behavior` (neutrality)

**Read-only:** `quilltap-core` entirely. `is_compressed_text_blob` and
`decode_blob` are already `pub` and reachable
(`quilltap_core::db::text_compression::…`, the same path `dbopen.rs:134`
uses), so no re-export or core edit is needed.

**Not this lane's:**

- `docs/v4/**`: the unifier's under the §R.9 precedent; pre-list per C.2
  unless the order hands it over explicitly.
- `crates/quilltap-harness/tests/ai_import_tier3_equivalence.rs`
  (`V4_APP_VERSION`): the anti-committee lane's.
- The 21 built-in prompts, `lib/` anti-committee code, and the drift ledger,
  `phase-4.md` and `dogfood-findings.md`.
