# Survey — P4.140: the Salon spine — Option V (a `TimeZone` VALUE through the turn, the build, the greeting and the spine), the two core zone-NAME wrappers, v4's `fileProcessing` SSE frame, and Carina's missing `CHAT_MESSAGE` row

**Date:** 2026-10-02 · **v4:** `f6426e196` (tree dirty by the three recorded
docs paths) · **v5 main:** `cb9ecf256` · **Kind:** read-only measurement —
greps and reads over both trees; one throwaway probe crate built OUTSIDE both
repos (`<scratchpad>/jiffprobe`, `jiff = "=0.2.31"` from the offline registry —
the exact `Cargo.lock` version) run under seven `TZ` values; `node -e` (v24.13.1,
the oracle's Node) under the same values for ICU's answer. Nothing in either repo
was built, run or written except this file. Paths: v4 relative to
`~/source/quilltap-server`, v5 relative to this repo; v5 core paths are under
`crates/quilltap-core/src/` unless prefixed.

## The finding in one line

All four items stand and land in one lane, but **the dogfood note that "Option V
must also fix the zone VALUE" is refuted by measurement**: jiff 0.2.31's
`TimeZone::try_system()` honours a POSIX `TZ` rule (`TZ='XST6XDT,M3.2.0,M11.1.0'`
→ offset `-05`, `iana_name() == None`), so `HostConfig.display_zone` is ALREADY
right on such a host — every surface the B5 walk measured (the executor tools,
`read_conversation`'s `Current time:`, AND the Scenario Builder clock, which is
NAME-fed at `crates/quilltap-host/src/spine.rs:1871`, not value-fed) falls to UTC
only because it re-derives its zone from `HostConfig.tz`, which is
`iana_name().unwrap_or("UTC")` = `"UTC"`. Threading the VALUE closes it; no host
read changes. The `fileProcessing` frame is ONE new `ChatEvent` variant in
`services/chat_events.rs` (not `api/types.rs` — no wire-type change) emitted
immediately before the `validating` status, red-first on exactly one recorded
frame. Carina's row is three `run_stream` call sites logging through P4.121's
existing `log_loop_leg`, red-first on exactly one `llm_logs` row. The real cost of
Option V is wider than "the `orchestrator_tier3` literals": the swipe path,
the autonomous step, the greeting's spine and the `ChatSpine` itself all carry
the NAME today, so the VALUE crosses **eight structs** (§A3).

---

## §A Item (k) — Option V

### A1 Every NAME-fed display-zone entry today (`display_zone_named(` callers)

`grep -rn 'display_zone_named(' crates --include='*.rs'` (production only):

| # | site | what renders there | name source |
|---|---|---|---|
| N1 | `services/orchestrator.rs:3122` (`process_message` → `build_turn_tool_runner(.., display_zone_named(input.server_tz.as_deref()))`, fn at `:230-237`) | every executor tool on the Salon turn (`read_conversation` header + `Current time:`, `list_mail`/`read_mail`/`send_mail` dates + the PERSISTED reply preface, `upsert_annotation`, `search_web` `Published:`) and the in-turn Carina tool runner | `ProcessMessageInput.server_tz` (`:464`) |
| N2 | `services/build_context.rs:3621` (`let display_zone = display_zone_named(input.server_tz.as_deref())`) | the Suparṇā mail LLM context, the whisper seam (`post_suparna_mail(.., zone)`, trait `:786-798`), the progressions section | `BuildContextInput.server_tz` (`:558`) — fed by `build_context_input` (`orchestrator.rs:4532-4559`) from `BuildContextArgs.server_tz` (`:4385`), whose two callers are the turn (`orchestrator.rs:2401-2414`) **and the swipe** (`services/regenerate_swipe.rs:633-645`, from `RegenerateSwipeOptions.server_tz`, `:306`) |
| N3 | `services/chat_create.rs:1232` (`&display_zone_named(Some(&deps.tz))` into `build_chat_context`) | the greeting's system prompt (its progressions report) | `ChatCreateDeps.tz` (`:970`) |
| N4 | `crates/quilltap-host/src/spine.rs:1165` (`ChatSpine::tool_runner()`, `pub(crate)` `:1139`) | every tool on the carina / ask_carina / Brahma / Run Tool runners; Carina's own progressions section through `ToolRunner::display_zone()` (`services/carina_query.rs:475`) | `ChatSpine.tz` (`:1082`) |
| N5 | `crates/quilltap-host/src/spine.rs:1871` (`jiff::Timestamp::now().to_zoned(display_zone_named(Some(&self.tz)))`) | the Scenario Builder prompt's clock | `ChatSpine.tz` |

Plus one test site: `services/orchestrator.rs:5026` (the wiring probe passes
`display_zone_named(Some("America/Chicago"))`). Carina (`carina_query.rs:475`,
`host_zone: &deps.tool_runner.display_zone()`) is not a helper caller — it reads
whichever runner it was handed (N1 on the in-turn path, N4 on the spine path) —
so it is fixed by fixing N1 and N4, with no Carina edit.

**Further NAME-derived surfaces the recorded scope never listed** (same
`HostConfig.tz` → `"UTC"` fall under a POSIX `TZ`, NOT display formatters, so NOT
item (k) — recorded for Tier 3):

- `crates/quilltap-host/src/spine.rs:2408-2415` `js_local_offset_minutes(tz_name, now_ms)`
  (`TimeZone::get(tz_name).unwrap_or(UTC)`), feeding `ProcessClock.local_offset_minutes`
  (`:1172`, chain `:1650`) and `StepDeps.local_offset_minutes` — v4's
  `getTimezoneOffset()` for the zone-less timestamp arm.
- The story-zone fallback `timezone: Some(self.tz.clone())` at `spine.rs:1372`
  (swipe), `:1592` (turn), `:1668` (chain), `:2117` (step) — v4's
  `resolveTimezone` (`lib/chat/timestamp-utils.ts:341-357`) returns `undefined`
  (system default) when neither the chat, the settings nor `QUILLTAP_TIMEZONE`
  names a zone.
- The memory distill's `local_tz` (`build_context.rs:2520`, `pre_compute.rs:281`,
  `recall_replay.rs:273`), cron (`enclave/cron.rs:92-97` takes a NAME), the LLM-log
  cleanup (`spine.rs:3345`), the Almanack's `timezone` fact
  (`almanack_services.rs:265`).

### A2 The VALUE-fed entries today

| site | fill |
|---|---|
| `HostConfig.display_zone` (`crates/quilltap-host/src/host.rs:94`, read ONCE at `:143` — `system_display_zone()`; `tz` derived at `:150`) | the one read |
| `HostAssembler` (`host.rs:253-254` copies both; struct `:409-410`) | |
| `CoreConfig.display_zone` (`api/engine.rs:65`, filled `host.rs:275`) → the Compose reply preface (`chat_send_mail_in_zone`) + `salon::chat_get`'s zone (the Salon-load whispers) | |
| `ConversationRenderHandler.display_zone` (`host.rs:638`) | the persisted render chunk text |
| Almanack (`host.rs:873-874` → `almanack_services.rs:48,245`) | |
| `BuiltInToolRunner::with_display_zone` (`tools/executor.rs`) — VALUE-typed, but BOTH production fills (N1, N4) compute the value from a NAME | |

### A3 The structs a `TimeZone` VALUE must cross — literal construction sites (measured)

`api/types.rs` carries **none** of these (`grep server_tz\|display_zone\|TimeZone\|ProcessMessageInput api/types.rs` → nothing): the value is a host-side fill on service inputs, **no wire type changes**. None of the structs derives `Default` (checked: `orchestrator.rs:445`, `build_context.rs:511`, `regenerate_swipe.rs:284`, `chat_create.rs:953`), so a new field touches every literal:

| struct | defined | literal sites | harness literals |
|---|---|---|---|
| `ProcessMessageInput` | `services/orchestrator.rs:445` | host `spine.rs:1563`, `:1651` (chain closure); core `enclave/step.rs:689`, `orchestrator.rs:4958` (test `chain_input`) | **`orchestrator_tier3_equivalence.rs:1797`, `:1879`** (the accepted cost — two literals, not "the literals" in general) |
| `BuildContextArgs` (`pub(crate)`) | `orchestrator.rs:4372-4386` | `orchestrator.rs:2401`, `regenerate_swipe.rs:633` | none |
| `BuildContextInput` | `build_context.rs:511` | prod `orchestrator.rs:4532`; tests `build_context.rs:4464`, `:4798`, `:4972`, `message_context.rs:2450` | `build_context_tier3_equivalence.rs:807` |
| `RegenerateSwipeOptions` | `regenerate_swipe.rs:284` | host `spine.rs:1364`; the exhaustive destructure `regenerate_swipe.rs:436` | `regenerate_swipe_tier3_equivalence.rs:549`, `salon_swipe_generate_equivalence.rs:211` |
| `StepDeps` (enclave) | `enclave/step.rs:143` (`tz: &'a str` `:151`) | host `spine.rs:2110`; core test `step.rs:1909` | `enclave_step_tier3_equivalence.rs:584` |
| `ChatCreateDeps` | `services/chat_create.rs:953` | host `spine.rs:2292`; core test `chat_create.rs:3886` | `chat_create_capstone_equivalence.rs:856` |
| `ChatCreateSpine` (host) | `spine.rs:2183` | `spine.rs:2193` (clone), `:3802` | `chat_create_capstone_equivalence.rs:1392`; web tests `chat_create_end_to_end.rs:171`, `swipe_spine/mod.rs:234`, `chat_send_smoke.rs:165`, `scenario_builder_spine/mod.rs:287` |
| `ChatSpine` (host) | `spine.rs:~1070` (`tz` `:1082`) | `spine.rs:1119` (`clone_state`), `:3779` | web tests `chat_create_end_to_end.rs:151`, `chat_send_smoke.rs:145`, `scenario_builder_spine/mod.rs:267`, `swipe_spine/mod.rs:214` |
| `ProductionSpineFactory` | `spine.rs:3660` (`tz` `:3663`; `new(base_dir, version, tz)` `:3673`) | `crates/quilltap-web/src/lib.rs:269` (`production_host_config`, shared by `quilltap-tauri/src/lib.rs:132`) + 9 test `new(..)` calls (web ×7: `impersonation_voice_preview_wire.rs:81`, `generators_wizard_routes.rs:45`, `image_profile_generate_dispatch_wire.rs:116`, `avatar_rolls_routes.rs:282,344`, `profile_bound_api_key_wire.rs:174`, `characters_generators_routes.rs:57`; host ×2: `host_llm_log_cleanup.rs:125`, `host_headshoulders_backfill.rs:107`) | — |

**Design recommendation (no wire change, smallest literal blast):**

- `ProcessMessageInput.display_zone: TimeZone`, `BuildContextArgs.display_zone`,
  `BuildContextInput.display_zone` (required; the two harness literals pass
  `TimeZone::UTC` beside their existing `server_tz: Some("UTC")`).
  `orchestrator.rs:3122` → `input.display_zone.clone()`; `build_context.rs:3621`
  → `input.display_zone.clone()`.
- `RegenerateSwipeOptions.display_zone` → `BuildContextArgs` (the swipe was
  invisible to the recorded scope: it reaches N2 through `build_context_input`).
- `StepDeps.display_zone: &'a TimeZone` → its `ProcessMessageInput` (the
  autonomous turn renders tools/whispers too; leaving it name-fed would keep
  the divergence on every autonomous room).
- `ChatCreateDeps.display_zone: TimeZone` → `chat_create.rs:1232` (the greeting).
- `ChatSpine.display_zone` + `ChatCreateSpine.display_zone`; `spine.rs:1165` →
  `.with_display_zone(self.display_zone.clone())`, `:1871` →
  `.to_zoned(self.display_zone.clone())`; the four ProcessMessageInput /
  Regenerate / StepDeps / ChatCreateDeps fills copy it.
- `ProductionSpineFactory`: a `with_display_zone(TimeZone)` builder defaulting to
  `TimeZone::UTC` (the `BuiltInToolRunner` precedent — the nine test `new(..)`
  calls stay untouched), production filling it at `quilltap-web/src/lib.rs:269-272`
  from `config.display_zone.clone()`, census-pinned (A7). Changing `new`'s
  signature instead costs ten edits across two crates' tests for no gain.
- `HostConfig`: the recorded nit "`tz` and `display_zone` are independent `pub`
  fields" bites harder once the spine reads the VALUE — `quilltap-tauri/tests/common/mod.rs:151-153`
  and `quilltap-web/tests/common/mod.rs` set `config.tz = "UTC"` and leave the
  value on the machine zone. Add `HostConfig::set_display_zone(&mut self, TimeZone)`
  that sets both (tz derived by ONE helper, A4), and move both commons onto it.

### A4 Does anything still need the NAME after the value is threaded?

**Yes — `server_tz` cannot retire** (phase-4.md NEXT 2(k), `phase-4.md:7344-7346`,
says "`display_zone_named` retiring with the `server_tz` NAME reads" — half
wrong, see §E). Remaining NAME readers: the distill's TODAY line and day-reference
scan (`build_context.rs:2520` `local_tz: input.server_tz.clone()`,
`pre_compute.rs:106,281`, `recall_replay.rs:117,273` — a calendar seam that resolves
by name, `day_references.rs`), cron (`StepDeps.tz`, `ChatCreateDeps.tz` "for the
cron next-run evaluation" `chat_create.rs:969`, `spine.rs:2282-2284`), the
LLM-log cleanup, the Almanack's `timezone` fact, the story-zone fallback and
`js_local_offset_minutes` (A1). The `Current time:` line is NOT a NAME reader (it
renders through the runner's VALUE, `tools/executor.rs:742,775`). The CLI's docs
listing (`crates/quilltap-cli/src/docs_cmd.rs`, `TimeZone::system(` — the census's
`AMBIENT_READ_ALLOWED`) is untouched. So: `server_tz` stays (documented as the
calendar NAME), `display_zone_named` retires (its only production callers are N1–N5),
and the name derivation `iana_name().unwrap_or("UTC")` gets ONE home
(`host_zone::zone_name(&TimeZone) -> &str`) used by `HostConfig::new`,
`set_display_zone` and item (g)'s autonomous rooms.

### A5 The POSIX-`TZ` VALUE "defect" — measured: there is none in the read

- The host's ONE read: `crates/quilltap-host/src/host.rs:143`
  `quilltap_core::host_zone::system_display_zone()` = `TimeZone::try_system().unwrap_or(TimeZone::UTC)`
  (`host_zone.rs:49-51`); `tz` = `display_zone.iana_name().unwrap_or("UTC")` (`host.rs:150`).
- jiff `0.2.31` (`Cargo.lock`; `jiff = "0.2.31"` in core `Cargo.toml:95`):
  `try_system` (`jiff-0.2.31/src/tz/timezone.rs:391`) → `system::get` → `get_force`
  (`src/tz/system/mod.rs:145-162`) → `get_env_tz` (`:177-205`): `TZ` is parsed with
  `PosixTzEnv::parse_os_str`; `Ok(PosixTzEnv::Rule(tz)) => return Ok(Some(TimeZone::from_posix_tz(tz)))`
  (`:204-205`). The docs say so (`timezone.rs:359-370`: "`TZ=EST5EDT,M3.2.0,M11.1.0` for setting
  a time zone via a daylight saving time transition rule"). `TimeZone::posix(&str)` also exists (`:506`).
- **Measured** (probe binary, instant 2026-09-29T19:40Z = the walk's D2):

| `TZ` | jiff `try_system` | Node 24 / ICU `toLocaleString` (macOS, `/etc/localtime` → America/Chicago) |
|---|---|---|
| `XST6XDT,M3.2.0,M11.1.0` | `Ok`, `iana=None`, `-05`, `14:40` | `2:40 PM`, zone `America/Chicago` |
| `XST5XDT,M3.2.0,M11.1.0` | (not run — same parse path as the next row) | `2:40 PM`, `America/Chicago` (NOT Eastern) |
| `EST5EDT,M3.2.0,M11.1.0` | `Ok`, `None`, `-04`, `15:40` | `2:40 PM`, `America/Chicago` |
| `PST8PDT,M3.2.0,M11.1.0` | `Ok`, `None`, `-07`, `12:40` | `2:40 PM`, `America/Chicago` |
| `XST-9XDT,M3.2.0,M11.1.0` | `Ok`, `None`, `+10` | `2:40 PM`, `America/Chicago` |
| `XST-9` (no DST) | — | `4:40 AM` (+9), zone `undefined` |
| `XST5XDT` (no rule, unparseable) | **`Err`** → v5 `UTC` | `2:40 PM`, `America/Chicago` |
| `CDT` / `GMT+2` / `EST` | `Err`→UTC / `-02` / `-05` (`EST`) | `7:40 PM` UTC / `5:40 PM` / `2:40 PM` |
| unset | `Ok`, `America/Chicago` | Chicago |

  So: (1) **HostConfig's value is already the POSIX rule's zone** — the walk's
  `XST6XDT` row would have rendered `02:40 PM` on every value-fed surface; the
  B5 "Scenario Builder clock `+00:00`" is the NAME at N5. (2) **v4 does not
  actually honour a POSIX DST rule**: Node 24's ICU ignores it and falls back to
  `/etc/localtime` (every DST-rule row above reads Chicago, whatever the rule
  says); the walk's row agreed with v4 only because the rule it chose IS
  Chicago's. ICU does honour a DST-less rule (`XST-9`). (3) An unparseable `TZ`
  is the one place the VALUE falls to UTC where ICU falls to the system zone.
- **Proposed fix:** none to the read; thread the value (A3). Record (2) as a
  ruled-shape divergence (v5 honours the rule, the "honour a POSIX `TZ`" ruling;
  v4/ICU silently shows the OS zone) and (3) as a Tier-3 item (jiff exposes no
  "system zone ignoring `TZ`" constructor — `timezone.rs` public fns: `system`,
  `try_system`, `get`, `posix`, `tzif` only; the honest options are a WARN on the
  `Err` arm or reading `/etc/localtime` by hand).
- **The child-process pin** (model: `crates/quilltap-harness/tests/host_zone_sites_census.rs:474-492`
  `production_entries_render_in_the_host_zone` spawning itself with
  `.env("TZ", "America/Chicago")` + `CHILD_MARKER`): add a second child under
  `TZ=XST6XDT,M3.2.0,M11.1.0` (machine-independent — jiff parses the rule itself)
  asserting `HostConfig::new(..).display_zone` has `iana_name() == None` and
  offsets `-5h` at the D2 instant / `-6h` at a January instant, `cfg.tz == "UTC"`
  (the documented NAME residue), and that the whisper / web-search / progressions
  renders built from `&cfg.display_zone` read `September 29, 2026 at 02:40 PM` /
  `(Published: 6/14/2026)`. The existing Chicago child's steps 2–5 build their
  zone as `display_zone_named(Some(&cfg.tz))` (`:526`, `:552`, `:600`) — after
  Option V they take `cfg.display_zone.clone()`.
- **Red-first on main:** under the POSIX child, `HostConfig::new(..).display_zone`
  offset `== -5h` is **GREEN** today (the premise refuted); the RED assertion is
  the production derivation — `display_zone_named(Some(&cfg.tz))` offset `== -5h`
  reads `0` (it is how N1–N5 build the zone today), and the whisper reads
  `07:40 PM`. Record that pre-fix run's failing line, then retire the expression
  with the helper. (`TZ=CST6CDT` is a legacy IANA name — `iana=Some("CST6CDT")`
  in the probe — and is NOT a POSIX-only probe, as the dogfood note says.)
- **Optional oracle arm (Tier 2):** `host-zone-dates.ts` under `TZ=XST-9` — ICU
  honours a DST-less rule, so v4's renders are machine-independent there. The
  oracle's zone row is `Intl.DateTimeFormat().resolvedOptions().timeZone`
  (`harness/oracle/cases/host-zone-dates.ts:76`), which is `undefined` under that
  `TZ`; the case must also record `process.env.TZ`, and
  `host_zone_dates_equivalence.rs`'s `run` (`:105-118`, `TimeZone::get(expected_tz)`)
  gains a `TimeZone::posix` arm. A DST-rule arm is NOT portable (ICU reads the
  host's `/etc/localtime`).

### A6 Item (g) — the two core zone-NAME wrappers

- `api/autonomous_rooms.rs:46-48` `fn system_tz() -> String { crate::host_zone::system_zone_name() }`,
  read at `:265` (start), `:284` (resume), `:319` (pause), `:346` (stop),
  `:390` (update-settings) — **five** routes, each feeding
  `cron::try_next_occurrence(expr, anchor, &tz)` (a NAME API, `enclave/cron.rs:92-97`).
  Dispatched from `api/engine.rs:4668`, `:4674`, `:4678`, `:4683`, `:4691`.
- `services/markdown_transcript.rs:498-500` `fn system_tz()`, read at `:511`
  in `chat_export_markdown(db, user_id, chat_id)` → `LocalOffset::Zone(&tz)`
  (`:589`; the enum `:93-96` resolves the name at `:102-110` — a DISPLAY use, the
  zone-less timestamp arm). Dispatched from `api/engine.rs:1680`.
- **Recommendation:** `chat_export_markdown(.., zone: &TimeZone)` with
  `LocalOffset::Zone(&'a TimeZone)` (display → VALUE; no other `LocalOffset::Zone`
  constructor exists — grep). The autonomous routes take `tz: &str` filled at the
  engine from `host_zone::zone_name(&self.inner.config.display_zone)` — a NAME on
  purpose: the schedule tick (`host.rs:596` `AutonomousRoomScheduleTickHandler { tz }`,
  `step.rs:1424` `TickDeps`) evaluates cron by `HostConfig.tz`, and a manual
  start computing `nextScheduledRunAt` under the POSIX VALUE while the tick runs on
  `"UTC"` would disagree. (A value-taking cron is a whole-calendar change — Tier 3.)
  Then `system_zone_name` has no caller and is deleted.
- **Exact harness edits:** `autonomous_rooms_routes_equivalence.rs` **×9** (not ×8
  as P4.127 §Survey 2 says): `:414`, `:429`, `:440`, `:453`, `:461`, `:468`, `:480`,
  `:495`, `:508`; `chat_export_equivalence.rs:326`, `:346`;
  `chat_scenario_routes_equivalence.rs:943` — **12 call sites**. `get_messages_caller_census.rs:267`
  needs NO edit (it names `chat_export_markdown` by function, `F` = fallback — a
  signature change does not move it). In core: `api/engine.rs` ×6 dispatch arms.

### A7 The census after Option V (`host_zone_sites_census.rs`)

- `CENSUS` (`:59-65`): `host_zone.rs` `2 → 1` (the `fn` alone once
  `system_zone_name` goes); the row's "two recorded Tier-3 name readers" text goes.
- `HELPER_SITES` (`:146-164`) and assertion (1b) (`:291-311`): **retire** with
  `display_zone_named`. Replace with a `VALUE_SITES` needle table: core
  `orchestrator.rs` (`input.display_zone` into `build_turn_tool_runner` + into
  `BuildContextArgs`), `build_context.rs` (`input.display_zone`),
  `regenerate_swipe.rs`, `chat_create.rs` (`deps.display_zone`), `enclave/step.rs`.
- `HOST_SITES` (`:70-120`): the two spine needles (`:107`, `:114`) change to
  `.with_display_zone(self.display_zone.clone())` / `.to_zoned(self.display_zone.clone())`;
  add needles for the spine's four input fills, the `ChatSpine`/`ChatCreateSpine`
  fills in `ProductionSpineFactory::build` (`spine.rs:3779`, `:3802`), and the
  factory fill in `crates/quilltap-web/src/lib.rs` (a new site OUTSIDE the host
  crate — the table already reads by repo-relative path).
- `UTC_ALLOWED` `host_zone.rs` `2 → 1` (the helper's fallback goes). The host-crate
  `TimeZone::UTC` count (`:424-431`, spine `1`) is `js_local_offset_minutes`'s
  `unwrap_or(UTC)` at `spine.rs:2411` (the comment at `:419-423` calls it "the cron
  parser" — it is the offset helper); +1 if `ProductionSpineFactory`'s UTC default
  lands in `spine.rs` (re-count).
- The divergence header (`:36-48`) retires; replace with the residue (A1's calendar
  NAME list) and the ICU-ignores-DST-rule note. The Chicago child's marker
  (`"CHILD OK: 5 host-wired entries"`, `:489`, `:611`) moves with its rewrite; the
  POSIX child gets its own marker.

### A8 The differential

- No v4 hunk, no new oracle arm owed (v4 formats in the process zone everywhere;
  `jest.config.ts:11` `process.env.TZ = 'UTC'` confirmed, with the why-comment
  `:4-10`). The proof is P4.127's 26 families green under BOTH `TZ=UTC` and NO `TZ`
  (record: `status-log.md` "## P4.127", `:156497-156513`): P4.119's 18 —
  `conversation_markdown`, `almanack_render`, `almanack_tier2`,
  `context_feeders_leaves`, `progressions_engine`, `mail_carina_tools`,
  `post_office_routes`, `post_office_concierge_lantern_suparna`,
  `scriptorium_tools`, `conversation_annotations_tier2`,
  `conversation_annotations_upsert_tier2`, `web_search_tool`, `web_search_wire`,
  `orchestrator_tier3`, `embedding_remainder`, `host_zone_dates`,
  `markdown_transcript`, `annotations_rendering_patterns` — plus
  `chat_admin_routes`, `tool_dispatch`, `salon_reads`, `help_chats_routes`,
  `chat_context_init`, `subprompts_prompt_tier2`, `carina_query_tier3`,
  `chat_create_capstone`. **Add** the families whose literals move and were not
  in the 26: `build_context_tier3`, `regenerate_swipe_tier3`,
  `salon_swipe_generate`, `enclave_step_tier3`; item (g)'s
  `autonomous_rooms_routes`, `chat_export`, `chat_scenario_routes`; and
  `host_zone_sites_census`, `get_messages_caller_census` (neutral).
- `tool_dispatch_equivalence`'s oracle-generation `TZ=UTC` pins (its tsx lines)
  stay; `host_zone_dates` (tsx, `harness/oracle/cases/host-zone-dates.ts`) is the
  family that proves both zones.
- Mutation proofs (file-backup revert): (M1) a spine fill → `TimeZone::UTC` →
  census host pin red; (M2) `ProcessMessageInput.display_zone` ignored in
  `process_message` (back to `TimeZone::UTC`) → the VALUE_SITES needle red AND
  `orchestrator_tier3` neutral (proving the differential cannot see it — why the
  needle exists); (M3) the web factory fill dropped → census red; (M4) the POSIX
  child's whisper built from `display_zone_named(Some(&cfg.tz))` → child red.

---

## §B Item — v4's `fileProcessing` SSE frame

### B1 v4's position and shape

`lib/services/chat-message/orchestrator.service.ts`: the debug frame
`controller.enqueue(encodeDebugInfo(…))` (`:1372-1384`), then

```ts
  // Send fallback processing info if any
  if (fileProcessing.fallbackResults.length > 0) {
    controller.enqueue(encodeFallbackInfo(encoder, fileProcessing.fallbackResults))
  }
```

(`:1386-1389`), then the `validating` status
(`safeEnqueue(controller, encodeStatusEvent(…{stage: 'validating', message: \`Validating context for ${character.name}...\`…}))`,
`:1397-1402`). It sits after the agent-mode block (`:1346-1370`) and after the
courier short-circuit (so a courier turn never sends it). Encoder
(`streaming.service.ts:564-576`): each row is
`{filename: result.processingMetadata?.originalFilename || 'Unknown', type: result.type, usedImageDescriptionLLM: result.processingMetadata?.usedImageDescriptionLLM || false, error: result.error}`;
the line is `data: ${JSON.stringify({ fileProcessing: fallbackInfo })}`; an
`undefined` `error` drops out. Type: `types.ts:471-476`
(`fileProcessing?: Array<{filename; type; usedImageDescriptionLLM; error?}>`).
No other v4 caller of `encodeFallbackInfo` (grep: `orchestrator.service.ts:58,1388`,
the `index.ts:81` re-export).

### B2 What `fallbackResults` holds

`loadAndProcessFiles` lives in `lib/services/chat-message/context-builder.service.ts:156-235`
(⚠ not `lib/chat-files-v2.ts` as the brief says): one `processFileAttachmentFallback`
result per loaded attachment, **pushed unconditionally** (`:194-210`) — including
the provider-supports-it-natively case, `type: 'unsupported'` with no `error`. So
the frame fires on **every streamed turn that loaded at least one attachment**,
not only when a fallback was used (the corpus row is exactly that native case:
`"type":"unsupported","usedImageDescriptionLLM":false`). `type` ∈
`'text' | 'image_description' | 'unsupported'` (`lib/chat/file-attachment-fallback.ts:266-267`).
Lantern images are loaded elsewhere (`buildMessageContext` section K) and never
enter `fallbackResults`.

### B3 v5's twin today

- `services/chat_files.rs:479-551` `load_and_process_files` computes
  `fallback_results: Vec<file_fallback::FallbackResult>` (`:514`, pushed `:533`,
  read by the keep-filter `:540`) and **drops it**: `ProcessedFiles`
  (`:467-474`) carries only `attached_file_ids`, `message_content_prefix`,
  `attachments_to_send`. (The harness comment cites `chat_files.rs:507-526`; the
  lines have drifted to `:514-533`.)
- `file_fallback::FallbackResult` (`services/file_fallback.rs:247-258`) +
  `ProcessingMetadata` (`:209-245`, `original_filename` always present,
  `used_image_description_llm: Option<bool>`) + `FallbackType` (`:198-205`,
  `snake_case` → `text`/`image_description`/`unsupported`) carry everything the
  row needs.
- Emission: `services/orchestrator.rs:1680-1700` runs `load_and_process_files`;
  the `preparing` status `:2647`; the courier short-circuit after it; the
  `validating` status `:2703-2709`. **Insert the frame immediately before `:2703`**
  — v4's exact neighbour (the `debugLLMRequest` frame between them is not ported
  and is dropped by the harness filter).
- The wire: `ChatEvent` is `services/chat_events.rs:263` (`#[serde(untagged)]`,
  single-key variants); `api/types.rs:5182` `EventPayload::Chat(ChatEvent)` wraps
  it and `Event` flattens it beside `chatId` (`:5164-5174`). **A new
  `ChatEvent::FileProcessing { #[serde(rename = "fileProcessing")] file_processing: Vec<FileProcessingEntry> }`
  changes NOTHING in `api/types.rs`.** No exhaustive `match` on `ChatEvent`
  exists outside its own constructors (grep: `ChatEvent::PascalResult` appears
  once; `quilltap-web/src/events.rs:81-83` serializes generically;
  `realtime/types.rs:167,217` match `Content` only).
- Shape to add: `pub struct FileProcessingEntry { filename: String, #[serde(rename="type")] type_: FallbackType, #[serde(rename="usedImageDescriptionLLM")] used_image_description_llm: bool, #[serde(skip_serializing_if="Option::is_none")] error: Option<String> }`
  (field order = v4's literal), mapped `processing_metadata.map(original_filename).unwrap_or("Unknown")`,
  `…used_image_description_llm.unwrap_or(false)`; `ProcessedFiles` gains
  `fallback_results` (or the mapped entries).

### B4 The harness

`crates/quilltap-harness/tests/orchestrator_tier3_equivalence.rs:510-531`
`EXPECTED_EVENT_DIVERGENCES` — one entry, case `empty_content_image_on_vision_seat`,
frame `{"fileProcessing":[{"filename":"lb_fit_a.webp","type":"unsupported","usedImageDescriptionLLM":false}]}`;
applied at `:533-568` (`(0,1)` removes it from v4's side; `(1,1)` → "VANISHED").
The trace compare is **ordered** whole-vector equality (`assert_events_eq`,
`:3314-3324`), so the position (before `validating`, which is in
`shared_status_stage`, `:1257-1274`) is a comparand. The oracle ALREADY records
the frame (the `(0,1)` arm passes on main — the pin is green), so **no regen is
owed for this item**. Red-first: implement the frame with the entry present →
`VANISHED` on exactly that case; delete the entry → green. The only other `fileIds`
case (`paused_hold_attachment`) is held before the stream on both sides, so exactly
one frame moves. After deletion `EXPECTED_EVENT_DIVERGENCES` is empty; it has no
enum, so nothing goes `dead_code` (keep the mechanism or delete it — unifier's call).
Also fix the stale `filter_events` comment (`:1288-1292`). Mutation: move the emit
after `validating` → the ordered compare reds.

### B5 Client handling (the contract for P4.145)

v4's client has **no** `fileProcessing` handling: `grep -rn fileProcessing app components hooks lib`
hits only the encoder and the type; `app/salon/[id]/hooks/useSSEStreaming.ts:600-720`
branches on `status`/`content`/`reasoning`/`error`/`toolsDetected`/`toolResult`/
`done`/`turnStart`… with no catch-all. v5's `apps/web/src/app/core/chat-stream.reducer.ts:211-333`
`reduceChatFrame` is the same independent-`if` shape — a `{chatId, fileProcessing}`
frame hits no branch and returns `prev` unchanged; the Salon (`salon-conversation.ts:3640-3647`)
and Brahma (`brahma-console-dialog.ts:356`) consumers then see no transition. Safe
today; P4.145's survey (same directory) reaches the same verdict. **Contract:**
`{"chatId":"<id>","fileProcessing":[{"filename":string,"type":"text"|"image_description"|"unsupported","usedImageDescriptionLLM":bool,"error"?:string}]}`
— one frame per streamed turn with ≥1 loaded attachment, before the `validating`
status, never on the courier path.

---

## §C Item (d) — Carina's missing `CHAT_MESSAGE` row

### C1 v4

`lib/services/carina/carina.service.ts:669-690` `runStream` calls the ONE funnel
`streamMessage({messages, connectionProfile, apiKey, modelParams, tools, useNativeWebSearch, userId, chatId, characterId: answerer.id})`
— no `messageId`, no `previousResponseId`, no `stop`, no `logType` (→
`'CHAT_MESSAGE'`). It is called at **three** sites: the initial call (`:693`), each
tool-loop turn (`:733`), and the forced-text final turn with `[]` tools / `false`
(`:754`). The funnel (`lib/services/chat-message/streaming.service.ts:388-519`)
logs at `chunk.done` when `userId` is truthy (`:476-514`): `logLLMCall({userId, type:
logType, messageId, chatId, characterId, provider, modelName, connectionProfileId:
connectionProfile.id, request: {messages: role/content/attachments, temperature,
maxTokens, tools: tools.length>0 ? tools : undefined}, response: {content:
accumulatedContent, finishReason}, usage, cacheUsage, rawProviderUsage,
requestHashes: computeRequestPrefixHashes(llmMessages, tools…), durationMs})`; a
failed write WARNs `'Failed to log LLM call from streaming service'` `{userId,
error}` (`:509-513`). So each Carina leg writes a row with `characterId =
answerer.id`, `messageId` NULL, `chatId` the chat. Under an autonomous run v4's
ambient `runWithAutonomousRunId` also stamps `autonomousRunId` on it.

### C2 v5

`services/carina_query.rs:1174-1240` `run_stream(streaming, ctx: &StreamCtx, messages, tools, use_native_web_search)`
streams through `watch_stream` and accumulates `(answer, raw)` — it observes no
usage and logs nothing. `StreamCtx` (`:1153-1170`) carries provider/base_url/model/
sampling/profile_parameters/api_key/user_id/chat_id/answerer_id — no `Db`, no
profile id. Built at `:618-630`; called at `:632` (initial), `:714` (loop),
`:733` (forced final) — the same three as v4.

P4.121's logger: `services/primary_stream.rs:1184-1216`
`pub(crate) async fn log_loop_leg(db, ids: &LegLogIds, started_at_ms, profile: Option<&EffectiveProfile>, params: &StreamParams, content, leg: &LegUsage, raw_response)`
(gates on empty `user_id` and a `None` profile), with `LegUsage::observe`
(`:1143-1163`) per chunk and `LegLogIds { user_id, chat_id, message_id: &str (""
→ NULL via `log_stream_message_call`, `:1094-1098`), character_id: Option, log_context }`
(`:1166-1176`). `log_chat_message_call` computes the request-prefix hashes over
`params.messages` incl. `name`/`toolCallId`/`toolCalls` (`:1037-1080`) — v4's
`computeRequestPrefixHashes(llmMessages…)`. `EffectiveProfile` (`:285-298`) is
built from a profile `Value` by `orchestrator::to_effective_profile` (`orchestrator.rs:4698`,
`pub(crate)`). Everything is reachable from `carina_query.rs` (same crate,
`services`). **So yes: `run_stream` can call `log_loop_leg` after its chunk loop**
— add `db: &Db` + the `EffectiveProfile` to `StreamCtx`, keep a `LegUsage` +
`started_at_ms = clock::now_unix_ms()` per call, accumulate content, and on the
`done` chunk call `log_loop_leg(db, &LegLogIds{user_id, chat_id, message_id: "",
character_id: Some(answerer_id), log_context: &LogContext::none()}, …, Some(&profile),
&params, answer.clone(), &leg, raw.clone())`. All three call sites log (v4 logs per
`streamMessage`), which a single-leg corpus case cannot distinguish from "once per
consult" — a Tier-2 unit test pins it.

**Neutrality:** `carina_query_tier3_equivalence.rs:602` opens its `Db` with
`llm_logs: None`, so `log_llm_call`'s write fails soft (`services/llm_logging.rs:256-…`,
`PartitionUnavailable`) and that family is unmoved. Of the families that both run a
real Carina engine and dump `llm_logs`, only `orchestrator_tier3` qualifies
(`enclave_step_tier3` wires `NoCarina`, `:293-307`; `answer_confirmation_tier3`
runs no `RealCarinaQuery`). The ask_carina TOOL path (`tools/ask_carina.rs:129,335`)
reaches the same `run_carina_query` and will log too — faithful (v4's handler →
`runCarinaQuery` → the funnel).

### C3 The red-first

`orchestrator_tier3_equivalence.rs:428-462`: `LogDivergence::V4OnlyRow` and the one
`EXPECTED_DIVERGENCES` entry `("carina_markup", "It is high noon.", V4OnlyRow)`,
applied at `:464-508` (matches `type == CHAT_MESSAGE && chatId == case chat &&
decoded response.content == leg`). With the hunk landed and the entry present →
`"carina_markup / \"It is high noon.\": the v4-only row VANISHED"` on exactly that
case. Delete the entry → the row compares WHOLE against v4's (request summary,
sampling knobs, `tools`, response content + `finishReason`, usage,
`requestHashes`, `characterId`, NULL `messageId`, `connectionProfileId`). The
oracle already records v4's row (the `(0,1)` arm is green on main) → **no regen
needed for this item either**. If the Carina leg's `historyTailHash` carries a clock
(it should not — a consult's tail is the question), it joins `CLOCK_TAIL_LEGS`
(`:586`) — measure, do not predict. With the entry gone, `LogDivergence` has no
constructed variant and is `dead_code` under `-D warnings` — delete the enum + arm
+ the empty table (the §S 2 precedent, P4.129's Unification), or keep the machinery
with one documented allow.

### C4 P4.129's recorded nits about the arm

From its Unification paragraph: (1) the `(0,1)` arm matches chat + reply only — it
"could pin `characterId == answerer` and `messageId == null`"; moot once the entry
retires and the row compares whole (the whole-row compare pins both). (2)
`<msgref>` collapses distinct unmapped ids (the enclave family's `<uuidref>`
class) — Carina's row has NULL `messageId`, so it does not exercise this.
(3) the `contentLength` placeholder on render rows — unrelated to Carina.

### C5 Overlap with P4.139's wrapped key reads

- `carina_query.rs:867-876` `carina_api_key` (already through
  `db::fallback::find_api_key_by_id_or_none`) — **disjoint** from this lane's
  hunks: `StreamCtx` `:1153-1170`, `run_stream` `:1174-1240`, the ctx literal
  `:618-630` (and its three call sites `:632`, `:714`, `:733` unchanged in shape).
- `orchestrator.rs:611-626` (the pricing context's `find_by_id_and_user_id` loop,
  `if let Ok(Some(..))` — a silent fold P4.139's census may convert) — **disjoint**
  from this lane's hunks: `:445-475` (struct), `:2401-2414` (BuildContextArgs
  fill), `:2703` (the frame), `:3115-3123` (runner zone), `:4372-4386` +
  `:4532-4559` (BuildContextArgs/Input), tests `:4956-4972`, `:5010-5035`.
  A split by named hunk is clean: neither lane needs the other's lines.

---

## What the recorded description got wrong

1. **"Option V must also fix the zone VALUE … the VALUE `HostConfig` reads resolves
   to UTC"** (`dogfood-findings.md` Standing notes `:651-658`; `phase-4.md:7351-7357`;
   P4.127's Unification "Dogfood … B5"). **Refuted:** jiff 0.2.31 parses the
   rule (`system/mod.rs:204-205`; probe: `-05`, `iana=None`). The "value-fed
   Scenario Builder clock" is NAME-fed (`spine.rs:1870-1871`,
   `display_zone_named(Some(&self.tz))` — and the census's own `HOST_SITES` row at
   `host_zone_sites_census.rs:112-119` pins it that way). No B5 surface was
   value-fed. Threading the value IS the fix.
2. **"v4's Node/ICU does [honour a POSIX `TZ`]"** (`dogfood-findings.md:656`). Half
   wrong: Node 24 ICU ignores a DST POSIX rule and shows `/etc/localtime`'s zone
   (`EST5EDT,…` / `PST8PDT,…` / `XST-9XDT,…` all render Chicago `2:40 PM`); it
   honours only a DST-less rule (`XST-9` → `4:40 AM`). The walk's `XST6XDT` agreed
   with v4 by coincidence (it is Chicago's rule on a Chicago host).
3. **"`display_zone_named` retiring with the `server_tz` NAME reads"**
   (`phase-4.md:7344-7346`). The helper retires; `server_tz` does not — the memory
   distill's calendar, cron and the cleanup still need a NAME (§A4).
4. **The scope "`ProcessMessageInput` / `BuildContextInput` (and the greeting's
   deps)" is short.** The swipe reaches `build_context` through
   `RegenerateSwipeOptions` → `BuildContextArgs`; the autonomous turn through
   `StepDeps`; the spine's own runner and SB clock through `ChatSpine`; the greeting
   through `ChatCreateSpine` → `ChatCreateDeps`; production through
   `ProductionSpineFactory` in `quilltap-web/src/lib.rs`. Eight structs, not three
   (§A3); harness literals: `orchestrator_tier3` ×2, `build_context_tier3` ×1,
   `regenerate_swipe_tier3` ×1, `salon_swipe_generate` ×1, `enclave_step_tier3` ×1,
   `chat_create_capstone` ×2; web tests ×8.
5. **P4.127 §Survey 2: "`autonomous_rooms_routes_equivalence.rs` ×8 …
   `get_messages_caller_census.rs:267`"** — it is ×9, and the census row needs no
   edit (it names the function). Total 12 call-site edits.
6. **The census comment "its ONE production `TimeZone::UTC` is `spine.rs`'s cron
   parser"** (`host_zone_sites_census.rs:419-423`) — that `TimeZone::UTC` is
   `js_local_offset_minutes`'s fallback (`spine.rs:2411`); cron parses inside core.
7. **The brief's v4 path `lib/chat-files-v2.ts` for `loadAndProcessFiles`** — it
   is `lib/services/chat-message/context-builder.service.ts:156`. And the
   `fileProcessing` frame is not "when a fallback was used": every loaded
   attachment yields a row (B2).
8. **The harness comment `chat_files.rs:507-526`** (`orchestrator_tier3_equivalence.rs:519-520`)
   has drifted to `:514-533`.
9. **`quilltap-web/src/main.rs:213-218`'s WARN** says a rejected `TZ` leaves the
   clock "follow[ing] the system zone" — for jiff an unparseable `TZ` (e.g. `CDT`,
   `XST5XDT`) is an `Err` and v5 falls to UTC (`host_zone.rs:50`). Not this lane's
   file; recorded (§G).

Re-verified as stated: v4 enqueues at `orchestrator.service.ts:1387-1388`;
`encodeFallbackInfo` `streaming.service.ts:564-576`; the frame's recorded JSON;
P4.129's `carina.service.ts:676-686` / `streaming.service.ts:478-515` citations;
`jest.config.ts:11`; Carina's `characterId: answerer.id` and no `messageId`; the
`V4OnlyRow` entry "trips VANISHED the day it does".

## Proposed tiered deliverables

### Tier 1 — must land

1. **Option V core:** `display_zone: TimeZone` on `ProcessMessageInput`,
   `BuildContextArgs`, `BuildContextInput`, `RegenerateSwipeOptions`,
   `StepDeps` (`&'a TimeZone`), `ChatCreateDeps`; `orchestrator.rs:3122`,
   `build_context.rs:3621`, `chat_create.rs:1232` read it. Every literal in §A3
   filled (`TimeZone::UTC` in the harness beside `server_tz: Some("UTC")`).
   Differential: the 26 + 7 families of §A8, green under `TZ=UTC` AND no `TZ`.
2. **Option V host:** `ChatSpine.display_zone`, `ChatCreateSpine.display_zone`,
   `ProductionSpineFactory::with_display_zone` filled in
   `quilltap-web/src/lib.rs` `production_host_config`; `spine.rs:1165`, `:1871`
   and the four input fills read the value. `HostConfig::set_display_zone` +
   `host_zone::zone_name`; the web + tauri test commons moved onto it.
3. **Retire `display_zone_named`** (and its unit test `host_zone.rs:106-119`; the
   orchestrator wiring probe `:5026` passes a `TimeZone::get("America/Chicago")`).
   Module docs (`host_zone.rs:14-35`, `:53-72`) rewritten: the divergence closes;
   the calendar NAME residue named.
4. **Item (g):** `chat_export_markdown(.., zone: &TimeZone)` with
   `LocalOffset::Zone(&TimeZone)`; the five autonomous routes take `tz: &str` from
   `zone_name(&config.display_zone)`; `system_zone_name` + both `system_tz` deleted;
   engine dispatch ×6; the 12 harness call sites.
5. **The census** reshaped (§A7) + the **POSIX child** (§A5) with its red-first
   recorded (the pre-fix `display_zone_named(Some(&cfg.tz))` reading `0`).
6. **The `fileProcessing` frame:** `ChatEvent::FileProcessing` +
   `FileProcessingEntry` in `services/chat_events.rs`; `ProcessedFiles` carries the
   results; emitted before `validating` (`orchestrator.rs:2703`). Red-first:
   `EXPECTED_EVENT_DIVERGENCES` trips VANISHED, then the entry is deleted
   (`orchestrator_tier3`).
7. **Carina's row:** `StreamCtx` gains `db` + `EffectiveProfile`; `run_stream`
   observes `LegUsage`, times its own call and calls `log_loop_leg` at `done` (all
   three legs). Red-first: the `V4OnlyRow` trips VANISHED; the entry, the
   `LogDivergence` enum and its arm deleted; the row compares whole
   (`orchestrator_tier3`).

### Tier 2 — should land

8. Mutation proofs M1–M4 (§A8), the frame-order mutation (B4), and a Carina
   mutation (drop the `log_loop_leg` call → `orchestrator_tier3` reds on the row).
9. A core unit test that a Carina consult driven through a tool-call loop + the
   forced final writes THREE rows (one per `run_stream`), with `characterId` the
   answerer and a NULL `messageId` (no corpus case has a multi-leg Carina).
10. `host-zone-dates` third arm under `TZ=XST-9` (oracle records `process.env.TZ`;
    Rust side `TimeZone::posix`).
11. Restore the seven `_in_zone` why-comments P4.127 dropped (its Unification
    "Recorded, not fixed") while those files are open — only where this lane
    already edits.

### Tier 3 — loud deferrals

12. The calendar NAME residue under a POSIX `TZ` (A1/A4): cron, the distill
    `local_tz`, `js_local_offset_minutes`, the story-zone `timezone: Some(tz)`
    fallback (v4 `resolveTimezone` → `undefined`), the LLM-log cleanup, the
    Almanack `timezone` fact — a value-taking cron + day-reference seam is its own
    order.
13. An unparseable `TZ` (jiff `Err` → UTC; ICU → `/etc/localtime`) — no jiff API
    for "system zone ignoring `TZ`"; a WARN or a hand-read of `/etc/localtime` is
    a ruling.
14. ICU ignoring a DST POSIX rule (v4 shows the OS zone; v5 honours the rule) —
    recorded as the ruled shape, not ported.
15. Carina's row under an autonomous run carries no `autonomousRunId` (v4's
    ambient ALS stamps it): `RunCarinaQueryOptions` (`carina_runner.rs:132`, literals
    `ask_carina.rs:129`, `carina_runner.rs:320`, `carina_query_tier3_equivalence.rs:627`)
    would need a `LogContext`; no corpus can see it today.
16. v4's funnel normalizes content-block chunks (`normalizeContentBlockFormat`,
    `streaming.service.ts:447-453`) and WARNs on a failed log write; v5's Carina
    `run_stream` does neither (the log write's silence is shared with every
    `log_loop_leg` caller).

## Files the lane would edit

Core (`crates/quilltap-core/src/`):
- `host_zone.rs` (retire `display_zone_named` + `system_zone_name`; add `zone_name`; docs)
- `services/orchestrator.rs` (`ProcessMessageInput`, `BuildContextArgs`, `build_context_input`, `:2401` fill, `:2703` frame emit, `:3122`, tests `:4958`, `:5026`)
- `services/build_context.rs` (`BuildContextInput` field, `:3621`, tests `:4464`, `:4798`, `:4972`)
- `services/message_context.rs` (test literal `:2450`)
- `services/regenerate_swipe.rs` (`RegenerateSwipeOptions`, destructure `:436`, `:633`)
- `services/chat_create.rs` (`ChatCreateDeps`, `:1232`, test `:3886`)
- `enclave/step.rs` (`StepDeps`, `:689`, test `:1909`)
- `services/chat_files.rs` (`ProcessedFiles` carries the results)
- `services/chat_events.rs` (`ChatEvent::FileProcessing` + `FileProcessingEntry` + constructor)
- `services/carina_query.rs` (`StreamCtx`, `run_stream`, the ctx literal `:618-630`)
- `services/markdown_transcript.rs` (`LocalOffset::Zone(&TimeZone)`, `chat_export_markdown` param, `system_tz` deleted)
- `api/autonomous_rooms.rs` (five routes take `tz`, `system_tz` deleted)
- `api/engine.rs` (six dispatch arms: `:1680`, `:4668-4691`)
- possibly `services/primary_stream.rs` (only if `log_loop_leg`/`LegUsage` visibility must widen — they are `pub(crate)`, so likely none)

Host / web / tauri:
- `crates/quilltap-host/src/host.rs` (`set_display_zone`, `zone_name` use)
- `crates/quilltap-host/src/spine.rs` (`ChatSpine`, `ChatCreateSpine`, `ProductionSpineFactory`, `:1165`, `:1364-1374`, `:1563-1593`, `:1643-1669`, `:1871`, `:2110-2117`, `:2292-2298`, `:3779`, `:3802`)
- `crates/quilltap-web/src/lib.rs` (`production_host_config` fill)
- `crates/quilltap-web/tests/chat_create_end_to_end.rs`, `chat_send_smoke.rs`, `scenario_builder_spine/mod.rs`, `swipe_spine/mod.rs` (struct literals), `crates/quilltap-web/tests/common/mod.rs` (`set_display_zone`)
- `crates/quilltap-tauri/tests/common/mod.rs` (`set_display_zone`)

Harness (`crates/quilltap-harness/tests/`):
- `orchestrator_tier3_equivalence.rs` (two `ProcessMessageInput` literals; delete the `fileProcessing` entry; delete the Carina `V4OnlyRow` + `LogDivergence`; the `:1288-1292` comment; `CLOCK_TAIL_LEGS` only if measured)
- `build_context_tier3_equivalence.rs`, `regenerate_swipe_tier3_equivalence.rs`, `salon_swipe_generate_equivalence.rs`, `enclave_step_tier3_equivalence.rs`, `chat_create_capstone_equivalence.rs` (literals)
- `autonomous_rooms_routes_equivalence.rs` (×9), `chat_export_equivalence.rs` (×2), `chat_scenario_routes_equivalence.rs` (×1)
- `host_zone_sites_census.rs` (§A7 + the POSIX child)
- `host_zone_dates_equivalence.rs` (Tier 2 item 10 only)

Oracle: `harness/oracle/cases/host-zone-dates.ts` (Tier 2 item 10 only — record `process.env.TZ`). **No committed fixture rebuilt; no oracle regen owed for the frame or the Carina row** (both already recorded); families regenerate from the round pin per the sweep driver.

## Files the lane must READ but not edit

`crates/quilltap-core/src/api/types.rs` (FROZEN — confirmed untouched by this
design); `services/primary_stream.rs` (`log_loop_leg`, `LegUsage`,
`log_stream_message_call` — reuse only); `services/file_fallback.rs`;
`tools/executor.rs` (`with_display_zone`); `services/tool_execution.rs`
(`ToolRunner::display_zone`); `enclave/cron.rs`; `services/llm_logging.rs`;
`crates/quilltap-web/src/main.rs` (TZ reconciliation);
`harness/oracle/cases/orchestrator-tier3.test.ts` and its fixture
(`harness/oracle/fixtures/orchestrator-tier3.json`) — no edit owed; v4's
`orchestrator.service.ts`, `streaming.service.ts`, `context-builder.service.ts`,
`carina/carina.service.ts`, `chat/timestamp-utils.ts`, `jest.config.ts`;
`apps/web/src/app/core/chat-stream.reducer.ts` (P4.145's).

## Cross-lane adjacencies / risks

- **P4.139** (API-key class): `carina_query.rs:867-876` and `orchestrator.rs:611-626`
  are its; this lane's hunks are disjoint (§C5). Both lanes edit `orchestrator.rs`
  and `carina_query.rs` — a split by named hunk is clean, but expect a textual
  merge. If P4.139's "host-level keyed recorder pin" lives in
  `crates/quilltap-web/tests/profile_bound_api_key_wire.rs` it calls
  `ProductionSpineFactory::new` (`:174`) — untouched by the builder design.
- **P4.145** (SPA): owns the client half of `fileProcessing`; the contract is §B5
  (frame JSON, position, firing rule). v4 has no client handling; v5's reducer
  already ignores it. Only coordination: P4.145's `FileProcessingEntry` TS mirror
  must match the Rust field order/keys.
- **P4.141** (model layer) — none of its files; `primary_stream.rs` is read-only here.
- **P4.142** (repository fallbacks): `chatGet`/`listChats` and the sync applier are
  not touched here; `salon::chat_get`'s `zone` parameter is unchanged.
- **P4.144** (memory + harness smalls): its `[FoldEpisodePass]` lines live near
  `build_context`'s fold path — a same-file textual adjacency in
  `services/build_context.rs` if it touches it; its `recipe_sweep.py` change
  affects how this lane's families are swept (run after it, or coordinate).
- **`orchestrator_tier3_equivalence.rs`** is touched by THREE of this lane's items
  — no sibling should own it this round.
- **`quilltap-web/src/main.rs:213-218`** WARN wording (§E 9) — no lane owns it;
  a smalls candidate.

## Versions / bumps

- `quilltap-core` (0.0.1146) — bump.
- `quilltap-harness` (0.0.1074) — bump.
- `quilltap-host` (0.0.174) — bump (spine/host fields).
- `quilltap-web` (0.0.207) — bump (`src/lib.rs` fill + tests).
- `quilltap-tauri` (0.0.7) — bump only if its `tests/common/mod.rs` moves to
  `set_display_zone` (recommended).
- `quilltap-cli` — none (constructs none of these types; its `docs_cmd.rs` read stays).
- SPA — none (P4.145's).
