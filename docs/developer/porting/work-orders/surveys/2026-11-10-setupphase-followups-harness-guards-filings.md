# Survey E — harness / test-craft / guards / the sweep driver / vendored trees / v4 filing candidates

Survey of v5 `main` @ `8ad1a0640`, against v4 at the pin `624d0d25c`. Read-only: no builds, no tests, no regens. The only tools run were `git`, `ggrep`, and the sweep driver's two read-only classifiers (`recipe_sweep.py --list`, `--collisions`, under `/opt/homebrew/bin/python3.13`; the tree stayed clean).
Orders covered: P4.D360, P4.D361, P4.D382, P4.172, P4.174. Named items: P4.D361 Tier 2 items 13 / 14; the two standing reds; the workspace gate's env-block artifacts; the non-extractable / refused sweep families; the v4 filing candidates; the harness residue of the `624d0d25c` unification.
For the v4 filing table ONLY, v4 was also read at its live `main` (`0d73a4aee`, 48 commits past the pin).

**The harness-only fence used below:** `crates/quilltap-harness/**`, `harness/oracle/**`, `harness/tools/**` (the sweep driver lives at `harness/tools/recipe_sweep.py`, NOT under `scripts/`), `scripts/**`. Three items touch `//!` header comments or tests in `crates/quilltap-web/tests/**` / `crates/quilltap-host/tests/**` — zero `src`, but outside the strict fence; each is flagged.

**Counts:**

| Status | Count |
|---|---|
| OPEN | 14 |
| CLOSED (by measurement) | 9 |
| NO-ACTION | 7 |
| NEEDS-RULING | 3 |
| DEFER-TO-CATCH-UP | 1 |

Filing candidates: 14 inventoried (4 with a written draft, 10 without); all 14 still present at v4 `main`; none filed or fixed by v4 since the pin.

---

## 1. P4.D361 — the REAIM order

### D361-13 — the export-level census of the un-grepped oracle trees   [CLOSED — run read-only by this survey]
- Source: P4.D361 Tier 2 item 13 (`work-orders/p4.d361-…md:681-692`), "NOT RUN" in the status header.
- Method (the order's own): `git diff 236c04e35 624d0d25c -- lib app components hooks | grep '^-export'`, minus names re-added in the span.
  - 157 removed declaration names, 497 added, **82 gone**; plus the 10 removed `export { … }` list lines (16 names) and the 20 deleted / renamed non-test modules as `@/…` specifiers.
  - Grepped (whole-word) over `apps/web/oracle/**` and ALL of `harness/oracle/**` (a superset of the order's `providers` / `provision` / `lib` / `fixtures/build-*.ts`).
- Result: **zero live hits.** Every hit is one P4.D361 already recorded:
  - comments citing the killing commits (`inventory-keystone-tier2.ts:140-141`, `doc-enum.test.ts:189`, `doc-ui.test.ts:19,215`);
  - the local `normalizeInventoryFolderName` wrapper (`item-folders.ts:82`, the planner's C1 rebuild);
  - the false positive `POST` (a comment and two `.mjs` method defaults).
- Limit of the measurement: text only. It cannot see a name still exported at the pin under a changed SIGNATURE (the MOCK-INERT class). That class is found only by running the family.

### D361-14 — a census guard over `harness/oracle/**` for dead v4 specifiers   [OPEN — S/M, a NEW guard, recommendation]
- Source: P4.D361 Tier 2 item 14: "only if one exists to extend … otherwise record the recommendation".
- Measured: none exists. Of the 66 `*guard*` / `*census*` files in `crates/quilltap-harness/tests/`, eight mention `harness/oracle` and none of them resolves v4 specifiers. `/setupphase` does the IMPORT-DEAD census by hand each round (memory note `setupphase-import-dead-census-and-reaim-lane`).
- Proposed: a NEW `crates/quilltap-harness/tests/oracle_v4_specifier_census.rs`.
  - Reads `QT_V4_CHECKOUT` / `QT_V4_ROOT` (loud `SKIP:` when unset, as `common::v4_root` does).
  - Collects every `'@/…'` specifier in `harness/oracle/**` and `apps/web/oracle/**` — static imports, dynamic `import()`, `jest.mock` / `jest.doMock` / `requireActual` strings.
  - Asserts each resolves to a file at the v4 root (`.ts`, `.tsx`, `/index.ts`, `.json`).
  - Red-first: plant one dead specifier in a scratch copy.
- What it buys: the module-level half of the IMPORT-DEAD census becomes a gate red at every baseline move instead of a planning chore. The export-level half stays manual (it needs a TS parser).
- Trap: `zz-p4d330-*` staged copies (the order's "not specifiers" note) — exclude staged mirrors by directory, not by name pattern.
- Files owned: the one new test file.

### D361 Tier 3 items 15–21   [CLOSED ×3, NO-ACTION ×2, other areas ×2]
- 15 (SSE replacement rows / `storyteller/sse_tag.rs`): CLOSED — the file is gone on main (P4.D366).
- 16 (un-routing the transfers `GET`): CLOSED — `wardrobe_transfer_destinations` / `enumerate_destinations` have no hit under `crates/*/src` (P4.D378).
- 19 (`dump-prompt-templates.ts`'s `MODERN_GENERAL` doc line): CLOSED — `:20` now says "RETIRED" (P4.D373).
- 20 (`build-item-mutations-fixture.ts:3` historical comment), 21 (`mount-read.ts` → a jest case): NO-ACTION, as the order ruled.
- 17 / 18 (`documents-routes.test.ts:78-81`'s resolving `reindexSingleFile` stub; awaiting the document chain): still as recorded. They are P4.D380's (R-2 (A)) — **the documents survey's**, not a harness-lane item.

## 2. The two STANDING reds

### STAND-1 — `search_replace_equivalence` (`missing_action_recorded`)   [OPEN — S, harness-only, no ruling needed]
- What fails: the recorded-only middleware arm at `crates/quilltap-harness/tests/search_replace_equivalence.rs:845-869` expects the oracle row `missing_action` to be 400 `Action parameter required: execute or preview`. v4 answers 400 `Unknown action: ` (status-log `182081-182082`).
- Cause, measured — an ORACLE-CASE premise, not a v5 divergence:
  - `harness/oracle/cases/chat-dialogs-search-replace.test.ts:273` builds EVERY request as `…/search-replace?action=${c.action}`.
  - The `missing_action` case (`:430-435`, "No ?action= at all") sets `action: ''`, so it sends a BARE `?action=`, not an absent parameter.
  - v4 `ad1c4c37f` ("Consolidate API action dispatch into single primitive (#68)", 2026-09-24) made a bare `?action=` an UNKNOWN action: `lib/api/middleware/actions.ts:88-96` at the pin.
  - The route's default-handler copy still exists at the pin: `app/api/v1/search-replace/route.ts:104`, reached only when the parameter is ABSENT (`getActionParam` returns `null`).
- Since when: first recorded 2026-09-26 (status-log `152588`, "unrelated to the Concierge … recorded for the unifier"); carried through every sweep since (about 45 days, some 25 gates).
- Why left standing: no lane ever owned the family's recorded-only arm; each record classed it "pre-existing, not this lane's".
- v5 has no counterpart by design (no REST action string — module header `:15-19`), so nothing in `src` moves.
- Done looks like:
  - the case builds the URL WITHOUT `?action` when `c.action` is absent → the row reads 400 `Action parameter required: execute or preview` again;
  - a NEW recorded-only row `bare_action` (`?action=`) asserts 400 `Unknown action: ` — v4's Bug-74 rule, pinned by shape as its two siblings are;
  - the Rust list at `:847-853` gains the third tuple; `driven` covers it.
- Trap: the recipe stages into `/tmp/qt-cd-oracle/{cases,fixtures}`, shared by SIX families (`chat_delete`, `chat_export`, `message_reattribute`, `outfit_llm_choose_tier3`, `search_replace`, `tools_inventory` — `--collisions`). Regenerate through the driver (it suffixes `TMPO` per family), from the pin.
- Files owned: `harness/oracle/cases/chat-dialogs-search-replace.test.ts`, `crates/quilltap-harness/tests/search_replace_equivalence.rs`.

### STAND-2 — `ariel_writers_tier3_equivalence` (`chat_messages row 0 content (bytes only)`)   [NEEDS-RULING, then S harness-only]
- What fails: one cell. The 16 K-elide terminal-output Ariel body (16,531 chars decoded) is stored as a compressed-text blob whose brotli BYTES differ between v4 and v5; the decoded text is byte-identical (status-log `152582-152586`; `195932`).
  - The test already tells the two apart: `decoded()` at `ariel_writers_tier3_equivalence.rs:199-213`, `dump_diffs` at `:218-244` tags the cell `(bytes only)` — and then still fails on it (the header `:24-25` calls it "the family's standing red — R-10").
- Since when: 2026-09-26 (the P4.D226 / P4.D227 sweeps), i.e. since the compressed-text codec port (P4.D203) started compressing this body. `memory_processor_tier3` stood beside it for the same reason until P4.D296 rewrote that family.
- It is a real port divergence of an already-RULED class, not a harness defect:
  - `crates/quilltap-core/src/db/text_compression.rs:47-51` records the ruling (the human, 2026-10-02, P4.144): ten MEMORY_EXTRACTION `request` envelopes store different brotli bytes than Node's; each side decodes the other's; "no codec change is owed"; `memory_pipeline_jobs_tier3` pins the count both ways (`STORED_BYTE_DIVERGENCES`, now 4).
  - The ruling names the memory-extraction envelopes only. The Ariel cell is the same mechanism on a second payload kind (the tier-1 parity corpus `text-compression.json` is byte-identical on all 35 rows, including prose to 262,293 bytes — so the divergence is input-specific, not size-specific).
- "Owner unidentified" (phase-4.md `7503`; status-log `195932`): the owner is the P4.144 ruling's class. No lane's code is at fault.
- Options:
  - **(A) Extend the P4.144 ruling to this cell (recommended).** Carve it both ways as the pipeline family does: a named `STORED_BYTE_DIVERGENCES = 1` in the Ariel family — exactly one `(bytes only)` cell, decoded text equal, any OTHER diff or a count of 0 or 2 is red (so a codec convergence retires the carve loudly). Harness-only, S. Add one sentence to `text_compression.rs`'s ruled-exception paragraph at the next product edit (a doc comment in `src` — a meeting point, or the unifier's).
  - (B) Chase byte parity in the encoder for this input (the `brotli` crate vs Node's bundled brotli 1.2.0 at quality 5). Product crate, L, uncertain; the ruling already declined it.
  - (C) Keep the standing red. It costs a line in every gate record and trains readers to skip a red.
- Files owned under (A): `crates/quilltap-harness/tests/ariel_writers_tier3_equivalence.rs`.

## 3. The workspace gate's "env-block artifacts" / "shared-fixture collisions"

### GATE-1 — the mechanism (measured)   [OPEN — see GATE-2..5]
The unifier's workspace gate runs `cargo test --workspace --no-fail-fast` under ONE flat env block harvested from every family's run stage (1,099 names in the `624d0d25c` round; status-log `202776`). The driver has no harvest mode — the block is hand-built each round. Two distinct mechanisms make a family red there and green by name:

- **(A) One env NAME, several VALUES.** A flat block holds one value per name ("the 8 dup-valued names withheld", status-log `165984`). Families that share a generic name but need different fixtures read a sibling's fixture or nothing. Measured on main (run-stage assignments in the `//!` headers):

  | Name (and its `_MOUNT` / `_LLM` twins) | Families → distinct values |
  |---|---|
  | `QT_FIXTURE_TMP_MAIN` | 9 families, **6 values**: `chat_window` (`qt-cw-`), `mail_alerts_delivered` (`qt-mailalerts-`), `mail_carina_tools` (`qt-mail-`), `search_tools` (`qt-search-`), `state_cascade` / `state_routes` / `state_sql_tools` (`qt-state-`), `reach` / `tiered_mount_pool` (`qt-p4d376-tmp-`) |
  | `QT_FIXTURE_WT_MAIN` | 4 families, 4 values: `wardrobe_tools`, `wardrobe_tools_avatar_trigger`, `wardrobe_announcement_job_tier3`, `wardrobe_wear_commit_tier2` |
  | `QT_FIXTURE_QTAPIMPORT_MAIN` | 3 values: `qtap_import`, `reset_builtins`, `seed_avatars` |
  | `QT_FIXTURE_CONSOLIDATION_MAIN` | 2 values: `commonplace_*` (×2) vs `memory_maintenance_consolidate_tier3` |
  | `QT_FIXTURE_INV_MAIN` | 2 values: `inventory_keystone_tier2` vs `pascal_inventory_effects` (a lane-private `/tmp/p4d329/` path left in a committed header) |
  | `QT_FIXTURE_CHARUPD_MAIN` | 2 values: `characters_update_tier2` vs `metadata_vault_roundtrip` |
  | `QT_FIXTURE_VAULT_READ_OVERLAY` | 2 values: `vault_read_overlay` vs `ensure_character_metadata_file` |
  | `QT_FIXTURE_OUT` / `QT_FIXTURE_MOUNT_OUT` | builder outputs — regen-stage only, but they leak into a harvested block |

- **(B) One /tmp PATH, several REBUILDERS.** `--collisions` reports **39 colliding /tmp paths** on main. The fixture ones are rebuilt by more than one family's recipe, and a rebuild re-mints ids, so the family whose oracle was taken against the earlier build is stale by the time the workspace run reads the fixture (memory notes `shared-fixture-rebuilds-and-jest-setup-mocks`, `parallel-lane-shared-resources`). The driver is immune: it regenerates and runs each family back to back. Shared fixture paths:
  - `/tmp/qt-itl-{main,mount}.db` — `inventory_title_lint`, `inventory_mount_owner_tier2`, `tool_gate_keys`;
  - `/tmp/qt-admission-{main,mount}.db` — `admission_tier3`, `update_memory_tier2` (recorded: status-log `196620`);
  - `/tmp/qt-dmf-fixture.db` — `doc_mount_files_tier2`, `doc_mount_folders_tier2`;
  - `/tmp/qt-dmfl-fixture.db` — `doc_mount_file_links_tier2`, `mount_case_moves`, `mount_case_resolution`, `mount_link_groups`;
  - `/tmp/qt-build-context-{main,mount}.db` — `build_context_tier3`, `chat_absent_mentions_routes`;
  - `/tmp/qt-consolidation-{main,mount}.db`, `/tmp/qt-imgcol-*`, `/tmp/qt-chat-informs-fixture.db`, `/tmp/qt-wks-*` (three families), `/tmp/qt-v4-fresh` (`provisioning`, `migration_index_backfill`);
  - shared oracle NDJSONs written by two recipes: `/tmp/oracle-dmf.ndjson`, `/tmp/oracle-salon-swipe.ndjson`, `/tmp/oracle-system-restore.ndjson`.

- **The families, by name.** The `624d0d25c` record gives the count (15) without the list. The last record that names them is the `322a5488e` follow-ups round (status-log `193861`, ten): `doc_mount_files_tier2`, `inventory_title_lint`, `mail_carina_tools`, `search_tools`, `state_cascade`, `state_routes`, `state_sql_tools`, `tiered_mount_pool`, `vault_read_overlay`, `wardrobe_folder_writes_tier2`. Every one maps onto (A) or (B) above: six on `QT_FIXTURE_TMP_*`, `vault_read_overlay` on its two-valued name, the other three on a shared rebuilt path. Later additions on record: `update_memory_tier2` (B), `embedding_provider_tier3` (status-log `199205`). The growth 10 → 14 → 15 tracks new families joining the same names (`chat_window`, `reach`, `mail_alerts_delivered` all arrived in the last two rounds).

### GATE-2 — a pinned sweep still reads the LIVE v4 checkout in seven run stages   [OPEN — S, the cheapest honest-gate fix]
- Symptom: `provider_sdk_version_guard` is red in the full sweep FROM THE PIN every round ("an env artifact: it reads the LIVE v4 plugin install", status-log `202775`; also `192653`).
- Mechanism, measured: the driver maps the pin by TEXT — `normalize()` replaces the literal `~/source/quilltap-server` with `--v4` (`harness/tools/recipe_sweep.py:935-945`). A run line that never names the checkout falls through to `common::v4_root`'s default, the live checkout.
- The run stages with no checkout on the line (grep of every test that calls `v4_root()` or reads `QT_V4_CHECKOUT` / `QT_V4_ROOT`):
  - `provider_sdk_version_guard.rs:140`, `zod_version_guard.rs:39`, `builtin_prompt_templates_guard.rs:41`, `maintenance_commit_hook_unreachable_census.rs:38` — bare `cargo test` lines;
  - `custom_tool_schema_embed_guard.rs:19` — `QT_V4_CHECKOUT=<the 322a5488e pin>` (a stale placeholder, also why it is non-extractable);
  - `recall_probe_fixture_equivalence.rs:33` — `QT_V4_ROOT=<the pin>` (non-extractable, hand-run every round);
  - `crates/quilltap-host/tests/startup_backup_before_ensures.rs:24` (host tests — outside the strict fence).
- Done looks like, either shape (recommend the first, it cannot be forgotten by a future header):
  - **driver:** when `--v4` is not the default, `run_family` exports `QT_V4_CHECKOUT` and `QT_V4_ROOT` = `--v4` into every RUN stage's environment, with a `--self-test` case (a run line with no checkout names the pin; an explicit assignment on the line still wins);
  - **headers:** each run line carries `QT_V4_CHECKOUT=~/source/quilltap-server`, as `public_schemas_vendor_guard.rs:21` and `qtap_schema_embed_guard.rs:15` already do.
- Either way, replace the two `<the … pin>` placeholders with `~/source/quilltap-server` (→ GATE-4).
- Files owned: `harness/tools/recipe_sweep.py`, `harness/tools/README.md`, the six harness headers (+ the host header, flagged).

### GATE-3 — make the workspace gate's residue mechanical   [OPEN — M, driver only]
- Today the unifier harvests the block by hand, runs the workspace, then re-runs each red by name and writes "green BY NAME (15/15)". Nothing checks that the 15 are the collision set and not a new red hiding in it.
- Proposed driver mode `--env-block <file>`:
  - emits the single-valued run-stage assignments as a sourceable block;
  - emits, as a second artifact, the RESIDUE list with a reason per family — every family with a multi-valued name (mechanism A), every family whose fixture path has more than one rebuilder (mechanism B, already computed by `cmd_collisions`), every refused / non-extractable family;
  - `--self-test` pins both shapes on synthetic recipes.
- The gate then reads: workspace reds ⊆ standing ∪ residue, and the residue is run by name through `--run-all --families <residue>`. A red outside that set is a finding by construction.
- This does not remove the collisions; GATE-5 does. It makes the gate HONEST about them with zero edits to any family another lane owns.
- Files owned: `harness/tools/recipe_sweep.py`, `harness/tools/README.md`.

### GATE-4 — the six non-extractable families, the refused one, `recall_probe_fixture`   [OPEN — S each]
`--list` on main: `ok` 737, `ok_restored` 90, `no_oracle` 26, `committed_corpus` 15, `exempt` 6, **`non_extractable` 6**:

| Family | The driver's reason | Cheap fix |
|---|---|---|
| `custom_tool_schema_embed_guard` (harness) | `unresolved_placeholder <the 322a5488e pin>` ×2 | S — the header's two placeholders → `~/source/quilltap-server` (GATE-2) |
| `recall_probe_fixture_equivalence` (harness) | `unresolved_placeholder <the pin>` | S — same one-token edit at `:33`; the regen stage is already complete (`:23-31`). Ends the every-round hand-run. Trap: confirm `--testTimeout=180000` survives the splice (memory note `parallel-lane-shared-resources`) |
| `memory_row_schema_refusal_lines` (harness) | `elided_or_missing_regen` | S — it consumes `memories_routes`' oracle (`QT_ORACLE_MEMORIES_ROUTES`) and has no regen lines; copy that family's regen stage into the header, or teach the driver a `borrows: <family>` header line (M, cleaner for all four) |
| `avatar_rolls_routes` (web tests) | `elided_or_missing_regen` | S — borrows `QT_ORACLE_AVATAR_ROLLS` (the `avatar_rolls_tier2` family's oracle); outside the strict fence (header comment only) |
| `chat_interrupt_rest_routes` (web tests) | `elided_or_missing_regen` | S — borrows `QT_ORACLE_SALON_SKIP`; same |
| `generator_sse_wire` (web tests) | `elided_or_missing_regen` | S — borrows the optimizer + wizard oracles; same |

- `backup_uuid_remap_equivalence` — `refused_repo_write`, BY DESIGN (`DELIBERATE_REPO_WRITERS`, `recipe_sweep.py:174-181`): its oracle writes the committed `harness/oracle/fixtures/uuid-remap-corpus.json` (`QT_CORPUS_OUT`, `backup-uuid-remap.test.ts:48,1862`) and the test re-hashes the committed file.
  - Extractable for S/M without weakening it: the sweep recipe writes the corpus to `/tmp` and the Rust test takes an optional `QT_UUID_REMAP_CORPUS` — it then asserts the regenerated bytes EQUAL the committed file before running. That mechanises what every round record states by hand ("the committed UUID-remap corpus regenerated byte-identical"). Authoring a new committed corpus stays a lane's explicit `cp`.
  - Rider (K6, phase-4.md `7499`): `uuid-remap-corpus.json:3` still says `"baseline": "f5e953a3f"`. Because the corpus is hash-pinned, deriving the field would move the committed file at every baseline move; rename it (`authoredAt`) or drop it instead. **K6 is listed under Files / carriers — a meeting point with that survey; it must land in the same change as this item.**
- All four "borrowing" families and `backup_uuid_remap` are also why the workspace gate reports "absent oracle" reds — fixing them shrinks the 21.

### GATE-5 — family-unique env names for the `QT_FIXTURE_TMP_*` group   [OPEN — M, optional this round]
- Renames the six-valued `QT_FIXTURE_TMP_{MAIN,MOUNT,LLM}` to a per-family name on both sides (the Rust test's `env::var` + header, the TS case / builder that reads the same name). Nine families, about eighteen files.
- It removes mechanism (A)'s largest group for good; GATE-3 only reports it.
- Trap: every one of those families belongs to some other area's surface (search, state cascade, mail, reach). **Meeting point** — schedule it only if no sibling lane edits `search_tools`, `state_*`, `mail_*`, `chat_window`, `reach`, `tiered_mount_pool` this round; otherwise leave it to GATE-3's report.
- Also S and independent: `pascal_inventory_effects`' committed header points at `/tmp/p4d329/qt-pie-main.db` (a lane-private path) — move it to a neutral `/tmp/qt-pie-*`.

## 4. Order headers and lane records — residue

### P4.174 (harness smalls, `322a5488e` follow-ups round)

- **D325-notdone** — the Inventory item-images `direct` generate 201 / 422 / 502 rows + the save-to-store INFO + the `Generate body is not valid JSON` DEBUG   [OPEN — M/L; recommend OUT]
  - Measured still absent: `harness/oracle/cases/inventory-item-images-routes.test.ts` has no `direct` row.
  - Needs an image profile planted on the pair, a provider seam below v4's generate on the oracle side, and a scripted image seam on the SERVED v5 host. The last is probably a product-crate seam — not harness-only.
  - The route became the `createItemImagesRoute` factory at the pin (P4.D377); re-survey before ordering.
- **D340-T2-2** — a NEW `inventory-title-lint-surfaces.test.ts` over v4's real route + doc-tool handlers   [DEFER-TO-CATCH-UP]
  - Measured still absent. v4 `main` moves `lib/inventory/title-lint-documents.ts` (+21) and `lib/inventory/pool.ts` (+70, the #155 convergence `921fe94bb`) past the pin; a surfaces family written now is re-recorded at the catch-up.
- **The `pass: 'carina'` provenance pin** (C-13)   [OPEN — S/M, harness-only]
  - `carina_memory_extraction_tier3_equivalence.rs:411` notes it is not compared: the value reaches only log lines. Done = a log-capture row for the admission line on both sides. Trap: admission runs on the writer thread — use the process-global capture (memory note `capture-rig-writer-thread`).
- **D313-16** (the test-only `is_moderation_refusal`, a `src` deletion)   [NO-ACTION here — product crate]
  - `services/dangerous_content/refusal.rs:381` is `pub fn` with callers only in `model/image_dialects.rs`'s test module (`:2796-2823`). A `src` edit: hand to whichever lane owns the Concierge / image dialects, or leave.
- The §3 fixes the unifier made to this lane's guards (the loosened `names_some_v4_commit`, the falsely "retired" drain census) are landed: ONE `no_lane_remove_markers_left`, mutation-checked.   [CLOSED]
- Lane gate note: `recipe_sweep.py --self-test` was "not run" in the lane (status-log `193766`, `193768`); the unifier's gate runs it.   [CLOSED]

### P4.172 (DQS off)

- **Tier 3** — harness test-only plain SQLite connections stay DQS-permissive   [NO-ACTION, by the order's list]
- **A known-weak guard, documented in itself:** `dqs_sql_literal_census.rs` matches UPPER-CASE SQL keywords only (a case-insensitive match flagged about 30 prompt-prose literals). A lower-case `select … = "x"` literal passes the census and is caught only when a family executes it with DQS off.   [NO-ACTION — recorded; the workspace run is the backstop. A tokenizer-grade census is M and has found nothing to date]

### P4.D360 / P4.D382 (the vendor orders)

- §S.6 NO-PORT ratifications into the ledger   [CLOSED — `drift-ledger.md:624-640` carries `NO-PORT-RATIFIED(624d0d25c)`; the `236c04e35` set landed with that baseline move]
- `e2e/aa-foundation.spec.ts` not run in lane   [CLOSED — both unifications ran the full Playwright suite]
- F1, `spelling_guard` red from the planner's own survey line   [CLOSED — `a11fbe767` (status-log `202440`)]
- 💸 the help reconcile 135 → 137 → 138 on a Friday copy's first boot   [NO-ACTION — the owed dogfood pass]
- **R-3 — re-key the `help-ensure` fixture's twelve scenarios to the pin's `brahma-console.md`?**   [NEEDS-RULING]
  - Asked by P4.D360 (`work-orders/p4.d360-…md:355-365`), repeated unanswered by P4.D382 (Tier 3 item 21, `:840-843`).
  - The committed two-page tree under `harness/oracle/fixtures/help-ensure/help/` and `help-ensure.json` are a SYNTHETIC corpus for the ensure's reconcile logic; the per-run regen from the pin is green on both rounds.
  - Options: (A) leave it frozen and close the ask (recommended — re-keying re-mints twelve hashes for no behaviour, and the vendored-tree parity is `help_tree_equivalence`'s job); (B) re-key each round (a standing chore).

### The `624d0d25c` unification record (status-log `202732-202743`, `202674-202677`)

- **Censuses summed BY NUMBER on the union**   [OPEN — M; recommend the unifier's, or OUT]
  - `EXCLUDED_BY_THE_ROUTE_IDENTIFIER_RULE = 532` (`crates/quilltap-web/tests/dispatch_wrong_type_census.rs:3016`);
  - the tri-state total `161` (`crates/quilltap-web/tests/tri_state_edges_share_the_decoder.rs:654`);
  - `doc_mount_fallback_sites_census.rs:983`'s nine-tuple `(103, 14, 20, 1, 13, 5, 1, 20, 5)` with its arithmetic in prose (`:860-861`).
  - Each is a bare total every route lane bumps; the union is arithmetic the unifier does by hand (three memory-note entries: "census counts unioned by text not number", "summed census constants", "the doc_mount census's union arithmetic").
  - Done would be a per-owner-file table (`&[(file, count)]`, the total DERIVED), so a union is a text merge of rows and a wrong sum cannot compile into a green.
  - Two of the three live in `crates/quilltap-web/tests/**` and every server lane touches them — a **meeting point with every web-route lane**. Do it in a round with few route lanes, or as the unifier's first commit.
- **The frozen-`ProcessClock` asymmetry** (the round record's "Harness asymmetry"; phase-4.md `7265-7266`)   [OPEN — product-crate seam needed; a harness-only guard is S]
  - Measured (`59ca6cadd`'s message): the orchestrator family drives v5 with a frozen `ProcessClock` (`orchestrator_tier3_equivalence.rs:2391-2392`, `:2468-2469`), which stamps posted USER rows; replies are stamped by `crate::clock::now_iso()` — the wall clock, "no harness seam" (`:488`, `:503-504`). v4's oracle clock advances 1 ms per read.
  - Consequence: two calls on ONE chat can order differently on the two sides (a frozen-stamped fact read older than a wall-stamped reply; two openers tied). Lane rows had gone green only because a sibling's earlier canned-key miss hid the prior turn; the unifier moved two rows onto their own chats.
  - The real fix is a clock seam in `quilltap-core` (`clock.rs` + the executor) — **a product-crate edit; a meeting point with the spine / Storyteller area**, M/L, and every committed orchestrator oracle re-records.
  - Harness-only mitigation (S): a corpus-integrity assert in `orchestrator_tier3_equivalence.rs` — no two calls share a chat unless the spec row names it in an explicit `sharedChatOk` list — so the next lane cannot re-create the tie silently. **`orchestrator_tier3_equivalence.rs` and `orchestrator-tier3.json` are the Storyteller-fold lane's too (the round's FIRST item) — a meeting point; hand this assert to that lane as a rider rather than opening a second owner.**
- The other union reds in that list were each fixed at unification (lantern census, the doubled `maxImagesPerRequest` plant, the count moves, the restore mock, the almanack re-seed, the characters columns, `group_wardrobe`, the subprompts / mail-alerts hint records). Nothing left weak by measurement.   [CLOSED]
- `provider_sdk_version_guard` vs a fresh pin's SDK floors is on the catch-up's list (phase-4.md NEXT 2); GATE-2 removes the sweep-side artifact now.

## 5. v4 filing candidates

Rules that apply: filing is outward-facing and waits for the human's go-ahead; v4 bugs are GitHub issues on `foundry-9/quilltap-server` (the Bug form), not `bugs.md` entries (frozen at Bug 269). This survey only inventories. `gh` was not called.

- v4 `main` = `0d73a4aee`. `git log 624d0d25c..main --grep='#1[0-9][0-9]'` returns nine commits; the issue numbers they carry are **#145** (OpenRouter off `callModel()`), **#147** (folder-move casing — this port's filing, fixed by `88c9cc81d`) and **#155** (the pool's consulted order — this port's filing, fixed by `921fe94bb`). None touches a candidate below.
- "Still present" below = the cited v4 file is byte-unchanged `624d0d25c..main` (`git diff --stat` empty) or the cited line reads the same at `main`.

| # | Candidate | Source | Draft | Still present at v4 `main`? | Recommendation |
|---|---|---|---|---|---|
| 1 | `POST /api/v1/memories` answers 400 AFTER writing the memory when the post-admission keystone mark is refused | P4.D341 R-3 (Tier 2 item 26) | **Yes** — status-log `196599-196607` (title / what happens / reproduce / expected) | Yes — `app/api/v1/memories/route.ts` unchanged | **File** (the draft is complete; say it is race-only) |
| 2 | A Storyteller round written with a NULL `triggerMessageId` cannot be read back (`.nullable()` without `.optional()`; the reader turns NULL into `undefined`) | P4.D343, status-log `197405` | No | Yes — `lib/schemas/storyteller.types.ts:131` still `z.string().nullable()`; `storyteller-rounds.repository.ts` unchanged | **Draft, then file** — needs a repro (an attachments-only post) measured at the pin first |
| 3 | C4 — the SVG sanitizer's `url()` filter misses CSS escapes and `image-set()` (an external fetch, not script) | P4.D353 header `:8`; status-log `197669` | No | Yes — `components/chat/diagrams/**` and `lib/markdown/diagrams.ts` unchanged; `sanitize-svg.ts:25-43` | **Draft, then file** as a security-flavoured issue; v5 shares it (a v5 fix order should ride the same ruling) |
| 4 | C5 — Mermaid `%%{init}%%` `themeCSS` can carry a `url()` (SUSPECTED) | same | No | Code unchanged (`render-mermaid.ts:32-33`, `securityLevel: 'strict'`) | **Re-measure** — never reproduced; do not file a suspicion |
| 5 | C7 — the diagram render queue is unbounded; a large Graphviz graph freezes the main thread | same | No | Yes — `render-queue.ts` unchanged | **Draft, then file** (low severity) |
| 6 | X5 — the referent ledger can quote a whisper to a non-recipient | P4.D347 header `:9`; status-log `197672` | No | Yes — `lib/chat/context/referent-ledger.ts` unchanged | **Draft, then file** — a privacy-of-whisper defect; needs the minimal repro written down |
| 7 | E3 — the Pascal hold dialog's overlay blocks the composer's Stop, and the dialog has no Stop of its own | P4.D346 header `:7`; status-log `197673` | No | Yes — no hold / dialog component in `git diff --stat 624d0d25c main -- components lib/chat app/salon` | **Draft, then file** |
| 8 | Roleplay rendering: `778b97b2b`'s nesting reaches the DEFAULT patterns (`[she said *softly* …]` renders narration nested in narration) | P4.D352, status-log `195147` | No | Yes — no roleplay / annotation renderer file moved | **Draft, then file** (v5 follows v4, so a v4 fix is a later convergence) |
| 9 | Roleplay rendering: a `*` inside two shown-delimiter wraps is escaped twice (`"a ((b *c* b)) a"` under Novel: a stray `\`, an `<em>`, no OOC span) | P4.D352, status-log `195148` | No | Yes — same | **Draft, then file** with #8 as one issue or two |
| 10 | `isCharacterAuthoredMessage` (truthiness) and `chatMessageFtsEligibilitySql` (`IS NULL`) disagree on empty-string authors | P4.D3xx FTS lane (R-Z), status-log `199025-199027` | **Yes** — one full paragraph, measured at the pin | Yes — `lib/chat/chat-activity.ts` and `chat-message-fts.ts` unchanged | **File** as Low / latent (no write path stores `''` today) |
| 11 | `findIdsInEventWindow` compares event times as TEXT: an `occurredAt` without milliseconds on a window's last second is left out | the window-first recall lane (R-Z), status-log `200276` | **Yes** — one sentence | Yes — `memories.repository.ts:1073-1087` unchanged (`COALESCE(occurredAt, createdAt) <= ?`) | **File** (expand the sentence into the form's fields) |
| 12 | Storyteller "All at once": one concurrent seat's provider failure rejects `runStorytellerRound` before `markClosed` — the round stays `running`, the other streams are cut, no `chainComplete` | P4.D368 (R-Z), status-log `200507` | **Yes** — a paragraph with a repro | Yes — `lib/storyteller/round.service.ts` unchanged | **File** — and note it to the Storyteller-fold lane: v5's failed seat rides its own error frame (`08ee30208`), so v5 already differs here |
| 13 | "a inventory" (an article slip in a v4 string) | P4.D377 item 29, status-log `200845` | No ("no v4 filing drafted") | Not re-located — the record gives no file:line | **Re-measure** (find the string at the pin) or drop as cosmetic |
| 14 | `put_zero_one_flag` / `Boolean('0.0')` | status-log `198820` | Drafted | — | **Owned by another survey — skipped here** |

- An OLDER batch was never drafted and is not on the carried list: the nine candidates in the previous smalls survey (`surveys/2026-10-27-setupphase-followups-inventory-log-spa-harness.md:16-25` — the SVG allow-list gaps, `copy_inventory_items_to_folder`'s N projections, the help chat's slate, `withArticle` / "an unicorn horn", the `.max(4000)` / `.slice(0, 4000)` astral disagreement, the `handleOpenDocument` residue, the Scriptorium's generic 500, `help/character-gallery.md:51`, "Log line" vs "Enter a log line"). Spot-checked two at `main`: `lib/inventory/equip-effects.ts:88-94` (`withArticle`) and `help/character-gallery.md:51` ("with its generation prompt") both still read as recorded. **Recommendation: re-measure the batch in one pass if the human wants a filing sweep; otherwise drop it from the carried lists by name** so it stops being silently half-carried.
- Already filed and resolved, for the record: #120 (the `cheapLLMSettings` row leak, Faithful), #147, #155; v4 Bug 180 (`surveys/2026-10-07-p4.159-v4-bug-179-draft.md`). The two draft files under `surveys/` are both marked FILED.
- **Suggested shape for the round:** one docs-only "FILING DRAFTS" unit — a single file under `work-orders/surveys/` holding the ten missing drafts in the Bug form's fields (Severity, Provenance, v5 status), each re-measured at the pin by a v4-only scratch script, nothing committed to v4. It edits no source and can run beside any lane. The human then files, or declines, from one page.

## 6. Proposed lanes (harness-only; file sets disjoint)

**Lane E-1 — `GATE`: the sweep driver and the honest workspace gate.** (GATE-2, GATE-3, GATE-4; GATE-5 only if scheduled.) M overall.
- Owns: `harness/tools/recipe_sweep.py`, `harness/tools/README.md`; the `//!` headers of `provider_sdk_version_guard.rs`, `zod_version_guard.rs`, `builtin_prompt_templates_guard.rs`, `maintenance_commit_hook_unreachable_census.rs`, `custom_tool_schema_embed_guard.rs`, `recall_probe_fixture_equivalence.rs`, `memory_row_schema_refusal_lines.rs`; `backup_uuid_remap_equivalence.rs` + `harness/oracle/cases/backup-uuid-remap.test.ts` + `harness/oracle/fixtures/uuid-remap-corpus.json` (with K6).
- Outside the strict fence, header comments only: `crates/quilltap-web/tests/{avatar_rolls_routes,chat_interrupt_rest_routes,generator_sse_wire}.rs`, `crates/quilltap-host/tests/startup_backup_before_ensures.rs`. If the fence is strict, the driver-side shapes (the run-stage export; a `borrows:` rule) avoid all four.
- Proof: `--self-test` grows a case per change; `--list` reads `non_extractable: 0`; a pinned `--run` of `provider_sdk_version_guard` and `recall_probe_fixture_equivalence` is green through the driver; `--collisions` unchanged unless GATE-5 runs.
- Must NOT regenerate any committed corpus except `uuid-remap-corpus.json` (and only if K6 is ruled in).

**Lane E-2 — `STANDING`: retire the two standing reds + the specifier guard + the Carina pin.** (STAND-1; STAND-2 under ruling (A); D361-14; the `pass: 'carina'` pin.) S/M overall.
- Owns: `harness/oracle/cases/chat-dialogs-search-replace.test.ts`, `crates/quilltap-harness/tests/search_replace_equivalence.rs`, `crates/quilltap-harness/tests/ariel_writers_tier3_equivalence.rs`, the NEW `crates/quilltap-harness/tests/oracle_v4_specifier_census.rs`, `crates/quilltap-harness/tests/carina_memory_extraction_tier3_equivalence.rs` + `harness/oracle/cases/carina-memory-extraction*`.
- Proof: both families green from the pin through the driver; the sweep's `run_failed` set drops from 3 to 0 with Lane E-1's GATE-2 (the third was `provider_sdk_version_guard`).
- After it lands the unifier's "the two standing" sentence retires from every gate record — update `/unify`'s expectation in the same round.

**Unit E-3 — `FILING DRAFTS` (docs only).** §5's last bullet. Owns one new file under `work-orders/surveys/`. No source, no v4 write, no filing.

Disjointness: E-1 and E-2 share no file. E-1's `recall_probe_fixture_equivalence.rs` header edit and E-2's guard are both new to this round.

## 7. Cross-area meeting points

- `crates/quilltap-harness/tests/orchestrator_tier3_equivalence.rs` + `harness/oracle/fixtures/orchestrator-tier3.json` — the Storyteller-fold lane (the round's FIRST item) and any spine lane. The shared-chat assert (§4) should ride THAT lane, not a harness lane.
- The `ProcessClock` seam proper — `crates/quilltap-core/src/clock.rs` and the executor: product crate, the spine area's call.
- `text_compression.rs`'s ruled-exception paragraph (one sentence under STAND-2 (A)) — a `src` doc comment; the unifier's or a core lane's.
- K6 (`uuid-remap-corpus.json` `_meta.baseline`) — listed under Files / carriers; it must be the SAME change as GATE-4's `backup_uuid_remap` item.
- GATE-5's nine families (`search_tools`, `state_*`, `mail_*`, `chat_window`, `reach`, `tiered_mount_pool`) — each some other area's family.
- The by-number censuses (`dispatch_wrong_type_census.rs`, `tri_state_edges_share_the_decoder.rs`, `doc_mount_fallback_sites_census.rs`) — every web-route / document lane bumps them.
- `documents-routes.test.ts:78-81` (P4.D361 items 17 / 18) — the documents survey's (P4.D380 R-2 (A)).
- `is_moderation_refusal` (D313-16) — `src`, the Concierge / image-dialects owner.
- `/unify`'s gate text ("the two standing", "fifteen env-block collisions") — moves when E-1 / E-2 land; the command file is the planner's.

## 8. Recommended OUT of this round

- **D325-notdone** (item-images `direct` rows): needs a served-host image seam (product), M/L, skipped twice already; re-survey against the `createItemImagesRoute` factory first.
- **D340-T2-2** (title-lint surfaces family): v4 moves `title-lint-documents.ts` and `pool.ts` past the pin — write it at the catch-up.
- **GATE-5** unless no sibling lane touches its nine families.
- **The by-number census reshape** unless the round has few route lanes — otherwise it conflicts with all of them.
- **The `ProcessClock` core seam**: re-records every orchestrator oracle; not a smalls item. Take the S assert only.
- **A tokenizer-grade DQS census** (P4.172's documented limit): M, has found nothing.
- **STAND-2 option (B)** (brotli byte parity for the Ariel body): the P4.144 ruling already declined the class.
- **`help-ensure` re-key** (R-3): recommend closing the ask as "leave frozen".
