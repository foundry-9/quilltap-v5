# Quilltap Changelog

## Recent Changes

### 5.0-dev

W4.10a (the spine wiring pass): closed three deferred composition-point seams.
(1) `model_supports_native_tools` is now sourced in-spine from the real
`check_model_supports_tools` over an injected `PricingFetcher` (the fetch stays a
seam); the `ProcessMessageInput` field was dropped. (2) The danger router is wired
with the real DB-backed `DbApiKeys` resolver, reading the fixture-seeded `api_keys`
table end to end (closing the W4.7d→W4.4b key-material handoff). (3) The real
`RunCarinaQuery` engine is wired: a `RealCarinaQuery` adapter over
`run_carina_query` at the finalizer markup path, plus an erased `ErasedAskCarina`
seam + `ask_carina` dispatch row on `BuiltInToolRunner` (additive; default = the
prior loud fallback). The orchestrator corpus gained a live `@Name:` markup case
(the recorded inner carina stream proves the engine's system-prompt bytes; the
carina message posts, the `carinaAnswer` event emits, the `CARINA_MEMORY_EXTRACTION`
job enqueues), and `tool_dispatch` gained an `ask_carina` row (a not-found answerer
drives the real engine's early-return against v4's real dispatch). Regenerated the
orchestrator oracle (un-mocked `checkModelSupportsTools` + empty `getPricingCache`;
un-monkey-patched `findApiKeyByIdAndUserId`; `textblock_mode` → OPENAI `o1-mini`).
`message_finalizer` / `carina_runner` / `mail_carina` / `tool_build` /
`regenerate_swipe` / `tool_dispatch` re-verified green. Deferred: a live
`ask_carina` tool-call THROUGH the `process_message` spine (the erased-seam
`'static` boundary needs owned engine providers, which the differential's shared
borrowed streaming provider cannot supply); the dispatch + engine are proven by the
seam unit tests, the live `@Name:` case, and the `tool_dispatch` row.

Wiring-round prep: wrote the three work orders for the post-Round-4 spine
closure — W4.10a (the spine wiring pass: source model_supports_native_tools
from the real check_model_supports_tools, wire the real DB-backed
ApiKeyResolver at the danger router, construct the real RunCarinaQuery at the
orchestrator/finalizer composition points with the ask_carina dispatch row and
the live @Name:/ask_carina orchestrator-corpus cases), W4.5b (the Brahma
one-shot console — v4's runBrahmaQuery composed from already-ported units,
implementing the frozen RunBrahmaConsole trait, with its own tier-3
differential), and W4.10b (the staged W4.7e3 llm_logs oracle regenerations,
steps 1-7). Round table updated with the three-lane parallel layout and
ownership rules; the spine with_logging wiring plus an orchestrator llm_logs
dump is deliberately post-round (it would couple W4.10a's corpus to W4.10b's
primary-stream regen). Written from two fresh surveys (the v5 composition
points; v4's brahma-console/one-shot.service.ts at 6b6e39ad — no drift). No
code changes.
W4.10b step 1 (logLLMCall regen — compression): converted the `compression_tier3`
oracle from a DB-free jest test to a real-DB one on both sides, un-mocking
`logLLMCall` and dumping the written `llm_logs` rows (`CONTEXT_COMPRESSION`), so
the writer is proven byte-for-byte through a real cheap-LLM call site. Six rows
land (happy-path + the two uncensored-fallback pairs + the unicode case; the
empty-window and llm-failure cases write none). Fixed a real port divergence the
row diff surfaced: v4's `summarizeResponse` always emits `error`/`finishReason`
(present as `null`), but the port's `LlmLogResponseSummary` skipped them when
`None` — changed both to the present-null-vs-absent double-`Option` (like `chats`'
`removedAt`), so the summarize path stores them present-null while a raw tier-2
write with the key absent still stores them absent (`llm_logs_tier2` re-verified).
Also fixed the corpus `userId` (`user-1` -> a real UUID) since the llm_logs schema
validates `userId` as a UUID and silently dropped the write otherwise. Added a
shared `tests/common` helper (real-Db-with-llm-logs setup + normalized dump) for
the remaining regen steps.

Round-4-remainder unification: integrated the four parallel lanes (W4.4b
file/attachment, W4.5 carina query, W4.7e2 TF-IDF vectorizer, W4.7e3 logLLMCall
call-site closures) onto main. No cross-branch code conflicts this time — the
disjoint-files discipline held completely (conflicts were docs/mod-decls only,
union-resolved; versions auto-merged to one round bump). Verified on the
integrated tree: the full workspace gate (886 tests, clippy -D warnings on
default and native-transport, fmt) and a fifteen-differential sweep against
freshly regenerated v4 oracles — the four units' own proofs (text_detection,
file_attachment, carina_query, carina_memory_extraction, tfidf_vectorizer,
embedding_refit) plus the regenerated orchestrator corpus, the shared-file
cross-checks (answer_confirmation, message_context_leaves, carina_runner,
mail_carina_tools over the now-async RunCarinaQuery seam), and the
e3-touched tier-3s (danger_gatekeeper, primary_stream, image_generation,
avatar_job) — all green.

W4.7e2: ported the BUILTIN TF-IDF/BM25 embedding provider (v4's zero-network
fallback embedder, `plugins/dist/qtap-plugin-builtin-embeddings/`). New
`quilltap-core::tfidf` module: the Porter stemmer + tokenizer (`porter` — a
byte-for-byte transcription of v4's hand-rolled stemmer, NOT a crate, since a
divergent stem shifts every stored vocabulary index; `STOP_WORDS`, `stem`,
`tokenize`, `generate_bigrams`), the BM25-enhanced vectorizer (`vectorizer` —
`fit_corpus`/`transform`/`get_state`/`load_state`/`is_fitted`, the BM25 IDF
`ln((N-df+0.5)/(df+0.5)+1)` and TF saturation, f64 throughout; the fit clock
injected), and the `BuiltinEmbeddingProvider` wrapper. Host glue
`services::builtin_embedding::generate_builtin_embedding` (v4
`generateBuiltinEmbedding`: load the persisted state via
`tfidf_vocabulary.findByProfileId`, transform, route through
`applyEmbeddingProfile`), plus new scoped reads
`embedding_profiles::find_by_id` and `tfidf_vocabulary::find_by_profile_id`.
The `EMBEDDING_REFIT` job handler (`services::embedding_refit_job` — gather
every character's memories + the help docs, `fit_corpus`, persist via
`tfidf_vocabulary.upsertByProfileId`, enqueue `EMBEDDING_REINDEX_ALL`; skip
branches for non-BUILTIN / no-characters / no-memories), registered with the
W4.8 runner via `EmbeddingRefitHandler`;
`queue_service::enqueue_embedding_reindex_all` added. The debounce scheduler is
host-timing (not ported — the only pure gate, BUILTIN-profile, is
`is_builtin_profile`). Two differentials: a tier-1
`tfidf_vectorizer_equivalence` (159 rows — stemmer suffix families, tokenizer,
bigrams, fit→getState + transform, loadState-from-JSON, the two throw messages;
`idf`/vectors compared at 1e-12) and a tier-3 `embedding_refit_tier3_equivalence`
(drives v4's REAL `handleEmbeddingRefit` over a two-DB fixture, diffs
`tfidf_vocabularies` + `background_jobs`, plus a runner-registration E2E).
Documented seam: the IDF's `Math.log` diverges from V8 by <=1 ULP on macOS libm
(and the `libm` crate), so the persisted `idf` JSON is compared numerically at
1e-12 in the tier-3 diff; everything else is byte-exact.

W4.7e3: wired the six `logLLMCall` call-site closures so ported call sites now
write `llm_logs` rows via the W4.7e `services::llm_logging` writer.
`CheapLlmTaskExecutor` gained an optional `CheapLlmLogConfig` (Db + per-service
userId/chatId/messageId + LogContext) and a per-call `task_type` on `execute`;
each successful cheap-LLM provider call writes one row (the log type mapped from
`task_type`), covering compression, answer-confirmation, image scene tasks,
memory extraction, context summary, scene-state, and recap. The gatekeeper's
LLM-classify path writes a `DANGER_CLASSIFICATION` row (`classify_content`
gained a `db` param); the moderation path is not ported (the projected
`ModerationResult` drops the raw per-category `flagged` v4 serializes — a
tracked seam). `generate_image` (4 sites), the avatar/story job handlers (via
the shared `generate_with_reroute`, 4 sites), and the primary stream (on
`chunk.done`, with the request-prefix hashes + finishReason) each write their
rows; `durationMs` emits 0 (the frozen-clock differential expectation; a real
value needs a spine-injected clock — a follow-up). All request-path sites pass
`LogContext::none()`. A new in-process self-test drives a cheap-LLM task through
a real single-writer `Db` (main + llm-logs partitions) and asserts one
correctly-shaped `llm_logs` row (the writer's through-a-real-call-site proof,
in process). The byte-exact per-oracle differential regenerations (compression,
danger_gatekeeper, answer_confirmation, image_generation, avatar/story,
primary_stream, memory_processor, context_summary — each un-mocking
`logLLMCall` + dumping `llm_logs`) are staged follow-ups. No spine files
touched.

W4.5: ported the Carina query engine (`services::carina_query`, v4
`carina.service.ts` `runCarinaQuery`) — the isolated reference-answer engine that
resolves an answerer character and produces a minimal, isolated answer. Composes
the ported subsystems: answerer resolution (all name matches oldest-first, prefer
`canBeCarina`, else the operator/user-controlled/`canBeCarina`-asker gate), the
not-participant-scoped connection-profile chain (answerer default →
`connections.findDefault` [new `connection_profiles::find_default`] → first
web-search-capable via the provider registry → no-profile), the system-prompt
build (identity stack + `## Scenario` + the surface-level asker identity card +
the Commonplace memory-recall block), prior-Carina-exchange replay, Carina's own
5-iteration detect→execute→re-stream tool loop + the forced-text final turn, the
`systemSender:'carina'` post + the live `carinaAnswer` emit, and the
`CARINA_MEMORY_EXTRACTION` enqueue. The Brahma one-shot console is an injected
seam (`RunBrahmaConsole`, default = the `llm-failed` shape; the gate + sentinel-id
post path ARE ported — the console engine is the W4.5b follow-up). Added
`services::carina_memory_extraction` (the SELF-only synthetic-transcript
extraction over the ported `process_turn_for_memory`) and
`queue_service::enqueue_carina_memory_extraction` (deduped by `carinaMessageId`).

W4.5: converted the `RunCarinaQuery` seam to async (RPITIT `-> impl Future +
Send`). The work orders' "frozen" constraint is the seam's behavior + argument
shape, not its sync-ness (an artifact of the canned test impl); every real caller
(the runner, the finalizer, the `ask_carina` dispatch) is already async and simply
awaits, matching how `BuildContextSeams` / `ContextSummarySeams` /
`LanternNotificationSink` went async. `run_carina_markup_query` / `execute_ask_carina`
became generic over the seam (RPITIT is not dyn-compatible); the sync `#[test]`
harnesses that drive the runner gained a current-thread runtime. `carina_runner_tier3`
and `mail_carina_tools` re-verified green against fresh v4 oracles (behavior
identical — oracles NOT regenerated). Verified: `carina_query_tier3_equivalence`
(13 cases driving v4's REAL `runCarinaQuery` — plain / name-collision /
profile-chain / memory-recall / prior-exchange / one tool iteration+threading /
forced-text / whisper vs public / Brahma reachable+unreachable / asker-gate→not-found
/ empty→llm-failed / extraction-enqueue — the system-prompt + recall bytes proven
via the canned stream key; no engine divergence) and
`carina_memory_extraction_tier3_equivalence` (the SELF-only outcome over v4's REAL
`handleCarinaMemoryExtraction`). Spine seam closure (the `ask_carina`
`BuiltInToolRunner` dispatch row + constructing the real `RunCarinaQuery` at the
orchestrator/finalizer composition point + the live `@Name:`/`ask_carina`
spine-corpus cases) is handed to the spine owner (W4.4b/unification) per the round
layout.

W4.4b: ported the chat file/attachment LLM-load subsystem and closed its two
standing seams (`OrchestratorSeams::process_files` and
`MessageContextSeams::load_lantern_images`). New pure leaves under `files::` —
`text_detection` (the full 96-entry ext→MIME table + content sniffing, with its
own tier-1 differential), `image_processing` (the base64-size + provider-limit
resize DECISION logic over an injected `ImageTranscoder` seam — no image codec in
the core; the geometric-scale loop and its quirks reproduced faithfully), and
`attachment_support` (v4's client-safe `PROVIDER_ATTACHMENT_CAPABILITIES` map).
New services — `file_fallback` (`file-attachment-fallback.ts`: the three-tier
image description [persisted-prompt reuse FIRST, then the vision call over the
`CompletionProvider` seam with the uncensored retry, then the `IMAGE_DESCRIPTION`
`logLLMCall` write], text→inline, the keep-vs-drop rule, the prefix markers) and
`chat_files` (the LLM-load half of `chat-files-v2`: `loadChatFilesForLLM` +
`loadMountFileAsAttachment` + `readFileAsBase64` over the injected `FileBytesStore`
byte seam, plus `loadAndProcessFiles` and the Lantern K-loader). The vision call
reuses the completion seam via new `CompletionParams.attachments` +
`CompletionResponse.finish_reason` + a backward-compatible
`canned_completion_key_with_attachments` (byte-identical to the base key when
attachments are empty, so every pre-W4.4b oracle keys unchanged). The K seam went
async (RPITIT + Send). Widened `db::files::FileEntry` with `size` + `description`;
added `find_link_meta_by_linked_to` and `doc_mount_file_links::find_with_content_by_file_id`.
Regenerated `orchestrator_tier3` and re-ran `message_context_leaves` green (the
new seams are inert on the existing corpus — file ids empty, no prior-image
attachments). Deferred (flagged, out of the deliverables checklist): the two
inherited spine handoffs — sourcing `model_supports_native_tools` from
`pricing_fetcher::check_model_supports_tools`, and wiring `ConnApiKeys` into the
danger/cheap/image composition points.

Docs: wrote the two remaining follow-up work orders — W4.7e2 (the BUILTIN
TF-IDF/BM25 vectorizer: Porter stemmer transcription, the BM25 fit/transform
math, loadState over the ported tfidf_vocabulary rows, and the EMBEDDING_REFIT
job handler) and W4.7e3 (the logLLMCall call-site closures: six in-scope sites
mapped with their log types, plus the staged per-oracle regeneration plan with
llm_logs dumped). Updated the W4.4b order (the IMAGE_DESCRIPTION logging seam
note retired — W4.7e landed — and the two inherited spine handoffs recorded)
and the chat-orchestration round table with the Round-4-remainder parallel
layout: W4.4b ∥ W4.5 ∥ W4.7e2 ∥ W4.7e3, contention rules included. No code
changes.

Round-4 unification: integrated the four parallel Round-4 branches (W4.7d,
W4.7e sub-units 1-4, W4.9c, W4.6c) onto main alongside the already-landed
W4.7f. One real cross-branch conflict fixed: the W4.9c handlers were written
against the pre-W4.7f `GeneratedImageData` (`data: String`); adapted both
handlers to the widened `Option<String>` + `url` shape with v4's exact falsy
semantics (`rawData = imageData.data || imageData.b64Json; if (!rawData)` —
missing AND empty-string payloads both no-op) and updated the two canned-image
test constructions. One clippy doc-comment fix (a doc_fm header line read as a
markdown list). Verified on the integrated tree: full workspace tests (619
core + harness self-tests), clippy `-D warnings` (default and
`native-transport`), fmt, and all eleven Round-4 differentials re-run green
against freshly regenerated v4 oracles (api_keys, llm_errors, google-wire,
pricing_fetcher, request_prefix_hashes, embedding_wire, avatar_job,
story_background_job, doc_fm/doc_blob/doc_ui with the Librarian announcements
live) plus build_context_tier3 confirming the harness `float_roundtrip`
enablement is inert on existing normalizations.

Phase 3 — W4.6c (the remaining Librarian doc-edit announcements, the Round-3
Group-6 leftover): the file-management, blob, and document-UI doc-edit handlers
now emit their Librarian announcements — move, copy, delete, folder-created,
folder-deleted, open, and blob-write (previously only the doc-save
`change:{diff}` write announcement fired, from G6). Generalized the shared
`DocEditToolResult.pending_librarian_announcement` field from
`Option<LibrarianWriteAnnouncement>` to an `Option<PendingLibrarianAnnouncement>`
enum (one variant per announcement kind, each carrying the frozen W4.6b writer's
argument struct); the field stays `#[serde(skip)]` so the ~23-handler serialized
result shape is byte-unchanged. Each database-store handler branch builds its
announcement inside the synchronous `Db::write` closure (it needs the RW
connections for `uriForResolvedPath` / `resolveActorOrigin` /
`documentHiddenFromCharacters`) and the executor spine dispatches by kind to the
matching async `postLibrarian*` poster after the closure returns (the G6 /
wardrobe-drain `pending*` precedent; best-effort, a failed post never fails the
tool). `doc_open_document` ports v4's bespoke open-origin resolution
(`characters.findById` name lookup → `opened-by-character` else `opened-by-user`,
NOT the shared `resolveActorOrigin`). Added sync `post_librarian_*_announcement_conn`
siblings for the seven writers that lacked one + a `post_pending_librarian_announcement_conn`
dispatcher so the direct-drive differentials post over the held RW `main`
connection, and a synchronous `document_hidden_from_characters` handler helper.
Regenerated `doc_fm` / `doc_blob` / `doc_ui` with the announcement writers LIVE
(un-mocked) and a MAIN-db `chat_messages` dump added to each (ordered by
`content`, a remap-invariant key), diffing the Librarian rows byte-for-byte
(7 file-management rows, 3 blob rows, 2 open rows). The open announcement is an
actual `type:'message'` event, so it bumps the chat's `updatedAt` on both sides
(the doc-ui "updatedAt never bumped by open/close" pin is retired accordingly).
`doc_text` + `tool_dispatch` re-verified green (the enum generalization is inert
for the write kind and for the non-announcing read handlers). The
filesystem-mount announcement sites stay behind the existing `FsSeam` (out of
scope); `syncChatDocuments*` stays the corpus-verified no-op seam.

W4.9c: ported the avatar + story-background background-job handlers
(`CHARACTER_AVATAR_GENERATION` / `STORY_BACKGROUND_GENERATION`), removing both
job types from the runner's loud fallback. New: the two scene cheap-LLM tasks
(`deriveSceneContext`, `craftStoryBackgroundPrompt` — the GROK 1000-char length
guidance, prompts byte-exact); the aesthetics module (`resolveAesthetic` tiered
project-official → Quilltap General, `resolveDepictionGuidelines` — the Ariel
Clause, `getProjectOfficialMountPointId`); the avatar prompt builder
(`buildCharacterAvatarPrompt` with the reworked bare-top collarbone-crop branch);
the two storage bridges (`writeCharacterAvatarToVault` → the character vault
`images/history/`, `writeLanternBackgroundToMountStore` → the Lantern Backgrounds
store `generated/`); the `enqueueStoryBackgroundGeneration` queue op +
`resolveImageProfileForChat` + the `queueStoryBackgroundIfEnabled` gate (the
TITLE_UPDATE handler wiring point is documented, not yet wired). Added a
`describeOutfit` omit-aware variant to the wardrobe leaf, and the
`characterAvatars` / `storyBackgroundImageId` / `lastBackgroundGeneratedAt`
`ChatUpdate` setters (no `updatedAt` bump). Aesthetics differ by handler: avatars
use aurora only (the Ariel Clause deliberately does not apply); story backgrounds
use lantern + aurora + the Ariel Clause. Both handlers reuse the W4.9a image
subsystem (image/completion/moderation/transcoder seams, the Concierge pre-scan +
post-hoc moderation reroute, `resolveOrientation`) and the W4.8 job runner. Both
verified by jest real-DB tier-3 differentials driving v4's REAL handlers.
`logLLMCall` stays a documented deferral (the generate_image precedent); the
project-store `fileStorageManager.uploadFile` branch is an injected host FsSeam.

Phase 3 — Wave 4 (W4.7e, pricing / capability / logging / embeddings): ported
four of the five W4.7e sub-units, each with a green differential against v4's
real code.

- The LLM logging service (`services::llm_logging`, v4 `llm-logging.service.ts`)
  closes the standing `logLLMCall` deferral: `summarize_request`/`_response`
  (full content, UTF-16 `contentLength`, `hasAttachments`, `toolCalls` mapped),
  `is_logging_enabled` (logs by default — missing settings and read errors both →
  enabled), the row writer over the ported `llm_logs.create` (usage/cacheUsage/
  requestHashes gated, `rawProviderUsage` null-collapsed), `map_task_type_to_log_type`
  (verbatim incl. the `SUMMARIZATION` default), the 19 `LLMLogType` constants
  (`TOOL_CONTINUATION` has no emitter), and an explicit `LogContext`
  autonomous-run-id (no thread-locals — v4's AsyncLocalStorage becomes a param).
- The cache-prefix hashes (`cache_prefix_hashes`, v4 `cache-prefix-hashes.ts`):
  per-tier SHA-256 (first 16 hex) of the cacheable request regions. Reproduces
  the sorted-key `stableStringify` (distinct from every insertion-order serializer
  in the port) and the history-tail `undefined`-renders-literally quirk. Tier-1
  differential (`request_prefix_hashes_equivalence`, 17 rows).
- The pricing fetcher + cost estimation + capability check
  (`services::pricing_fetcher`, v4 `pricing-fetcher.ts` + `cost-estimation.service.ts`
  + `checkModelSupportsTools`): sans-IO (the fetch is an injected `PricingFetch`
  seam, `now_ms` injected), the two OpenRouter response casings ported as separate
  parsers, JS `parseFloat` string-price semantics (garbage → NaN), the 24 h TTL +
  5 min negative cache, slug exact-then-fuzzy match, `findCheapestAvailableModel`
  filters, and the `estimateMessageCost` cascade with all source tags. Closes the
  finalizer cost-estimation seam; the `LEGACY_FALLBACK_PRICING` rows are a
  generated Rust static. Tier-1 differential (`pricing_fetcher_equivalence`,
  6 scenarios driving v4's real async exports with fetch/SDK/repo mocked).
- The embedding wire (`model::embedding_wire`, the plugin embedding providers):
  sans-IO per-provider request builders + response parsers — OpenAI
  (`{model, input, dimensions?}`), Ollama (empty-input guard, `/api/embed` with
  the `/api/show`-derived `num_ctx`, the 404 legacy fallback, the finite-vector
  guard), and OpenRouter (the SDK request body + the base64-Float32 decode). Tier-1
  differential (`embedding_wire_equivalence`, 12 rows). `applyEmbeddingProfile`
  was already ported (`embedding_vector`).

Enabled the `float_roundtrip` serde_json feature in the harness so an oracle's
exact-float text (e.g. a price `0.09999999999999999`) parses correctly-rounded,
matching the core's own f64 (the default fast parser is 1-ULP lossy).

Tracked follow-ups (explicit, per the W4.7e work order's degradation plan): the
`logLLMCall` writer's through-a-real-call-site row diff (regenerate the smallest
cheap-LLM oracle with logging un-mocked) + the call-site closures (`cheap_llm_exec`,
`primary_stream`, gatekeeper, answer confirmation, image generation) and their
oracle regenerations; and sub-unit 5, the BUILTIN TF-IDF/BM25 vectorizer, split
off as W4.7e2 (it has no dependency on sub-units 1–4). The `model_supports_native_tools`
field removal is handed to Round-4's spine owner (W4.4b) per the work order.

Phase 3 — Wave 4 (W4.7d): transport, the LLM error taxonomy, and the `api_keys`
table (the last unported repo). Ported:

- `db::api_keys` — the plaintext `api_keys` table (hosted inside v4's
  ConnectionProfilesRepository). `create`/`update`/`delete`/`recordUsage` +
  `findById`(unscoped)/`findByIdAndUserId`/`getApiKeysByUserId` (the per-row
  safeParse DROP). Tier-2 differential `api_keys_tier2_equivalence` (minted-values
  remap; proves the recordUsage lastUsed set + the malformed-row drop).
- `services::api_key_service` — `get_api_key_for_connection_profile` /
  `get_api_key_for_cheap_llm_selection` + the user-scoped wrappers +
  `find_active_api_key_for_provider` (the web-search/moderation provider scan).
  Closed the `ApiKeyResolver` seam with a real DB-backed resolver
  (`ConnApiKeys`); spine wiring at the composition points is handed to W4.4b.
- `services::llm_errors` — the 8-class error taxonomy + `handleProviderError`
  (precedence-ordered normalizer) + `getUserFriendlyError`. Tier-1
  `llm_errors_equivalence` (54 rows, incl. precedence collisions + predicate
  regression rows).
- `model::response_parse` — non-streaming response parsers for all 5 wire
  families (chat-completions flavors, responses-API, anthropic, google, ollama)
  → LLMResponse. `model::provider_models_api` — validate/models endpoints + list
  parsers. Unit-tested; the recorded-payload differential is a tracked follow-up.
- `model::transport` — the `ProviderTransport` IO boundary (trait + policy +
  per-provider header builder, all IO-free) with a feature-gated
  (`native-transport`) reqwest impl. `model::completion_provider` — the production
  CompletionProvider composition (build → transport → parse → CompletionResponse).
- Closed the W4.7c Google `config → wire` framing deferral: `build_request` now
  emits the genai-SDK wire body for GOOGLE (generationConfig split,
  `{name,args}`→`{args,name}`, systemInstruction wrapper). Byte-verified against
  the recorded wire (`request_builder_google_wire_equivalence`, 5 cases).

W4.7f: image wire dialects + OpenAI moderation + Serper web search. Ported the
five sans-IO image-generation dialects (`model::image_dialects` —
`build_image_request` + `parse_image_response` for OPENAI, GOOGLE Imagen +
Gemini, GROK, OPENROUTER, Z-AI), with every rejection path normalized to the
exact error strings v4 surfaces and the three refusal-keyword gaps (Gemini
"No images returned", OpenRouter "Model declined", z-ai's absent moderation
handling) carried faithfully. Added `RealImageProvider` composing build + a new
injected `model::wire::WireTransport` seam + parse. Transcribed the real
per-provider orientation/constraint declarations into `image_gen_data`
(OPENAI/GOOGLE/OPENROUTER per-model, GROK/Z-AI provider-level). Ported the
OpenAI moderation wire (`dangerous_content::moderation_wire` +
`RealModerationProvider`) and the Serper web-search wire (`tools::web_search` —
`build_serper_request` / `map_serper_results` / the plugin + fallback error sets
/ `RealWebSearchProvider`), closing the W4.2 and W4.1d5 provider seams (the
api-key lookups stay behind the existing seams pending W4.7d's `db::api_keys`).
`GeneratedImageData` now carries `url` + an optional `data` (v4's
`GeneratedImage`, for z-ai's dual b64+URL happy path). Three new tier-1
differentials against v4's REAL plugins (`image_dialects_equivalence`,
`moderation_wire_equivalence`, `web_search_wire_equivalence`); regenerated
`web_search_tool` (real provider + the env-var fallback path), `danger_gatekeeper`
(real moderation plugin over canned wire, the failure case a canned 500), and
`image_generation` (real dialect over canned wire) tier-3 differentials green.

Docs: Round-4 work orders complete. Wrote the five remaining agent-ready work
orders from fresh v4 surveys at `6b6e39ad`: W4.7d (transport, errors, the
`api_keys` table — the last unported repo, a hand-rolled plaintext collection
inside v4's ConnectionProfilesRepository), W4.7e (pricing fetcher, model
capability, logLLMCall, embedding wire + the BUILTIN TF-IDF vectorizer — the
decomposition's "builtin already ported" claim corrected: only the storage repo
is), W4.7f (the FIVE image wire dialects — z-ai was omitted from the plan —
plus moderation and web search, with the refusal-keyword gap matrix documented
as faithful), W4.9c (the avatar + story-background job handlers, carrying the
`6b6e39ad` bare-top drift), and W4.6c (the remaining Librarian doc-edit
announcements — the Round-3 Group-6 leftover). Round-4 lane layout + contention
notes added to chat-orchestration.md; provider-manifest.md decomposition
corrected. No code changes.

Phase 3 — Round-3 unification (Group 6, the Librarian doc-save `change:{diff}`
announcement coupling — **Round 3 now fully complete**): the five mutating doc-edit
write handlers (`doc_write_file` / `doc_str_replace` / `doc_insert_text` /
`doc_update_frontmatter` / `doc_update_heading`) now emit the Librarian doc-save
announcement (v4 commit `8617ce7a`) — a `change:{created,body}` payload for a fresh
file, a `change:{edited,diff}` unified diff for an edit (via the W4.d1
`generate_unified_diff`). Ported `resolveActorOrigin`; added a
`pending_librarian_announcement` field to the shared `DocEditToolResult` (never
serialized — v4 puts `change` only in the announcement call, not the tool result)
so a handler can build the announcement inside the synchronous `Db::write` closure
and the async caller (the executor spine) posts it via the already-ported
`post_librarian_write_announcement` after the closure returns (the wardrobe-drain
`pending*` precedent). A failed announcement never fails the tool (best-effort, as
v4). Added the synchronous `post_librarian_write_announcement_conn` (posts over an
already-held RW `main` connection) so the direct-drive differential can post it.
Regenerated `doc_text_equivalence` with the write announcement LIVE on the v4 side
(un-mocked `postLibrarianWriteAnnouncement` + `contentHiddenFromCharacters`), the
fixture's existing chat + participant now targeted, and a third dumped table — the
MAIN-db `chat_messages` (ordered by `content`, a remap-invariant key) — diffing the
10 Librarian rows (8 edited-by-character + 2 created-by-character) byte-for-byte
(persona content + opaque content + `systemSender:'librarian'` + per-kind
`systemKind` + null targeting). `doc_fm` / `doc_ui` / `doc_blob` / `doc_enum` /
`tool_dispatch` re-verified green (the additive field is `None` for every non-write
handler). The file-management / blob / open announcements (move / copy / delete /
folder-created / folder-deleted / open / blob-write) remain separate seams the port
still omits — out of Group 6 scope.

Phase 3 — Round-3 unification (Group 7, context-summary vault-mirror + relevant-
conversations-refresh LIVE): `RealContextSummarySeams::mirror_summary_to_vaults` and
`refresh_relevant_conversations` (previously no-ops) now run live — the fold mirrors
the fresh summary into every participant character's vault
(`writeConversationSummaryToVaults`) and then re-runs the relevant-past-conversations
search against it (`refreshRelevantConversationsOnFold`), in that order (the refresh
must read the fresh corpus). The seam trait's two methods now take the built inputs,
and `RealContextSummarySeams` is generic over an embedding provider (the refresh
embeds the query). Extended `context_summary_service_tier3` to a two-DB fixture
(main + mount-index with one provisioned vault + a pre-seeded prior summary whose
chunk carries a canned unit embedding) and regenerated the differential un-mocking
the mirror/refresh one-for-one: the mirror's write is proven by the
`doc_mount_file_links` path set (`Conversation Summaries/Old Title A.md` appears on
both sides), the refresh's `relevant-conversations` whisper by the `chat_messages`
dump. `vault_summary_mirror_tier2` (which separately proves the mirror byte-exact)
and `orchestrator_tier3` (whose summary check keeps `NoopSeams`) re-verified green.

Phase 3 — Round-3 unification (Group 8, cheap-LLM-selection spine threading): the
`processMessage` spine now resolves a real `CheapLlmSelection` at the composition
point (v4 `getCheapLLMProvider` over the user's connection profiles + the chat
settings' `cheapLLMSettings`, registry-cheapest seam injected `None`) and threads it
into `buildContext` (activating the proactive memory recap + the keyword-distillation
feeders, plus the cached-compression window) and the finalizer's async-compression
trigger — previously hardcoded `None`, which left those feeders inert in
`process_message`. Regenerated `orchestrator_tier3` dropping the `generateMemoryRecap`
+ `extractMemorySearchKeywords` mocks one-for-one: v4's real recap produces empty
content (no memories/vault summaries seeded), and the distill feeder now fires 61
live cheap-LLM calls across the 22 cases — each replayed byte-for-byte by the Rust
distill (proving the spine-resolved selection matches v4's). The empty `memories`
table yields no search results either way, so the stream canned keys do not cascade.
`regenerate_swipe_tier3` re-verified green (its BuildContextArgs takes `None`,
behavior-preserving).

Phase 3 — Round-3 unification (Group 5, commonplace-builder dedup): removed the
private `CommonplaceParts` + `build_commonplace_persona_whisper` /
`build_commonplace_llm_context` copies from `build_context.rs` and reused the
canonical `commonplace_notifications` versions (the per-turn consolidated whisper
leaves `relevant_conversations` empty, so the output is byte-identical). No behavior
change; `build_context_tier3` re-verified green.

Phase 3 — Round-3 unification (Group 4, Lantern sink rewire): deleted the truncated
`lantern_character_image_notification` placeholder in `generate_image` and wired the
W4.9a Lantern sink to the canonical W4.6b writer. `LanternNotificationSink` is now
async with a `RealLanternNotification` impl delegating to
`lantern_notifications::post_lantern_image_notification` (which composes the full
byte-exact `build_content`, incl. the "attached here" tail the placeholder dropped).
Regenerated `image_generation_tier3` with the Lantern writer un-mocked and the
persisted `character-image` notification content diffed byte-exact.

Phase 3 — Round-3 unification (Group 3, end-of-turn wardrobe drain): the
`processMessage` spine now threads ONE shared `pendingWardrobeAnnouncements` set
through every per-turn tool context (native loop + text passes) and drains it at
turn close (before finalize, v4 orchestrator.service.ts:1406) via
`aurora_notifications::flush_pending_wardrobe_announcements`, which enqueues one
`WARDROBE_OUTFIT_ANNOUNCEMENT` job per affected character. Added
`WardrobeOutfitAnnouncementHandler` (a `JobHandler` wrapping
`handle_wardrobe_outfit_announcement`) for the host/runner to register. The
pending-set recording remains proven by `wardrobe_tools_equivalence`; the flush /
enqueue / handler are individually ported (W4.1d2 / W4.8 / W4.1d2). Residual: a
Db-based end-to-end drain differential (the wardrobe_tools harness uses raw
writers, no Db).

Phase 3 — Round-3 unification (Group 2, W4.6b post-office writers): wired the
personified-system whisper POSTs live. `BuildContextSeams` is now async (RPITIT,
matching `ContextSummarySeams`) with a `RealBuildContextSeams` production impl that
delegates each POST to its W4.6b writer — core-whisper + commonplace (each with the
v4 stale-whisper sweep), host timestamp + off-scene (the off-scene scan now returns
the newcomer cards so the writer builds the announcement + stamps
`introducedCharacterIds`), and Suparṇā mail (built from the unalerted letters,
targeted at the responding participant). The commonplace `posted` still gates the
scene-cache / recall-history persists. The Prospero cadence block (public context
announcement + group-context whisper) is wired directly into the `processMessage`
spine (dropped the `post_prospero_context` seam). Regenerated `build_context_tier3`
(un-mocked writers, BuiltContext diff green) and `orchestrator_tier3` (whisper rows
— commonplace / host / prospero group-context — now appear in the diffed
chat_messages dump, matching v4's real writers). Residual: the Prospero public
project/general announcement needs a provisioned General store in the fixture (the
group-context cadence whisper is proven).

Phase 3 — Round-3 unification (Group 1, W4.7c spine wiring): wired the provider
tool reshape + native detector + provider text-markers strategy live into the
`processMessage` spine. `tool_build::build_tools` now applies
`format_tools_for_provider` as its final step, so the orchestrator sends
provider-shaped tools at the wire (Anthropic `input_schema`, etc.); OPENAI passes
through byte-identically so `tool_build_equivalence` stays green. The orchestrator
constructs `RegistryToolCallDetector::built_in()` and gates the provider-text pass
on `provider_has_text_markers` internally (dropped the `tool_detector` /
`provider_text_strategy` seam fields from `OrchestratorDeps` and the
`NoToolCallDetector` call site). Regenerated `orchestrator_tier3` with the real
provider registry initialized on the v4 oracle side so both reshape identically;
the tools-at-wire assertion now compares the reshaped slate.

Phase 3 — wave 4 (W4.4a4): the Courier transport + the compression-cache spine
plumbing. Ported v4's `courier-transport.service.ts` (the manual / clipboard
dispatch) as `services::courier_transport` + `courier::render_markdown`: the two
Markdown renderers (`renderCourierRequestAsMarkdown` / `renderCourierDeltaAsMarkdown`
— byte-exact, incl. the `\n{3,}`→`\n\n` collapse and `trimEnd()+'\n'`),
`buildCourierDeltaEvents` (the per-character checkpoint scan with the strict
`createdAt <= resolvedAt` skip, targeted-whisper filtering, the exact Staff speaker
labels, and file-attachment loading), `dispatchCourierTransport` (the placeholder
ASSISTANT message with the rendered bundle in `pendingExternalPrompt` + the delta
fallback + the union attachments, the chat pause, and the `pendingExternalTurn` +
`done{pendingExternalTurn:true}` SSE frames), and the paste/cancel resolvers
(`resolve_external_turn` / `cancel_external_turn` — public service functions; the
HTTP route is Phase-4). Closed the orchestrator courier gate (was erroring): after
`build_message_context` + the `preparing` status, a courier-transport turn now
dispatches (tool build skipped, no tool instructions — matching v4). Added the
`pendingExternalTurn` frame + `DonePayload.pendingExternalTurn` to `chat_events`
and the `ChatUpdate.courier_checkpoints` write setter. Compression-cache plumbing:
the finalizer's real `AsyncCompressionTrigger` (now async, over
`compression_cache::trigger_async_compression`) computing + persisting the cache
when the gate fires, and the `build_context` cached-compression window
(`cached_compression_result` / `cached_compression_message_count` — phase-1 uses a
warm cache verbatim, no sync compression call; the dynamic effective-window sizing).
The orchestrator reads `get_cached_compression` before buildContext (inert until the
spine threads a `cheap_llm_selection`, the tracked deferral). New differential
`courier_transport_tier3_equivalence` (drives v4's REAL `dispatchCourierTransport`
over a four-case corpus — first send / delta with whisper-filter + boundary + staff
label / forced-full / attachment union — diffing the result + SSE trace + the
persisted placeholder bytes + `isPaused`). Regenerated + green:
`orchestrator_tier3` (added a `courier_send` spine case), `message_finalizer_tier3`
(the trigger adaptation), `build_context_tier3` (a warm-cache case proving the
cached window), `compression_cache_tier3`. Marshaling: `courierCheckpoints` +
`pendingExternal*` were already ported (no drift); added the
`ChatUpdate.courier_checkpoints` setter. Tracked deferral: the paste/cancel route
handlers aren't exported (Phase-4 HTTP transport); their constituent repo ops are
tier-2/tier-3-proven and the ported service functions are unit-tested.

Phase 3 — wave 4 (W4.6b): the post-office / personified whisper writers. Ported
every v4 `lib/services/<persona>-notifications/writer.ts` into new
`services::<persona>_notifications` modules — Host, Prospero, Librarian,
Concierge, Suparṇā, Aurora (core-whisper post + the outfit whispers + the
`WARDROBE_OUTFIT_ANNOUNCEMENT` drain), Commonplace (persona/LLM whisper builders +
`refreshRelevantConversationsOnFold`), and the Lantern image notification — each
posting one `chat_messages` row through the ported `add_message` with the exact
`systemSender` / `systemKind` / targeting / `opaqueContent` / `hostEvent` /
`summaryAnchor` tuple, best-effort/error-swallowing. The steampunk/Wodehouse voice
strings are byte-exact. Also ported the conversation-summary vault bridge
(`writeConversationSummaryToVaults` + `removeConversationSummariesFromVaults`, over
the ported document store + frontmatter emitter) and composed the `chats.delete`
participant-vault summary sweep (`delete_conversation_with_vault_sweep`) — closing
the LAST Phase-2 deferral — plus the cost/system-event writer (`createSystemEvent`
+ the memory/title/context-summary wrappers, posting a SYSTEM row + the ported
token-aggregate bump). Non-spine seams closed live: the Concierge announcer seams
in `dangerous_content` (`RealDangerAnnouncer` / `RealConciergeAnnouncer` — the W4.2
`postConcierge{Danger,Manual}Announcement` deferrals), and the context-summary
Librarian re-post + cost events (`RealContextSummarySeams`); the announcer/seam
traits went async (RPITIT `-> impl Future + Send`, no boxing). Verified: six
tier-1 pure-builder differentials (host/librarian/prospero/commonplace/aurora +
concierge-lantern-suparna, byte-exact vs v4's real exports); a combined
`post_office_writers_tier3_equivalence` (drives v4's real post functions over a
two-DB fixture, diffs `chat_messages` + the cost `chats` aggregate, one case per
row-shape/systemKind); a `vault_summary_mirror_tier2_equivalence` (mirror +
rename-in-place + `syncVaults` skip + the delete sweep, five mount-index tables in
the shared-cross-db id-map remap form); and the regenerated
`context_summary_service_tier3` + `danger_gatekeeper_tier3` + the manual-flip case
(the writers now post live on both sides). Handoffs (spine-owned, deferred): wiring
the `BuildContextSeams` post methods (`post_core_whisper` /
`post_commonplace_whisper` / `post_host_*` / `post_suparna_mail`), the
`OrchestratorSeams::post_prospero_context`, and the end-of-turn wardrobe drain into
the orchestrator/build_context spine; the context-summary vault-mirror +
relevant-conversations-refresh seams (need vault fixtures + embedding); rewiring the
image subsystem's Lantern sink to the full byte-exact writer; and the Librarian
save-announcement `change:{kind:'edited',diff}` coupling in the doc-edit handlers.

Phase 3 — wave 4 (W4.7c, part 2): the request builders + the four RequestTransform
hooks. Ported the sans-IO per-provider request-envelope builders into
`quilltap-core::model::request_builder` (build a request VALUE — method/url/headers/
body — no HTTP; the transport is W4.7d). Dispatched by the W4.7a manifest
(baseUrl+endpoint → url, auth → headers). Every SDK/raw-fetch sends
`JSON.stringify(body)` verbatim, so bodies are built key-order-exact (preserve_order,
integer-valued numbers bare). The four hooks: anthropic (mid-history cache
breakpoint + tool-result batching + adaptive-thinking/sampling-param-rejection for
Sonnet 5 / Opus 4.7+ / Fable / Mythos — the rejected-model list ported as a compiled
constant, not lifted to the manifest [noted]), openai (previous_response_id chaining
— the fallback-to-full-input is a transport concern), google (the recursive
JSON-Schema sanitizer + the thoughtSignature round-trip), deepseek (reasoning_content
echo + thinking-incompatible-param strip). Chat-completions family (deepseek, z-ai
[+ web search + reasoning-effort default], openrouter [raw-fetch tools path], ollama,
openai-compatible base) and responses-API family (openai, grok) are byte-exact
against the wire. Google's genai-SDK config→generationConfig wire framing is deferred
to the transport; the google request LOGIC (sanitizer + contents/thoughtSignature)
is verified against v4's real plugin. Verified by two new differentials:
`request_builder_equivalence` (31 rows byte-exact vs v4's real plugin requests,
captured by intercepting fetch in `record-request-envelopes.mjs`) and
`request_builder_google_equivalence` (5 rows: contents/systemInstruction/
shouldDisableTools + the sanitizer via the wire functionDeclarations). With this,
W4.7c is fully DONE; the remaining provider-layer units are W4.7d/e/f.

Phase 3 — wave 4 (W4.7c, part 1): the provider tool-wire. Ported v4's
`packages/plugin-utils/src/tools/*` + the per-plugin tool glue into
`quilltap-core::model::tool_wire` — the tool-format reshape (`formatTools`:
Anthropic `input_schema` / Google `parameters` / OpenAI passthrough), the native
tool-call parse (`parseOpenAIToolCalls` / `parseAnthropicToolCalls` /
`parseGoogleToolCalls` + the Google `functionCalls` fast path), and the
spontaneous XML text-marker detect/parse/strip (the full `hasAnyXMLToolMarkers` /
`parseAllXMLAsToolCalls` / `stripAllXMLToolMarkers` suite + Google's tool_use-only
variant), all dispatched by the manifest `toolFormat` (the registry replaces
`getProvider`). The one backreference regex (`<key>value</key>`) is hand-rolled;
the other regexes reproduce JS ASCII `\w`/`\s` semantics. Closes three live seams:
the native-tool-loop `ToolCallDetector` (new `RegistryToolCallDetector`), the
text-tool-loop provider-text-markers strategy (new `ProviderTextMarkersStrategy`),
and the W4.1g `formatTools` provider reshape
(`tool_build::format_tools_for_provider`, available + tested; wiring into
`build_tools` is a documented spine handoff). Verified by `tool_wire_equivalence`
(231 rows byte-exact against v4's real plugin methods over the real b.3 catalog +
recorded rawResponses), and by regenerating `native_tool_loop_tier3_equivalence`
(real Anthropic detector over real anthropic rawResponses) and
`text_tool_loop_tier3_equivalence` (real DeepSeek provider strategy) — both green.
Deferred to W4.7c part 2: the per-provider request-envelope builders + the four
`RequestTransform` hooks.

Drift check: v4 `8617ce7a..6b6e39ad` audited — no ported unit is stale. The
commit (image-description reuse off the reply hot path + the bare-topped
avatar crop) touches only pending surfaces. Docs only: the W4.4b
file/attachment work order is retrofitted to the reworked
`file-attachment-fallback.ts` (the persisted-text reuse tiers before any
vision call, the hardened/logged/timeout-bounded vision fallback, new corpus
cases), a W4.9c drift note records the avatar-prompt bare-top branch (the
ported `describeOutfit` leaf is unchanged), and the `docs/v4/` CHANGELOG
mirror is refreshed. New oracle baseline for future orders: `6b6e39ad`.

Phase 3 — wave 4 (W4.3): the answer-confirmation service. Ported v4's
`answer-confirmation.service.ts` (the pre-landing Salon consistency check +
re-affirmation): the gate/leaf functions (`isAnswerConfirmationActive`,
`hasCheckableInputs`, `findLatestCommonplaceWhisper`, `isUserDrivenTurn`,
`gatherConfirmationInputs` with the 24 K oldest-first reference truncation) and
`runAnswerConfirmation` (the cheap-LLM consistency check, the fenced-JSON verdict
parser, the uncensored escalation of the check's cheap selection on a dangerous
chat, and the re-affirmation pass on the character's own model — consistent →
confirmed; stood by → not-confirmed + notes; rewrote → confirmed + revised +
original stashed; empty rewrite / parse failure / error → could-not-verify). The
byte-exact prompts live in a generated `prompt_text` submodule. The finalizer
seam (`NoAnswerConfirmation`) is closed with the real runner at the composition
point: the finalizer now reads the prior messages, finds the Commonplace whisper,
assembles the reference, emits the `confirming` / `affirming` status frames, and
applies the outcome (the rewrite's tool-anchor drop + reasoning collapse). The
finalizer's `isAnswerConfirmationActive` / `isUserDrivenTurn` gate leaves were
hoisted into the service (single source of truth). Verified by
`answer_confirmation_tier3_equivalence` — a jest real-DB oracle driving v4's real
`finalizeMessageResponse` with the feature ON over a 14-case corpus (the gate
matrix, user-driven skip, no-checkable-inputs skip, whisper-only /
whisper-plus-tool references, the 24 K truncation, every outcome band, and the
dangerous-chat escalation whose recorded canned key proves the cheap-profile
switch to the uncensored profile), completions pinned by oracle-recorded canned
keys; results + the ordered event trace + `chats` / `chat_messages` diffed. The
timeout wrappers are host-side (no tokio timers in the core; only the
failure→could-not-verify mapping is ported). Re-verified
`message_finalizer_tier3` + `orchestrator_tier3` green against regenerated
oracles. Full workspace `cargo test` / `clippy -D warnings` / `fmt --check`
green.

Phase 3 — wave 4 (W4.9a): the image-generation subsystem (`generate_image`).
Ported v4's `executeImageGenerationTool` end to end and dispatched it, closing
the long-deferred image handler. New `model::image` boundary (the tier-3 seam at
v4's `provider.generateImage(params, apiKey)`): the `ImageProvider` trait +
`CannedImageProvider` keyed by the exact merged request (the key proves
`mergeParameters` + `applyOrientation`), plus a separate `ImageTranscoder` seam
for the WebP transcode (no image-codec crate in the core — the `doc_blob`
precedent; `PassthroughTranscoder` is the default). Three cheap-LLM tasks
(`services::image_scene_tasks` — `craftImagePrompt` / `resolveAppearance` /
`sanitizeAppearance`, prompts byte-exact in a generated `prompt_text` submodule)
over the ported `CheapLlmTaskExecutor`. Appearance resolution
(`services::appearance_resolution` — the sceneState/trivial-skip/cheap-LLM
resolution + the five-step Concierge sanitize gate IN ORDER). The handler spine
(`tools::generate_image`): input validation, profile load/validate (API key via
the `ApiKeyResolver` seam), the Concierge integration composing W4.2 (prompt
classification when `scanImagePrompts`, expanded-prompt classification when
`scanImageGeneration`, the AUTO_ROUTE reroute, and the post-hoc reroute on a
provider moderation error), `resolveOrientation` mutating the merged params, and
`saveGeneratedImage` (base64 decode → WebP transcode seam → SHA-256 → the Lantern
Backgrounds store write under `tool/` via `link_blob_content` → the `files` row
with `source='GENERATED'` / `category='IMAGE'` / generation metadata → tag
inheritance → the Lantern notification, a recorded seam with the byte-exact
string handed to W4.6b). The avatar trigger (`services::avatar_generation` —
`triggerAvatarGenerationIfEnabled`, the `avatarGenerationEnabled` gate + the
autonomous-chat skip + profile resolution + the `CHARACTER_AVATAR_GENERATION`
enqueue in `queue_service`), closing the W4.1d2 wardrobe deferral. `generate_image`
is dispatched through the `BuiltInToolRunner` (removed from the loud-fallback set)
via an erased `ImageGenerationRunner` seam, threading the generated-image paths
into `process_tool_calls` + the finalizer link loop. Verified by the tier-3
differential `image_generation_tier3_equivalence` (jest real-DB oracle driving
v4's REAL `executeImageGenerationTool`, mocking only the image provider [canned
by exact request], the completion boundary [recorded keys prove all three task
prompts + classification], WebP transcode [deterministic pass-through both
sides], and the Lantern notification). Tracked deferrals (host / cross-subsystem
seams): the aesthetic subsystem (`resolveAesthetic` / `resolveDepictionGuidelines`
— v4 error-swallows it, so the port supplies `None` and keeps the swallow shape),
`logLLMCall`, the real WebP encoder, and the personified Lantern writer (W4.6b).
The avatar + story-background JOB HANDLERS are the follow-up W4.9c.

Phase 3 — wave 4 (W4.6a): the buildContext feeder closures. Closed the
READ/COMPUTE half of the `BuildContextSeams` trait in `services::build_context`
— the ten former seams now run real, leaving only the W4.6b whisper-POSTing
methods. New feeder modules: `services::frozen_archive`
(`getOrComputeFrozenArchive` — the effective-weight-ranked top-25, process-cached
per compaction generation, `localeCompare` id sort), `services::memory_recap`
(`generateMemoryRecap` composing the tiered-memory narrative + the vault
conversation-summary recall lists over `search_document_chunks` /
`read_database_document` / `parse_frontmatter`; prompt bodies byte-exact in a
generated `prompt_text` submodule) with the `distill` submodule
(`extractMemorySearchKeywords`), `services::off_scene` (the Host off-scene SCAN +
the content builders + `applyHostTemplates` + `findIntroducedOffSceneCharacterIds`
— the POST stays W4.6b), `services::core_whisper` (Aurora's
`resolveCoreWhisperConfig` + `assembleCorePacket` reading own + group `Core/**.md`
+ the three content builders — the POST stays W4.6b), `services::suparna_mail`
(the mail READ — `collectUnalertedMail` + `markAlerted` +
`buildSuparnaMailLLMContext`), and `services::scene_state_tracking` (the
`updateSceneState` cheap-LLM task + `capClothingSummary`, prompt bodies byte-exact;
the full `handleSceneStateTracking` job wrapper lands with the W4.8 runner
dispatch). Closed with existing code: the tiered mount pool,
`getMemoryRecallSettings`, and the live-wardrobe clothing override (adding the
small `hash_equipped_slots` / `has_equipped_items` /
`decorate_outfit_items_title_only` leaves + a `resolve_equipped_outfit_leaf_values`
variant of the outfit resolver). The scene-cache + recall-history persist writes
(`chats.update({ commonplaceSceneCache })` / `{ commonplaceRecallHistory }`) are
ported directly, gated on the commonplace POST (W4.6b). New reads:
`instance_settings::get_memory_recall_settings`,
`chats_read::find_core_whisper_overrides`,
`characters_read::find_core_whisper_enabled`,
`groups::find_name_and_official_mount_point_id_raw`, and a recursive variant of
`doc_mount_documents::find_many_by_mount_points_in_folder`. Three `ChatUpdate`
setters added (`sceneState` / `commonplaceSceneCache` / `commonplaceRecallHistory`).
Verified: `build_context_tier3_equivalence` runs green with the feeder mocks
dropped one-for-one against the real feeders (memories → frozen archive, vault
summaries → recap, mount pool, core-whisper config); a new
`context_feeders_leaves_equivalence` tier-1 differential proves the pure
builders/formatters/config resolvers byte-exact against v4's real exports;
`knowledge_injector` / `first_message_context` / `orchestrator_tier3` re-verified.
Tracked deferral: the orchestrator spine still passes `cheap_llm_selection: None`
into buildContext (it threads only a `cheap_llm_settings_present` bool), so the
recap/distill feeders are gated OFF there and stay mocked in the orchestrator
oracle — closing that is a spine-owner follow-up (thread a resolved
`CheapLlmSelection`); the scene-state job wrapper is W4.8. Full workspace
`cargo test` / `clippy -D warnings` / `fmt --check` green.

Integration of the five parallel wave-4 units (W4.7a / W4.7b / W4.2u / W4.8 /
W4.9b), each developed and verified in isolation. Two reconciliation touches:
the two independent ports of `doc_mount_file_links.findByIdWithContent` were
merged — the job-runner stale-chat sweep keeps the full-`LinkRow` shape as
`find_link_row_by_id`, and the photo tools keep the content-subset
`find_by_id_with_content` (both v4-faithful; a post-port cleanup may unify them);
and the process-global wake-hook unit test's exact-count assertion was relaxed to
monotonic, since the shared `OnceLock` hook is fired by concurrent enqueues from
sibling tests in the larger integrated suite. Full workspace `cargo test` /
`clippy -D warnings` / `fmt --check` green.

Phase 3 — wave 4 (W4.7a): the provider manifest + registry core. Replaced v4's
npm-plugin provider registry — which does not survive the port (no Node, no
dynamic import, no shipping third-party JS into the Rust core) — with a
declarative-manifest + compiled-discriminator design. New `provider_manifest`
module: serde structs for the manifest schema (deserialization is the schema
validation; a missing field, a bad enum, or a wrong `schemaVersion` each fails
loud with a typed `ManifestError` naming the field), the `StreamDecoder` /
`RequestTransform` closed enums (the values W4.7b/c implement against), the nine
built-in provider manifests generated from v4's registered plugin metadata by a
checked-in generator (`harness/oracle/providers/gen-provider-manifests.mjs`,
transcription not re-derivation — embedded via `include_str!`, parsed once behind
a `LazyLock`), the `Registry` accessors reproducing v4's provider-registry
convenience getters (`get_provider` exact-case lookup — v4 does not resolve
`legacyNames`, they are display metadata; the capability getters with their v4
defaults `charsPerToken` 3.5 / `defaultContextWindow` 8192 / `toolFormat`
"openai"), and `rewrite_localhost_url` (pure — the host gateway resolution
injected). Verified by `provider_registry_equivalence` (a tsx oracle driving v4's
real registry over every provider × getter — 253 rows, incl. absent-field
defaults, legacy-name lookups that must not resolve, and a determinism dump) plus
malformed-manifest fail-loud unit tests.

Also closed the four registry-seam replacements in their leaf consumers. The big
one: `message_formatter::get_provider_name_support` now consults the manifest
registry before the legacy fallback, matching v4's `getProviderNameSupport` — a
real behavior change from the pre-W4.7a empty-registry state (DEEPSEEK / Z_AI /
OPENAI_COMPATIBLE now report message name-field support via the registry, where
the legacy table alone said no); its differential regenerated with the real
registry initialized. `model_context`'s registry-default input and `cheap_model`'s
recommended-list / default input keep their injected parameters (the orchestrator
spine populates them), but their oracles were regenerated with the real registry
so the injected values reflect the real manifest data (e.g. ANTHROPIC default
200000, DEEPSEEK/Z_AI 131072); `tool_build`'s `provider_supports_web_search` stays
a corpus-controlled input in its differential. The pins for all four moved to
"the registry value equals the pinned value," asserted in
`provider_registry_equivalence` so a manifest drift is caught there. Spine-side
seam removals (sourcing these injected inputs from the registry at the
orchestrator composition point) are deferred to the orchestrator-spine owner.
Phase 3 — wave 4 (W4.7b): the five stream decoders. Ported the sans-IO
push-state-machine wire decoders that turn a provider's streamed bytes into the
normalized `StreamChunk` sequence, in a new `model::decoders` module: a shared
spec-faithful SSE frame splitter (`sse`) plus `chat_completions_sse`
(openai-compatible / deepseek / z-ai / openrouter — the tool-call accumulator
keyed by `tool_calls[].index`, reasoning routing, usage in the trailing chunk,
`[DONE]`), `responses_api_sse` (openai / grok — the Responses-API event
taxonomy, cumulative reasoning re-sends, terminal `response.completed`),
`anthropic_sse` (`content_block_start`/`delta`/`stop` state machine,
`input_json_delta` per-index buffering, thinking/signature, usage split across
`message_start`/`message_delta`, mid-stream `error` events),
`google_parts` (genai `generateContentStream` — `data:`-SSE parts iteration,
`thought===true` → reasoning, `thoughtSignature`, functionCall parts), and
`ollama_ndjson` (newline-delimited JSON, whole-object tool_calls normalized to
OpenAI shape, `done:true` terminal). Each also assembles the terminal
`rawResponse` value v4 hands back for tool-call detection. `StreamChunk` was NOT
extended. Each decoder is a `StreamDecoder` (`push` / idempotent `finish`)
correct when fed one byte at a time. Verified by `stream_decoders_equivalence`:
a checked-in fetch-mock recorder drives v4's REAL plugin `streamMessage` parsers
over committed wire transcripts and records the normalized chunk NDJSON; the
Rust decoders replay each transcript at whole-buffer / per-frame /
byte-at-a-time and diff the chunk sequence + rawResponse. Two documented
transport-artifact normalizations: google's SDK-injected `sdkHttpResponse` is
stripped, and ollama's no-cross-read-buffer split-line loss (a faithfully ported
v4 bug) is diffed at line-aligned chunkings only (byte-at-a-time bug-parity is a
Rust-side unit test). Three STOP-rule divergences from the design-doc table,
flagged: the four "chat-completions-sse" providers do not share one
normalization (deepseek/z-ai via the OpenAI SDK vs openrouter's raw-fetch
`streamViaChatCompletions`, distinct rawResponse/reasoning shapes; deepseek and
z-ai further differ on cache source + `rawProviderUsage`), reproduced via an
internal `Flavor` selector over one shared parser; google is `data:`-prefixed
SSE, not JSON-array/newline as the table's caption said; and openrouter's
no-tools OpenResponses SDK path is out of scope (a deferred distinct wire).
Phase 3 — wave 4 (W4.2u): danger spine unification. Wired the real
dangerous-content resolver + router into the `process_message` orchestrator
spine, replacing the injected `NoRouter` / hardcoded `DETECT_ONLY` test stub.
The spine now resolves the effective danger settings via
`resolve_dangerous_content_settings` (the global `dangerousContentSettings`
sub-object + the chat's `conciergeOverride` / `chatType` off-duty /
moderation-exempt collapse), computes `is_chat_active_dangerous`, and
reproduces v4 `resolveMessageDangerState`'s first branch: an actively-dangerous,
non-continue turn with content synthesizes danger flags and — under AUTO_ROUTE
with a non-`isDangerousCompatible` profile — reroutes the primary stream through
an uncensored provider via the real `DangerContentRouter` (constructed with its
`ApiKeyResolver` seam), attaching the flags to the saved user message. The
finalizer's danger-classification enqueue now honors the resolver's OFF
short-circuit (`FinalizerChatSettings.danger_mode_off`); the memory-extraction
and danger-classification enqueues use the original `connectionProfile.id`
(distinct from the rerouted `effectiveProfile.id`, added as
`FinalizeOptions.connection_profile_id`), while the persisted assistant message
and cost tracking stay on the effective profile — matching v4. The
classification branch (cheap-LLM / moderation of the current user message) stays
the gatekeeper seam (behavioral no-op on the diffed trace/tables when
not-dangerous). Added two orchestrator-corpus cases driving v4's real danger
resolution: `danger_off_short_circuit` (off-duty chat → resolved OFF → no
classification enqueue, router never consulted) and `danger_live_reroute`
(permanently-dangerous chat + AUTO_ROUTE + uncensored profile → primary stream
rerouted, proven by a distinct recorded canned stream key). The oracle now runs
v4's real `resolveMessageDangerState` (global mode AUTO_ROUTE, no
`uncensoredTextProfileId` so the empty-response failover stays inert) with a
canned `findApiKeyByIdAndUserId` seam. `orchestrator_tier3_equivalence`,
`message_finalizer_tier3_equivalence`, `primary_stream_tier3_equivalence`,
`danger_resolver_equivalence`, `danger_routing_equivalence`, and
`danger_gatekeeper_tier3_equivalence` all green against regenerated oracles; the
pre-existing orchestrator cases are a behavioral no-op under the real resolver.
Phase 3 — wave 4 (W4.8): the background job runner. Ported v4's forked-child
job processor as an in-process runner over the single-writer runtime. The
fork/IPC/buffered-write-proxy architecture does not port — v5's `Db` already
enforces the single-writer invariant in the type system, so job handlers run
in-process and write through `Db` directly. New `services::job_runner`: the
claim-loop core (`pump_claim` with the reentrancy lock, the `maxConcurrentJobs`
instance-settings read each pump [default 4, clamp 1–32], the claim-until-full
loop over the ported `claim_next_job`, and the next-`scheduledAt` wake-delay
decision returned to the host), dispatch by job type through a `HandlerRegistry`
with a loud fallback for unported/unknown types (v4's failure shape),
completion/failure marking (`markCompleted` now wiring the `merge_result_into_payload`
path — closes Phase-2 deferral #3, forward-only since v4-on-SQLite throws
there), and startup/stuck recovery (`reset_orphaned_jobs` / `tick_stuck_reset`).
All timers are host-driver seams (no timers in the runner core), per the enclave
`step()` philosophy. New `services::job_scheduler` with the pure decision leaves
(`clamp_wake_delay`, `should_run_startup_tick`) + the cadence constants. Closed
the `ensureProcessorRunning` seam: `queue_service` enqueues now fire a
process-global wake hook (`set_wake_hook` / `JobRunner::install_wake_hook`); the
runner's `wake()` signals an immediate pump. Extended `queue_service` with the
read/admin surface (`get_job_status` / `get_queue_stats` /
`get_active_counts_by_type` / `cancel_job` / `get_pending_jobs_for_chat` /
`cleanup_old_jobs` / `cleanup_finished_jobs`), the retention windows, and the
portable scheduler sweep bodies (`run_scheduled_housekeeping` /
`run_scheduled_cleanup`). Ported the stale-chat asset maintenance sweep
(`services::maintenance::collapse_stale_chat_assets`, v4
`collapse-stale-chat-assets.ts`) with the new `chats.getLastPlayedMessageAt`
scoped read, the keep-set avatar-sha resolution, and the four protection
branches (current / current-sha / album-or-vault-link / character-reference);
the storage-bytes delete is a host FsSeam. Verified by a tier-1 differential
(`photos_relative_path_equivalence`) and a tsx real-DB tier-2 differential
(`maintenance_sweep_tier2_equivalence`, driving v4's REAL
`collapseStaleChatAssets` over a two-DB fixture), plus eleven runner self-tests
(concurrency cap, wake-on-enqueue, claim-order, loud fallback, stuck/orphan
reset, drain-on-shutdown, and one end-to-end memory-housekeeping dispatch
enqueue→claim→dispatch→markCompleted-merge); the `memory_watermark_tier3` and
`context_summary_service_tier3` differentials regenerated green with the wake
hook (the DB effect is unchanged).
Phase 3 — wave 4 (W4.9b): the photo trio (`keep_image` / `list_images` /
`attach_image`), the last deferred tool handlers, is ported and dispatched.
New `photos` module: `keep_image_markdown` (the kept-image Markdown builder +
parser — YAML frontmatter, prompt/revised-prompt/scene/attribution sections,
the caption regex, slug/filename, `linkedByRole` back-compat), `photos_paths`
(the `photos/` folder helpers), and `save_image_to_album` (resolve the FileEntry
with the mount-blob fallback, dedup by sha within the mount's `photos/` folder,
build the markdown, hard-link the binary, roll up the link's chunk counts). The
three `tools::photo` handlers compose that over the ported vault reads/search,
wired into `BuiltInToolRunner` (removed from the loud fallback) each inside a
both-connections `Db::write` closure. Image bytes stay behind an injected
`FileBytesStore` seam; the mount invalidation + embedding enqueue are recorded
no-op seams; the chunker is not re-ported (chunkCount pinned / doc_mount_chunks
excluded, the groups/projects precedent). Added photo-facing reads
(`files::find_by_id`/`find_by_sha256`, `doc_mount_file_links::find_by_id_with_content`
+ the chunk-rollup setters). Verified by `photo_tools_tier3_equivalence` (a
jest-real-DB oracle driving v4's REAL handlers over a two-DB fixture with baked
photos — keep fresh/duplicate/malformed-scene with six-table dumps, plain +
semantic + peer-vault + silent-fallback listing, attach by link-id/file-id +
cross-vault + missing) and one new `list_images` row in `tool_dispatch`; the
five `doc_*` handler differentials re-verified green.

Phase 3 — wave 4 (W4.d1): drift re-port of the unified diff. v4 commit
`8617ce7a` replaced the greedy look-ahead line diff with a real, minimal,
git-style unified diff, so the ported `doc_edit::unified_diff` no longer
matched. Ported the new v4 `lib/doc-edit/line-diff.ts` as a new leaf
`doc_edit::line_diff` (`diff_lines` — a Myers O(ND) shortest-edit-script diff
over line arrays, a byte-faithful transcription including the exact tie-break
so the recovered op order matches under ties — plus `changed_block_indices`),
and rewrote `doc_edit::unified_diff` on top of it: git-style hunks with three
lines of context, maximal changed runs coalesced when their expanded ranges
touch, correct `@@ -start,count +start,count @@` ranges (count 0 →
`start-1,0`), empty content treated as zero lines, and a whole-file
replacement-hunk fallback past 10,000 combined lines. Deleted the old greedy
walker. Regenerated and extended `doc_edit_leaves_equivalence` (coalesce vs
split hunks, context truncation at file start/end, the formatRange shapes
incl. the delete-at-top/empty-side `0,0` range, create-from-empty and
empty-from-content, a shifted-block case, a Unicode line, the >10,000-line
fallback, plus `diff_lines`/`changed_block_indices` rows driven directly); the
`doc_text` and `doc_fm` handler differentials re-verified green against
regenerated oracles (their handlers do not build the diff payload). No handler
change: the ported doc-edit handlers still omit the `change` payload that
consumes this diff — that seam closes with the Librarian save-announcement
writer in W4.6b.

Phase 3 — the endgame plan. Docs only. Re-planned the remainder of the port
from fresh surveys of every unported v4 subsystem (courier, answer-confirmation,
carina query, file/attachment, the buildContext feeders, the post-office
writers, the job runner, image generation, the photo trio, the provider layer,
the autonomous-room engine). Every remaining unit now has a self-contained work
order under `docs/developer/porting/work-orders/` (W4.2u, W4.3, W4.4a4, W4.4b,
W4.5, W4.6a/b, W4.7a/b, W4.8, W4.9a/b, U4), with the batch table and per-round
parallelism/ownership rules in `chat-orchestration.md`. New docs: the W4.7
provider-layer decomposition (six units, appended to `provider-manifest.md`)
and the enclave (Unit 4) decomposition (`enclave-engine.md`). Key decisions
recorded: the job runner drops v4's fork/IPC/buffered-proxy architecture
(in-process handlers over the single-writer runtime; the autonomous turn keeps
the `write_apply` main-primary batch path), file bytes / image transcode are
injected host seams, the provider core stays sans-IO, and image generation gets
a canned `model::image` seam ahead of the real wire dialects. Also a drift
check of v4 `42242a3e..8617ce7a`: one ported unit is stale —
`doc_edit::unified_diff` (v4 replaced the greedy walker with a Myers line
diff + git-style hunks) — scoped as work order W4.d1, first in Round 1; the
`docs/v4/` CHANGELOG mirror refreshed.

Phase 3 — wave 4 (W4.4a, part 3): the compression cache service. Ported v4's
`compression-cache.service.ts` — `triggerAsyncCompression` /
`getCachedCompression` / `invalidateCompressionCache` (+ `hashString` /
`isCacheValid` / `cacheKey` / the `persistToDatabase` / `loadFromDatabase` /
`clearFromDatabase` DB layer) into `services::compression_cache`. The durable
cache lives in the `chats.compressionCache` column (a JSON object, per-participant
in multi-char chats); a process-global in-memory map is the fast path. Added the
`ChatUpdate.compression_cache` update setter (a JSON `null` clears the column to
SQL NULL, no `updatedAt` bump) and `Deserialize` to `ContextCompressionResult` /
`CompressionDetails`. v4's per-chat promise lock (`withPersistLock`) is not ported
— the single-writer task already serializes the load-modify-save; and there is no
in-flight-promise state (`trigger_async_compression` computes synchronously within
its async fn), so `isFallback` is always false. Verified by
`compression_cache_tier3_equivalence` — a five-op corpus (trigger→persist,
trigger-guard [too few messages], get-DB-hit, get-miss, invalidate) driving v4's
REAL functions, diffing the persisted column (minted `createdAt` normalized) + the
`getCachedCompression` return; the canned cheap-LLM key proves the compression
prompt. The two seam closures — the finalizer's `AsyncCompressionTrigger` real
production impl (needs the trigger inputs — messages / systemPrompt / options —
threaded through the finalizer) and the `buildContext` cached-compression window
(the `cachedCompressionResult` / `cachedCompressionMessageCount` inputs, computed
by the spine via `getCachedCompression`) — are additive spine plumbing tracked as
the remaining part of W4.4a; the differentials keep the recording / empty-cache
seams meanwhile.

Phase 3 — wave 4 (W4.4a, part 2): regenerate-swipe. Ported
`regenerateMessageAsSwipe` (`services::regenerate_swipe`), the sibling entry
point to `processMessage`: it generates an alternative ("swipe") for an existing
ASSISTANT message and persists it as a properly-attributed variant, grouped in
place. Composes the ported services — responder resolution, user identity,
`buildMessageContext` (continue-mode, everything strictly before the target), the
`CompletionProvider` seam for a single non-streaming generation, the swipe-group
bookkeeping on `chat_messages` (write back the original's `swipeGroupId` on the
first regeneration; the new swipe shares the original's `createdAt` +
participant), and the ported `deleteMemoriesBySourceMessageWithVectors` cascade
(gated by the per-user `memoryCascadePreferences.onSwipeRegenerate`). The
orchestrator's `build_context_input` / `BuildContextArgs` were made reusable
(scalar clock/model-limit fields instead of `&ProcessMessageInput`). Verified by
`regenerate_swipe_tier3_equivalence` — a four-case corpus (first regeneration,
existing group, KEEP_MEMORIES, and the not-assistant throw) driving v4's REAL
`regenerateMessageAsSwipe`, diffing `chats` / `chat_messages` / `memories` /
`vector_indices` / `vector_entries` (the canned completion key proves the
rebuilt continue-mode prompt bytes). Tracked deferral: the swipe's
`rawResponse` / `reasoningContent` / `thoughtSignature` are null (the cheap-LLM
`CompletionResponse` subset carries none; the corpus canned response has none, so
null is byte-faithful — the richer wire-decoded response lands with W4.7).

Phase 3 — wave 4 (W4.4a, part 1): the agent-mode resolver. Ported
`resolveAgentModeSetting` (the Global → Character → Project → Chat cascade),
`DEFAULT_AGENT_MODE_SETTINGS`, and `buildAgentModeInstructions` into
`services::agent_mode`, closing the orchestrator's agent-mode seam. The spine now
computes the real resolution: reads the project's `defaultAgentModeEnabled` (a
store-managed field, via the overlaid projects read), resolves the cascade, fires
the `agentTurnCount: 0` reset on a new user turn, feeds `agentMode.enabled` to
`buildTools` (adding `submit_final_response`), injects the agent-mode
system-prompt block into `formattedMessages`, and passes the resolved
`ResolvedAgentMode` to the native loop. The orchestrator tier-3 corpus gained an
`agent_mode_on` case (chat-level opt-in, custom `maxTurns: 15` via settings)
banking the byte-exact instruction injection, the `submit_final_response`
slate addition at the wire, and the turn-count reset (seeded 5 → 0); resolver
unit tests cover the cascade matrix.

Phase 1 — pure-function ports to `quilltap-core`, each with a tier-1 differential
test against the v4 oracle:

- Memory: weighting/decay, ranking blend, recall-tag multipliers, recall-history
  ring buffer.
- Write path: write-batch partitioning, main-primary policy, folder-conflict id
  remap, unique-constraint detection.
- Context: sliding-window compression sizing; per-purpose context-budget
  arithmetic (summarize trigger, recent-message count, max-available, allocation
  split); the summarisation cadence (fold/hard gate, interchange count,
  title-check crossing, turn partition); per-character context shaping
  (history-access gate, presence windows, whisper visibility, role/name
  attribution).
- Enclave: autonomous-run budget verdict and progress-toward-binding-cap, plus
  the per-turn context cap that paces a token-budgeted room across turns
  (`computeAutonomousContextCap` = remaining-budget / turns-left, floored).
- LLM: completion cost estimate, cost-aware model selection, model classes,
  character-based token estimation.
- Turn manager: the turn-state machine — queue ops, history-derived state, and
  the spoken-this-cycle wrap; the all-LLM auto-pause thresholds; the
  participant-list filters (user/LLM/active resolvers); the display-only
  predicted turn order; and the weighted-random next-speaker selection (with the
  RNG injected for determinism).
- Memory name-resolution leaves: reinforced-importance formula, name+pronoun
  formatting, the about/holder name-set builders, and the word-boundary name
  matchers (presence / occurrence-count / about-character resolution) — the
  Unicode-boundary + lookahead regex reproduced without a backtracking engine.
- Embedding: L2 vector normalisation, the profile storage policy (Matryoshka
  truncate + optional normalise), cosine similarity with the dimension-mismatch
  guard and message, the fallback keyword/phrase scorer, the literal-phrase
  boost helpers, Float32 ↔ little-endian-byte BLOB conversion, and the legacy
  JSON-text recovery (`parseLegacyEmbeddingText` — reproducing JS `Object.values`
  ascending integer-key ordering for the index-keyed-object shape).
- Canon: the memory-extraction canon blocks (self / other ALREADY ESTABLISHED
  rendering) and the New-Chat scenario-text combiner.
- Mentioned-character scan: detecting non-participant characters named in a chat
  corpus (ASCII word-boundary alternation, longest-token-first, lowercased
  token→ids map).
- Novel-detail extraction: the deterministic proper-noun / date / currency /
  number-with-unit / CamelCase / acronym scanner (ASCII `\d`/`\b`, the JS `\s`
  whitespace set reproduced exactly, case-insensitive dedup).
- Chat-task text shaping: tool-artifact stripping, visible-conversation
  extraction, and the chat-card preview, over shared JS string primitives (the
  JS `\s`/`trim` set and UTF-16 length/slice).
- Docs: added `docs/developer/porting/phase-2-onramp.md` scoping the tier-2
  DB-state oracle and its fixtures (the next build); cross-linked from the
  porting overview and CLAUDE.md, and marked Phase 1 complete in the roadmap.
- Model context limit: `getModelContextLimit` (+ `hasExtendedContext`,
  `getSafeInputLimit`) — the override / provider-default tables ported as
  constants, with the plugin model-info, `FALLBACK_PRICING` rows, and registry
  default injected; reproduces v4's lookup order and substring matching, and the
  JS-truthy fall-through on a zero/null context value.
- Cheap-model classifiers: `isCheapModel` / `estimateModelCost` /
  `getCheapestModel` and their deprecated fallback tables — the registry-sourced
  recommended-list and default-model are injected (empty / none takes the
  fallback path), the string heuristics (expensive/mid/cheap indicators, the
  dashed-vs-undashed `o1`/`o3` split) are pure.
- Version compare: documented `compareVersions`' `localeCompare` fallback (the
  malformed-input path) as a deferred ICU-collation seam — the parseable
  numeric path stays exact; faithful collation waits on the ICU-crate decision.
- Tool canonicalization: byte-stable `UniversalTool` serialization for
  cache-prefix stability — deep code-unit key-sort of `function.parameters` plus
  the tool-name array sort. The name sort is a documented `localeCompare`
  residual seam (the lowercase snake_case tool-name corpus collates identically
  under code-unit order; the ICU-collation decision is deferred).
- Number formatting: the JS `Number.prototype.toFixed` kernel (V8
  half-away-from-zero rounding on the f64's exact value, via IEEE-754
  mantissa/exponent + u128 — distinct from Rust's half-to-even formatter), and
  the display formatters built on it (`formatBytes`, `formatCostForDisplay`, and
  both the `K` and lowercase-`k` `formatTokenCount` variants).
- Small leaf utilities: chat-type/participant predicates, semver parse/compare,
  pronoun→gender hint, tag-style merge, char-count colour class.

Drift catch-up — v4's answer-confirmation feature (a Salon consistency check +
re-affirmation) added columns/keys to six already-ported marshaling surfaces; this
extends each to match, re-verified byte-exact against v4's current oracle output
(no existing test regressed — the new columns are additive/nullable-default, so
the pre-catch-up corpora still passed unchanged before these edits).

- `chat_settings.answerConfirmationSettings` (global default JSON object,
  `{"enabled":false}`) — a new typed struct in schema position between
  `thinkingDisplay` and `storyBackgroundsSettings`; corpus create/update now set
  it.
- `chats.answerConfirmationOverride` (nullable `'ON'|'OFF'` TEXT, parallel to the
  existing `conciergeOverride`) — wired in both the writer and the read path;
  corpus banks both enum values plus the NULL case.
- `chat_messages`' five new `MessageEvent` fields (`confirmed`,
  `confirmationChecked`, `confirmationRevised`, `confirmationNotes`,
  `confirmationOriginalContent`) — ordinary nullable boolean/string columns
  (INTEGER 0/1, NOT the `isSilentMessage` TEXT-affinity union seam); wired in the
  message insert and the read marshaling, so `updateMessage`'s read-modify-write
  carries them through unchanged. Corpus banks all three badge states (Vouched /
  Stood-by / Amended-with-original-content) across the write and read fixtures.
- `projects` properties.json's `answerConfirmationOverride` (now a 17-key bag) —
  added to `ProjectPropertiesSchema`'s field order and to
  `PROJECT_STORE_MANAGED_FIELDS`; corpus create sets it and the roster
  read-modify-write ops prove it survives untouched.
- `llm_logs`' `ANSWER_CONFIRMATION` enum member — the column is plain TEXT on the
  port side (no code change), so this is corpus-only: one surviving row now
  banks the new value.

Phase 3 — the writer-task runtime (Unit 0) and the model-boundary core (Unit
0.5). Native infrastructure that replaces v4's child-process write machinery, so
verified by self-tests rather than a v4 oracle diff.

- `db::runtime`: `Db`, the `Clone + Send + Sync` handle every service holds — a
  per-partition read pool plus a `tokio::mpsc` write channel that is the only
  mutator. A dedicated OS thread owns the `WriterSet` (main + optional
  mount-index/llm-logs RW writers) and drains the channel serially, so batch
  apply stays serial (the property the folder-conflict remap and main-primary
  ordering assume). A write is a type-erased `FnOnce(&mut WriterSet)` closure
  carrying its own `oneshot` reply; `write_apply` remains available for the
  multi-DB job path, invoked inside a closure. Reads go direct to a pooled
  read-only connection (`PRAGMA key` first-and-only, per the read-path rule).
  API: `write` (async) / `write_blocking` / `read_main` / `read_mount_index` /
  `read_llm_logs`, plus `DbError::{WriterGone, WriterSpawn, PartitionUnavailable}`.
  Four self-tests: 100 concurrent writers serialize with no lost updates,
  read-after-write sees committed state, `write_blocking` commits, and a
  missing-partition read is a clean typed error.
- `model::embedding`: `EmbeddingProvider` (the tier-3 seam mirroring v4's
  `generateEmbeddingForUser`) with `EmbeddingResult` / `EmbeddingError` /
  `EmbeddingPriority`, plus `CannedEmbeddingProvider` — a deterministic responder
  keyed by exact input text (fixed vector; explicit failures for
  `SKIP_EMBEDDING_FAILED`; an unregistered input errors rather than answering).
  Async and generic (no trait object), three self-tests. The v4-oracle-side
  canned injection lands with Unit 1's memory-gate differential.
- Added `tokio` (`sync` only in the library — the writer is a plain OS thread, so
  no scheduler is pulled into the core; `macros`/`rt-multi-thread` dev-only).
- Docs: CLAUDE.md's "Never accept unverified Rust" corrected — `cargo
  build`/`test`/`clippy` do run in this environment and should be run before
  presenting Rust as done; the real-instance open + oracle diff remain the proof
  for crypto/cipher. Status sections (CLAUDE.md, `overview.md`, `phase-3.md`)
  updated for Units 0 and 0.5.

Phase 3 — the **memory gate** (Unit 1), the first decision service. Ported v4's
`createMemoryWithGate` / `runMemoryGate`, verified the new tier-3 → tier-2 way (a
canned embedding injected identically on both sides, then a structural DB diff).

- `services::memory_gate`: the pre-write similarity gate — `INSERT` /
  `INSERT_RELATED` / `REINFORCE` / `SKIP_NEAR_DUPLICATE` / `SKIP_EMBEDDING_FAILED`
  by cosine band (`NEAR_DUPLICATE_THRESHOLD` 0.90 / `MERGE_THRESHOLD` 0.85 /
  `RELATED_THRESHOLD` 0.70; the stale v4 header comment ignored). Async, generic
  over an `EmbeddingProvider`, reading off the read pool and funnelling every
  mutation through the writer thread — the first service to drive the whole Unit-0
  write path end to end. Reinforcement re-extracts novel details, appends
  footnotes, bumps count/importance, and re-embeds on a content change; related
  inserts bidirectionally link. Deferred (tracked): `maybeEnqueueHousekeeping`,
  the `skipGate` direct path, `applyNamePresenceCheck`'s cross-character lookup,
  and the 500 ms inter-retry delay.
- `db::vector_store`: the in-memory `CharacterVectorStore` shim (v4
  `vector-store.ts`) — load off a read connection, linear cosine top-K (stable
  descending, dimension guard), and an incremental flush (add/update/saveMeta)
  through the writer.
- `db::memories::MemUpdate` gained `embedding` (the `Some`-gated BLOB setter the
  gate writes through) and `related_memory_ids` setters; `dump_table_json_conn`
  lets the harness snapshot a table off a read-only pooled connection after a
  service commits.
- Differential: a tier-3 oracle drives v4's REAL `createMemoryWithGate` under jest
  (mocking only `generateEmbeddingForUser`, with the real cipher binding wired in
  via `better-sqlite3-multiple-ciphers`) over a seven-scenario corpus — one per
  outcome, each on its own character — and the Rust gate is diffed across
  `memories` + `vector_indices` + `vector_entries` in the shared-cross-table
  id-map remap form. Four core self-tests exercise the outcomes over an in-memory
  `Db` + canned provider.

Phase 3 — the memory deletion chokepoint (the first memory-family follow-on).
Ported v4's `deleteMemoryWithUnlink` / `deleteMemoriesWithUnlinkBatch` (memory-gate.ts)
as `MemoriesRepository::delete_with_unlink` / `delete_many_with_unlink` — the single
point every cascade (housekeeping sweeps, chat-wipe, swipe-group cleanup) deletes
through, so a removed id never lingers in another memory's `relatedMemoryIds`.

- `delete_with_unlink`: `LIKE '%"<id>"%'` neighbour pre-filter, per-neighbour
  character-scoped `relatedMemoryIds` rewrite, then the row delete. Idempotent — a
  missing row returns false without touching neighbours.
- `delete_many_with_unlink`: one-pass scan of every row with a non-empty links
  array, scrubs every doomed id from each neighbour in one update, then deletes the
  doomed set grouped by character (`bulkDelete` is characterId-scoped). Empty → 0.
- Differential: a tsx real-DB oracle drives v4's REAL chokepoint over a pre-seeded
  nine-memory graph (cross-linked across two characters), and the `memories` dump
  is diffed in the sentinel-aware minted-`updatedAt` form (an untouched row stays at
  the seed sentinel — proving no stray bump). Four repo self-tests cover the
  single/batch scrub, the missing-row no-op, and the empty batch.

Phase 3 — the memory-service cascade-delete family (the second memory-family
follow-on). Ported v4's `deleteMemoryWithVector` and the three
`deleteMemoriesBy*WithVectors` cascades (memory-service.ts) as
`services::memory_service` — the vector-store-aware wrappers around the deletion
chokepoint that every bulk delete path (single UI delete, source-message cascade,
swipe-group cascade, chat wipe) goes through.

- `services::memory_service`: `delete_memory_with_vector` (ownership check before
  the characterId-agnostic chokepoint; non-fatal vector cleanup after a
  successful delete), `delete_memories_by_source_message_with_vectors`,
  `delete_memories_by_source_messages_with_vectors` (gathers the whole swipe
  group up front so the neighbour scan sweeps once), and
  `delete_memories_by_chat_id_with_vectors` (adds `characterCount`). Cascades
  group the doomed set by character in first-appearance order, count only vectors
  the store actually held (`hasVector` first), guard each character's cleanup
  non-fatally, then batch-delete through the chokepoint. Three self-tests.
- `db::vector_store::CharacterVectorStore::remove_vector` (v4 `removeVector`):
  un-adds a same-flush add, otherwise tracks the id for deletion and drops any
  pending update; a store whose sweep removed nothing flushes as a no-op, so its
  `vector_indices.updatedAt` is not bumped. Three unit tests.
- Differential (`memory_cascade_tier2_equivalence`): a tsx real-DB oracle drives
  v4's REAL memory-service over an 8-op sequence on an 11-memory / 6-character
  fixture (cross-character links, two vector-less memories, one entry-less
  store), asserting each op's return against the spec on both sides, then diffing
  `memories` + `vector_indices` + `vector_entries` in the sentinel-aware
  minted-`updatedAt` form — the untouched stores' metadata provably keeps the
  seed sentinel.

Phase 3 — memory housekeeping (the third memory-family follow-on). Ported v4's
`runHousekeeping` / `getHousekeepingPreview` / `needsHousekeeping`
(housekeeping.ts) as `services::housekeeping` — the retention sweep the
`MEMORY_HOUSEKEEPING` job runs. No model call: the merge pass searches the
already-stored vector index against itself.

- `services::housekeeping`: three passes then a gated apply. (1) Retention —
  MANUAL is a hard protection override, otherwise the blended
  `calculate_protection_score` >= 0.5 protects; an unprotected memory goes only
  when below the importance floor AND old AND inactive. (2) Opt-in similarity
  merge over stored vectors (>= threshold folds into the more-important/newer
  survivor; the merge pass does not consult protection — faithful to v4).
  (3) Cap enforcement deletes the lowest-effective-weight unprotected memories
  from the tail, with v4's all-protected pre-check. Apply deletes through the
  chokepoint then cleans the vector store non-fatally; `dry_run` reports without
  writing. Detail reasons formatted with the ported JS `toFixed` so they match
  v4 byte-for-byte at equal wall clock. Three self-tests.
- `clock` gained `now_unix_ms` and `iso_to_ms` (the strict inverse of
  `iso_from_unix_ms`, matching JS `Date.parse` on the repo-minted shape);
  `CharacterVectorStore` gained `all_entries` (v4 `getAllEntries`, load order).
- Differential (`memory_housekeeping_tier2_equivalence`): a tsx real-DB oracle
  drives v4's REAL housekeeping over a 6-op sequence (dry-run, retention sweep,
  merge sweep, cap sweep, both `needsHousekeeping` branches) on a 15-memory /
  3-character fixture, then BOTH the per-op result objects (counts, id lists,
  details — age/inactive month numbers placeholdered, being wall-clock-derived)
  and the three table dumps (sentinel-aware minted-`updatedAt`) are diffed.
  Corpus-freshness note recorded: the "recent" seed dates age past the 6-month
  windows ~2026-12; refresh them when regenerating after that.

Phase 3 — the completion half of the model boundary (`model::completion`),
mirroring `model::embedding`'s shape. Native tier-3 infrastructure (like Unit
0.5), so verified by self-tests; the v4-oracle-side canned injection lands with
the memory-processor differential.

- `model::completion`: `CompletionProvider` — the seam every completion call
  goes through, sitting at v4's `provider.sendMessage(params, apiKey)` (the
  `LLMParams`/`LLMResponse` subset the cheap-LLM path consumes: role+content
  messages, model, optional temperature, maxTokens, strictMaxTokens, cacheKey,
  profileParameters). Everything above the seam (the temperature fallback, the
  uncensored-provider retry, response parsing) is ported orchestration that must
  sit inside the differential; API-key acquisition stays host-side.
- `CannedCompletionProvider`: a deterministic responder keyed by the exact call
  input (`canned_completion_key` = provider | model | temperature-or-`-` | the
  `[{role, content}]` JSON) → fixed response text + token usage. Unregistered
  input errors rather than answering; failure entries carry their exact error
  message so message-inspecting fallbacks can be driven deterministically. Five
  self-tests (incl. temperature-presence and provider/model key separation, the
  two fallback paths' key shapes).

Phase 3 — the memory-extraction pure leaves (`memory_tasks`), the tier-1 half
of the memory-processor unit. Ported from v4
`lib/memory/cheap-llm-tasks/memory-tasks.ts`.

- `memory_tasks`: the SELF/OTHER extraction prompt builders
  (`get_self_memory_extraction_prompt` / `get_other_memory_extraction_prompt` —
  the byte-stable bodies, the first-person-user and autonomous-room preambles,
  the ORIENTING CONTEXT footer with its 1500-UTF-16-unit truncation, the
  numbered multi-subject CONTEXT footer), the shared turn-context renderer
  (`render_turn_context` — roster branches, the user-controlled-slice
  single-rendering rule, the standalone-opener fallback), the message builders
  (`build_self_extraction_messages` / `build_other_extraction_messages`, `None`
  = v4's no-slice/no-subjects early return), and the response parsers
  (`parse_memory_candidate_array` / `parse_other_candidates_by_subject` /
  `coerce_memory_candidate` / `apply_targeting_tags` — fence stripping, closed-
  vocabulary tag validation with present/wide/information defaults, JS-truthy
  content/summary coercion via `JSON.stringify`, `HARD_CANDIDATE_CAP` = 2, the
  per-subject and total caps, JS `Number.isInteger` subjectIndex semantics, and
  the null-item TypeError that empties the whole SELF array). `importance` is
  kept as the raw JSON number so integer emissions re-serialize bare.
- The big prompt bodies live in a **generated** submodule
  (`memory_tasks/prompt_text.rs`), extracted mechanically from the v4 source —
  no hand transcription. Also hosts `strip_code_fences` (v4 keeps it in
  `ai-import.service.ts`); `jsstr` gained `js_trim_end`; the `recall_tags`
  closed-vocabulary parsers (`from_kw`) went public for the extraction side.
- Differential (`memory_tasks_equivalence`): a jest oracle (the seam is a
  module export only `jest.mock` can replace — the same seam v4's own
  extraction tests use) drives v4's REAL `extractSelfMemoriesFromTurn` /
  `extractOtherMemoriesFromTurn` over a committed 14-case corpus with ONLY
  `executeCheapLLMTask` mocked, capturing the built messages byte-for-byte and
  feeding each case's canned response text into the real parser. Four
  self-tests on top.

Phase 3 — the **memory processor** (`services::memory_processor`, v4
`processTurnForMemory`), the model-dependent per-turn extraction service — the
first tier-3 differential to pin BOTH model boundaries (completion +
embedding). Also closes the memory gate's `applyNamePresenceCheck` deferral.

- `cheap_llm`: v4 `lib/llm/cheap-llm.ts`'s pure selection logic —
  `get_cheap_llm_provider` (the five-priority order: global default cheap
  profile, USER_DEFINED, any `isCheap` profile local-preferred, local-first
  Ollama, current-provider-cheapest fallback, with the registry seam injected
  as in `cheap_model`) and `resolve_uncensored_cheap_llm_selection` (dangerous
  chats swap to the configured uncensored profile, then any
  dangerous-compatible one, else fail open). Plus `build_character_cache_key`
  (v4 `lib/llm/cache-key.ts`) and the `CheapLlmProfile` / `CheapLlmSelection` /
  `DangerousContentSettings` / `UncensoredFallbackOptions` types. Three
  self-tests.
- `services::cheap_llm_exec`: v4 `core-execution.ts`'s pipeline —
  `CheapLlmTaskExecutor` holds the session-level no-custom-temperature cache
  (v4's module-global `profilesWithoutCustomTemp`, instance state here); the
  0.3-temperature first try with the message-inspecting retry-without-
  temperature; the strict 2048 max-tokens floor; the uncensored-provider retry
  on empty responses (`should_attempt_uncensored_fallback`, incl. the exact
  both-providers-empty error string); parse-and-wrap into
  `CheapLlmTaskResult`. **Deferred (tracked):** API-key acquisition (host-side;
  the boundary starts at the provider call) and the fire-and-forget
  `logLLMCall` llm-logs write. Two self-tests.
- `services::memory_processor`: the orchestration — per-character rate limits
  (`countCreatedSince` over the last wall-clock hour; skip at the cap,
  throttle past the soft-start fraction with the importance floor), the
  once-per-turn selection resolve, the SELF pass (own-fields canon) and the
  multi-subject OTHER pass (canon from the observer's vault
  `Others/<subject>.md` via the new `read_vault_text_file` +
  `load_canon_for_observer_about_subject`, falling back identity →
  description → none), dry-run collection, and every candidate written through
  the ported memory gate with the per-outcome debug lines reproduced
  byte-for-byte (JS number interpolation, `toFixed(3)` similarity,
  `${undefined}` semantics).
- Memory gate: the `applyNamePresenceCheck` **lookup branch is now ported**
  (deferral closed) — a cross-character AUTO proposal reads both characters
  through the vault-overlaid `characters_read::find_by_id` and resolves via the
  Phase-1 `resolve_about_character_id`, collapsing a mis-attributed
  about-target to a self-reference; any lookup failure passes through
  unchanged (v4's never-block-a-write catch). `MemoryGateOutcome` gained
  `reinforcement_count` (the extraction driver's debug line reads it).
- Differential (`memory_processor_tier3_equivalence`): a jest oracle drives
  v4's REAL `processTurnForMemory` over a two-database fixture (characters
  with real vaults + a seeded `Others/Charlie.md`, gate-band vector seeds, and
  future-dated rate-limit ballast — a 2099 `createdAt` is always "in the last
  hour", so counts are wall-clock-proof) with only the model/infra seams
  stubbed. The completion mock resolves calls by (pass, CONTEXT-footer label,
  model, autonomous-clause) rules and RECORDS each exact
  `provider|model|temperature|messages` canned key; the Rust side replays
  those entries through `CannedCompletionProvider`, so any prompt/selection
  divergence surfaces as a canned-miss. Three calls (a full mixed turn, an
  autonomous dangerous dry run, an empty turn) banking: throttle drops +
  skip/duplicate-user logs, all five gate outcomes (incl. the uncensored
  fallback feeding SKIP_EMBEDDING_FAILED), all four canon sources, the
  name-presence flip, sourceMessageTimestamp pinning, and usage aggregation
  (the discarded empty-response usage included). Result objects (debug logs
  byte-for-byte) AND the three tables (shared-id-map remap form) are diffed;
  the memory-gate differential re-verified green after the gate change.

Phase 3 — the memory gate's **watermark auto-housekeeping check** (v4
`maybeEnqueueHousekeeping`), closing the gate's last write-side deferral.
After an INSERT / INSERT_RELATED the gate now checks whether the character has
reached the watermark fraction (0.9) of its auto-housekeeping cap and, if so,
enqueues a `MEMORY_HOUSEKEEPING` background job — unless backed off.

- `services::queue_service`: the `enqueueJob` + `enqueueMemoryHousekeeping`
  slice of v4's queue service — mint a PENDING `background_jobs` row; the
  housekeeping variant de-dupes against in-flight (PENDING/PROCESSING) jobs
  for the same (userId, characterId) and caps attempts at 1 (retry-hostile).
  **Deferred:** `ensureProcessorRunning` (the job runner is a later unit; the
  oracle pins v4's auto-start to a no-op to match).
- `services::housekeeping_outcome_cache`: v4's in-memory ineffective-sweep
  back-off. **Rust home decision:** v4 holds it as a module-global Map; the
  port keeps the same process-global shape (`OnceLock<Mutex<HashMap>>`),
  keyed by characterId. One self-test.
- The gate's `maybe_enqueue_housekeeping`: enabled-settings gate (via a new
  scoped `chat_settings::find_auto_housekeeping_settings_by_user_id` read —
  the full `findByUserId` marshaling remains a later chat-settings read
  sub-unit), the `perCharacterCapOverrides ?? perCharacterCap ?? 2000` cap
  resolution, the post-write count vs `floor(cap × 0.9)`, the in-memory
  back-off, and the durable 15-minute throttle over
  `findRecentByType('MEMORY_HOUSEKEEPING', 50)`. Never propagates an error
  (v4's catch); the port awaits the call v4 `void`s — same DB effect once
  settled, no detached-task machinery in the core.
- Differential (`memory_watermark_tier3_equivalence`): seven
  `createMemoryWithGate` INSERTs over a seeded fixture (settings rows,
  watermark-exact memory ballast, a future-`updatedAt` COMPLETED sweep and a
  PENDING dedupe target — future timestamps make the wall-clock windows
  deterministic) banking: a real enqueue, below-watermark, the override
  raise, disabled settings, the durable throttle, the in-flight dedupe, and
  the in-memory back-off (both sides record the same outcome through their
  real cache first). Four tables diffed; the memory-gate and memory-processor
  differentials re-verified green with the watermark path live.

Phase 3 — chat orchestration (Unit 3) started: the decomposition doc plus
waves 1–2, ported in parallel (six pure-leaf agents, then three composed
units), each with its own fresh-oracle differential.

- Added `docs/developer/porting/chat-orchestration.md`: the survey of v4's
  send-message engine (`lib/services/chat-message/`, `buildContext`, the
  stateful turn chain), the SSE event vocabulary → `Event`-channel mapping, and
  the four-wave leaf-first decomposition with per-unit verification plans.
- Template processor (`templates`): `processTemplate` / `buildTemplateContext`
  / `processCharacterTemplates` — ASCII-`\w` token rule, single-pass
  non-recursive replacement, and the two-pass `{{trim}}` quirk (the paired
  macro can never fire) ported faithfully. Turn-predicate gap closed
  (`is_users_turn` / `is_participants_turn` / `get_selection_explanation`).
- Chat timestamps (`chat_timestamp`): timezone resolution, real/fictional
  timestamp calculation (clock injected), injection cadence, system-prompt
  formatting. Added `jiff` (pinned) for the IANA UTC-offset lookup — proven
  byte-exact against `Intl.DateTimeFormat` across both US DST boundaries,
  fractional-offset zones, and the invalid-zone throw; v4's CUSTOM-token
  sequential-replace bug reproduced. Plus the formatting prompt hint
  (`template_prompt_hint`).
- Memory-injector formatters (`memory_injector`): metadata tag, scene state
  (sceneHash + `_unchanged_` compaction), memory/inter-character/frozen-archive
  /dynamic-head/summary blocks — sort stability, insertion-order maps, and
  UTF-16 slicing all byte-exact.
- Message selector (`message_selector`, the greedy tail fit) and the
  core-whisper cadence gate (`core_whisper`).
- Carina markup parser (`carina_parser`): JS-dot / ASCII-`\w` / smart-quote
  pairing semantics.
- Message formatter (`message_formatter`): the anti-hijack cleanups
  (name-prefix strip, foreign-speaker truncation, content-block normalization)
  and provider name-field helpers; finish-reason extraction (`finish_reason`).
- System-prompt builder (`system_prompt`): identity stack, public identity
  card, other-participants info, identity reinforcement, `buildSystemPrompt` —
  composed over `templates` + `chat_timestamp`.
- Stateful turn-orchestration decision core (`services::turn_orchestrator`):
  `should_chain_next` (guard chain, all-LLM auto-pause write, turn-queue pop +
  write-back, weighted selection with injected RNG), `persist_turn_participant_id`,
  and the turn-action mutation core (nudge/queue/dequeue/skipUserTurn/query).
  `ChatUpdate` gained `turn_queue` + nullable `last_turn_participant_id`
  setters. Verified by a 13-op tsx real-DB tier-2 differential (two-DB seeded
  fixture, zero normalization).
- Streaming model boundary (`model::stream`): `StreamChunk` (v4's normalized
  chunk vocabulary — the target for the future manifest stream decoders),
  `StreamingCompletionProvider`, and `CannedStreamingProvider` with
  first-class mid-stream failures; oracle-side injection lands with the
  wave-3 primary-stream differential.
- Eleven new tier-1 oracle cases + the turn-orchestrator tier-2 case/fixture;
  the `chats` tier-2 differential re-verified green with the new setters.

Phase 3 — chat orchestration wave 3, batch 1: the seven mutually-independent
model-calling/DB-reading services, ported in parallel (six agents on disjoint
files; shared `ChatUpdate` setters + `services/mod.rs` pre-staged serially),
each with its own fresh-oracle differential.

- Compression service half (`services::compression`): `applyContextCompression`
  + `compressConversationHistory` over the ported sizing leaves and the
  cheap-LLM executor; system-prompt compression stays disabled (result shape
  matched, dead path not ported). Result-object tier-3 differential, 6 cases.
- Context-summary service half (`services::context_summary`):
  `generateContextSummary` / `invalidateContextSummaryIfMessageCovered` /
  `checkAndGenerateSummaryIfNeeded` + `foldChatSummary` and both title
  generators; the prior-generation Librarian-whisper sweep ported;
  `queue_service` gained `enqueue_title_update`. Librarian re-post, vault
  mirror, relevant-conversations refresh, and cost events deferred behind a
  no-op `ContextSummarySeams` trait (oracle mocks match). 11-op tier-3
  differential over `chats` + `chat_messages` + `background_jobs`.
- Knowledge injector (`services::knowledge_injector`) with
  `search_document_chunks` and the qtap-uri/tier-dedupe leaves; first-message
  context (`services::first_message_context`) with
  `memory_service::search_memories_semantic` (recallContext re-rank deferred).
  Two read-differentials, zero normalization, embeddings canned both sides.
- Participant resolver (`services::participant_resolver`, incl.
  `resolveConnectionProfile`) and user-identity resolver
  (`services::user_identity_resolver`); scoped reads added to
  `connection_profiles` / `roleplay_templates` / `users`; the inherited
  roleplay template persists via the new `ChatUpdate.roleplay_template_id`
  setter. API-key acquisition stays host-side. Two tsx real-DB differentials.
- Primary stream / recovery / provider failover (`services::primary_stream`,
  `services::recovery`, `services::provider_failover`) over `model::stream`,
  with the first typed event vocabulary (`services::chat_events`: `ChatEvent`
  + `EventSink`, byte-identical to v4's SSE frames) and
  `save_assistant_message` as the shared persistence primitive; the
  `lib/llm/errors.ts` classifiers ported. 12-call tier-3 differential diffing
  the ordered event trace, both table dumps, and result objects.
- Carina markup runner (`services::carina_runner` + the `postCarinaResponse`
  writer). `runCarinaQuery` established as an injected seam (it requires the
  wave-4 tool subsystem and other unported services); Prospero error-posting
  behind a recorded seam. 7-case tier-3 differential.
- `ChatUpdate` gained the summary-counter/anchor/title-watermark and
  `roleplay_template_id` setters; `chats_tier2` and `turn_orchestrator`
  differentials re-verified green against regenerated oracles.

Phase 3 — chat orchestration wave 3, batch 2: the message finalizer and the
`buildContext` capstone, ported in parallel, each with a tier-3 differential.

- Message finalizer (`services::message_finalizer`): `finalizeMessageResponse`
  + `calculateNextSpeaker` — the core clean → re-base → persist → carina →
  next-speaker → done-event → background-triggers path. The tool /
  answer-confirmation / async-compression / RNG / cost-estimation subsystems
  are injected seams with their gate conditions reproduced and banked;
  `save_assistant_message` extended (confirmation bag, isSilentMessage, image
  links via the new `db::files::add_link`); `chat_events` gained the full done
  payload plus `CarinaAnswer`/`ConfirmationResult` variants (recovery frames
  unchanged — primary-stream differential re-verified); `queue_service` gained
  `enqueue_memory_extraction` + `enqueue_chat_danger_classification`. Ten-call
  tier-3 differential diffing results, ordered event traces, seam records, and
  `chats`/`chat_messages`/`background_jobs`/`files`.
- `buildContext` capstone (`services::build_context`): the full context
  assembler composed from the ported subsystem (system prompt, budgets,
  phase-1 compression, two-pool memory retrieval, scene state, inter-character
  memories, knowledge retrieval, summary anchor + Librarian cache breakpoint,
  attribution/whisper shaping, timestamps, the Commonplace recall fold).
  Unported feeders and whisper-posting side effects behind a
  `BuildContextSeams` trait mirrored by the oracle mocks. Seven-op tier-3
  differential diffing the full `BuiltContext` byte-for-byte (frozen wall
  clock both sides).
- Remaining wave-3 unit: `processMessage` + `executeTurnChain` (also picks up
  the finalizer's deferred summary-check invocation and buildContext's
  autonomous-cap plumbing).

Phase 3 — chat orchestration wave 3 capstone: the `processMessage` spine +
`executeTurnChain` (`services::orchestrator`), completing the planned wave-3
roadmap.

- Composes every landed wave-1..3 service into the full user-message →
  assistant-response cycle; the finalizer's deferred summary-check invocation
  is closed here (wired where v4 wires it). `chat_events` gained the
  `turnStart`/`turnComplete`/`chainComplete` frames and the empty-response
  done fields. Unported subsystems (attachments, tools, agent mode, danger
  reroute, courier, RNG, prospero cadence) are `OrchestratorSeams` with their
  v4 gates reproduced and banked inactive.
- First end-to-end tier-3 differential: six cases (full single turn,
  continue-mode, empty-response retry, mid-stream preserve-partial, a real
  summary fold, a multi-character chain) driving v4's real send path with
  frozen clock/RNG; ordered event trace + chats/chat_messages/background_jobs
  diffed; message-finalizer and primary-stream differentials re-verified.
- Discovered and documented: v4's `buildMessageContext` wrapper
  (context-builder.service.ts) is not yet ported (reduced to a passthrough on
  both differential sides) — the remaining orchestrator-family unit; and a
  chain-depth divergence on non-continue single-LLM-character chats is
  flagged for a dedicated follow-up corpus.

Drift check — v4 `8efe1ba9..f69200bb` (17 commits) audited against the ported
surface; no ported unit is stale. Docs only, no crate source changed.

- Confirmed in the port already: the `profileParameters` forwarding fix
  (`8cf7272e`) and the answer-confirmation service halves (`29f3ae63` — the
  finalizer gates + the `confirmationResult` event) landed inside the wave-3
  ports; corrected the stale CLAUDE.md note that called the forwarding fix
  unported.
- v4's jest-config change (`69fa611e` — `.integration.test` files excluded from
  unit runs; `better-sqlite3-multiple-ciphers` now mapped to the DB mock)
  verified harmless to the oracle machinery by regenerating the memory-gate
  oracle under the new config and re-running its differential green.
- New unported v4 surfaces recorded in the plans: the anthropic
  adaptive-thinking / sampling-param-rejection rules (`provider-manifest.md`),
  the wardrobe transfers endpoint + public READ trio as archetype-tier
  consumers (`overview.md`), and server-side markdown rendering +
  `qtap-linkify` (with its lookbehind-regex porting note) plus the expanded
  answer-confirmation unit in `chat-orchestration.md`'s wave-4 list.
- Refreshed the `docs/v4/` mirror (CHANGELOG, DDL.md, the answer-confirmation
  feature doc).

Phase 3 — chat orchestration: the chain-depth divergence resolved and the
`buildMessageContext` wrapper ported, closing the two orchestrator-family open
items the wave-3 capstone flagged.

- Chain-depth divergence investigated and resolved as an oracle-harness
  artifact, not a v5 bug: the differential's oracle froze `Date.now()`, so
  identical `createdAt` values let `getMessages`' `ORDER BY createdAt ASC`
  tie-break the non-continue user row after the assistant replies, flipping
  `calculateTurnStateFromHistory`'s `lastSpeakerId` to the user and re-picking
  the sole LLM character to max depth. The Rust spine stamps `createdAt` from a
  real monotonic clock, so it correctly stops at `user_turn`; proven by ticking
  the oracle clock +1ms/read (v4 then also stops at `user_turn`). Fix: the
  orchestrator oracle clock advances 1ms per read, the differential now diffs
  `spokenThisCycleParticipantIds`/`turnQueue`/`lastTurnParticipantId` exactly
  (previously placeholdered) with the job-payload anchor ids remapped through
  the shared message idmap, and two chain-depth cases were added
  (`noncontinue_single_user_chain` → `user_turn`; `noncontinue_two_llm_maxdepth`
  → genuine `max_depth`).
- `buildMessageContext` wrapper ported (`services::message_context`, v4
  `context-builder.service.ts`), leaf-first. Three pure leaves ride a tier-1
  differential (`message_context_leaves_equivalence`): `buildConversationMessages`
  (type/role filter, `assistantAfter` reverse pass, TOOL-result render with the
  `>3`-turn elision), `normalizeWhisperRoles` (Staff→USER re-role, opaque-body
  swap, attachment-bearing exemption), and `collectLanternImageFileIdsForCharacter`
  (own-turn-stop walk, history cutoff, dedup, lookback cap). The composition runs
  the A–D whisper pre-filters (commonplace strip + relevant-conversations
  exception; TOOL-whisper target filtering; opaque-anywhere over LLM participants'
  `systemTransparency`; whisper re-role), `buildConversationMessages`, the ported
  `buildContext`, `formatMessagesForProvider`, the Lantern merge, trailing-prefix
  injection, and the multi-character scene block (Anthropic system-instruction
  route vs non-Anthropic `[Name]` prefill). Wired into the orchestrator spine
  where the direct `build_context` call sat, so `formatMessagesForProvider` + the
  scene block now reach the wire. The K file-loading half is the injected
  `MessageContextSeams` (wave-4 file subsystem); the id-collection leaf is
  exercised.
- Orchestrator oracle rebuilt to drive v4's REAL `buildMessageContext` (the
  passthrough mock dropped; only the K file-loader mocked, mirroring the Rust
  seam). Every corpus chat is multi-character, so the scene block + name
  prefixing apply throughout (changing the canned stream keys, re-recorded and
  reproduced byte-for-byte). Five cases added: `nonanthropic_scene`,
  `commonplace_strip`, `opaque_swap` vs `transparent_no_swap`, and
  `tool_whisper_filter`. `orchestrator_tier3_equivalence` re-verified green.

Phase 3 — wave 4 (W4.2): the dangerous-content ("Concierge") orchestration
subsystem (`services::dangerous_content`), replacing the injected
`DangerousContentRouter` stub with the real resolution. Ported v4's
`lib/services/dangerous-content/` + the `CHAT_DANGER_CLASSIFICATION` job runner:

- `chat_override` — the two-field danger-status derivation (`isConciergeOffDuty`
  / `getConciergeState` / `isChatActiveDangerous`; off-duty preserves the label,
  wins over the classification).
- `resolver` — `resolveDangerousContentSettings` (global + per-chat off-duty /
  moderation-exempt short-circuits; the DEFAULT / OFF_DUTY constant shapes).
- `gatekeeper` — content classification: the moderation-provider path (an
  injected `ModerationProvider` seam collapsing v4's plugin registry +
  `autoDetectModerationApiKey` + `provider.moderate`; the port still runs the
  ported `mapModerationResult` over the raw result), the cheap-LLM classify path
  (the byte-exact `CLASSIFICATION_SYSTEM_PROMPT` in a generated `prompt_text`
  submodule, temperature 0.1 / maxTokens 500, over the `CompletionProvider`
  seam), `parseClassificationResponse`, `CATEGORY_LABELS` /
  `MODERATION_CATEGORY_MAP`, and the module-global classification LRU cache.
- `provider_routing` — the REAL implementor of the frozen
  `DangerousContentRouter` seam: `resolveProviderForDangerousContent` +
  `resolveImageProviderForDangerousContent` +
  `resolveUncensoredImageProfileForReroute` + `isImageModerationError` (the five
  exact reason strings preserved). API-key material stays host-side (an injected
  `ApiKeyResolver` seam); `DangerContentRouter` maps the resolution into the
  failover's `RouteResult`. Added additive `connection_profiles` / `image_profiles`
  `find_by_id` / `find_all` / `find_by_user_id` net reads.
- `manual_flip` — `applyConciergeFlip` (the tri-state operator flip; raw
  multi-column chat `UPDATE` that mints no `updatedAt`, byte-identical to v4's
  `chats.update` — the frozen `ChatUpdate` is owned by the parallel W4.4a batch).
- `gatekeeper_job` — `handleChatDangerClassification` (the job runner): the
  sticky/exempt/off-duty/mode-OFF bails, the context-summary-else-concatenated-
  messages classification input, the cheap-LLM selection, the classify call, the
  `DANGER_CLASSIFICATION` system event + token aggregate (only on the LLM path,
  which mints `updatedAt`), and the chat-level danger-field persistence.

Three differentials, all green against v4 HEAD: `danger_resolver_equivalence`
(tier-1 resolver + override matrix, plus a tier-2 manual-flip chat-row dump),
`danger_routing_equivalence` (the reroute matrix — decision + profile identity +
resolved key + exact reason, canned api-key seam both sides), and
`danger_gatekeeper_tier3_equivalence` (drives v4's REAL job runner over a seeded
fixture — safe/dangerous/borderline/parse-failure LLM classifications, the
moderation-provider path incl. a provider failure, the system-event + chat writes
diffed sentinel-aware). Seams (tracked deferrals): the moderation plugin registry,
the cheap-LLM / routing API-key acquisition, `logLLMCall`, the job-runner
infrastructure, and the Concierge personified-announcement writers (W4.6).
Spine integration — constructing the real router/gatekeeper at the orchestrator
composition point + the OFF-short-circuit / live-reroute orchestrator-corpus
cases — is deferred to unification (it edits W4.4a-owned files).

Phase 3 — wave 4 (W4.1g): `buildTools` + the tool-slate spine wiring (closes
W4.1). Ported v4's `buildTools` + the built-in half of `buildToolsForProvider`
(`services::tool_build`): the flag→tool-set construction over the b.3 definition
catalog, the individual disabled-tool filter, the `allowToolUse === false` and
`disabledTools === undefined` short-circuits, and the canonical (universal/OpenAI)
provider shape. `checkModelSupportsTools` + `provider.supportsWebSearch` are
injected registry-seam inputs (the `getModelContextLimit` precedent); the plugin
tool registry, the provider `formatTools` reshape, and image-provider constraint
enrichment are documented W4.7 deferrals. Ported the orchestrator flag region
(`canDressThemselves` / `canCreateOutfits` / `helpToolsEnabled` /
`documentEditingEnabled`, the `characterIsTransparent` + `self_inventory` strip,
the `askCarinaEnabled` overlay-free probe, the autonomous-room destructive-tool
filter, `resolvedToolMode` / `useTextBlockTools` / `actualTools`, and the
mode-switched `toolInstructions`), and closed the spine seams: the real slate now
flows into the primary stream, the native loop (with the real `BuiltInToolRunner`
+ the injected W4.7 tool-call detector), and the text-tool passes'
`continuationTools`; the finalizer receives the real tool messages/images. Added
`plugin_config::find_by_user_id`. Verified by a new `tool_build_equivalence`
differential (27 flag-matrix cases driving v4's REAL `buildTools`, byte-exact
slate) and the rebuilt `orchestrator_tier3_equivalence` (18 cases running the REAL
`buildTools` + flag region; a per-call tools-at-wire assertion proves the slate
reaches the provider on every case; new cases bank the `self_inventory` strip
[transparent vs not], the `ask_carina` transparency probe, disabled-tools
filtering, and text-block-mode empty slate). `native_tool_loop`, `text_tool_loop`,
`message_finalizer`, and `primary_stream` differentials re-verified green.

Phase 3 — wave 4 (W4.1d batch 3b): the doc-edit tool handlers (part 2 — the
remaining handler groups + the dispatcher wiring). Ported the file-management
group (`doc_move_file` / `doc_copy_file` / `doc_delete_file` / `doc_create_folder`
/ `doc_delete_folder` / `doc_move_folder`, over the `db::database_store`
primitives; the `chat_documents` move-sync is a corpus-verified no-op seam), the
document-UI group (`doc_open_document` / `doc_close_document` / `doc_focus`, with
three new `chat_documents` scoped ops and the `documentMode` chat update that
does not bump `updatedAt`), the blob group (`doc_write_blob` / `doc_read_blob` /
`doc_list_blobs` / `doc_delete_blob`, over the newly-ported `linkBlobContent`
binary storage primitive + blob-repo methods; the WebP transcode is a native
passthrough seam), and the enumeration group (`doc_grep` / `doc_list_files`, over
a new `doc_mount_documents` finder + `list_database_files`). Wired all 23
non-photo `doc_*` tools into `BuiltInToolRunner` (one `run_doc_edit` dispatch
through `execute_doc_edit_tool` inside a both-connections write closure) and
extended `tool_dispatch_equivalence` with two doc-edit dispatch rows. Verified by
four new jest-real-DB differentials (`doc_fm` 20 ops, `doc_ui` 9, `doc_blob` 11,
`doc_enum` 14) driving v4's REAL handlers byte-exact. The photo group stays a
tracked scoped deferral (unported images-v2 + `keep-image-markdown` +
`chunkAndInsertExtractedText`) — it routes to the loud fallback. With this the
entire doc-edit tool subsystem except the photo trio is ported and dispatched.

Phase 3 — wave 4 (W4.1d batch 3b): the doc-edit tool handlers (part 1 — the
foundation + the text/markdown handlers). Ported the database-backed
document-store primitives (`db::database_store`: read/write/move/delete
documents, folder create/delete/move, existence checks — composing the ported
storage leaves) plus the repo finders they need (`doc_mount_folders` /
`doc_mount_file_links` find-by-path/by-mount + a `LinkRow` join, and a
REAL-affinity coercion fix on `chunkCount`/`fileSizeBytes` that was silently
failing the access-control gates); the `tools::doc_edit::shared` access-control
family (cross-character vault visibility, `systemTransparency` opacity, the
`character_read`/`character_write` gates, the folder-protected-descendants
guard, the read/write resolution-context builders, `getAccessibleMountPoints`,
`resolveOfficialProjectMount`); and the first eight `doc_*` handlers
(`doc_read_file` / `doc_write_file` / `doc_str_replace` / `doc_insert_text` +
`doc_read_frontmatter` / `doc_update_frontmatter` / `doc_read_heading` /
`doc_update_heading`) behind a v4-faithful `executeDocEditTool` dispatcher. The
Librarian-announcement and reindex layers are documented no-op seams (mocked in
the oracle, as with the wave-3 whisper-posting seams). Added a `documentMode`
`ChatUpdate` setter. Verified by `doc_text_equivalence`, a jest-real-DB
differential driving v4's REAL `executeDocEditTool` + `formatDocEditResults` over
a 26-op corpus (read line/offset/JSON, self + project + qtap:// addressing,
blocked read + read-only write, str_replace unique/not-found/multiple/diacritics,
insert start/end/before, frontmatter read/keys/none/merge/replace, heading
read/not-found/update) plus a two-table dump. The remaining handler groups
(grep/list, file-management, document-UI, blob) follow; the photo group
(`keep_image`/`list_images`/`attach_image`) is a tracked scoped deferral — it
drags in the unported images-v2 store + `keep-image-markdown` sidecar builder +
`chunkAndInsertExtractedText`, beyond the named byte-source seam.

Phase 3 — wave 4 (W4.1f): the text-tool loop. Ported `runTextToolPass`
(`services::text_tool_loop`): the strategy-driven detect-text-markers →
execute → re-stream-continuation pass the orchestrator runs after the native
loop. The engine is strategy-agnostic behind a `TextToolStrategy` trait
(`hasMarkers`/`parse`/`strip`/`formatToolResult`/`stopSequences`); ships
`SimpleJsonStrategy` and `TextBlockStrategy` composed from the b.1 leaves, and
takes a provider-text-markers strategy as an injected seam (the provider plugin
detector/parser/stripper is W4.7). Reproduces the duplicate-cap nudge (byte-exact
synthetic user message), the iteration cap, the un-stripped-assistant-turn ledger,
the per-continuation reasoning-display-only path, the `usage`/`cacheUsage`/
`rawResponse`/`thoughtSignature` overwrite-on-done, and `assembleStrippedWithOffsets`
(strip once per segment, drop whitespace-only segments with offset carry, UTF-16
`\n\n`-join anchor math). Wired into the orchestrator spine after the native loop
(provider pass seam-gated on an injected strategy, then simple-json vs the
text-block fall-through per an injected `ResolvedToolMode`; the real tool-config
plumbing + tool slate is W4.1g) — corpus-dormant, `orchestrator_tier3` re-verified
green. Differential `text_tool_loop_tier3_equivalence` (nine case families,
DB-free — the pass writes nothing): simple-json single-iteration + text-block
multi-call over the REAL strategy functions, and a synthetic `<<T:name:args>>`
strategy (identical in TS + Rust) for multi-iteration, the duplicate nudge, the
parse-empty no-op, a mid-continuation stream failure, the iteration cap, empty-
stripped-segment assembly (surrogate-pair UTF-16), and stopSequences forwarding.

Phase 3 — wave 4 (W4.1d batch 4): the four search/introspection tool handlers,
each byte-exact against v4's REAL handler and wired into `BuiltInToolRunner`.

- `search` (`tools::search`): the Scriptorium unified search over memories (the
  ported `search_memories_semantic`), conversations (new `db::conversation_search`
  = v4 `searchConversationChunks`, a sibling of `document_search` over
  `conversation_chunks` BLOB embeddings), documents (`document_search`), and
  knowledge (the same document search narrowed per tier to `Knowledge/`), merged
  and ranked. Reproduces the per-source error-swallowing branches, the
  tier-ordered knowledge dedup (character > group > project > global, knowledge
  wins over document for a shared chunk), the `qtap://` URI tagging via
  `DocStoreUriResolver`, the operator/Brahma surface (memory forced off,
  operator-wide stores + conversations by userId), the 500-char content
  truncation, and the exact result-strings/labels (`(score*100).toFixed(0)%` via
  the ported `to_fixed`). Serves both the standard and Brahma tool definitions.
- `project_info` (`tools::project_info`): `get_info` (overview, roster, item
  counts, and the linked store summary via the new pure leaf
  `db::project_store_naming::pick_primary_project_store` = v4
  `pickPrimaryProjectStore`) and `get_instructions`, byte-exact including the
  no-project error.
- `help_search` (`tools::help_search` + new `db::help_search`): semantic search
  over `help_docs` embeddings with the automatic keyword fallback when embedding
  fails (the `extract_search_terms` keyword extractor added to `embedding_vector`).
  The `ensureHelpDocsSynced` disk index-build is a documented host seam (no-op once
  `help_docs` is populated); the tool path is a pure read.
- `request_full_context` (`tools::request_full_context`): flips the chat's
  `requestFullContextOnNextMessage` flag. Ported as a self-contained single-column
  `UPDATE` (byte-identical to v4's `repos.chats.update`, which does not bump
  `updatedAt`) so it needs no `db/chats.rs` change.
- Dispatcher: the runner now carries an injectable `ErasedEmbeddingProvider`
  (default a never-succeeds `NoEmbeddingProvider`) so `search`/`help_search` reach
  the embedding seam without a second generic on the shared struct; a real provider
  wires with W4.1g.
- New read helpers (all additive): `conversation_chunks::find_all_with_embeddings`,
  `help_docs::find_all`/`find_all_with_embeddings`, `doc_mount_blobs::count_by_mount_point`,
  `files::count_by_project_id`, `doc_mount_points::find_store_naming_by_id`.
- Differential `search_tools_equivalence` (24 cases across two jest real-DB oracles
  driving v4's REAL handlers, only `generateEmbeddingForUser` mocked to canned
  vectors, `Date.now()` frozen): each case on a fresh two-DB fixture copy (search
  bumps `lastAccessedAt`; request_full_context writes), comparing serialized result
  JSON + `format*` strings (float-safe) and, for request_full_context, the full
  `chats` row. `knowledge_injector` / `first_message_context` /
  `tool_execution_process_tier3` re-verified green (the `document_search` module was
  made public + a read added, no behavior change).

Phase 3 — wave 4 (W4.1d batch 5, part 4): the `generate_image` pure leaves
(`crate::image_gen`), ported leaf-first ahead of the stateful handler. Ported
`resolveOrientation` (v4 `lib/image-gen/orientation.ts`) — the pure `(provider,
model, orientation)` → concrete-request-mutation mapping (`matchModel` exact +
longest-prefix, `realize` strategy-honouring + degrade-to-hint, the host fallback),
with the plugin-registry declarations (`getImageGenerationModels` /
`getImageProviderConstraints`) passed in as data — and `parsePlaceholders`
(`prompt-expansion.ts`, the `{{name}}` scanner, name `.trim()`-ed). Differential
`image_gen_leaves_equivalence` (tier-1, DB-free) drives v4's REAL functions (the
registry jest-mocked to canned declarations) and diffs `JSON.stringify`.
**Scoped deferral:** the full `executeImageGenerationTool` handler +
`saveGeneratedImage` persistence — they compose the image-provider call + WebP +
Lantern store/notification (host seams), the entire W4.2 dangerous-content
classify/route path (with a double profile reroute), and three cheap-LLM tasks
(`craftImagePrompt` / `resolveCharacterAppearances` / `sanitizeAppearance`),
several themselves large unported units; the handler lands once those exist.

Phase 3 — wave 4 (W4.1d batch 5, part 3): the `search_web` tool handler
(`tools::web_search`), byte-exact against v4's REAL handler. The whole search
boundary (the plugin `searchProviderRegistry` + API-key lookup + Serper fallback)
is the injected `WebSearchProvider` seam (canned outcome both sides); the portable
half is the input validation, the outcome → output mapping (byte-exact error
strings for the not-configured / missing-key / provider-failure branches), and the
built-in result formatter (a `publishedDate` renders via a UTC-pinned
`toLocaleDateString()` added to `format_time`). Wired into `BuiltInToolRunner` with
a default `NotConfiguredWebSearch` provider (faithful to a no-search-plugin
instance — v4's "not configured" error; a real provider is host-wired). Differential
`web_search_tool_equivalence` (DB-free, jest-mocked registry) diffs the serialized
output + `format_web_search_results` over success/failure/missing-key/not-configured/
validation cases. Deferrals: the provider's own `formatResults`, host-side API-key
acquisition, and a date-only `publishedDate` (the corpus uses full-ISO dates).

Phase 3 — wave 4 (W4.1d batch 5, part 2): the Post Office (`send_mail` /
`list_email`) + `ask_carina` tool handlers, byte-exact against v4's REAL handlers.

- New `post_office` module (v4 `lib/post-office/`): the mailbox storage layer
  (`mailbox` — slugify/compose/parse/reply-preface + `deliver_letter` /
  `read_letter` / `list_mailbox`), the shared delivery service (`deliver` —
  `compose_and_deliver_letter` / `resolve_reply_in_sender_mailbox`), and the
  agent-facing instruction snippets (`instructions`). All over the ported vault
  primitives (`write_database_document` / `ensure_character_vault` / the
  `Mail/` folder conventions); the delivery `sentAt` is injected so it can be
  pinned. Plus `db::character_resolver` (`resolve_character_by_name_or_id`) and
  `format_time` (the UTC-pinned `formatDateTime` — v4's system-timezone
  `toLocaleDateString`, reproduced in UTC for the differential).
- `send_mail` / `list_email` (`tools::send_mail` / `tools::list_email`) compose
  those over both writer connections; wired into `BuiltInToolRunner`.
- `ask_carina` (`tools::ask_carina`) over the existing `RunCarinaQuery` +
  `PostProsperoCarinaError` seams from `services::carina_runner`. The handler +
  differential are complete; its dispatch stays on the loud fallback until the
  W4.5 Carina query engine is orchestrator-injected as the seam (the `onPosted`
  / `emitCarinaAnswer` slot is the documented tool-context deferral).
- Differential `mail_carina_tools_equivalence`: the mail half (real-DB, delivery
  clock pinned) drives v4's REAL handlers over a fresh two-DB fixture copy per
  scenario — diffing the serialized output + `format*` and reading the delivered
  letter's content back byte-for-byte (send-then-list round-trip, reply preface,
  every validation/refusal path, empty + single + plural listings); the carina
  half (DB-free) injects canned seams and diffs output + `format*` + the recorded
  Prospero args. Deferrals: the Suparṇā mail-check helpers
  (`collect_unalerted_mail` / `mark_alerted`).

Phase 3 — wave 4 (W4.1d batch 5, part 1): the `state` + `run_sql` tool handlers,
each byte-exact against v4's REAL handler and wired into `BuiltInToolRunner`.

- `state` (`tools::state`): persistent per-chat / per-project key-value state.
  Ported `parsePath` (dot notation + array indexing), `getAtPath` (undefined vs
  stored-null distinguished), `setAtPath` (intermediate object/array creation),
  `deleteAtPath` (object delete + array splice), and the `mergeState` spread
  (chat overrides project). Chat writes go through `chats.update({state})` (no
  `updatedAt` mint); project writes route to the store-backed `state.json`
  overlay. The output serializes in a fixed field order that reproduces every
  per-branch `JSON.stringify` (undefined dropped, null kept), and
  `formatStateResults` matches byte-for-byte.
- `run_sql` (`tools::run_sql`, Brahma Console read-only SQL): the read-only guard
  ported faithfully (the literal/comment-stripping pre-scan + forbidden-keyword +
  single-statement + mutating-PRAGMA checks, then rusqlite `Statement::readonly`
  fail-closed, then the `max_rows` cap). BLOB cells sanitize to `<blob: N bytes>`;
  REAL cells render via `js_number_to_json`. SQLite prepare/exec error strings are
  byte-identical (same SQLite3MC engine). The `operatorSurface` gate is a
  dispatcher guard. Zod-validation-message fidelity is limited to the non-object
  case (documented; the pre-scan/prepare failures cover the real refusals).
- Differential `state_sql_tools_equivalence` (34 cases, one jest real-DB oracle
  driving v4's REAL handlers over a fresh three-DB fixture copy per case): state
  cases diff the serialized output + `formatStateResults` + the `chats` table dump
  (zero normalization — no `updatedAt` mint) + a project-`state` read-back (the
  overlay bytes are already proven by `projects_tier2`); run_sql cases diff the
  serialized envelope, covering each target DB, blob sanitize, truncation, and
  every refusal path.

Phase 3 — wave 4 (W4.1d batch 3a): the doc-edit foundation, part 3 — the path
resolver + URI producers (completing batch 3a). Ported `resolveDocEditPath`
(`doc_edit::path_resolver`) — the `document_store` scope (over the tiered mount
pool: the SELF token, name-vs-id mount matching, ambiguity/not-found/disabled
errors, traversal/absolute/missing-path guards) and the `project` scope's
official-mount alias — with byte-exact `PathResolutionError` codes + messages, plus
`resolveSelfVaultMountPointId` / `resolveMountPointRef`. The legacy on-disk
branches (`filesystem`/`obsidian` real paths, the project legacy fallback, the
whole `general` scope) are a **host-filesystem seam** deferred to the Phase-4 host.
Ported the async URI producers (`doc_edit::uri_producers`: `docStoreUriFor`,
`uriForResolvedPath`, `buildDocStoreUriResolver`) over the ported qtap producers +
`doc_mount_points::{count_by_name, find_enabled}`. Verified by a 23-case
read-differential (`doc_edit_path_resolver_equivalence`) driving v4's REAL resolver
+ producers over a two-DB fixture (a character + vault, a real project with a
provisioned official store, P-linked stores incl. a duplicate-named pair + a
disabled store, the General singleton); every store database-backed so the FS seam
is never hit. Added `projects::find_official_mount_point_id_raw`. With this the
whole doc-edit foundation (batch 3a) is complete; the ~26 `doc_*` tool handlers
(batch 3b) sit on it.

Phase 3 — wave 4 (W4.1d batch 3a): the doc-edit foundation, part 2 — the pure
leaves. Ported `lib/doc-edit/{diacritics, mime-registry, unified-diff,
markdown-parser}.ts` into `doc_edit::{diacritics, mime_registry, unified_diff,
markdown_parser}`, each verified by one grouped tier-1 differential
(`doc_edit_leaves_equivalence`, 81 rows) against v4's REAL exports. Diacritics:
NFD normalize + strip-combining (via `unicode-normalization`, proven byte-exact on
precomposed/decomposed Latin + Hangul) and the `findAllMatches`/`findUniqueMatch`
UTF-16 index/length remap. MIME registry: `detectMimeFromExtension`, the `isJson*`
predicates, and `parseContent`/`serializeContent`/`validateJson` (JSON +
JSONL) — the happy-path bytes byte-exact (`serde_json` pretty ==
`JSON.stringify(x, null, 2)`), with the V8 `JSON.parse` error TEXT a documented
normalized seam (structure/values/line-numbers compared exactly, failure messages
normalized). Unified diff: the hand-rolled greedy look-ahead algorithm reproduced
exactly (git-style `@@` hunks), not "a" diff. Markdown: `slugifyHeading` (ASCII
`\w` + JS `\s`), `parseHeadingTree` (ATX headings, code-fence exclusion, duplicate-
slug counter suffixes, UTF-16 offsets), `findHeadingSection` (byte-exact thrown
messages), `readHeadingContent`/`replaceHeadingContent`, and
`serializeFrontmatter`/`updateFrontmatterInContent` — the latter reusing the
already-ported eemeli scalar emitter so `YAML.stringify` is byte-exact over the
frontmatter value space (string/bool/number/null scalars + flat sequences; nested
maps/exotic numbers a documented seam). `document-policy.ts` needed no new port
(its leaves already live in `db::doc_mount_file_links`). The DB-backed path
resolver + URI producers follow.

Phase 3 — wave 4 (W4.1d batch 3a): the doc-edit foundation, part 1 — the tiered
mount pool + the `qtap://` URI codec. Ported `resolveTieredMountPool` /
`classifyMountTier` / `flattenTierPool` and hoisted the canonical
`dedupeTierTriple` into `db::tiered_mount_pool` (v4's
`lib/mount-index/tiered-mount-pool.ts` — its true home), refactoring the
knowledge injector to consume the dedup from there (its differential re-verified
green). The five-tier character/participant/group/project/global resolution
reproduces the ownership gate (fails closed without `userId`), the pre-resolved
character-mount fast path, the per-RESPONDING-character group tier, graceful
global-null, per-tier error swallowing, and the character>group>project>global
dedup — verified by a 9-case read-differential (`tiered_mount_pool_equivalence`)
against v4's REAL resolver over a two-DB fixture (2 characters + vaults, a group
with an official + linked store + membership, a project with colliding links, the
General singleton). Ported the full `qtap://` URI codec (`doc_edit::qtap_uri`,
v4's `qtap-uri.ts`) — `parseQtapUri` / `formatQtapUri` / `isQtapUri` /
`qtapUriToResolverInput` / `QtapUriError` + the producer helpers — unifying it
with the producers previously hoisted into the knowledge injector (now re-exported
from the canonical home). Reproduces JS `encodeURIComponent` /
`decodeURIComponent` exactly (a V8-faithful `Decode` with UTF-8 run validation),
the last-`:` fragment split, BAD_LEVEL bounds, the encoded-slash segment, and the
insertion-ordered query map; verified by a 54-row tier-1 differential
(`qtap_uri_equivalence`) incl. malformed-percent-encoding + non-ASCII round-trips.
Added the scoped mount-point reads the resolver needs
(`doc_mount_points::{find_by_id_for_docedit, find_enabled_for_docedit,
count_by_name}`, `groups::find_official_mount_point_id_raw`). Remaining batch-3a
foundation (diacritics, MIME registry, unified diff, markdown heading/frontmatter
ops, path resolver, URI producers) follows.

Phase 3 — wave 4 (W4.1e): the native tool loop + the finalizer response-RNG.
Ported `runNativeToolLoop` (`services::native_tool_loop`): the bounded
stream → detect → execute → thread → re-stream loop after the primary stream,
including the agent-mode `submit_final_response` accept (siblings-first,
replace-vs-preserve, ghost-wrap reject), the output-token truncation guard, and
the max-turns force-final pass. Two injected seams: a `ToolCallDetector` (the
provider wire parse is W4.7) and the frozen `ToolRunner` (W4.1d). Added the
partial `services::agent_mode` (the pure helpers the loop consumes; the resolver
cascade is W4.4), the `ChatUpdate.agentTurnCount` setter (the loop's only DB
write), a public `StreamingState::next_turn_seq`, and `jsstr::js_index_of`
(UTF-16). Wired into the orchestrator spine at v4's composition point
(corpus-dormant until `buildTools`, W4.1g). Closed the finalizer's assistant-
response RNG seam: the ported detector + executor now run inline (the
`auto-detect-response` TOOL-row shape with a UTF-16 `anchorOffset`), only the
CSPRNG byte source injected; the orchestrator shares one `rng_bytes` across the
user-message and assistant-response auto-detect (dropping the `finalizer_rng`
generic). Differentials: `native_tool_loop_tier3_equivalence` (seven case
families, a three-boundary mock split) and the extended
`message_finalizer_tier3_equivalence` (RNG fire + no-fire; its oracle un-stubs
detection and mocks `crypto.randomBytes`); `orchestrator_tier3` re-verified green.

Phase 3 — wave 4 (W4.1d batch 1): the first tool-handler batch. Ported the nine
immediately-portable tools (every underlying repo already ported, no model
calls) plus the real dispatching `ToolRunner` that batches 2–5 will extend. Each
handler ships a differential driving v4's real handler byte-exact.

- Handlers (`tools::{read_conversation, annotations, terminal, whisper, help,
  self_inventory}`): `read_conversation`, `upsert_annotation`/`delete_annotation`
  (over the ported `conversation_annotations` repo, extended with the find/delete
  readers + the ported `scriptorium::{merge,strip}_annotations` leaves),
  `terminal_read`/`terminal_list` (over `terminal_sessions` reads + the ported
  `terminal_clean::clean_terminal_output`; the live-PTY/transcript scrollback is
  an injected seam), `whisper` (resolves the target by name/alias among
  whisper-receivable participants, writes one `chat_messages` row — no post-office
  side effect), `help_settings`/`help_navigate`/`submit_final_response` (the
  first two + the pure agent-mode validator; `help_settings` needed the full
  `chat_settings::find_by_user_id` read marshaling, now ported), and the big
  `self_inventory` (the ten-section introspection report over ~a dozen repo
  readers + `build_system_prompt`; the runtime-mode/client-shell/release-notes/
  changelog/mount-index-degraded host bits are an injected `SelfInventoryEnv`
  seam — `quilltap.version` covered, releaseNotes/changelog deferred).
- `LoadedMemoriesContext` is now typed (`{ semantic, interCharacter, recap }`) —
  its consumer `self_inventory` landed.
- The dispatching runner (`tools::executor::BuiltInToolRunner`): routes a tool
  call by name to the ported handlers (reproducing v4
  `executeToolCallWithContext`'s built-in dispatch rows — the `{ formattedText,
  … }` result shape, the failure `null`/`error` mapping, the dispatcher-side
  guards + annotation character-name resolution), with an injected inner
  `ToolRunner` fallback for unported names (the loud default reproduces v4's
  `Unknown tool: <name>` for names v4 doesn't know, and a "recognized but not yet
  available" failure naming a not-yet-ported built-in). Plugin-vs-built-in
  routing precedence is a documented deferral (the plugin registry is unported).
- New leaf modules: `scriptorium`, `terminal_clean`, `folder_utils`;
  `format_scoped_uri` added to `knowledge_injector::qtap_uri`.
- Differentials: per-handler tsx/jest-real-DB oracles (success / invalid-input /
  edge per tool) + an end-to-end dispatcher differential driving v4's real
  `executeToolCallWithContext` over a mixed batch (read, two writes with
  character-name resolution, a pure tool, a handler failure, an invalid-input
  failure). The unknown-tool loud fallback is unit-tested (v4's genuine unknown
  path depends on the unported plugin registry). Existing `tool_execution_*` +
  `message_finalizer` + `orchestrator` differentials re-verified green.

Phase 3 — wave 4 (W4.1d batch 2): the seven wardrobe tool handlers. Ported
`wardrobe_list` / `wardrobe_read` / `wardrobe_create` / `wardrobe_update` /
`wardrobe_archive` / `wardrobe_wear` / `wardrobe_take_off` (`tools::wardrobe_*`)
over the already-ported vault-public CRUD, the public read trio + shared-archetype
tier, and the equipped-outfit ops, plus the pure `crate::wardrobe` leaves
(`unionTypes`, `describeOutfit`, `expandComposites`, the flag-driven equip
primitives, `describeWardrobeEffect`, sentinel normalization), the DB-touching
`tools::wardrobe_shared` helpers (across-tier item resolution, the persisted equip
primitives, `resolveEquippedOutfitForCharacter`, the coverage summary,
`resolveProjectMountPointIdsForChat`), and `find_by_ids_for_character`. Extended
`BuiltInToolRunner` with the seven dispatch rows (each runs inside a single writer
closure holding both the main + mount-index connections). The
`pendingWardrobeAnnouncements` field became `Arc<Mutex<HashSet<String>>>` so the
handlers can record an announcement through the immutable `ToolRunner::run`
boundary without changing the trait signature; the end-of-turn drain stays a
documented deferral. Avatar generation on equip is an image-subsystem seam (out of
scope; gated off in the corpus). Differentials: `wardrobe_tools_equivalence` (a
25-op sequence — success / invalid / edge per handler, gift, composite+equip,
shared read-only, slot mismatch, plus a read-back of both wardrobes / archetypes /
equipped outfit, minted ids/timestamps positionally normalized) drives v4's REAL
handlers; the dispatcher differential gained a `wardrobe_list` call; the existing
`tool_execution_*` + `tool_dispatch` differentials re-verified green.

Phase 3 — wave 4 (W4.1c): tool execution + persistence primitives
(`services::tool_execution`, v4 `tool-execution.service.ts`) — the harness and
the TOOL-row writer between the tool loops (W4.1e/f) and the tool handlers
(W4.1d).

- `save_tool_messages` + `compute_tool_message_targets` + `files::add_tag`:
  the TOOL-row persistence primitive (one `type:'message'`/`role:'TOOL'` row per
  tool message through the ported `chats_messages::add_message` path) with the
  whisper gate (ALWAYS_PRIVATE tools + VAULT_READ tools vs
  `allowCrossCharacterVaultReads`, whispered to the **user participant**) and the
  generated-image link+tag loop; the generic content JSON in v4 field order.
  Tier-2 differential (`tool_execution_tier2_equivalence`) driving v4's real
  `saveToolMessages` over the whisper matrix, content omission (anchorOffset/seq/
  callId + metadata), the multi-message batch + `firstToolMessageId`, and the
  image link+tag — byte-exact across `chat_messages`/`chats`/`files`.
- `process_tool_calls` + the injected `ToolRunner` boundary +
  `ToolExecutionContext`: the per-call dispatch harness (detection frame,
  per-tool `tool_executing` status, tool-result frame, generated-image
  extraction, the failure `ToolMessage` shape). `chat_events` gains the additive
  `toolsDetected` + `toolResult` frames. Tier-3 differential
  (`tool_execution_process_tier3_equivalence`) driving v4's real
  `processToolCalls` with only `executeToolCallWithContext` mocked — ordered
  frames + `toolMessages` + `generatedImagePaths` matched.
- Spine wiring: `save_tool_messages` wired into the finalizer's
  `toolMessages.length > 0` gate (inside `save_assistant_message`, before the
  assistant image-link loop, so a generated image's `linkedTo` order matches v4),
  and the orchestrator tool-only terminal branch (`saveToolMessages` + `updatedAt`
  bump + the `toolsExecuted: true` done frame). Fixed the finalizer done frame's
  `toolsExecuted` (was hardcoded `false`; now `toolMessages.length > 0`) — caught
  by the finalizer direct-drive. `message_finalizer_tier3_equivalence` gained a
  `tool-save` case driving v4's real finalizer with an injected tool slate;
  `orchestrator_tier3_equivalence` re-verified green (branches corpus-dormant
  until the tool loops).
- The canonical `ToolMessage` now lives once in `services::tool_execution`;
  `services::tool_call_threading` reuses it (its narrow subset removed), matching
  v4's single `chat-message/types.ts` definition. Threading differential
  re-verified.

Phase 3 — wave 4 (W4.1b): the tool-subsystem pure leaves. The pure foundations
the tool loops, executor, and handler catalog will consume — all tier-1 exact
against v4's real `lib/tools/` + service code.

- Tool-call threading (`services::tool_call_threading`, v4
  `tool-call-threading.ts`): `build_assistant_tool_call_message` /
  `build_tool_result_messages` — the callId-present-vs-absent pairing rule,
  empty/whitespace-prose collapse, reasoning/thoughtSignature forwarding, and the
  `[Tool Result: <name>]` text fallback. Tier-1 differential
  (`tool_call_threading_equivalence`, 22 cases).
- Pseudo-tool machinery (`tools::{simple_json_parser, text_block_parser,
  simple_json_prompt, text_block_prompt, native_tool_prompt, pseudo_tool_support}`
  + `services::pseudo_tool`): the three-tier simple-json parser, the text-block
  parser/converter, both prompt builders, the native-tool prompt, mode
  resolution, and the service wrappers. The two backreference regexes are
  hand-rolled (`regex` crate has no backreferences); the `jsonrepair` tier is a
  bounded hand-rolled subset (single/smart quotes, unquoted keys, trailing
  commas) that resolves conservatively (tier-fail → `[]`) outside its documented
  scope, corpus-pinned on both sides of the boundary. Tier-1 differentials
  (`pseudo_tool_parsers_equivalence`, 138 cases; `pseudo_tool_prompts_equivalence`,
  40 cases) driving v4's real exports.
- Tool-definition catalog (`tools::definitions`, all 57 definitions from the 56
  `*-tool.ts` files): byte-exact static JSON transcribed from v4's
  `JSON.stringify` output (not by re-implementing the Zod→JSON-Schema emitter),
  generated by a checked-in script. Byte-exact differential
  (`tool_definitions_equivalence`) proving the serde round-trip reproduces JS
  `JSON.stringify`, catalog completeness, and a `canonicalize_universal_tools`
  spot-check over the full real catalog.

Phase 3 — wave 4 (W4.1a): the RNG subsystem. v4's pre-message RNG auto-detect
path — scan the user message for dice/coin/bottle patterns, execute them, write
TOOL messages into the chat before the model turn — is ported and verified end
to end, closing the orchestrator's `user_message_rng` seam.

- `rng_patterns` (pure): v4's `rng-pattern-detector.service` —
  `detect_rng_patterns` / `convert_patterns_to_tool_calls` /
  `detect_and_convert_rng_patterns`. The three regexes reproduce JS fidelity:
  ASCII `\b`/`\d` via `(?-u:\b)`/`[0-9]`, the JS-`.` line-terminator exclusion,
  the "flip a coin" 1–3-char quirk (so "flip the coin" does NOT match), and the
  spin-bottle `{0,50}` bound. Tier-1 differential (`rng_patterns_equivalence`, 54
  cases) driving v4's real exports over both the detected patterns and the
  converted tool calls, incl. bounds rejections, non-ASCII adjacency, and a ReDoS
  adversarial string.
- `tools::rng` (executor): v4's `rng-handler` — `execute_rng_tool` /
  `secure_random_int` (rejection sampling) / `roll_dice` / `flip_coin` /
  `spin_the_bottle` / `format_rng_results` + the Zod input validation. The
  randomness source is an injected `RandomBytes` byte stream (production
  `OsRandomBytes`; the differential replays a committed sequence), so
  `secureRandomInt`'s variable-length byte consumption is itself part of what the
  diff proves. `RngType` serializes back to v4's number-or-string union.
  Differential (`rng_executor_equivalence`, 14 cases) drives v4's real
  `executeRngTool` against a real fixture DB (spin resolves participant names
  through the repos) with `crypto.randomBytes` pinned, diffing the output + the
  formatted string + asserting byte-exact stream consumption.
- Orchestrator seam closed: the ported detector + executor run inline in
  `process_message`, writing a TOOL message per detected pattern (byte-identical
  content JSON in v4's field order) and appending it to the context so the model
  turn sees the results. The byte source is injected via
  `OrchestratorDeps::rng_bytes`. The `user_message_rng` seam method was removed.
  The tier-3 corpus gained three cases (`rng_dice`, `rng_two_patterns`,
  `rng_no_fire`) and `autoDetectRng` was flipped on globally (a per-user setting;
  existing content carries no patterns, so they no-op); the whole
  `orchestrator_tier3_equivalence` corpus re-verified green.

Phase 3 — wave 4 (W4.0): the wardrobe drift batch. The public wardrobe READ
trio, the General/project shared-archetype tier, and the wardrobe transfers
service are all ported and verified — closing the 2026-07-03 drift-check's
wardrobe surfaces and the long-deferred archetype tier.

- `db::instance_settings`: the per-instance key/value store (main db);
  `get_general_mount_point_id` resolves the provisioned "Quilltap General" store
  id, tolerating a missing table like v4's `readSetting`. Unit tests.
- Archetype seeding generalized into the read overlay:
  `read_character_vault_wardrobe` gained `seed_archetypes` + an injected archetype
  fetch, and `resolve_and_check_component_items` moved from index-valued to
  `SeedArchetype`-seeded maps (v4's local-wins gap-fill) so a composite can
  reference a shared archetype it doesn't hold. Backward-compat: the existing
  `vault_wardrobe_read` / `vault_wardrobe_public` differentials stay green (empty
  seed = no-op), plus two new resolver unit tests bank real seeding + an
  archetype-routed cycle.
- `db::archetype_wardrobe`: `read_general_wardrobe` / `read_project_wardrobe`,
  the `find_archetypes` insertion-ordered General-under-project merge, and
  `find_archetype_by_id`.
- Public READ trio (`db::wardrobe_read::find_by_character_id` /
  `find_by_id_for_character`) — vault-aware reads over the seeded overlay.
  `findByCharacterIdRaw` is a tracked deferral (deprecated; reads the pre-cutover
  `wardrobe_items` table the vault era drops; no W4.0 consumer). Verified by a
  read-differential (`wardrobe_public_read_equivalence`) against v4's REAL repo:
  five cases where a character composite references a General archetype by slug
  AND UUID (both resolve only via seeding) plus the archetype fallback.
- Public WRITE generalized to a `WardrobeLocation` (character/General/project)
  with `create/update/delete_project_wardrobe_item` and General archetypes seeded
  into the cycle-peer check; a `null` characterId now resolves to Quilltap
  General instead of erroring. Re-verified green.
- `services::wardrobe_transfers`: v4's `/api/v1/wardrobe/transfers` POST
  (move/copy across the four tiers) + GET destination enumeration, composed over
  the ported repo ops + `ensure_official_store`. Verified by a tier-2 differential
  (`wardrobe_transfers_tier2_equivalence`) driving v4's REAL POST handler under a
  jest-real-DB oracle (session mocked, real encrypted DB) over five scenarios
  (copy→general, move→project, copy→character, same-location reject, id-collision
  reject), diffing the outcome + seven mount-index tables in the
  shared-cross-db-id-map remap form. The normalizer assigns `fileId` tokens by the
  `file_links` walk (store+path stable — a copy's minted-timestamp `.md` perturbs
  the content-addressed sha) and pins `chunkCount` before sorting.

Docs — Phase 2 marked complete; Phase 3 kickoff drafted. Docs only, no crate
source changed.

- `overview.md`: the Phase-2 roadmap row now reads repo-inventory-complete (every
  v4 repository round-trips green through the tier-2 harness), with the residual
  Phase-3-coupled deferrals named; the stale "nineteen repos" status prose was
  corrected the same way, and the document list + Phase-3 row now point at the new
  kickoff doc.
- `phase-2-onramp.md`: deferred seam #4 (`write_apply`'s `__finalizeFile` +
  post-commit effects) flipped from open to resolved, matching the
  ported-and-verified state.
- Added `docs/developer/porting/phase-3.md` — the Phase-3 kickoff: the tier-3
  mocked-LLM tier; the writer-task runtime (Unit 0); the tier-3 harness scaffold
  (Unit 0.5); the memory gate as first service (Unit 1), with a caution to port
  its similarity-band constants (0.90 / 0.85 / 0.70), not the file's stale
  0.80/0.70 doc comment; and the three Phase-2-carried deferrals.

Phase 2 on-ramp — the tier-2 DB-state oracle (structural DB diff for repo/service
ops), built as a thin vertical slice over the `folders` repo:

- Oracle harness (TypeScript, drives v4's real `lib/`): a committed plaintext
  fixture spec (`harness/oracle/fixtures/folders-tier2.json`) under a throwaway
  test pepper; a fixture builder that materializes a fresh ChaCha20 DB at test
  time via v4's own `ensureCollection` + `FoldersRepository.create`; and the
  `folders-tier2` case that copies the fixture, runs a fixed create + update
  through the real repo, and emits the canonical post-op `folders` dump as NDJSON.
- Canonical dump shaping (`harness/oracle/lib/tier2.ts`): columns in on-disk
  order, rows sorted by a stable key, BLOBs as hex, nulls explicit.
- Determinism: ids and timestamps pinned on both sides (CreateOptions on create,
  explicit `updatedAt` on update), so the dump needs zero normalization — the
  strongest tier-2 form. The id-remap / timestamp-placeholder fallbacks are
  reserved for later repos that cannot take injected ids/clocks.
- Rust DB layer (`quilltap-core::db`): the writable cipher-correct open (key
  pragma first, then `foreign_keys = ON` + `journal_mode = TRUNCATE`), the
  single-writer `Writer` that solely holds the RW connection, the `folders`
  repo's `create` + `update` ported from v4, and a canonical `dump_table_json`
  matching the oracle's shape.
- Build: the SQLite3MultipleCiphers amalgamation build (`build.rs` + `vendor/`)
  moved from the probe into `quilltap-core`, which now links the ChaCha20/sqleet
  library for the whole workspace; the workspace `rusqlite` dependency switched
  off `bundled-sqlcipher` to the amalgamation (`buildtime_bindgen`). The
  throwaway `sqlcipher-probe` / `sqlite3mc-probe` crates are retired.
- Harness: tier-2 differential test `folders_tier2_equivalence` — copies the
  same seed fixture, runs the Rust ops, structural-diffs the dump against the
  oracle NDJSON (`QT_ORACLE_FOLDERS` + `QT_FIXTURE_FOLDERS`, skip-if-unset).
  The `folders` repo round-trips green.

Phase 2 — the `chats` repo, sub-unit 1: slim-row marshaling
(`quilltap-core::db::chats`). The first cut of the last and largest repo (v4's
`ChatsRepository`, a `TaggableBaseRepository`). Ports `create` / `update` /
`delete` over the **~96-column** `chats` table (MAIN db) — the widest marshaling
surface in Phase 2. Banks: the typed `participants` **array-of-objects JSON
column** (`ChatParticipant`, 18 fields in schema order, nullable optionals
`skip_serializing_if`, `displayOrder` an `i64`, `talkativeness` rendered the JS
way so an integer-valued `1.0` → `1`; the schema `.refine()` requires ≥1
participant); the simple JSON-array columns; the **plain-string** `turnQueue` /
`spokenThisCycleParticipantIds` columns (which hold JSON text `'[]'` but are
`z.string()`, bound raw); the number-affinity columns (all bound `f64`);
booleans; enum TEXT; and the long tail of nullable strings/uuids/timestamps. Two
invariants banked: `update` **never mints `updatedAt`** (it preserves the
existing value unless the caller passes one — only a new message bumps it), so
the whole differential is the pinned zero-normalization form; and on SQLite
`create` writes nothing to `chat_messages`. Verified by a tier-2 differential
(`chats_tier2_equivalence`) driving v4's REAL `ChatsRepository` over a
create×3 / update×3 (both the preserved- and explicit-`updatedAt` branches) /
delete sequence, diffing the `chats` dump byte-for-byte. **Tracked deferrals:**
`delete`'s participant-vault summary sweep (external subsystem), the open-JSON
object columns' multi-key insertion order (constrained to `{}`/single-key/null),
and the rest of the repo (messages, participants, impersonation, tokens, search,
outfits, read queries) — the remaining sub-units.

The `chats` repo — sub-unit 2: the **slim-row read path** (`db::chats_read`,
`chats_read_equivalence`). Ports the read marshaling (the inverse of sub-unit 1's
~96-column write = v4 `_findById` = hydrateRow + Zod parse) + the `findBy*`
queries (`findById` / `findAll` / `findByUserId` / `findByCharacterId` /
`findByType` / `findRecentSummarizedByCharacter`). The marshaling reproduces v4's
net read shape: nullable-optional columns OMITTED when `NULL` (v4 `undefined`
dropped by `JSON.stringify`), `.default(...)` numbers/bools/enums/arrays + `state`
(`{}`) materialized, numbers rendered the JS way, and `participants` re-parsed
per-element so each participant's own defaults materialize (`controlledBy: 'llm'`,
`displayOrder: 0`, `isActive: true`, `status: 'active'`, `hasHistoryAccess:
false`) and its nullable-optionals drop. `findByCharacterId` /
`findRecentSummarizedByCharacter` use the nested `participants.characterId`
`json_each` + `json_extract` match v4's query translator emits; the latter
reproduces the `$exists`/`$nin`/`$ne` → `IS NOT NULL` / `NOT IN` / `!=` filter +
`ORDER BY "lastMessageAt" DESC` + `LIMIT`. Verified by a read-differential: both
sides READ a copy of one fixture baked by v4's REAL `repos.chats.create` (seven
chats — a rich chat exercising every marshaling branch, a minimal chat, salon /
help / brahma types, summarized chats with distinct `lastMessageAt`), running 16
queries compared exactly (no normalization — nothing mutated).

The `chats` repo — sub-unit 3: the **`chat_messages` read path**
(`db::chats_messages_read`, `chats_messages_read_equivalence`). Ports v4's
`ChatMessagesOps` read surface — `getMessages` / `getMessageCount` /
`findChatIdForMessage`. Messages live in their own MAIN-db `chat_messages` table
(one row per event); `getMessages` reads every row for a chat ordered by
`createdAt` and validates each through `ChatEventSchema`, a three-member union
(`MessageEvent` / `ContextSummaryEvent` / `SystemEvent`). The marshaling
dispatches on the `type` discriminator and reconstructs each member: required
columns read directly, nullable-optional columns OMITTED when `NULL`, and the
array/object JSON columns (`rawResponse` [`z.record`], `attachments`,
`reasoningSegments`, `dangerFlags`, `hostEvent`, `customAnnouncer`, `carinaMeta`,
`pendingExternalAttachments`, `summaryAnchor`, …) parsed straight to JSON. No
read-side default materialization is needed: v4 runs `ChatEventSchema.parse`
*before* every insert, so each `.default(...)` (e.g. `attachments` → `[]`, a
`DangerFlag`'s `userOverridden` / `wasRerouted` → `false`) and the exact
int-vs-float number representation are already baked into the stored bytes.
Verified by a read-differential: both sides READ a copy of one fixture baked by
v4's REAL `repos.chats.addMessages` (one chat + twelve messages covering every
event member and JSON column), running 7 queries compared exactly (no
normalization). **Tracked seam:** `isSilentMessage` — its
`z.union([boolean, number.transform])` maps to TEXT affinity, so a stored boolean
round-trips as the string `"1"` and v4 drops the whole message on read; the
corpus keeps it absent and the column is not read here (close before reading real
data that sets it).

The `chats` repo — sub-unit 4a: the **`chat_messages` write path**
(`db::chats_messages`, `chats_messages_tier2_equivalence`). Ports v4's
`ChatMessagesOps.addMessage` / `addMessages` — the row insert plus the chat
metadata side-effect. The write marshaling is the inverse of sub-unit 3 but
harder: the port must reproduce `ChatEventSchema.parse`'s output bytes itself —
materialize each Zod `.default(...)` (`attachments` → `[]`, a `DangerFlag`'s
`userOverridden`/`wasRerouted` → `false`) and emit every JSON-column object in
schema field order (matching v4's `JSON.stringify` of a Zod-parsed object) with
integer-valued nested numbers rendered bare (`1`, not `1.0`), since the stored
bytes are compared directly. Each fixed-shape nested object (`dangerFlags`,
`reasoningSegments`, `hostEvent`, `customAnnouncer`, `carinaMeta`,
`summaryAnchor`, `pendingExternalAttachments`) is a typed struct in schema order;
the open-JSON `rawResponse` is corpus-constrained to `{}`/single-key (seam #5). A
`message` insert names the `MessageEvent` columns (always writing `attachments`);
a `context-summary`/`system` insert omits `attachments` so SQLite fills its
`DEFAULT '[]'` — matching v4's insert-only-validated-keys behavior. The metadata
side-effect recounts visible messages (`countVisibleMessages`), bumps
`lastMessageAt`/`updatedAt` to a minted `now` only for an actual `type:'message'`
event, and folds `spokenThisCycleParticipantIds` over the batch via the
already-ported `computeSpokenThisCycleAfterMessage`; it routes through the
sub-unit-1 `chats.update` (extended with `lastMessageAt` +
`spokenThisCycleParticipantIds` setters). Verified by a tier-2 differential
driving v4's REAL `addMessage`/`addMessages` over a kitchen-sink message (every
JSON column), a context-summary (non-actual: no `lastMessageAt` bump, `updatedAt`
preserved, count 0), and a mixed batch (whisper + system event + public message),
diffing BOTH the `chat_messages` and `chats` tables. `chat_messages` is pinned;
the `chats` `lastMessageAt`/`updatedAt` collapse to `<ts>` only when they differ
from the seed sentinel (so a preserved-sentinel `updatedAt` stays pinned and a
stray mint would be caught). The differential caught a real bug: serde's
`camelCase` rename produced `estimatedCostUsd`, dropping the schema's
`estimatedCostUSD` value — fixed with an explicit rename.

The `chats` repo — sub-unit 4b: the **`chat_messages` mutation path**
(`db::chats_messages`, `chats_messages_ops_tier2_equivalence`). Ports v4's
`updateMessage` / `deleteMessagesByIds` / `clearMessages`. `updateMessage`
reproduces v4's `{...existing, ...updates}` → `ChatEventSchema.parse` →
`$set: validated`: it reads the existing event (reusing the sub-unit-3 read),
overlays the update keys, re-validates into the typed `ChatEventInput`, and
DELETE + re-INSERTs the merged event — which yields the byte-identical row
(a validly-created row's non-member columns already sit at their DDL defaults, so
resetting them is a no-op) while reusing the 4a insert marshaling. A
freshly-added `dangerFlags` bakes its defaults; an untouched `reasoningSegments`
round-trips byte-for-byte; a context-summary's `attachments` stays at its
`DEFAULT '[]'`; a not-found id no-ops. `deleteMessagesByIds` deletes each
`(id, chatId)` row and, when any were removed, recounts `messageCount` (so
`update` preserves `updatedAt`); a nonexistent id removes nothing and leaves
metadata untouched. `clearMessages` deletes all of a chat's rows and resets
`messageCount`→0 + `lastMessageAt`→null (`updatedAt` preserved). Verified by a
tier-2 differential driving v4's REAL methods over a seed of three chats
pre-populated via `addMessages`, diffing BOTH the `chat_messages` and `chats`
tables with ZERO normalization — no 4b op mints a chat timestamp, so the seed's
baked timestamps are read identically by both sides.

The `chats` repo — sub-unit 5: the **participant ops** (`db::chats_participants`,
`chats_participants_tier2_equivalence`). Ports v4's `ChatParticipantsOps`:
`addParticipant` / `updateParticipant` / `removeParticipant` /
`setParticipantStatus` plus the four pure in-memory filters
(`getCharacter`/`getActive`/`getLLMControlled`/`getUserControlled`Participants).
Each mutator is a read-modify-write of the `participants` JSON column —
`findById` → mutate the array in memory (minting the participant's own
id/createdAt/updatedAt) → `update` the chat — and the chat's OWN `updatedAt` is
never bumped (v4 `_update` preserves it; the minted clock values live inside the
participants JSON). `addParticipant` validates through the participant schema
(materializing the Zod defaults, stripping unknown keys) and carries the
user-control side-effect (a `controlledBy: 'user'` participant is appended to
`impersonatingParticipantIds` and, when nobody is typing, set as
`activeTypingParticipantId`); `removeParticipant` carries the last-participant
guard (throws, leaving the chat unmutated). Banks the `removedAt` three-shape
seam: absent (never removed), the minted string (removed), and an explicit JSON
`null` (a `setParticipantStatus` to a non-removed status clears it) — which
forced widening `ChatParticipant.removedAt` to a double-`Option` with a
present-keeps-null deserializer (plain serde maps a stored `null` to the outer
`None`, dropping it; v4's Zod `.nullable().optional()` keeps it through a re-read
+ re-write). Tier-2 differential drives v4's REAL ops (with `setParticipantStatus`
reached via the private ops field — not on the repository surface) over four
seeded chats, diffing the `chats` table; participant ids (pinned seed + minted)
are remapped to first-appearance tokens across the three referencing cells, and
nested participant timestamps are sentinel-placeholdered (a value equal to the
seed sentinel stays pinned — proving createdAt preservation and no stray mint),
while chat-level timestamps are diffed exactly.

Phase-2 on-ramp — the real-snapshot fixture sanitizer (Deliverable B), a new
`quilltap-fixture-sanitizer` crate (library + `--source/--dest/--verify` CLI). It
takes a COPY of a real instance, recovers the pepper from the copy's `.dbkey` (in
memory only — never printed, logged, or written), sanitizes each database, and
re-keys the output under the committed throwaway test pepper. It is schema-frozen
by construction: the destination schema is replayed verbatim from the source's own
`sqlite_master`, every row is copied (row counts + the FK-id graph preserved), and
numbers / 0-1 booleans / enum tokens (by name + the `*Type`/`*Status`/`*Kind`/
`*Mode`/`*Role` suffixes) / timestamps / ids + UUID-valued TEXT are kept, while all
other TEXT is scrubbed to deterministic same-length pseudo-text, JSON columns are
deep-scrubbed to stay valid (keys / numbers / bools / uuid-and-enum leaves kept),
BLOBs become deterministic same-length bytes, and the document store's content↔sha
invariant is recomputed so a scrubbed file's `sha256` still matches its bytes.
Document-store PATH strings keep their structural skeleton (folder names + the
managed vault filenames like `properties.json`) so a sanitized vault still resolves,
scrubbing only the title stems. The scrub is one-way (`SHA-256(column ‖ original)`,
the original never appears in the output) and equality-preserving (identical
originals map identically, keeping content-dedup relationships). The binary refuses
a source path that looks like a live instance and never writes the `.dbkey`. Per the
project decision (2026-07-01) NO Friday-derived data is committed — the committed
test is synthetic (a re-key A→B round-trip proving the policy: structure preserved,
free text / JSON / BLOB scrubbed, content↔sha recomputed); real snapshots are
regenerated locally on demand. Verified locally against a copy of Friday: 188,031
main-db rows + 20,772 mount-index rows sanitized and re-keyed, 3,400 document-store
files re-hashed, and the sanitized output read back through the ported repos —
20,868 memories, 609 chats, and 33 characters (through the full vault overlay,
which resolves because the structural path segments are preserved) — marshaling
cleanly against real-shaped rows.

Phase-2 deferred-seam closure — ported the `characters` startup-backfill family,
closing the last three characters deferrals: the `ensureCharacterVault` adopt
branch, provision-on-the-fly, and physicalDescription-via-update. On a
managed-field `update` to a vault-less character, `apply_document_store_write_overlay`
now provisions a vault on the fly (build the post-cutover write input →
`ensure_character_vault` → re-read + confirm FK → continue routing) instead of
erroring. `ensure_character_vault` now first searches for a populated same-name
`'character'` store (`doc_mount_points::find_by_name` — `enabled=1`, trimmed
case-insensitive match) that passes the new `vault_has_required_files` check (all
six required files present in `doc_mount_file_links`) and adopts it when exactly
one qualifies (ambiguous or zero → fresh provision); the FK-write-and-confirm is
factored into the shared `link_character_to_vault`. The two seams compose — a live
`update` is how a character reaches the adopt branch. physicalDescription-via-update
(writing `physical-description.md` + `physical-prompts.json` on a non-null patch and
stripping it from the DB patch) was already coded; it is now proven. Each seam
ships a green six-table cross-DB shared-id-map remap differential
(`characters_adopt` / `characters_provision` / `characters_physical`
`_tier2_equivalence`) driving v4's REAL `repos.characters.update`/`.create`; the
adopt case asserts a single surviving mount point (the orphan store reused and its
FK relinked, no duplicate). Added `doc_mount_points::find_by_name` and
`doc_mount_file_links::relative_paths_lower`.

Phase-2 deferred-seam closure — closed the WRITE side of the
`chat_messages.isSilentMessage` seam (#8), completing it. The read side was
already resolved; this closes the write. A `message`-type insert now emits the
same TEXT-affinity bytes v4 stores: `true` → `"1.0"`, `false` → `"0.0"`, absent →
`NULL`. That representation arises because v4's `prepareForStorage(bool)` returns
the JS number `1`/`0`, better-sqlite3 binds it as a REAL, and SQLite converts the
REAL to text on store (`"1.0"`) — confirmed by a raw better-sqlite3 probe. The
Rust binding reproduces it by binding `Some(1.0_f64)` / `Some(0.0_f64)` / `None`;
context-summary / system inserts still omit the column so SQLite fills its DDL
default. Verified by a new `chats_messages_tier2` `addMessages` op carrying both a
`true` and a `false` silent message, byte-compared in the pinned `chat_messages`
dump against v4's REAL `addMessages`.

Phase-2 deferred-seam closure — ported the PUBLIC wardrobe write path (seam #7):
v4's `WardrobeRepository.create`/`update`/`delete`, in the new
`quilltap-core::db::vault_wardrobe_public`. These are v4's vault-only overrides —
resolve the owning character's document-store mount, read the current
`Wardrobe/*.md` items, apply the change, cycle-check, and re-project the folder,
throwing when no mount resolves (there is no SQL mirror). The prior
`wardrobe_tier2` port verified only the legacy base-SQL marshaling; this ports the
composition itself, over the already-verified leaves (`read_character_vault_wardrobe`
+ `project_vault_wardrobe` + `detect_component_cycles` + characters
`find_by_id_raw`), including the read-modify-project round-trip, the minted-`updatedAt`
on update, and the `assertNoCycles` guard (v4's exact `… → …; …` message). Verified
by a **read-back differential** (`vault_wardrobe_public_equivalence`) driving v4's
REAL public repo over a baked character+vault fixture: create, a composite create
referencing the first by id, a rename update, a cycle-forming update that throws, a
real delete (with the surviving composite's now-dangling ref DROPPING on read), a
delete of the already-gone id returning false, and a create against a non-existent
character that throws no-mount — comparing each op's read-back item list (minted
`updatedAt` normalized). A read-back tier rather than a table dump because
`build_wardrobe_item_file` writes the item's minted `updatedAt` into the
content-addressed `.md`, which a byte-level dump can't normalize; the projection
primitive is separately byte-verified (`vault_wardrobe_write_equivalence`). Scope:
the character tier only — the General/project archetype tiers stay deferred (same
boundary as `read_character_vault_wardrobe`). Four unit tests cover the patch merge,
cycle rejection, and the read→item conversion.

Phase-2 deferred-seam closure — ported the write applier's `__finalizeFile` +
post-commit side effects (seam #4), the last deferred pieces of
`quilltap-core::write_apply`. `__finalizeFile` now runs inside the main-DB
transaction loop (ensure-dir + staging→final rename), tracked so a later failure
in that partition undoes the renames in reverse before rethrowing; `cleanupStagingDirs`
drops the per-job `.staging/<jobId>` shell post-commit; and `dispatchInvalidations`
fires the deduped, ordered vector-store / mount-cache targets post-commit (both
skipped when the batch throws). The engine keeps v4's orchestration-vs-effect
split — the pure path/target computation (`path_dirname` = Node posix `dirname`,
`find_staging_root`, `collect_invalidations`) lives in the engine; the fs/cache
ops route through four new `ApplyHost` methods (production wires real fs/IPC; the
harness records them). The `write_apply_equivalence` trace differential grew four
observable fields (renames incl. undo-on-rollback, mkdirs, staging cleanup,
invalidation notifications) and three scenarios, verified against v4's REAL
`applyWritesUnsafe` — the oracle now records the fs mutators via jest `fs` mock +
the `notifyChild` mock (12 scenarios green). Also added four `write_apply` unit
tests.

Phase-2 deferred-seam closure — closed the `chat_messages.isSilentMessage` seam
(#8), and corrected its premise. The deferral claimed the TEXT-affinity round-trip
(`z.union([boolean, number.transform])` → TEXT) made v4's `getMessages` DROP a
silent message. Probed empirically against v4: it does NOT — a written `true` is
stored as numeric TEXT (`"1.0"`), and the read applies the row-schema union
(coerce to number, `=== 1`) → a real boolean, so the message is KEPT with
`isSilentMessage: true`. The real gap was that `db::chats_messages_read` never read
the column and so omitted the field. Fixed by reading `isSilentMessage` and
reproducing the coercion (numeric-TEXT `=== 1.0` → bool; `NULL` → omitted); the
read corpus gained a silent-message row proving the output matches the oracle. (The
write side does not yet emit the `"1.0"` representation — a bounded follow-up, since
the write corpus never sets it.)

Phase-2 deferred-seam closure — ported `TagVisualStyleSchema`'s per-field defaults
(seam #3). v4's base `_create` runs the doc through `TagSchema.parse`, so a PARTIAL
`visualStyle` gets its missing fields materialized; the Rust `TagVisualStyle` now
carries serde defaults matching each Zod `.default(...)` (`foregroundColor` →
`#1f2937`, `backgroundColor` → `#e5e7eb`, the four bools → `false`). `emoji`
(`.optional().nullable()`, no default) gained a double-`Option` + present-keeps-null
deserializer for the absent-vs-null trichotomy (absent → dropped as v4 `undefined`;
explicit `null` → kept). Proven by two partial-style tags corpus creates —
`{ bold: true }` (emoji dropped, all six defaults expand) and `{ emoji: null,
italic: true }` (emoji null kept) — each byte-identical to the oracle.

Phase-2 deferred-seam closure — closed the `toLowerCase` case-mapping seam
(`tags.nameLower`, `text_replacement_rules` conflict detection) by proving
`str::to_lowercase` is byte-identical to JS `String.prototype.toLowerCase`. Both
implement locale-independent Unicode default case mapping; verified empirically on
every gnarly case — `İ` → `i` + combining dot (`0069 0307`), a FINAL `Σ` → `ς`
(the context-sensitive Final_Sigma rule), `ß` (unchanged), `É`→`é`, and titlecase
digraphs (`ǅ`→`ǆ`). The evaluated `icu_casemap` option is therefore unnecessary —
no code change, just differential proof: the `tags` tier-2 corpus gained a tag
named `İSTANBUL ÉCOLE ΣΟΦΟΣ Straße` (whose stored `nameLower` matches the oracle
byte-for-byte), and `text_replacement_rules` a non-ASCII case-insensitive conflict
pair (`Café` then `CAFÉ`, both lowercasing to `café`) that fires duplicate
rejection identically on both sides. With the collation seam (above) this closes
the whole Unicode-fidelity cluster.

Phase-2 deferred-seam closure — added ICU collation (`icu` 2.2, ICU4X) as
`quilltap-core::collation::locale_compare`, closing the `localeCompare` seam. v4
sorts several lists with `a.localeCompare(b)` (no locale) — true ICU collation,
not the code-unit order Rust's `str: Ord` gives. Node's no-arg `Intl.Collator`
resolves to en-US / tertiary (probed against ICU 78); `Collator::try_new` returns
a `CollatorBorrowed<'static>` over the baked compiled data (held in a `LazyLock`),
and ICU4X's tables match Node's for common Latin + accents (verified the order
`a,A,ä,b,B,e,é,z,Z` and the pairwise signs). The two ported `localeCompare` sites
now use it — `compareVersions`' malformed-input fallback and `canonicalize`'s
tool-name array sort — and each differential gained a divergent row (mixed
case/accents, e.g. `apple` < `Banana`) that exercises the ICU path against the
oracle, where code-unit order would disagree. The `canonicalize` `parameters`
key-sort stays code-unit (v4 uses `Object.keys().sort()` there, not collation).
Future Phase-3 name sorts reuse `locale_compare`. (The `toLowerCase` case-mapping
seam is separate and closed next.)

Phase-2 deferred-seam closure — proved the open-JSON multi-key key-order fix (#5)
end-to-end. With `preserve_order` enabled (below), a MULTI-KEY value in
deliberately NON-SORTED key order was added to each affected corpus and its
differential re-run green, confirming the port emits v4's `JSON.stringify`
insertion order rather than sorted keys: `plugin_config.config`,
`character_plugin_data.data`, `image_profiles.parameters`,
`connection_profiles.parameters`, `chat_settings.tagStyles`, `chats.state` +
`chats.sillyTavernMetadata`, and `chats_outfits.equippedOutfit` (a key-order chat
that appends a higher-sorting characterId before a lower one). Refreshed the
now-stale `chats_outfits` doc comment (it described the pre-`preserve_order`
sorted-key seam). Corpus-only; no Rust logic change.

Phase-2 deferred-seam closure (begins) — enabled `serde_json`'s `preserve_order`
feature workspace-wide (both crates), so every `Value::Object` is an `IndexMap`
emitting INSERTION order, matching v4's `JSON.stringify`. This is the locked
decision for the open-JSON multi-key key-order seam (`parameters` / `config` /
`equippedOutfit` / `sillyTavernData` / `state` / `tagStyles` / `data` / …), which
the typed-struct trick could not cover. Foundational + no-regression: the full
suite stays green (the existing single-key corpora are order-invariant), and it
makes the harness stricter — a re-serialized `Value` now preserves on-disk key
order instead of sorting, so a masked key-order difference would surface (none
did). Per-column multi-key corpus proofs follow as each affected repo is swept.

The `chats` repo — sub-unit 6: the **remaining four ops files**, ported in
parallel (four agents, each on its own new module + differential; the shared
`ChatUpdate` setters + `mod.rs` wiring pre-staged serially). This **completes the
`chats` capstone** — the entire `ChatsRepository` public surface is now ported.
- **impersonation** (`db::chats_impersonation`, `chats_impersonation_tier2_equivalence`):
  v4 `ChatImpersonationOps` — `addImpersonation`/`removeImpersonation`/
  `getImpersonatedParticipantIds`/`setActiveTypingParticipant`/
  `updateAllLLMPauseTurnCount`. RMW on `impersonatingParticipantIds` +
  `activeTypingParticipantId` (the activeTyping reassign-or-clear on remove) +
  `allLLMPauseTurnCount`; mints nothing, so the differential is zero-normalization.
- **tokens** (`db::chats_tokens`, `chats_tokens_tier2_equivalence`):
  v4 `ChatTokenTrackingOps`. `incrementTokenAggregates` lowers v4's `$inc`/`$set`
  to one self-referential `UPDATE … SET col = col + ?` with an unconditionally
  minted `updatedAt` and a conditional `estimatedCostUSD = current + cost` (+
  `priceSource`); `resetTokenAggregates` zeroes the counters + nulls the cost via
  `update` (preserving `updatedAt`). Sentinel-aware `updatedAt` normalization
  (increment mints → `<ts>`; reset preserves → pinned, diffed exactly).
- **search** (`db::chats_search`, `chats_search_equivalence`):
  v4 `ChatSearchReplaceOps` — `countMessagesWithText`/`findMessagesWithText`/
  `searchMessagesGlobal`/`replaceInMessages`. The `searchMessagesGlobal`
  `$regex`→SQL `LIKE` translation reuses `memories`' exact mangling
  (`escapeRegex` → `source.replace(/\.\*/g,'%').replace(/\./g,'_')`, bare `LIKE`,
  no `ESCAPE`), reproducing v4's broken-but-exact behavior on regex-special
  inputs; the role filter + `createdAt DESC` + `limit`; and the split/join
  replace-all (which mints nothing). Read-differential over the method results +
  the post-replace `chat_messages` dump.
- **outfits** (`db::chats_outfits`, `chats_outfits_tier2_equivalence`): v4's
  `getEquippedOutfit`/`getEquippedOutfitForCharacter`/`setEquippedOutfit`/
  `removeEquippedItemFromAllChats` (in `chats.repository.ts`). RMW on the
  `equippedOutfit` JSON column, stored as **raw `Value`** (v4 never re-validates
  it through Zod), so partial / extra-key slots objects are preserved verbatim —
  the remove path mutates each character's slots in place, dropping the item only
  from slots it was actually in (v4's `before.includes` guard), never
  materializing absent slots. Corpus banks a partial-slot character to prove the
  shape-preservation. **Tracked seam:** the open-JSON key-order divergence
  (`serde_json::Value` sorts; v4 emits insertion order) — corpus constrained to
  sorted key order, same as `parameters`/`sillyTavernData`.

Build — extracted the SQLite3MC (ChaCha20/sqleet) amalgamation into a dedicated
`quilltap-sqlite3mc-sys` crate (its `build.rs` + `vendor/`, moved out of
`quilltap-core`). Cargo's build-script fingerprint includes the package version,
so the per-commit version bump on `quilltap-core` used to throw away the cached
`libsqlite3.a` and recompile the 12 MB amalgamation from scratch (~4 min). The
sys crate's version is pinned, so that C compile now caches across our version
bumps: a `quilltap-core` version bump rebuilds in ~2 s instead of ~4 min. No
`links` key (libsqlite3-sys already claims `sqlite3`); `quilltap-core` depends on
the sys crate and references it as `use quilltap_sqlite3mc_sys as _;` so its
link-search flags reach the final binary. Cipher behavior unchanged, verified by
the tier-2 differentials still opening real ChaCha20 databases.

Phase 2 — the `memories` repo, ported whole
(`quilltap-core::db::memories` + `db::memories_read`). A plain main-DB
`AbstractBaseRepository<Memory>` (no overrides except the `embedding` BLOB
registration, no vault overlay), so every read is a single-connection SELECT +
marshal. Ports the full surface: the write/mutation side (`create` with embedding
BLOB + JSON-array columns + the three numeric columns — `importance` /
`reinforcedImportance` are INTEGER-affinity, `reinforcementCount` REAL, all bound
`f64`; `update` leaving the BLOB untouched; `delete`; `updateForCharacter` /
`deleteForCharacter` ownership gates; `bulkDelete`; `updateAccessTime{,Bulk}`;
`replaceInMemories`; `deleteByChatId` / `deleteBySourceMessageId{,s}`) and the
read side (all ~30 `findBy*` / `count*` queries, incl. the `$regex` → SQL `LIKE`
mangling reproduced byte-for-byte, the `findByCharacterAboutCharacters` window
function, `findByCharacterIdPaginated`'s in-memory search, and the importance
tiers). Banks a marshaling seam: the normal `findByFilter` path omits NULL
nullable-optional columns (v4's `undefined` dropped by `JSON.stringify`), but the
raw-SQL `findByCharacterAboutCharacters` path keeps them as `null` (its rawQuery
rows carry explicit NULLs that `MemorySchema.safeParse` retains) — the port
mirrors both. Verified two ways: a tier-2 differential (`memories_tier2_equivalence`,
the write/mutation sequence, minted-timestamp placeholder form) and a
read-differential (`memories_read_equivalence`, 39 queries over a v4-baked fixture,
zero normalization — nothing mutated, so no minted timestamp; a returned
embedding is the `Float32Array` `{"0":…}` object rebuilt from the BLOB).

Phase 2 — the `CharactersRepository` read path
(`quilltap-core::db::characters_read`), characters sub-unit 4c — the capstone's
last piece. Ports the slim-row read marshaling (row → `Character`, the inverse of
sub-unit 2's write marshaling = v4 `hydrateRow` + Zod parse) + the `findBy*`
queries, each overlaying the character vault. The marshaling reproduces v4's net
read shape over the slim columns: required strings present; `.nullable().optional()`
TEXT/UUID/JSON cells **omitted** when `NULL` (v4 emits `undefined`, dropped by
`JSON.stringify`) and parsed when present; `.default(false)` booleans coerced from
INTEGER; `.nullable().optional()` booleans omitted/coerced; `.default([])` arrays
parsed (`NULL`/empty → `[]`); `controlledBy` defaulting to `'llm'`. The managed
columns sit at their DDL defaults, so it reproduces their Zod defaults directly
(`scenarios`/`systemPrompts`/`aliases` → `[]`, `talkativeness` → `0.5`, the nullable
managed fields omitted); for a vault-linked character the read overlay then
overwrites every managed field. Queries: `find_by_id` / `find_by_id_raw` /
`find_all` / `find_by_user_id` / `find_user_controlled` / `find_llm_controlled` /
`find_by_ids` / `find_by_default_image_id` / `find_by_avatar_override_image_id` /
`find_by_tag` (the last two via SQLite `json_each`, matching v4's query translator).
Verified by a read-differential (`characters_read_equivalence`): both sides READ a
copy of one fixture baked by v4's REAL create (four characters + vaults), run the
same 11 queries, and compare the hydrated lists exactly (ids/timestamps identical —
no remap — only `physicalDescription`'s read-minted createdAt/updatedAt
placeholdered, lists sorted by id). `findByIdRaw` isolates the slim marshaling (no
overlay). Also refactored sub-unit 4b's array ops to ride this full `find_by_id`
(re-verified green), closing the scoped-reader deferral.

Phase 2 — the `CharactersRepository` array / sub-array ops
(`quilltap-core::db::vault_character_arrays`), characters sub-unit 4b. Ports the
`systemPrompts` / `scenarios` / `partnerLinks` mutators + the
`setFavorite` / `setControlledBy` / `setCanBeCarina` setters. Each sub-array op is
v4's three-beat shape: `find_by_id` (the read overlay) → mutate the array in memory
(applying the per-op `onBeforeAdd` / `onAfterBuild` / `onAfterRemove` default
normalization) → `update_character` (the 4a write overlay) reprojects the
`Prompts/` / `Scenarios/` folder (or writes the slim `partnerLinks` column). The
minted item `id` / `createdAt` / `updatedAt` never reach disk — the projection
writes `<sanitize(name|title)>.md` from `build_system_prompt_file` /
`build_scenario_file`, and the read side re-derives a prompt's id from its path —
so the DB effect is deterministic. Added a scoped `find_by_id` (the slim columns
the ops consume — `id` / `characterDocumentMountPointId` / `partnerLinks` — plus
the overlaid `systemPrompts` / `scenarios`; full slim-row read marshaling is
sub-unit 4c). The setters are thin `update_character(id, { … })` wrappers (no read,
no vault). Verified by a tier-2 differential (`characters_arrays_tier2_equivalence`)
over a fixture baked by v4's REAL create (one baked prompt / scenario / partner
link), driving v4's REAL repository methods across SIX tables in the
shared-cross-db-id-map remap form (`chunkCount`/`doc_mount_chunks` pinned/excluded);
the id-taking prompt/scenario ops carry a `targetName` / `targetTitle` resolved to
the current id via `findById` on each side. Banks addSystemPrompt (default-demote +
non-default), updateSystemPrompt (rename → sweep + content), setDefaultSystemPrompt,
deleteSystemPrompt (deleting the default → survivor promotion), the three scenario
ops, the two partner ops, and the three setters.

Phase 2 — `applyDocumentStoreWriteOverlay` + the `CharactersRepository.update`
integration (`quilltap-core::db::vault_character_update`), characters sub-unit 4a.
The managed-field write **router** — distinct from sub-unit 1's create-time writer
(which projects every field unconditionally): the update path routes only the
fields **present in the patch**, and `properties.json` is a **read-modify-write**
(a patch touching only `title` preserves pronouns/aliases/firstMessage/
talkativeness). Routes markdown (`None`→`""`), the properties RMW (seeded from the
current `properties.json`, falling back to the empty-managed default), physical
(non-null writes the two files; null leaves them), and `systemPrompts`/`scenarios`
(reproject the folder — sweep + write). Returns the unmanaged remainder;
`update_character` runs the slim `_update` for it (skipped when empty — a
managed-only update does NOT bump the slim row's `updatedAt`). The DB-bound
remainder is marshaled back through the slim repo's typed update. Verified by a
tier-2 differential (`characters_update_tier2_equivalence`) over a fixture baked by
v4's REAL create, driving v4's REAL `repos.characters.update` across SIX tables
(slim `characters` row + the five store tables) in the shared-cross-db-id-map remap
form (`chunkCount`/`doc_mount_chunks` pinned/excluded). Banks markdown routing, the
properties RMW preserving untouched keys (asserted), a DB-only field update
(`isFavorite` true→false → slim `_update`), and a `systemPrompts` reprojection
(sweep the old `Prompts/Default.md`, write the new one) on a managed-only update —
the orphan-on-rewrite + sweep-GC row counts matching v4 byte-for-byte via the
shared DDL. Added the public `render_properties_json` (the RMW serializer, reusing
the create-time `properties.json` shape + the `talkativeness` js-number rule) and
`DocMountFileLinksRepository::ensure_folder_path`'s sibling read
`link_exists_at_path` (used by 3a). **Tracked deferral:** provision-on-the-fly (a
patch with managed fields on a vault-less character) — the corpus always has a
vault; lands with the startup-backfill slice.

Phase 2 — `ensureCharacterVault` + the `CharactersRepository.create` integration
(`quilltap-core::db::character_vault`), characters sub-unit 3b — the store-backed
capstone's keystone. `create_character` runs v4's full create end-to-end: the
slim-row `_create` (FK nulled — a fresh character always provisions a fresh vault),
then `ensure_character_vault` mints a `<name> Character Vault` mount point
(mount-index DB), scaffolds its preset structure, projects the managed fields
(`write_character_vault_managed_fields`, sub-unit 1), and links it by setting
`characterDocumentMountPointId` on the slim row (main DB) — confirming the write
stuck (v4's `linkCharacterToVault` turns a silent "linked but not linked" into a
loud error). A character spans two databases, so the differential
(`characters_create_tier2_equivalence`) drives v4's REAL `repos.characters.create`
and diffs SIX tables — the main slim `characters` row + the mount-index store
tables (`doc_mount_points` / `_folders` / `_files` / `_documents` / `_file_links`)
— in the shared-cross-db-id-map remap form (nothing pinned; every id minted, FKs
verify by relationship; timestamps placeholdered; the link `chunkCount`
pinned and `doc_mount_chunks` excluded, as for groups/projects). Banks the 6-step
create, the **orphan-on-rewrite** default-`properties.json` file/document row (the
scaffold writes it, then the managed bag overwrites it; `writeDatabaseDocument`
does no GC, so the old row persists — 9 files, 8 live + 1 orphan), the five
identity markdown overwrites (the `physical-*` scaffold defaults survive — no
physicalDescription), and one systemPrompt + one scenario projected into `Prompts/`
+ `Scenarios/` (10 links). **Tracked deferral:** the `ensureCharacterVault` adopt
branch (startup-heal of a hand-linked same-name store) — the corpus always
provisions fresh; it needs a richer `doc_mount_points` read and lands with the
startup-backfill slice.

Phase 2 — `scaffoldCharacterMount` (`quilltap-core::db::character_vault`),
characters sub-unit 3a (the store-backed capstone's stateful provisioning glue,
mount-index DB). Populates a freshly-created database-backed character store with
the preset structure: seven empty top-level folders (Prompts/Scenarios/Wardrobe/
Outfits/lore/images/files), six blank Markdown files
(identity/description/manifesto/personality/physical-description/example-dialogues,
content `""`), and two seeded JSON files (`properties.json` +
`physical-prompts.json`, FIXED default content). The six blank files share the
empty-string content sha, so they dedup to ONE `doc_mount_files` /
`doc_mount_documents` row with six distinct links; result: 7 folders, 3 files, 3
documents, 8 links. All writes go through the verified storage primitive — folders
via the new `DocMountFileLinksRepository::ensure_folder_path` (v4 `ensureFolderPath`,
walks the path directly so a single segment makes one root folder; a sibling of
`ensure_link_folder_id` which walks a file's dirname), files via
`write_database_document` (idempotent, skip-if-link-exists). Verified standalone
(the create flow's `writeCharacterVaultManagedFields` overwrites the five identity
markdown files + `properties.json`, so the create differential would mask the
scaffold defaults — verifying here pins the default bytes). Tier-2 differential
(`characters_scaffold_tier2_equivalence`) drives v4's REAL `scaffoldCharacterMount`
and diffs five mount-index tables (points / folders / files / documents / links) in
the shared-cross-table-id-map remap form; the seeded `mountPointId` is pinned, the
link `chunkCount` (a `reindexSingleFile` artifact) pinned and `doc_mount_chunks`
excluded (as for groups/projects).

Phase 2 — the `characters` repo **slim-row marshaling**
(`quilltap-core::db::characters`), the first sub-unit of v4's
`CharactersRepository` (the store-backed capstone). Ports the base-repository SQL
CRUD (`_create`/`_update`/`_delete`) over the MAIN-db `characters` table. v4's
public `create`/`update` orchestrate the character vault (provision + project +
overlay) — a later sub-unit; both strip the `MANAGED_FIELDS` set (identity,
description, manifesto, personality, exampleDialogues, pronouns, aliases, title,
firstMessage, talkativeness, physicalDescription, systemPrompts, scenarios) before
the SQL write, leaving the non-managed "slim row" this differential checks. A
fresh fixture's table still has the managed columns (`ensureCollection` generates
them from `CharacterSchema`), but both sides omit them from every write, so they
sit at their DDL defaults identically. Banks the **widest nullable-boolean surface
in Phase 2** — seven `z.boolean().nullable().optional()` columns
(`defaultAgentModeEnabled`, `defaultHelpToolsEnabled`, `canDressThemselves`,
`canCreateOutfits`, `systemTransparency`, `coreWhisperEnabled`, `canBeCarina`),
INTEGER 0/1 when present, SQL NULL when absent — plus a typed JSON-object column
(`defaultTimestampConfig`, a nine-field struct in schema order so the compact JSON
matches `JSON.stringify` key order, NOT `serde_json::Value`), an open JSON column
(`sillyTavernData`, kept `null`/single-key per the multi-key seam), two
typed-struct array columns (`partnerLinks` `{partnerId,isDefault}`,
`avatarOverrides` `{chatId,imageId}`), a string-array column (`tags`), two
boolean-default columns (`isFavorite`/`npc`), an enum TEXT column (`controlledBy`),
and many nullable UUID columns. `update` is a partial `SET` that reproduces v4's
full `$set` on-disk result (the fixture cells are already in validated canonical
order). Verified by a tier-2 differential (`characters_slim_tier2_equivalence`)
driving v4's REAL protected internals via a thin subclass over a create / create /
update / delete sequence, diffing the `characters` table in the pinned
zero-normalization form (ids + timestamps pinned both sides).

Phase 2 — the `background_jobs` repo (`quilltap-core::db::background_jobs`), v4's
`BackgroundJobsRepository` — the durable work queue (memory extraction, context
summaries, embedding generation, autonomous room turns, …). A
`UserOwnedBaseRepository` (a `userId` column) with NO base-method override, so
`create`/`update`/`delete` honor pinned id/createdAt/updatedAt; on top of CRUD it
ports the full queue API. Banks three **REAL-affinity** number columns
(`priority`/`attempts`/`maxAttempts` — all bare `z.number().default(N)` → REAL,
NOT INTEGER; integer-collapsed in the dump) and the open-JSON `payload` column
(kept `{}`/single-key per the multi-key key-order seam). Ports and verifies the
queue ops: `claimNextJob` (atomic `SELECT … ORDER BY priority DESC, createdAt ASC
LIMIT 1` then UPDATE in a transaction, `attempts += 1`), `markFailed` (exponential
backoff `min(30·2^attempts, 300)`s, DEAD-vs-FAILED on `attempts >= maxAttempts`),
`markCompleted`, `pause`/`resume`, `cancel`, `cancelByType`, `resetAllProcessingJobs`,
`resetStuckJobs`, and `deleteByTypesAndStatuses` — with the exact `lastError`
strings byte-for-byte (`"Cancelled by user"`, `"Superseded by new reindex"`, the
em-dash `"Orphaned on startup — killed"`, `"Timed out after N minutes"`). The
nested-JSON path finders (`findPendingForChat`/`ForEntity`) reproduce v4's
`json_extract(payload, '$.chatId')` translation. Verified by a tier-2 differential
(`background_jobs_tier2_equivalence`) driving v4's REAL repo over a 13-op sequence
and diffing the table in the minted-timestamp placeholder form (ids + createdAt +
every deterministic column — status/attempts/lastError/payload/priority/maxAttempts
— diffed EXACTLY; only the four mintable timestamp columns placeholdered).
**Discovered v4-on-SQLite limitation:** `markCompleted`'s dotted `payload.result`
merge throws `no such column: payload.result` on v4's SQLite backend (no dotted
JSON sub-key translator), so that path is unreachable there; the port keeps the
merge as a forward v5 capability (via the pure `merge_result_into_payload`, three
unit tests) and the differential exercises only the no-result path (v4's working
behavior).

Phase 2 — the `vector_indices` repo (`quilltap-core::db::vector_indices`), v4's
`VectorIndicesRepository`. The first **standalone two-table** repo — it does NOT
extend the base repository; it manages `vector_indices` (per-character metadata)
+ `vector_entries` (per-embedding rows) in the MAIN db directly. Banks the third
Float32-BLOB embedding column (little-endian via `embedding_blob::float32_to_blob`,
`None`/empty → SQL NULL, never a zero-length blob; dumped as hex for a bit-exact
compare), two REAL-affinity number columns (`version`/`dimensions`, bare
`z.number()` → REAL, integer-collapsed in the dump), and a `saveMeta` upsert keyed
by `characterId` (`id == characterId`, so the meta `id` is pinned, not minted).
Reproduces v4's exact op semantics: `addEntries` mints one shared `createdAt`
across the batch; `removeEntries` is a per-id delete loop (not a single `IN (…)`);
`updateEntryEmbedding` touches only the embedding column (no timestamp);
`deleteByCharacterId` is two independent ops (entries then meta), not one SQL
transaction. Verified by a tier-2 differential (`vector_indices_tier2_equivalence`)
driving v4's REAL repo over a full op sequence (saveMeta create/update, addEntry,
addEntries, updateEntryEmbedding, removeEntries, and a `deleteByCharacterId` that
wipes a second character entirely) and diffing both tables in the minted-values
remap form (entry `id` remapped, timestamps placeholdered, `characterId`/embedding
pinned).

Phase 2 — repo-by-repo over the real DB (each ported repo arrives with its
tier-2 case):

- `tags` repo (`quilltap-core::db::tags`): `create`, `update`, and `delete`
  ported from v4's `TagsRepository` + base-repo internals. Widens the tier-2
  marshaling surface past `folders`' all-strings shape — a boolean column
  (`quickHide` stored as INTEGER 0/1), a nullable JSON-object column
  (`visualStyle` stored as compact JSON in schema field order, reproduced with a
  typed struct so key order matches v4's `JSON.stringify` rather than a sorted
  map), and the `nameLower` derivation (`(nameLower || name).toLowerCase()` on
  create; re-derived from `name` on update). Adds the `delete` op to the harness.
- Harness: tier-2 differential test `tags_tier2_equivalence` plus its fixture
  builder + `tags-tier2` oracle case, driven by the committed
  `harness/oracle/fixtures/tags-tier2.json` (the create op carries a
  fully-specified `visualStyle` so no Zod inner-default expansion is involved).
  Ids and timestamps pinned both sides → zero normalization. The `tags` repo
  round-trips green (`QT_ORACLE_TAGS` + `QT_FIXTURE_TAGS`, skip-if-unset).
- Generated-UUID remap + timestamp-placeholder normalization (the tier-2
  machinery for ops that mint their own ids/clocks, not just the pinned-id sync
  path). `folders.create` now ports v4 `_create`'s minted-values defaults
  (`id = options?.id || generateId()`, timestamps `|| now`) and returns the id
  used, so a caller can wire it into a dependent op. New `quilltap-core::clock`
  (`now_iso` / pure `iso_from_unix_ms`) reproduces v4's
  `new Date().toISOString()` shape; `uuid` (v4) generates ids. Verified by the
  `folders_remap_tier2_equivalence` test: a parent + child created with NOTHING
  pinned, so both v4 and Rust mint different random UUIDs and timestamps. One
  normalization (in the harness) runs over both dumps — rows walked in
  natural-key (`path`) order, id columns (`id`, `parentFolderId`) collapsed to
  first-seen tokens (`ID_0`, `ID_1`), so the child→parent FK relationship is
  verified without pinning the literal id; timestamps placeholdered after
  asserting the `createdAt == updatedAt` create invariant per row. Round-trips
  green (`QT_ORACLE_FOLDERS_REMAP` + `QT_FIXTURE_FOLDERS_REMAP`, skip-if-unset).
- The partitioned write APPLIER (`quilltap-core::write_apply`) — the writer-task
  apply path ported from v4's `applyWritesUnsafe` / `applyPartition` /
  `applySecondaryBestEffort` / `applyFolderCreateIdempotent`. Sequences the pure
  `write_partition` leaves into the real orchestration: each partition (main /
  mount-index / llm-logs) commits in its own `BEGIN IMMEDIATE` transaction;
  main-primary jobs (`AUTONOMOUS_ROOM_TURN`) commit main first then apply
  secondaries best-effort (a dropped doc-store effect can't lose the chat turn),
  while idempotent jobs apply secondaries first so a secondary failure prevents
  the main commit; and the concurrent `docMountFolders.create` unique-conflict
  reconcile resolves to the existing row and remaps the discarded buffered folder
  id for the rest of the batch. The engine is generic over an injected
  `ApplyHost` seam (the three connections + repo dispatch + the reconcile
  lookup), mirroring how v4 unit-tests this orchestration with fakes.
- Harness: `write_apply_equivalence` — a tier-1-style TRACE differential over a
  committed 9-scenario corpus (`harness/oracle/fixtures/write-apply.json`). Both
  sides emit the same observable trace (per-partition exec sequence, ordered repo
  dispatches with post-remap args, reconcile lookups, resolved/threw outcome).
  The oracle (`harness/oracle/cases/write-apply.test.ts`) drives v4's REAL
  `applyWritesUnsafe` — it runs under v4's jest (not tsx) because the applier's
  `getRawDatabase()` / `getRepositories()` singletons are `jest.mock`-injected;
  v4's jest resolves the v5-tree oracle file via an extra `--roots`. Deferred
  (documented): `__finalizeFile` (fs rename + undo-on-rollback) and the
  post-commit `cleanupStagingDirs` / `dispatchInvalidations` side effects.
- `text_replacement_rules` repo (`quilltap-core::db::text_replacement_rules`):
  `create`, `update`, and `delete` ported from v4's
  `TextReplacementRulesRepository`. The first repo with **conflict detection** —
  and so the first to need a repo-level *read*: `create`/`update` scan the
  existing rows and reject a duplicate `(fromText, caseSensitive)` pair
  (case-sensitive rules compare `fromText` exactly, case-insensitive ones
  compare lowercased; the `caseSensitive` flag is part of the key, and `update`
  only re-checks when that pair changes). A conflict surfaces as
  `TrrError::Conflict`, the analogue of v4's `TextReplacementRuleConflictError`.
  Single-user (no `userId`). Widens the tier-2 marshaling surface past `tags`
  with a real INTEGER number column (`sortOrder`) and two boolean columns
  (`caseSensitive`, `enabled`).
- Harness: tier-2 differential `text_replacement_rules_tier2_equivalence` plus
  its fixture builder + `text-replacement-rules-tier2` oracle case, driven by the
  committed `harness/oracle/fixtures/text-replacement-rules-tier2.json`. The op
  sequence includes two conflicting ops flagged `expectThrow`: both the oracle
  (asserting v4 threw `TextReplacementRuleConflictError`) and the Rust port
  (asserting `TrrError::Conflict`) prove the rejection independently, and the
  final-state dump confirms the rejected writes left no trace (a port lacking the
  check would have diverged). Ids + timestamps pinned → zero normalization.
  Round-trips green (`QT_ORACLE_TRR` + `QT_FIXTURE_TRR`, skip-if-unset). The
  toLowerCase case-mapping seam (shared with `tags.nameLower`) gains a second
  site here — tracked in the deferred-seams list.
- Canonical dump: `js_number_to_json` — the dump's REAL-cell rendering now
  mirrors JS `JSON.stringify(number)`, collapsing an integer-valued double
  (`9.0` → `9`) so a REAL-affinity numeric column (e.g. `z.number().int()`,
  which SQLite stores as an 8-byte float) matches the oracle, where
  better-sqlite3 hands JS a `Number` and `JSON.stringify` drops the `.0`. First
  exercised by `text_replacement_rules`' `sortOrder`.
- `prompt_templates` repo (`quilltap-core::db::prompt_templates`): `create`,
  `update`, and `delete` ported from v4's `PromptTemplatesRepository` (built-in
  *seeding* is a startup concern, out of scope). Widens the tier-2 marshaling
  surface with the **first JSON array column** (`tags: z.array(UUIDSchema)` →
  compact JSON text, `["id"]` / `[]`; reproduced via `serde_json::to_string` of a
  `Vec<String>` — arrays are order-preserving, so no key-order subtlety) and
  several **nullable string columns** (`userId` null-for-built-in, `description`,
  `category`, `modelHint`). Adds the **built-in read-only guard**: `update`/
  `delete` read the target's `isBuiltIn` and refuse to mutate a built-in row,
  returning a not-modified result (`Ok(false)`; v4's `null` / `false`) rather
  than throwing — a read-then-guard pattern that suppresses the op instead of
  raising. Plain `AbstractBaseRepository` (nullable `userId`).
- Harness: tier-2 differential `prompt_templates_tier2_equivalence` plus its
  fixture builder + `prompt-templates-tier2` oracle case, driven by the committed
  `harness/oracle/fixtures/prompt-templates-tier2.json`. The op sequence
  exercises the array column on create and on update (replacing the array), the
  nullable columns (null vs present), and the guard two ways via an `expectNoop`
  flag — an update and a delete that both target the built-in seed row; both
  sides assert the op reported not-modified (Rust `Ok(false)`; oracle `null` /
  `false`) and the final-state dump confirms the built-in row stayed
  byte-identical. Ids + timestamps pinned → zero normalization. Round-trips green
  (`QT_ORACLE_PROMPT_TEMPLATES` + `QT_FIXTURE_PROMPT_TEMPLATES`, skip-if-unset).
- Three more plain-base repos ported in parallel (each `create` / `update` /
  `delete`, pinned form, its own tier-2 case round-tripping green):
  - `conversation_annotations` (`quilltap-core::db::conversation_annotations`):
    banks a **REAL-affinity unbounded-int column** — `messageIndex` is
    `z.number().int().min(0)` with no `.max()`, and v4's schema translator
    (`mapToSQLiteType`) only assigns INTEGER affinity when a numeric field has
    both an integer min and max, so it maps to REAL; bound as `f64`, the dump's
    `js_number_to_json` collapses the integer-valued cell back to a bare integer.
    Also a **nullable UUID column** (`sourceMessageId`). Harness
    `conversation_annotations_tier2_equivalence` (`QT_ORACLE_CONV_ANNOTATIONS` +
    `QT_FIXTURE_CONV_ANNOTATIONS`).
  - `provider_models` (`quilltap-core::db::provider_models`): banks **two
    nullable REAL number columns** (`contextWindow`, `maxOutputTokens` — both
    bare `z.number()`, no min/max → REAL), **two boolean-default columns**
    (`deprecated`, `experimental` → INTEGER 0/1), and **enum TEXT columns**
    (`provider`, `modelType`). The corpus supplies every column explicitly so no
    Zod create-time default is relied on. Harness
    `provider_models_tier2_equivalence` (`QT_ORACLE_PROVIDER_MODELS` +
    `QT_FIXTURE_PROVIDER_MODELS`).
  - `help_docs` (`quilltap-core::db::help_docs`): the **first tier-2 BLOB
    column** — `embedding` is a Float32 buffer (little-endian `f32` bytes via
    `embedding_blob::float32_to_blob`), with empty/null → SQL NULL and the dump
    emitting BLOBs as lowercase hex on both sides for bit-exact comparison
    (fixture uses only exactly-float32-representable values so the f64→f32 cast
    is lossless). Banks that a **text-only update preserves the BLOB**: the
    partial `UPDATE SET` never names the embedding column, mirroring v4's
    whole-row rewrite that re-persists the existing embedding unchanged. Harness
    `help_docs_tier2_equivalence` (`QT_ORACLE_HELP_DOCS` + `QT_FIXTURE_HELP_DOCS`).
- A second parallel batch of three repos (each `create` / `update` / `delete`,
  pinned form, its own tier-2 case round-tripping green):
  - `roleplay_templates` (`quilltap-core::db::roleplay_templates`): the **first
    array-of-objects JSON column** — `renderingPatterns: z.array(...)` stored as a
    compact JSON array of objects, each element modeled by a typed serde struct in
    schema field order (`#[serde(rename_all = "camelCase")]` + `skip_serializing_if`
    on the optionals) so the key order and omitted-optional behavior match v4's
    `JSON.stringify(zodParsed)` byte-for-byte — plus a **nullable JSON-object
    column** (`dialogueDetection`). `delimiters` is held empty and
    `narrationDelimiters` kept to its plain-string form (the discriminated-union /
    tuple forms buy no new marshaling coverage). No built-in guard ported (the
    corpus never mutates a built-in row). Harness
    `roleplay_templates_tier2_equivalence` (`QT_ORACLE_ROLEPLAY_TEMPLATES` +
    `QT_FIXTURE_ROLEPLAY_TEMPLATES`).
  - `image_profiles` (`quilltap-core::db::image_profiles`): banks the **Taggable
    lineage** (`userId` + a JSON `tags` array) and the first **open / arbitrary-
    JSON object column** (`parameters`, `z.record`), modeled as `serde_json::Value`
    → compact JSON text, plus boolean and nullable-string columns. Harness
    `image_profiles_tier2_equivalence` (`QT_ORACLE_IMAGE_PROFILES` +
    `QT_FIXTURE_IMAGE_PROFILES`).
  - `connection_profiles` (`quilltap-core::db::connection_profiles`): the
    workhorse profile repo and the **widest marshaling surface** to date — ~29
    columns spanning three enum TEXT columns, eight booleans, two nullable REAL
    int-overrides (`maxContext`/`maxTokens`), five REAL token counters, three
    nullable strings, the `tags` array, and the open `parameters` object. The
    corpus supplies every column explicitly. Harness
    `connection_profiles_tier2_equivalence` (`QT_ORACLE_CONNECTION_PROFILES` +
    `QT_FIXTURE_CONNECTION_PROFILES`).
  - New tracked deferred seam (open-JSON multi-key key order): an open-JSON object
    column with **two or more keys** would diverge — `serde_json::Value` sorts keys
    while v4's `JSON.stringify` preserves insertion order. The `image_profiles` /
    `connection_profiles` corpora constrain `parameters` to `{}` or single-key
    objects; see "Deferred seams" in `docs/developer/porting/phase-2-onramp.md`.

- A third parallel batch — five plain-base single-table repos (each `create` /
  `update` / `delete`, its own tier-2 case round-tripping green):
  - `plugin_config` (`quilltap-core::db::plugin_config`): the **UserOwned lineage**
    (a `userId` scope column) plus an **open-JSON object column** (`config`,
    `z.record`) and an **optional (nullable) boolean** (`enabled`,
    `z.boolean().optional()` with no default → INTEGER 0/1 when present, SQL NULL
    when the key is absent — confirmed empirically). Harness
    `plugin_config_tier2_equivalence` (`QT_ORACLE_PLUGIN_CONFIG` +
    `QT_FIXTURE_PLUGIN_CONFIG`).
  - `embedding_profiles` (`quilltap-core::db::embedding_profiles`): the Taggable
    lineage again, widened with an **enum TEXT** column (`provider`), two **nullable
    REAL number** columns (`dimensions` bare `z.number()`, `truncateToDimensions`
    `z.number().int().positive()` — min-only, so REAL not INTEGER), and two
    **boolean-default** columns (`normalizeL2`, `isDefault`). Harness
    `embedding_profiles_tier2_equivalence` (`QT_ORACLE_EMBEDDING_PROFILES` +
    `QT_FIXTURE_EMBEDDING_PROFILES`).
  - `terminal_sessions` (`quilltap-core::db::terminal_sessions`): a clean
    string-heavy repo — nullable string columns (`label`, `transcriptPath`), a
    nullable timestamp (`exitedAt`), and a **nullable REAL** column (`exitCode`,
    `z.number().int()`, no max). v4's `create` injects no nondeterministic defaults,
    so the pinned zero-normalization form holds. Harness
    `terminal_sessions_tier2_equivalence` (`QT_ORACLE_TERMINAL_SESSIONS` +
    `QT_FIXTURE_TERMINAL_SESSIONS`).
  - `character_plugin_data` (`quilltap-core::db::character_plugin_data`): the first
    **open-JSON _value_ column** (`data`, `z.unknown()`) — any JSON value stored as
    compact JSON text via v4's `prepareForStorage`, modeled as `serde_json::Value`.
    Harness `character_plugin_data_tier2_equivalence`
    (`QT_ORACLE_CHARACTER_PLUGIN_DATA` + `QT_FIXTURE_CHARACTER_PLUGIN_DATA`).
  - `tfidf_vocabulary` (`quilltap-core::db::tfidf_vocabulary`): the first repo that
    **overrides the base `create`/`update`** — v4 mints `updatedAt =
    getCurrentTimestamp()` unconditionally (a passed `updatedAt` is ignored), so the
    port mints it via `clock::now_iso` and the harness placeholder-normalizes only
    that one column (ids / `createdAt` / every payload column stay pinned and diff
    exactly). Also the first **plain-string columns that hold JSON text**
    (`vocabulary`, `idf`, bound single-encoded, not re-stringified), plus a bare
    `z.number()` REAL (`avgDocLength`) and an int-positive REAL (`vocabularySize`).
    Harness `tfidf_vocabulary_tier2_equivalence` (`QT_ORACLE_TFIDF_VOCABULARY` +
    `QT_FIXTURE_TFIDF_VOCABULARY`).
  - The `plugin_config` / `character_plugin_data` open-JSON corpora are constrained
    to `{}` or single-key objects, same as the tracked multi-key key-order seam.

- A fourth parallel batch — five more main-DB repos (each `create` / `update` /
  `delete`, its own tier-2 case round-tripping green):
  - `users` (`quilltap-core::db::users`): the plainest surface yet — all strings
    plus five **nullable TEXT** columns (`email`, `name`, `image`, `emailVerified`,
    `passwordHash`), no booleans/numbers/JSON/BLOB. Harness
    `users_tier2_equivalence` (`QT_ORACLE_USERS` + `QT_FIXTURE_USERS`).
  - `conversation_chunks` (`quilltap-core::db::conversation_chunks`): the **second
    tier-2 BLOB column** (`embedding`, Float32 LE bytes via
    `embedding_blob::float32_to_blob`, null/empty → NULL, dumped as hex; a text-only
    update leaves it untouched) plus a REAL int (`interchangeIndex`,
    `z.number().int().min(0)` — min-only → REAL) and two **JSON string-array
    columns** (`participantNames`, `messageIds`). Harness
    `conversation_chunks_tier2_equivalence` (`QT_ORACLE_CONVERSATION_CHUNKS` +
    `QT_FIXTURE_CONVERSATION_CHUNKS`).
  - `files` (`quilltap-core::db::files`): the **widest repo to date** (~23 columns,
    Taggable) — a bare-`z.number()` REAL (`size`), two **nullable REAL** columns
    (`width`/`height`), an **optional boolean** (`isPlainText` — banks both the
    present 0/1 and the absent → NULL case), two JSON arrays (`linkedTo`, `tags`),
    three enum TEXT columns (`source`, `category`, `fileStatus`), and several
    nullable strings. Harness `files_tier2_equivalence` (`QT_ORACLE_FILES` +
    `QT_FIXTURE_FILES`).
  - `chat_documents` (`quilltap-core::db::chat_documents`): an enum TEXT column
    (`scope`), a boolean (`isActive`), and two nullable strings. Harness
    `chat_documents_tier2_equivalence` (`QT_ORACLE_CHAT_DOCUMENTS` +
    `QT_FIXTURE_CHAT_DOCUMENTS`).
  - `embedding_status` (`quilltap-core::db::embedding_status`): the second repo that
    **overrides the base `create`/`update`** with an unconditionally-minted
    `updatedAt` (like `tfidf_vocabulary`) — the port mints it via `clock::now_iso`
    and the harness placeholder-normalizes only `updatedAt` (id / `createdAt` /
    payload pinned). Two enum TEXT columns (`entityType`, `status`) + a nullable
    timestamp + a nullable string. Harness `embedding_status_tier2_equivalence`
    (`QT_ORACLE_EMBEDDING_STATUS` + `QT_FIXTURE_EMBEDDING_STATUS`).

Phase 2 — the mount-index sibling-DB slice (the first repos NOT in the main DB).
These tables live in v4's dedicated `quilltap-mount-index.db`. The tier-2
machinery was extended to target a sibling DB: the fixture builder + oracle point
`SQLITE_MOUNT_INDEX_PATH` at the fixture (with a throwaway main DB at
`SQLITE_PATH`), seed/run through v4's real repos (whose `getCollection` override
routes there), flush via `closeMountIndexSQLiteClient`, and read back through
`getRawMountIndexDatabase` directly (not `rawQuery`, which targets the main
backend). The Rust `Writer` needed no change — `open_writable` already opens any
ChaCha20 file by path, so the partition is simply which file the writer opened.
Five repos ported in one slice (a serial pilot, then four parallel), each with its
own tier-2 case round-tripping green (pinned ids + timestamps → zero
normalization):

  - `group_character_members` (`quilltap-core::db::group_character_members`): the
    pilot — the plainest join table (`id` + two UUID-as-TEXT refs + timestamps).
    Harness `group_character_members_tier2_equivalence`
    (`QT_ORACLE_GROUP_CHARACTER_MEMBERS` + `QT_FIXTURE_GROUP_CHARACTER_MEMBERS`).
  - `project_doc_mount_links` / `group_doc_mount_links`
    (`quilltap-core::db::{project_doc_mount_links,group_doc_mount_links}`):
    structurally identical join tables (cross-DB refs stored as plain TEXT — v4's
    `generateCreateTable` emits no FK constraints). Harnesses
    `project_doc_mount_links_tier2_equivalence` /
    `group_doc_mount_links_tier2_equivalence`.
  - `doc_mount_folders` (`quilltap-core::db::doc_mount_folders`): adds a **nullable
    UUID** column (`parentId`, null = mount-point root) — banks both the null and
    non-null paths. Harness `doc_mount_folders_tier2_equivalence`.
  - `doc_mount_points` (`quilltap-core::db::doc_mount_points`): the widest of the
    family (18 columns) — four enum TEXT columns, a boolean (`enabled`, banks 0 and
    1), two **JSON string-array** columns (`includePatterns`/`excludePatterns`,
    banks empty and non-empty), three nullable strings/timestamp, and three
    **REAL-affinity int counters** (`fileCount`/`chunkCount`/`totalSizeBytes`,
    `z.number().int()` with no min&max → REAL, integer-collapsed in the dump). Its
    runtime ALTER-TABLE "migrations" are no-ops on a fresh schema-generated table.
    Harness `doc_mount_points_tier2_equivalence`.

Phase 2 — the llm-logs sibling DB + the deferred `upsert*` methods (two
independent slices).

`llm_logs` (`quilltap-core::db::llm_logs`): the SECOND sibling-DB partition (v4's
`quilltap-llm-logs.db`) and the widest repo in Phase 2 — 18 columns including FIVE
nested typed-struct JSON columns (`request`, `response`, `usage`, `cacheUsage`,
`requestHashes`), an open-JSON `rawProviderUsage`, a nullable REAL (`durationMs`),
an 18-variant enum, and four nullable UUIDs. Same TS-only sibling-DB machinery as
the mount-index slice but pointed at `SQLITE_LLM_LOGS_PATH` / read back through
`getRawLLMLogsDatabase()` (the backend disconnect closes this client, so the
oracle reads before `closeDatabase()`). The nested JSON is reproduced byte-for-byte
with serde structs in schema field order: integer-valued nested numbers as `i64`
(so they render `3`, not `3.0`, matching `JSON.stringify`), `temperature` the lone
`f64` (kept fractional), optional nested fields `skip_serializing_if` (omitted, not
null). Pinned zero-normalization form; `rawProviderUsage` constrained to
null/`{}`/single-key (the open-JSON seam). Harness `llm_logs_tier2_equivalence`.

The deferred `upsert*` methods on six already-ported repos are now implemented,
each with its own tier-2 case in the REMAP (minted-values) form: the upsert mints
`id`/`createdAt`/`updatedAt` on the create branch and `updatedAt` (preserving
`id`/`createdAt`) on the update branch, so the test pins nothing for the upsert
ops — it remaps `id` to first-seen tokens in natural-key order and placeholders
both timestamps (the folders-remap `createdAt == updatedAt` invariant is dropped,
since an upsert-update legitimately differs). Each `upsert*` adds a private
find-by-key SELECT and mints via `clock::now_iso` + `uuid`.

  - `conversation_annotations.upsert` — find by (chatId, messageIndex,
    characterName); update sets only {content, sourceMessageId}. Added a nullable
    setter (`Option<Option<_>>`) for `sourceMessageId`. Harness
    `conversation_annotations_upsert_tier2_equivalence`.
  - `help_docs.upsertByPath` — find by `path`; update sets {title, url, content,
    contentHash}, leaving the `embedding` BLOB untouched; create stores a NULL
    embedding. The test proves an upsert-update preserves a non-null embedding.
    Harness `help_docs_upsert_tier2_equivalence`.
  - `provider_models.upsertModel` (+ a thin `upsertModelForProvider` loop) — find
    replicates v4's `findByProviderAndModelId`: `baseUrl` joins the predicate only
    when truthy (a falsy baseUrl leaves the column unconstrained — NOT "match
    NULL"). Update writes the full data. Harness
    `provider_models_upsert_tier2_equivalence`.
  - `plugin_config.upsertForUserPlugin` — find by (userId, pluginName); update
    MERGEs `{...existing, ...new}` config (corpus keeps the merge {}/single-key).
    Harness `plugin_config_upsert_tier2_equivalence`.
  - `character_plugin_data.upsert` — find by (characterId, pluginName); update sets
    {data} (open-JSON, {}/single-key). Harness
    `character_plugin_data_upsert_tier2_equivalence`.
  - `tfidf_vocabulary.upsertByProfileId` — find by `profileId`; update writes full
    data. Builds on the base-method-override minting (create/update mint
    `updatedAt` themselves). Harness `tfidf_vocabulary_upsert_tier2_equivalence`.

Phase 2 — a fifth parallel batch of five repos (`create` / `update` / `delete`
each, pinned ids + timestamps → zero normalization), spanning the main DB and the
mount-index sibling DB:

  - `chat_settings` (`quilltap-core::db::chat_settings`): a plain main-DB
    `AbstractBaseRepository`, and the **widest JSON-object surface in Phase 2** —
    ~33 columns including ~15 nested typed-struct JSON columns reproduced in schema
    field order (serde structs, not key-sorting `serde_json::Value`), nested integer
    fields typed `i64` so they render bare. Banks the **first INTEGER-affinity number
    column** (`sidebarWidth`, `.min(256).max(512)` — both bounds integer → INTEGER,
    unlike the prior min-only/bare REAL numbers). The `cheapLLMSettings` column keeps
    its uppercase acronym (camelCase would mangle it). The `*ForUser`
    default-injecting helpers and the multi-key open-JSON `tagStyles` key order are
    out of scope (the corpus keeps `tagStyles` `{}`). Harness
    `chat_settings_tier2_equivalence`.
  - `wardrobe` (`quilltap-core::db::wardrobe`, table `wardrobe_items`): the first
    repo whose **public CRUD is vault-only** — v4's `WardrobeRepository` writes to
    the document store and throws without a mount, with no SQL write mirror — so the
    differential drives v4's **real base-repository SQL CRUD** (`_create`/`_update`/
    `_delete`) against the table via a thin subclass exposing the protected
    internals (the marshaling the schema-translator builds from `WardrobeItemSchema`
    and the table's reads consume). Banks the first repo with **two JSON array
    columns** (`types` — the first enum-string array — and `componentItemIds`) and a
    **nullable soft-delete timestamp** (`archivedAt`, exercised null and
    set-to-non-null), alongside two booleans and several nullable string/UUID
    columns. The vault-overlay write path itself is NOT ported/verified (tracked
    deferral); the unarchive (`archivedAt` → NULL) nullable-setter is implemented but
    not in the corpus. Harness `wardrobe_tier2_equivalence`.
  - `doc_mount_files` (`quilltap-core::db::doc_mount_files`): a mount-index sibling-DB
    repo and the **narrowest tier-2 repo to date** (all-required columns, no JSON/
    boolean/nullable). Re-banks a REAL-affinity min-only int (`fileSizeBytes`,
    `.int().min(0)` → REAL, integer-collapsed) and two enum TEXT columns; v4's
    `getCollection` adds a non-UNIQUE sha256 lookup index that touches no row bytes.
    Harness `doc_mount_files_tier2_equivalence`.
  - `doc_mount_documents` (`quilltap-core::db::doc_mount_documents`): a mount-index
    sibling-DB repo — the database-backed file-content store keyed by a UNIQUE
    `fileId`. Banks a `plainTextLength` min-only REAL int, a UUID-as-TEXT UNIQUE
    natural key, and plain TEXT content/sha columns (the content-addressable +
    joined-view read helpers are out of scope). Harness
    `doc_mount_documents_tier2_equivalence`.
  - `doc_mount_chunks` (`quilltap-core::db::doc_mount_chunks`): a mount-index
    sibling-DB repo and the **first sibling-DB repo to carry a BLOB column** — the
    `embedding` Float32 little-endian BLOB (empty/null → NULL, dumped as hex for
    bit-exact compare, and a text-only update proven to leave it untouched, like
    `conversation_chunks`/`help_docs`) plus two REAL-affinity min-only int counters
    (`chunkIndex`/`tokenCount`) and a nullable `headingContext`. The `updateEmbedding`
    BLOB-mutating path is out of scope. Harness `doc_mount_chunks_tier2_equivalence`.

Phase 2 — the document-store STORAGE PRIMITIVE
(`quilltap-core::db::doc_mount_file_links`), build step 1 of the document-store
overlay slice. Ports v4's `writeDatabaseDocument` + `linkDocumentContent` +
`ensureLinkFolderId` — the byte-landing path every store-backed entity
(project/group store, character vault) ultimately calls. A
`(mountPointId, relativePath, content)` write is content-addressed by SHA-256 and
split across three tables in one transaction (find-or-create `doc_mount_files` by
sha → upsert `doc_mount_documents` by `fileId` → upsert `doc_mount_file_links` by
`(mountPointId, relativePath)`), with `doc_mount_folders` rows auto-created for any
parent path. Also ports the pure leaves it needs: `sha256OfString`,
`detectDatabaseFileType`, `normaliseRelativePath`, and the per-document policy
(`coercePolicyBool` / `policyFromFrontmatterData` / `policyFromContent`, scalar
frontmatter subset). The tier-2 differential (`doc_mount_file_links_tier2_equivalence`)
drives v4's REAL `linkDocumentContent` against a mount-index fixture and diffs all
FOUR resulting tables in the minted-values remap form, extended with a SHARED
cross-table id-map (so `document.fileId` / `link.fileId` / `link.folderId` /
`folder.parentId` FKs verify by relationship); `mountPointId` is the pinned seeded
store id. The corpus covers a fresh JSON + markdown write, subfolder creation,
dedup-by-sha (a second path with identical content reuses one file + one document
row), link upsert-in-place (rewriting a path), and the markdown frontmatter policy
cascade (`character_read: false` → all `allow*` = 0). The oracle drives
`linkDocumentContent` directly rather than `writeDatabaseDocument` to avoid the
post-write `reindexSingleFile` chunk/embed pass (which would mutate the link rows;
its only skip-switch, `QUILLTAP_JOB_CHILD=1`, reroutes repos through the
forked-child write proxy). Deferred: arbitrary-YAML frontmatter (scalar subset
only — lands with the character-vault YAML decision), the UTF-16 `plainTextLength`
vs UTF-8 `fileSizeBytes` split is reproduced but only exercised on ASCII content,
and `linkBlobContent` / the read/GC/conversion helpers.

Phase 2 — the document-store OVERLAY ENGINE + the `groups` store-backed pilot
(`quilltap-core::db::{document_store_overlay, ensure_official_store, groups}`),
build steps 2-3 of the overlay slice. Ports v4's generic
`createDocumentStoreOverlay` + `AbstractStoreBackedRepository` as a Rust generic
over a `StoreEntity` trait, plus `ensureOfficialStore` provisioning, bound to
`groups`. A group's substantive content lives not in `groups` columns but in its
official document store as four overlay files (`properties.json` — the typed
`color`/`icon` bag in schema order, 2-space pretty-print; `description.md` /
`instructions.md` — raw markdown, empty → `null` on read; `state.json`). The slim
row (id/name/officialMountPointId/timestamps) lives in the MAIN db, the store in
the MOUNT-INDEX db, so `GroupsRepository` spans both connections (new
`Writer::connection()` seam). Reads overlay the store (the `doc_mount_documents`
3-table path→content join, new `find_[many_by]_mount_point[s]_and_path`); writes
route store-resident fields to the store and strip them from the slim patch
(properties via read-modify-write so a partial patch preserves untouched keys);
create runs the 5-step sequence (slim row → provision a `Group Files: <name>`
mount point + link + raw FK → write the four files → overlay re-read). Failure is
asymmetric (v4): `find_by_id` THROWS `OverlayError::Unavailable`, `find_all` DROPS
the bad row. Also ports the pure `nextUniqueMountPointName` (tier-1 unit test).
The tier-2 differential (`groups_tier2_equivalence`) drives v4's REAL
`repos.groups.create`/`.update` end-to-end (no mocked storage boundary, no
`QUILLTAP_JOB_CHILD`) and diffs SEVEN tables across BOTH dbs — the slim `groups`
row + `doc_mount_points` / `_files` / `_documents` / `_file_links` / `_folders` +
`group_doc_mount_links` — in the minted-values remap form with ONE shared
cross-db id-map (so `groups.officialMountPointId` → the store, `link.fileId` →
`file.id`, etc. verify by relationship). v4's post-write `reindexSingleFile` runs
(database-backed stores chunk with no model — deterministic); its only divergence,
the link `chunkCount` + the derived `doc_mount_chunks` rows, is pinned/excluded.
The corpus banks the 5-step create, `properties.json` byte-exact (both keys + the
empty bag), a store-only update (slim `updatedAt` NOT bumped) with a properties
RMW that preserves the untouched `icon`, a DB-only `name` update (store
untouched), dedup-by-sha (`"{}"` shared by three links across two stores; `""` by
two), and orphan-on-rewrite. A second test banks the keystone throw-vs-drop
asymmetry. Deferred: step-2 store adoption (the startup-heal heuristic — the
corpus always provisions fresh), `state`/property null-vs-absent + multi-key
insertion order (open-JSON seam — corpus kept `{}`/single-key), and the
`projects` generalization (a larger bag + roster ops).

Phase 2 — the character vault **managed-fields write projection**
(`quilltap-core::db::vault_character_write::write_character_vault_managed_fields`),
v4's `writeCharacterVaultManagedFields` — the first piece of the `characters`
repo (a `TaggableBaseRepository` with a bespoke vault overlay, not a generic
store-backed entity). Projects every vault-managed content field of a character
out to its file, in v4's exact order: `properties.json` (the typed
`pronouns`/`aliases`/`title`/`firstMessage`/`talkativeness` bag, 2-space
pretty-print), the five markdown files (`identity` / `description` / `manifesto`
/ `personality` / `example-dialogues`, `None` → `""`), and — only when a primary
`physicalDescription` is present — `physical-description.md` +
`physical-prompts.json` (`renderPhysicalPromptsJson`), then the `Prompts/` and
`Scenarios/` folder projections. Composes the already-ported pure leaves
(`build_system_prompt_file` / `build_scenario_file` / `sanitize_file_name`) and
the folder projector (`project_array_into_vault_folder`) over the document-store
write primitive. `properties.json` feeds the content-dedup SHA, so an
integer-valued `talkativeness` (e.g. `1.0`) is serialized as the bare integer `1`
(a `serialize_with` mirroring `js_number_to_json`) to match `JSON.stringify`
byte-for-byte; the five `properties.json` keys are a typed struct (serde
preserves struct field order, unlike `serde_json::Value`). Verified by a tier-2
differential (`vault_character_write_equivalence`) driving v4's REAL
`writeCharacterVaultManagedFields` over a two-op sequence (a full create with a
`Prompts/` filename collision `Default Voice.md`/`Default Voice-1.md` and two
scenarios, then a reproject that sweeps the dropped prompt + both old scenarios,
clears `physicalDescription` — physical-* files PERSIST, v4 skips and does not
delete — and renders `talkativeness: 1`) and diffing five mount-index tables in
the shared-cross-table-id-map remap form; plus four exact unit tests. v4's
post-write reindex runs (database-backed chunking, no model); its only divergence
(link `chunkCount` + `doc_mount_chunks`) is pinned/excluded, exactly as the
groups/projects/wardrobe store-backed tests do.

Phase 2 — the character vault **wardrobe write projection**
(`quilltap-core::db::vault_wardrobe_write`), v4's `projectVaultWardrobe` +
`projectArrayIntoVaultFolder` — the final wardrobe write piece, and with it the
whole document-store slice is complete. Re-projects an authoritative
`WardrobeItem` list into a vault store's `Wardrobe/` folder: each item is written
as `Wardrobe/<title>.md` (filename collisions disambiguated with `-1`/`-2`/…
suffixes), any `.md` file in the folder not produced by the current list is swept,
and the legacy `wardrobe.json` is deleted so the folder layout is the single
on-disk source. Composes the already-ported pure leaves
(`build_slug_by_item_id_map`, the Decision-A `build_wardrobe_item_file` emitter,
`sanitize_file_name`) over the document-store write primitive
(`write_database_document`) and a new GC delete (`delete_database_document` +
`delete_with_gc`: unlink, then drop the file row when its last link is gone —
chunks/documents cascade via the FK). Verified by a tier-2 differential
(`vault_wardrobe_write_equivalence`) driving v4's REAL `projectVaultWardrobe` over
a two-op sequence (an initial 5-item projection with a `Hat.md`/`Hat-1.md`
filename collision and a composite emitting `componentItems` slugs, then a rename
that sweeps the old file + recomputes the composite's slug and removes two items)
and diffing five mount-index tables (`doc_mount_points` / `_files` / `_documents`
/ `_file_links` / `_folders`) in the shared-cross-table-id-map remap form. v4's
post-write reindex runs (database-backed chunking, no model); its only divergence
(link `chunkCount` + `doc_mount_chunks`) is pinned/excluded, exactly as the
groups/projects store-backed tests do.

Phase 2 — the character vault **wardrobe YAML emitter** (Decision A — the only
eemeli/yaml site), `quilltap-core::vault_overlay::build_wardrobe_item_file`, v4's
`buildWardrobeItemFile`. Projects a `WardrobeItem` to its `Wardrobe/*.md` content:
a YAML frontmatter block (keys in v4's exact insertion order; `componentItemIds`
translated to slugs with a UUID fallback) plus the description body. Per locked
Decision A the YAML is hand-rolled — the emitted bytes feed the content-dedup
SHA, so a quoting mismatch is a silent mis-dedup, not just a test gap. The emitter
is a faithful port of eemeli/yaml 2.9.0's `stringifyString` + `foldFlowLines`
(default options) for the bounded value space (string scalars, the boolean `true`,
block sequences of string scalars): plain/single/double quote selection, the
core-schema reparse-safety quoting (a scalar that would reparse as
number/bool/null is quoted), line folding past width 80, and block scalars
(`|`/`|-`/`>`) for multiline values. It operates on UTF-16 code units throughout
(as JS does) so fold offsets, the control-char force-quote check (matched on code
points, per eemeli's `/u` flag — a valid astral character is not a surrogate
match), and `JSON.stringify` escaping align byte-for-byte. Verified by a tier-1
differential (`vault_wardrobe_emit_equivalence`) against v4's real
`buildWardrobeItemFile` over a 100-item corpus spanning every quoting edge,
folding, block scalars, surrogate-pair fold offsets, the slug/UUID map, and all
flag branches; plus three exact unit tests. This was the last open vault decision;
the only wardrobe write piece still ahead is the stateful folder projection
(`projectVaultWardrobe` — filename dedup/rename/sweep + multi-table writes).

Phase 2 — the character vault **wardrobe read overlay**
(`quilltap-core::db::vault_read_overlay::read_character_vault_wardrobe` +
`quilltap-core::vault_overlay::resolve_and_check_component_items`), v4's
`readCharacterVaultWardrobe`. Enumerates `Wardrobe/*.md` (the Decision-B code-unit
sort, then `parseWardrobeItemFile`, dropping unparseable files), builds the
in-vault slug/id lookup maps (first-claimer wins a slug; every item is addressable
by id), and resolves each item's raw `componentItems:` refs to canonical ids —
slug-first then UUID, unknown refs dropped — before a cycle check that clears any
item whose resolved components form a cycle. The cycle pass reads the **live**
(already-mutated) component lists, so clearing one item mid-pass changes later
items' walks, exactly mirroring v4's mutable `itemById` (proven in the corpus: a
mutual `a → b`/`b → a` cycle clears `a`, then `b` survives because `a` was already
emptied when `b`'s walk ran). An empty/missing `Wardrobe/` folder falls through to
the legacy `wardrobe.json` (`parseLegacyWardrobeJson`); neither present → `null`.
Verified by a read-differential (`vault_wardrobe_read_equivalence`, three cases)
driving v4's REAL `readCharacterVaultWardrobe` over a shared seeded fixture —
slug/UUID/collided-slug/unknown resolution, the live-mutation cycle asymmetry, a
self-cycle clear, an archived item, the legacy fallback, and the empty-vault
`null` — comparing each `{ items } | null` exactly (no normalization; this read
path mints no clock value). Plus four tier-1 unit tests on the resolver.
**Tracked deferral:** the archetype-seeding branch (`findArchetypes` over the
General/project `Wardrobe` stores) is not ported — the corpus keeps no General
store provisioned, so v4's `findArchetypes` returns `[]` and the seed is a
verified no-op.

Phase 2 — the character vault **read overlay** (`quilltap-core::db::vault_read_overlay`),
the heart of the Family-B read path: v4's `hydrateOne` + `applyDocumentStoreOverlay`
+ `applyDocumentStoreOverlayOne`. Folds a character's vault files onto the
character so every read sees vault values transparently. Because the overlay is a
plain JSON merge, the port operates on the character as a `serde_json::Value`
object (not a fully-typed `Character`), patching the managed keys with values from
the already-ported pure parsers: `properties.json` →
pronouns/aliases/title/firstMessage/talkativeness; the five markdown fields
(identity/description/manifesto/personality/exampleDialogues) via
`markdownToNullable` (empty → null); `physical-description.md` +
`physical-prompts.json` → `physicalDescription` (base-reuse when the character
already has one, else a minted base with `stableUuidFromString('physical:<mp>')` +
clock-minted timestamps); `Prompts/*.md` → `systemPrompts` (the Decision-B
code-unit sort + parse + the exactly-one-`isDefault` normalization: keep the first
declared default and demote the rest, or promote the first when none is marked);
`Scenarios/*.md` → `scenarios`. The keystone is `properties.json`: a linked vault
that lacks it is broken — the batched apply DROPS the character (one corrupt vault
can't take down the roster) while the single apply returns an Unavailable error
(v4 throws → 503). Verified by a read-differential
(`vault_read_overlay_equivalence`) driving v4's REAL `applyDocumentStoreOverlay`
over seven input characters against a six-store seeded fixture — pass-through, full
overlay, drop, partial (arrays replaced with `[]`), physical mint, and all three
prompt-default cases — comparing the hydrated characters exactly (only the minted
physical timestamps placeholdered), plus the `…One` throw on the broken vault.

Phase 2 — the vault read overlay's directory-listing load
(`DocMountDocumentsRepository::find_many_by_mount_points_in_folder`), the first
stateful sub-unit of the character read overlay (Family B). Ports v4's
`findManyByMountPointsInFolder`: the 3-table join with a SQL
`LOWER(relativePath) LIKE '<folder>/%'` prefilter, then v4's JS post-filter
(case-folded prefix, non-empty remainder, single-level only — no `/` in the
remainder — and an extension match). The overlay-consumed subset of the row is
returned (`content`/`mountPointId`/`relativePath`/`fileName` + the document
`createdAt`/`updatedAt`); v4's unused `recursive` option is not ported. Verified
by the first **read-differential**: a fixture builder seeds two pinned stores and
writes a corpus via v4's real `linkDocumentContent` (driven directly — not
`writeDatabaseDocument`, whose `QUILLTAP_JOB_CHILD=1` skip-switch reroutes repos
through the forked-child write proxy and breaks `initializeDatabase`); both v4 and
the Rust port then READ the SAME fixture, so minted ids/timestamps are identical
and the returned rows compare exactly (sorted by `(mountPointId, relativePath)`,
the read having no defined order). The corpus covers the IN-clause across two
stores and excludes a top-level file, a nested file, and a wrong-extension file,
plus the empty-mount-point short-circuit (`vault_folder_read_equivalence`).

Phase 2 — the vault `Wardrobe/*.md` parser
(`quilltap-core::vault_overlay::parse_wardrobe_item_file`), the third and last
per-file frontmatter parser. Reuses the title fallback chain (frontmatter `title`
→ first `# heading` → filename-without-`.md`) and the already-ported
`parse_wardrobe_types_field` (a valid `types` list is required, else skip) /
`parse_component_items_field` (raw author refs kept for the overlay's later
resolution pass). Reproduces the id sanity check (`/^[0-9a-f-]{36}$/i` — 36 chars,
hex-or-`-`; otherwise `stableUuidFromString`, incl. a 36-char non-hex id that must
fall back), the non-empty-string fields (`appropriateness`/`imagePrompt`), the
boolean flags (`default || isDefault`, `replace`), the `archivedAt` precedence
(non-empty string wins, else `archived: true` → `doc.updatedAt`), the
`typeof === 'string'` keep of `migratedFromClothingRecordId` (incl. empty), and
the frontmatter-vs-doc timestamp precedence. Output is built directly (not via
Zod), so its nullable fields are ALWAYS present (`null` or value) and a heading
used as the title is dropped from the body (an empty body → `null` description,
NOT a skip). Tier-1 exact differential (`vault_wardrobe_item_file_equivalence`)
over 20 cases against v4's real `parseWardrobeItemFile`.

Phase 2 — the vault frontmatter READ parsers
(`quilltap-core::vault_overlay::parse_prompt_file` / `parse_scenario_file`),
built on the hand-rolled frontmatter reader. Each turns a vault markdown file
into a `CharacterSystemPrompt` / `CharacterScenario`, or `None` (skip — the
overlay falls back to the DB value for that one file). Faithful to v4: the
objects are built directly (not via Zod), so the JS `.trim()` / `.slice(0, n)`
caps are reproduced with the `jsstr` UTF-16 primitives (name ≤100, title ≤200,
description ≤500); `isDefault` is `=== true` (a `"true"` string → false); the
prompt body is the content after the frontmatter, `trimStart`ed; scenario title
resolution is frontmatter `name` → first `# heading` (`/^#\s+(.+)$/` with the JS
whitespace set) → filename-without-`.md`, and a heading used as the title is
dropped from the body while a frontmatter-supplied title leaves the body intact.
Added `jsstr::js_trim_start` and `markdown::body_after` (UTF-16-offset → byte
slice). Tier-1 exact differential (`vault_frontmatter_parsers_equivalence`) over
26 cases against v4's real `parsePromptFile`/`parseScenarioFile`, incl. multibyte
content to cover the UTF-16 body offset and every skip condition.

Phase 2 — the Markdown frontmatter parser + a hand-rolled YAML reader
(`quilltap-core::markdown::parse_frontmatter`), the shared read-path foundation
for the vault's per-file parsers. v4's `parseFrontmatter`
(`lib/doc-edit/markdown-parser.ts`) calls eemeli/yaml's `YAML.parse`; the read
side is the companion to locked Decision A, so this hand-rolls a parser for the
constrained subset our own emitters produce plus simple hand-edits — no YAML
crate in the vault — matching eemeli/yaml's **YAML 1.2 core-schema** output on
that subset. Reproduces the structural logic exactly (the `---\n`-only opener so
CRLF frontmatter isn't recognized; the exactly-`---` closing line; UTF-16
`bodyStartOffset` computed even when the YAML fails to yield an object;
empty/whitespace/comments-only → `{}`; array/scalar root → null; duplicate keys
→ null, since eemeli throws) and the scalar resolution (`~`/`null`/empty → null;
`true`/`false` case-variants → bool while `yes`/`no` stay strings; decimal
int/float → number; ISO timestamps and URLs stay strings; double-quoted
JSON-style escapes incl. `\uXXXX`; single-quoted `''`; the whitespace-gated `#`
comment rule; flow `[a, b]` and block `- item` sequences). Tier-1 exact
differential (`markdown_frontmatter_equivalence`) over 52 cases against v4's real
`parseFrontmatter`. Nested maps, flow maps, block scalars, anchors/tags, and
exotic numbers (hex/octal/exponent/`.inf`/`.nan`) are the documented
out-of-subset seam — kept out of the corpus; they resolve conservatively (a
null/string or a parse error), never to a silently-wrong typed value.

Phase 2 — the legacy `wardrobe.json` migration parser
(`quilltap-core::vault_overlay::parse_legacy_wardrobe_json`), the next
decision-free vault-overlay leaf (Family B). Unlike the two JSON projection
parsers, this validates an array of full `WardrobeItemSchema` items, so it
reproduces Zod 4's `z.uuid()` and `z.iso.datetime()` string formats verbatim
(the regex sources lifted from the live schema: version-nibble `[1-8]` /
variant `[89abAB]` UUIDs plus the all-zero/all-`f` sentinels; ISO dates with
leap-year arithmetic and a `Z`-only zone; JS `\d` rewritten to ASCII `[0-9]`).
Faithful to Zod's rules — any single bad item nulls the whole array; `.default()`
keys (`componentItemIds`/`isDefault`/`replace`) are materialized; output is in
schema order regardless of input key order; unknown keys are stripped (root
`presets`, per-item extras, in-`outfit` extras); and a present `outfit` is
validated (a malformed one fails the parse) then discarded — only `{ items }` is
returned. Tier-1 exact differential (`vault_legacy_wardrobe_equivalence`) over 39
cases against v4's real `parseLegacyWardrobeJson`, covering the valid shapes
(full/minimal-with-defaults/all-nulls/multi/empty/presets-stripped/outfit-valid)
and every interesting violation (bad/missing id, empty/missing title, bad-enum/
empty/non-string types, bad-uuid/non-array/null componentItemIds, non-bool/null
booleans, bad timestamps incl. non-leap `2023-02-29`, offset-zone, no-zone, and
trailing-newline rejection — confirming the `regex` `$` matches JS's absolute-end
anchor).

Phase 2 — the vault JSON projection parsers (`quilltap-core::vault_overlay`), the
next decision-free slice of the character/wardrobe vault overlay (Family B, build
step 6). `parseVaultProperties` + `parseVaultPhysicalPrompts` reproduce v4's Zod
`safeParse`-then-fall-back-to-`null` semantics (`vault-overlay/parsers.ts`): parse
the file JSON, validate against the vault schema, return the typed value or `None`
on a JSON-parse error OR any schema violation. Faithful to Zod's rules — unknown
keys stripped (default `z.object`, top-level and inside `pronouns`); a
`.nullable()` field is required-present (key must exist, value may be `null`) and
serializes `null` when unset; a `.nullable().optional()` field may be absent;
`talkativeness` is range-checked `0.1 ≤ t ≤ 1.0`; the nested `pronouns` fields are
required strings of 1–20 UTF-16 code units. Tier-1 exact differential
(`vault_json_parsers_equivalence`) over 24 cases against v4's real functions
(valid/all-nulls/extra-stripped/invalid-JSON/non-object/missing-key/range-bounds/
non-array-aliases/non-string-element/pronoun-missing-field/too-long/empty/
wrong-type), with integer-valued floats canonicalized on both sides so
`talkativeness: 1.0` (which v4 emits as `1`) compares equal. (`headAndShoulders`
present-`null` is the one tracked null-vs-absent divergence, kept out of the
corpus.)

Phase 2 — the vault write-projection string leaves (`quilltap-core::vault_overlay`),
the next decision-free slice of the character/wardrobe vault overlay (Family B,
build step 6). Five pure functions from v4's `character-vault.ts`:
`slugifyWardrobeTitle` (kebab slug — `toLowerCase` → JS-trim → collapse
non-`[a-z0-9]` runs to `-` → strip ends; the `[^a-z0-9]→-` filter makes it
collation/case-safe, so no ICU per the locked Decision B), `buildSlugByItemIdMap`
(first-wins `(itemId → slug)` list), `sanitizeFileName` (replace `\ / : * ? " < >
|` with `_`, collapse JS-whitespace runs, JS-trim, 100-UTF-16-unit slice,
`untitled` fallback — reusing the existing `jsstr` whitespace/trim/UTF-16
helpers), `buildSystemPromptFile` (the `Prompts/*.md` frontmatter, exercising the
private `escapeYaml` = `if /[:#"'\n]/ then JSON.stringify(v) else v`, reproduced
with `serde_json::to_string` which matches `JSON.stringify` for strings), and
`buildScenarioFile` (plain `# title\n\nbody`, no frontmatter). Tier-1 exact
differential (`vault_string_leaves_equivalence`) over 27 cases against v4's real
functions, incl. unicode→dash slugs, punctuation, the `escapeYaml` quote triggers
(`:`/`#`/`"`/`'`/`\n`), and the empty→`untitled` filename path. Per the locked
decisions, this confirms the prompt/scenario write projections need NO eemeli/yaml
(only `Wardrobe/*.md`, build step 7, does) and the slug path needs no ICU.

Phase 2 — the vault wardrobe-component pure leaves (`quilltap-core::vault_overlay`),
the first slice of the character/wardrobe vault overlay (Family B, build step 6),
ported leaf-first ahead of the stateful overlay so the YAML-emitter and
ICU-collation decisions the *write* path forces are not yet on the critical path.
Three decision-free pure functions: `parseComponentItemsField` (coerce a raw
`componentItems:` value → clean `Vec<String>`: non-arrays → `[]`, trim, drop
empty/non-string), `parseWardrobeTypesField` (validate a `types:` value against
`WardrobeItemTypeEnum` — all-or-nothing, de-dup first-seen, `None` on
empty/invalid), and `detectComponentCycles` (the save-time component-graph cycle
check: direct self-ref, indirect, sub-cycle, diamond-safe, deep-chain). Tier-1
exact differential (`vault_component_leaves_equivalence`) over 22 cases against
v4's real `parsers.ts` / `expand-composites.ts`. No YAML, no
case-mapping/collation — the JSON/array/graph leaves the vault needs, verified
before the projection that consumes them.

Phase 2 — `doc_mount_blobs` (`quilltap-core::db::doc_mount_blobs`), build step 8
of the document-store overlay slice: the document store's **binary** byte-store,
the sibling of the (ported) text store `doc_mount_documents`. Bytes (avatars,
PDF/DOCX content, any non-text) live in a `data BLOB NOT NULL` column keyed UNIQUE
by `fileId`. Unlike the Zod-schema repos, v4 hand-writes this repo and its DDL —
the `data` column is deliberately ABSENT from `DocMountBlobMetadataSchema`
(metadata reads never hydrate the bytes) — so the port reproduces the hand-written
`CREATE TABLE` verbatim (incl. the `FOREIGN KEY (fileId) REFERENCES
doc_mount_files(id)`). Ports `upsertByFileId` (insert-or-replace by `fileId`,
**recomputing `sha256` from the actual bytes** — the caller's sha is advisory —
with `sizeBytes = data.len()`; an existing row overwritten in place) plus the
metadata/`readData`/`delete` accessors. The tier-2 differential
(`doc_mount_blobs_tier2_equivalence`) drives v4's REAL `upsertByFileId` against a
mount-index fixture that seeds the parent `doc_mount_files` rows the FK requires
(enforced under the writable open's `foreign_keys = ON`), and diffs the table with
the `data` BLOB dumped as lowercase hex (bit-exact, mirrors `help_docs` /
`doc_mount_chunks`) in the minted-values remap form (`id` remapped, timestamps
placeholdered; `fileId` pinned, content compared directly). Banks a fresh insert,
an overwrite-in-place on a repeat `fileId`, the sha-recompute rule (every op
passes an all-zero advisory sha), and a non-UTF-8 binary payload (a PNG header +
`deadbeef`) round-tripping through the BLOB. `linkBlobContent` (the
`(mountPointId, relativePath)` content/link split, the binary analogue of
`linkDocumentContent`) remains deferred.

Phase 2 — `stableUuidFromString` (`quilltap-core::vault_overlay`), build step 5
of the document-store overlay slice: the first **character/wardrobe vault** leaf,
ported leaf-first ahead of the stateful vault overlay (Family B). It derives the
deterministic id every folder-enumerated vault entity (system prompts, scenarios,
wardrobe items) carries — `stableUuidFromString('<kind>:<mountPointId>:<relativePath>')`
— which chat references depend on. SHA-256 over the source's UTF-8 bytes → first
16 bytes → version nibble 8 (custom) + RFC-4122 variant → hyphenated lowercase
hex. Tier-1 exact differential (`stable_uuid_equivalence`) against v4's real
function over the `prompt:`/`scenario:`/`wardrobe-item:` prefixed forms, an empty
string, and a non-ASCII path (SHA-256 runs over UTF-8 both sides — the accented
source agrees byte-for-byte; there is no case mapping here, unlike the
`toLowerCase`/`localeCompare` seams).

Phase 2 — the `projects` store-backed entity + the store-backed GENERALIZATION
(`quilltap-core::db::{store_backed, projects}`), build step 4 of the overlay
slice. Generalizes the slim-row plumbing + provisioning that `groups` proved into
a reusable `StoreBackedRepository<E: StoreEntity>` (v4's
`AbstractStoreBackedRepository`): the `StoreEntity` trait gains `slim_table` /
`store_name_prefix` / `find_store_links` / `link_store`, and `ensure_official_store`
becomes generic over `E` (the group/project ensure wrappers collapse into one).
`GroupsRepository` is refactored to a thin wrapper over the generic base (still
green); `projects` is the second instance. `ProjectsRepository` adds the **16-key
`properties.json` bag** (`ProjectPropertiesSchema` — five Zod-`.default` keys
ALWAYS materialized in schema order: `allowAnyCharacter` / `characterRoster` /
`defaultDisabledTools` / `defaultDisabledToolGroups` / `backgroundDisplayMode`; the
other eleven `.nullable().optional()` → `skip_serializing_if`) and the
**character-roster operations** (`addToRoster` / `removeFromRoster` /
`setAllowAnyCharacter` / `canCharacterParticipate` / `findByCharacterId`), each a
`properties.json` read-modify-write through `update` (or an in-memory `findAll`
filter). The tier-2 differential (`projects_tier2_equivalence`) drives v4's REAL
`repos.projects.create`/`.update`/roster ops end-to-end and diffs the same seven
tables across both dbs (the slim `projects` row + the store tables +
`project_doc_mount_links`) in the shared-cross-db-id-map remap form, `chunkCount`
pinned + `doc_mount_chunks` excluded (database-backed reindex uses no model). The
corpus banks a rich create (roster + color + `defaultImageProfileId` +
`backgroundDisplayMode`, the optional keys interleaved with the materialized
defaults in schema order — byte-exact) and a minimal create (only the five
defaults), `addToRoster`/`removeFromRoster` (the `characterRoster` array RMW
preserving the other fifteen keys), `setAllowAnyCharacter` (a bool RMW), and a
DB-only `name` update. The `ensureOfficialStore` step-2 adopt branch stays
deferred (corpus always provisions fresh); the property null-vs-absent +
multi-key insertion-order seam is unchanged (corpus kept to present/absent +
`{}`/single-key `state`).

Docs — the document-store-overlay design slice
(`docs/developer/porting/document-store-overlay.md`): the port plan for the
store-backed entities (`projects`, `groups`, `characters`, the `wardrobe` vault).
Establishes that the "document store" is DB rows in the mount-index DB (text in
`doc_mount_documents`, binary in `doc_mount_blobs`), not filesystem files, so no
filesystem fixture is needed; maps the generic overlay engine
(`createDocumentStoreOverlay` + `AbstractStoreBackedRepository`) shared by projects
and groups vs the heavier character/wardrobe markdown-vault family; sets a
dependency-first build order (port `doc_mount_file_links` + `linkDocumentContent` +
`writeDatabaseDocument` first, then the engine, then `groups` as pilot, then
`projects`); and specifies the tier-2 oracle strategy (drive v4's real storage code
against the existing mount-index fixtures with `QUILLTAP_JOB_CHILD=1`, dump the four
storage tables + the slim row, minted-values remap form). Linked from `overview.md`
and `CLAUDE.md`.

