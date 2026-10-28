//! The `zod` version tripwire (P4.77 — phase-4 candidate 4 from the
//! `d883a5ee1` round).
//!
//! v5 hand-rolls Zod semantics in TWO engines (`pascal/custom_tool_types.rs`
//! plus its SPA twin, `apps/web/src/app/pascal/custom-tool-types.ts`) and
//! transcribes Zod's own `ZodError.message` sentences at ~150 edge sites
//! (settings routes, the wardrobe/chat-outfits/brahma `zod_uuid` gates, the
//! `web_edge_body_parse_guard` census, …). The oracle's `node_modules`
//! resolve v4's LIVE tree, so a `zod` dependency bump in the checkout is a
//! regen event for every one of them — and it moved once already without
//! anyone ordering a regen: `6e1a64ea6` bumped 4.4.3 to 4.5.4 mid-round, and
//! the `d883a5ee1` drift catch-up round (P4.D158 unit 2 item 4, see
//! `docs/developer/porting/status-log.md`) only caught it at a family's
//! first red, by consequence, not by design. This guard is the design: it
//! fails the workspace gate the moment the installed `zod` moves past the
//! version recorded below, so the obligation is visible at ordering time
//! instead of discovered mid-round.
//!
//! The locator is the shared `common::v4_root` (`QT_V4_CHECKOUT`, then the
//! `QT_V4_ROOT` alias, then `$HOME/source/quilltap-server`,
//! the convention every recipe header uses). An absent checkout directory
//! prints a loud `SKIP:` line, since a CI box with no v4 checkout must not
//! fail here, and never a silent pass. A checkout whose `zod` has moved is a
//! FAIL, never a skip, naming the obligation the P4.D158 lane record proved
//! out: re-run that same three-part measurement (the `v4/locales/en.js`
//! sentence diff, the `v4/core/regexes.js` regex diff, and a grep for any
//! newly-reachable validator) against the new version, then regenerate every
//! Zod-transcribing family, both hand-rolled engines, the SPA corpus, and
//! the ~150 edge sites, before trusting any of them again.
//!
//! Shape precedent: `spelling_guard.rs` (a repo-reading guard with no
//! fixture); `db_error_key_guard.rs` (an executable census against a
//! recorded constant).
//!
//! No recipe stage — this reads the checkout's installed `zod`, not the v4
//! oracle, so there is nothing to regenerate.
//!
//! Run standalone:
//!   cargo test -p quilltap-harness --test zod_version_guard

use std::path::PathBuf;

// P4.157 R-H: the ONE v4-checkout locator (`common::v4_root`).
mod common;

/// v4's installed `zod`, measured 2026-10-16 from the `25b8fab6a` pin's own
/// `npm ci --offline` root tree (`node -p
/// "require('./node_modules/zod/package.json').version"` — `4.6.7`,
/// post-`eb5d42338`). Previously `4.6.5` (measured 2026-09-21 from the
/// `f45a517a9` pin, post-`6b0615807`), `4.5.4` at the `d883a5ee1` baseline
/// (post-`6e1a64ea6`, measured 2026-09-05) and `4.4.3` before that. Bump this
/// ONLY alongside the re-measurement + regen the module doc above describes.
///
/// # The `4.6.5 -> 4.6.7` re-measurement (P4.D311, 2026-10-16)
///
/// `diff -rq` of the only 4.6.5 copy on the machine
/// (`~/source/quilltap-website/node_modules/zod`) against the pin's 4.6.7
/// (declaration files and maps excluded). **Not one sentence, regex or issue
/// shape moved — but ONE behaviour change did.**
///
/// - **IDENTICAL:** `v4/locales/en.js` (zero sentences), `v4/core/regexes.js`,
///   `v4/core/errors.js`, `core.js`, `to-json-schema.js`, and every other
///   `v4/**` file not named below.
/// - **`v4/core/versions.js`:** `patch: 5 -> 7`.
/// - **`v4/core/util.js`:** NEW `sizeCaps(defChecks)` — the LEADING checks of
///   a container whose name matches `/_(length|size)$|_equals$/` with a finite
///   upper bound (the walk stops at the first non-size check, which may
///   rewrite the value).
/// - **`v4/core/schemas.js`:** NEW `capSize` + `arraySize` / `mapSize` /
///   `setSize`, applied in `ZodArray` / `ZodMap` / `ZodSet` init (and
///   `compile.js`'s NEW `generateSizeGate` for the JIT paths). **The one
///   behaviour change:** a container whose size exceeds a leading
///   `.max(N)` / `.length(n)` / `.size(n)` now fails with ONLY its own
///   `too_big` (same `code` / `origin` / `maximum` / `inclusive` / message
///   bytes, `continue = false`) BEFORE any element is parsed, so element
///   issues and any later `.refine` on the container no longer appear —
///   even when every element is valid (the `too_big` aborts, so a following
///   refine is skipped). A within-bounds container and every string / number /
///   object check are byte-identical.
/// - **Where it reaches v5:** the four hand-written twins that emitted
///   "element issues THEN the array's `too_big`" — `services/chat_create.rs`
///   `check_opt_subprompt_ids` (`selectedSubpromptIds`, `.max(100)`),
///   `memory_candidate.rs` `string_array` (`entities`, `.max(MEMORY_ENTITY_CAP)`),
///   `api/memories.rs` (`entities`), `services/scenario_builder/
///   request_schema.rs` `uuid_array_field` (`characterIds` / `groupIds`,
///   `.max(32)`) — each now returns the lone `too_big` early, pinned red-first
///   by rows recorded through v4's REAL schemas at the pin in
///   `chat_create_capstone_equivalence`, `memories_routes_equivalence`,
///   `memory_candidate_equivalence` and `scenario_build_request_schema_
///   equivalence` — and a FIFTH twin the census missed, the Pascal
///   definition's `outcomes` / `effects` / `triggers` / wardrobe `items`
///   arrays in `pascal/custom_tool_types.rs` (P4.D306 ported `effects`;
///   `outcomes` and the string-input length checks landed at the
///   `25b8fab6a` unification, pinned by `pascal_custom_tool_definition_
///   equivalence`). v4 uses no `.length(n)` / `.size(n)` / `Map` / `Set` size
///   cap on a v5-transcribed schema (grep in the P4.D311 lane record).
/// - **The string-input half (`ZOD_STRING_LENGTH_PAST_INVALID_TYPE`, P4.169
///   F-2):** the four hand twins stopped at the array's `invalid_type`, but
///   4.6.7's length checks still run on any input with a `.length` — a string
///   past the cap adds a string-origin `too_big`, an object with a numeric
///   `length` an `unknown`-origin one. ONE helper,
///   `api::zod_issues::zod_length_checks_past_invalid_type`, now serves
///   `check_opt_subprompt_ids`, `string_array` and `uuid_array_field` (rows in
///   `chat_create_capstone`, `memory_candidate` and
///   `scenario_build_request_schema`); `api/memories.rs`'s `entities` twin is
///   P4.168's to adopt (a HANDOFF).
/// - **Regression ring:** every Zod-text family re-run from the pin through
///   the sweep driver (the lane record lists them) — no red attributable to
///   zod.
///
/// # The `4.5.4 -> 4.6.5` re-measurement (P4.D211, 2026-09-21)
///
/// The three-part read came back **wholly neutral for v5**: not one sentence,
/// regex or issue shape this port transcribes moved. Recorded here because the
/// next bump's lane starts from this table.
///
/// - **`v4/locales/en.js` — ZERO existing sentences changed.** The diff is 12
///   lines: two NEW nouns in the format-name dictionary (`currency_code:
///   "currency code"`, `iban: "IBAN"`). v4 calls neither `z.currencyCode()`
///   nor `z.iban()` (grep over `lib/ app/ packages/ plugins/`: 0 hits each),
///   so neither noun is reachable.
/// - **`v4/core/regexes.js` — three regexes CHANGED, three are NEW.** Changed:
///   `email` (the `(?!\.)(?!.*\.\.)` lookahead form became the grouped
///   `(?:X+\.)*X*` one), `_emoji` (a leading anchor lookahead), `base64url`
///   (`^[A-Za-z0-9_-]*$` became the group-of-four form — a real semantic
///   change). New: `currencyCode`, `iban`, `anyString`. **Only `email` is
///   reachable in v4** (7 sites; `.emoji(`/`.base64url(` have zero), and the
///   rewrite is semantically neutral over a 1,014-row corpus recorded from
///   v4's real validator at BOTH versions — `zod_email_equivalence`, whose
///   corpus exists because of this read.
/// - **Newly reachable validators: NONE.** `z.property()` (new
///   `$ZodCheckProperties`), `fromJSONSchema` (a whole new module, with the
///   only new prose in the release) and the new opt-in `abortEarly` /
///   `reportInput` parse flags all measure 0 hits in v4's source.
/// - **Beyond the two files the procedure names**, four more homes were read
///   because they shape issue BYTES rather than sentences: `core/errors.js`
///   and `core/core.js` moved only property-descriptor plumbing;
///   `core/util.js`'s `finalizeIssue` swapped object-rest for an own-key loop
///   that additionally drops an own `__proto__` — **key order is identical**,
///   which is what `api/zod_issues.rs` pins; and `core/to-json-schema.js`
///   changed by a function RENAME only. That last one matters: v4 derives all
///   ~57 tool `parameters` through `z.toJSONSchema()`
///   (`lib/tools/zod-to-openai-schema.ts`), so it feeds the provider wire.
///   Measured empirically rather than read — the emitted schema for a
///   15-field probe object is byte-identical across the two versions **except
///   for `email`'s `pattern`**, and no tool schema uses `z.email()` (0 hits in
///   `lib/tools/`), so `tool-wire.recorded.ndjson` re-recorded byte-identical.
/// - **`classic/iso.js` is byte-identical**, so the `datetime()` rewrite the
///   4.4.3 -> 4.5.4 read had to record did not move again.
///
/// Regenerated and proven byte-identical at both versions (same v4 source,
/// only `zod` swapped — see the lane record's control recipe): the two
/// hand-rolled engines' shared corpus (`pascal-custom-tool-definition.oracle
/// .ndjson`, 362 rows) and all nine provider recorders' corpora.
const RECORDED_ZOD_VERSION: &str = "4.6.7";

/// Read `<checkout>/node_modules/zod/package.json`'s `"version"` field
/// without pulling in a JSON dependency this test alone would need — the
/// field is a short quoted string near the top of a small, well-formed file.
fn installed_zod_version(pkg_json: &std::path::Path) -> String {
    let text = std::fs::read_to_string(pkg_json)
        .unwrap_or_else(|e| panic!("read {}: {e}", pkg_json.display()));
    let value: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", pkg_json.display()));
    value["version"]
        .as_str()
        .unwrap_or_else(|| panic!("no string \"version\" field in {}", pkg_json.display()))
        .to_string()
}

#[test]
fn v4s_installed_zod_matches_the_recorded_version() {
    let checkout = common::v4_root().unwrap_or_else(|| PathBuf::from(common::V4_DEFAULT_CHECKOUT));
    if !checkout.is_dir() {
        println!(
            "SKIP: no v4 checkout at {} (set QT_V4_CHECKOUT) — cannot verify the installed \
             zod version on this machine",
            checkout.display()
        );
        return;
    }

    let pkg_json = checkout.join("node_modules/zod/package.json");
    if !pkg_json.is_file() {
        println!(
            "SKIP: {} has no node_modules/zod/package.json (run `npm ci` in the checkout) — \
             cannot verify the installed zod version",
            pkg_json.display()
        );
        return;
    }

    let installed = installed_zod_version(&pkg_json);
    assert_eq!(
        installed,
        RECORDED_ZOD_VERSION,
        "v4's installed `zod` moved {RECORDED_ZOD_VERSION} -> {installed} at {}.\n\n\
         The oracle's node_modules resolve v4's LIVE tree, so this dependency bump is a \
         regen event for every family that transcribes Zod semantics: the two hand-rolled \
         engines (`crates/quilltap-core/src/pascal/custom_tool_types.rs` + its SPA twin \
         `apps/web/src/app/pascal/custom-tool-types.ts`), the SPA corpus, and the ~150 \
         Zod-sentence/regex edge sites (settings routes, the `zod_uuid` gates, the \
         `web_edge_body_parse_guard` census, …).\n\n\
         Before trusting any of them again, repeat the measurement against the new \
         version — diff `v4/locales/en.js`'s sentences and `v4/core/regexes.js`'s regexes, \
         grep v4 for any validator it newly reaches, and (the P4.D211 additions) read \
         `core/util.js`'s `finalizeIssue` for issue key ORDER and prove \
         `core/to-json-schema.js`'s emitted bytes empirically, since v4 derives every tool \
         `parameters` through `z.toJSONSchema()`. Then regenerate the affected families and \
         bump RECORDED_ZOD_VERSION here.\n\n\
         Two worked records to copy from, both in `docs/developer/porting/status-log.md`: \
         \"Lane record — P4.D158 unit 2 item 4: the Zod 4.4.3 -> 4.5.4 read\" and the \
         P4.D211 lane record (4.5.4 -> 4.6.5), whose result table is repeated verbatim in \
         RECORDED_ZOD_VERSION's own doc comment above.\n\n\
         One trap the P4.D211 lane measured and the recipe must carry: a pinned v4 \
         worktree CANNOT reproduce a past dependency state. Its `node_modules` are \
         symlinks into the live checkout, so a regen \"at the baseline pin\" still runs the \
         CURRENT zod and the CURRENT provider SDKs. To isolate the dependency, build a \
         control whose `node_modules` is a directory of per-entry symlinks with the one \
         package swapped — and prove the control LIVE by mutating it before believing any \
         \"byte-identical\".",
        pkg_json.display()
    );
}
