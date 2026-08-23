# Quilltap Changelog

Newest first. Each entry is one commit: the header carries the commit date
and the commit subject; the line beneath it records the crate versions that
commit bumped (or notes a docs-only change). Entries up to 2026-08-18 were
restructured mechanically from git history and also carry the short commit
hash; new entries omit the hash (it doesn't exist when the entry is
written — see `.claude/commands/commit.md` §7 for the format). Older months
are archived under `docs/changelog/`.

Archived months: [July 2026 (days 16–end)](changelog/2026-07b.md), [July 2026 (days 1–15)](changelog/2026-07a.md), [June 2026](changelog/2026-06.md).

## August 2026

#### 2026-08-23 — fix(model): the wire layer honors the attachment anchor instead of re-stamping last-user (bug 95, the stamp)

_Versions: core 0.0.633, host 0.0.78._

The downstream-stamp measurement the order required, and the one real
re-anchor it found: the streaming spine and the google merge are faithful
(per-message placement flows through; google merges per user-run exactly
as v4's plugin does), but the non-streaming regenerate funnel flattened
attachments into `CompletionParams.attachments` and the wire layer
re-stamped them onto the LAST user message — undoing the anchor whenever
a staff whisper trails the human's turn. Fixed with a defaulted
`send_message_with_anchor` trait method (canned/test providers ignore it;
their keys deliberately never see placement), the anchor threaded through
`execute_completion_with_anchor` into `request_input_from_params` (mapped
across the tool-role drop, last-user kept as the floor), the host's
`WireCompletionProvider` override, and the regenerate funnel computing
the carrier's position. Every pre-existing caller is byte-identical
(`CompletionParams` untouched — extending it would have edited a sibling
lane's file). Pinned by the updated last-user test, a new
anchored-placement unit test, and a new wire-byte pin through the full
composition on Z.AI (the anchored message's `image_url` part vs the
trailing whisper's plain string); neutralizing the anchored slot reddens
both.

#### 2026-08-23 — fix(chat): attachments anchor to the human's turn, not the last user-role message (bug 95)

_Versions: core 0.0.632, harness 0.0.555._

Ports v4 `a14a1811`'s bug-95 fix: staff whispers format as role=user, so
"the last user-role message" stopped meaning the human's turn — on a
regenerate the image landed on a connection-profile-change bubble, and
after a tool call attachments dropped silently. The message selector's
main-loop copy now carries the source row id (the force-include arm still
omits it, as v4's does); `ContextMessage.metadata` gains `isUserTurn` and
the history/new-user pushes stamp their metadata; and the new
`select_attachment_anchor_index` (this turn's user input → the last
genuine human turn by id, captured pre-normalization with the
`!systemSender` clause → the old rule as a floor → -1 with a warn)
replaces last-user-wins for BOTH the Lantern prefix and the attachment
splice, with the final construction extracted and unit-pinned. Pinned by
the new tier-1 `attachment_anchor_equivalence` family (v4's six shapes by
name + 12 adversarial rows, each of the three scans droppable only redly)
and seven affected families regenerated fresh at the pin — the
message-selector comparator was id-blind and now compares it (red-first
proven), and both build-context metadata stamps are mutation-proven live.

#### 2026-08-23 — fix(files): images route to the describer when the plugin can't send them (bug 91, the wirings)

_Versions: core 0.0.631, harness 0.0.554._

Wires the transport predicate into the three fallback sites, mirroring v4
`a14a1811`: `needs_fallback_processing` gains the image-only second arm
(model reads it but the plugin can't send it → describe-fallback, with
v4's info log), the auto-pick describer filter excludes non-transporting
providers, and a configured describer whose plugin cannot transport
answers `unsupported` with v4's guard sentence byte-exact — before any
model call is made. `file_attachment_tier3` regenerated at the pin with
six new ops (the Ollama-vision route, the non-image control, the
describer guard with the send-never-made assert at the mock level, the
auto-pick exclusion) over a fixture whose uncensored describer is
corrected to a transporting provider as v4's test was — Z.AI rather than
v4's OpenRouter pick, since OpenRouter only transports in the tier v4's
tests see (the unit-2 upstream finding). Three mutations proven red.

#### 2026-08-23 — feat(files): the image-transport predicate pair (bug 91, the predicate half)

_Versions: core 0.0.630, harness 0.0.553._

Ports v4 `a14a1811`'s two-tier "can this plugin actually send an image?"
predicate: the new `files/image_transport.rs::provider_can_transport_images`
(manifest tier → static tier → unknown-is-true) and
`attachment_support.rs::static_provider_can_transport_images` (keys off the
types list after the known-provider guard). The static map gains the §C1
rows — NANOGPT and Z_AI with the four image types, DEEPSEEK known-empty —
which also moves the settings route's `supportsImageUpload` create default
for those providers exactly as v4's shared map does. The registry-vs-static
collapse in v5 is measured and recorded in the module header: manifests are
baked, so the manifest tier answers for every known provider and the static
tier only for names the manifest set lacks. Pinned by the new tier-1
`image_transport_equivalence` family driving v4's REAL code in BOTH
configurations (registry uninitialized → static fallback, and initialized
with all ten real dist plugins → production truth; 51 rows). The NANOGPT
registry row rides an ACTIVATE-AT-UNIFY gate (`P4D107_NANOGPT_MANIFEST_
LANDED`) that self-arms when P4.D107's manifest regen lands. Two findings
recorded for upstream: v4's OpenRouter plugin registry entry declares
`supportsAttachments: false` while its static map and working vision path
say otherwise (production routes OpenRouter vision profiles to the
describe-fallback), and v4's own unit tests only ever see the static tier.
Not yet wired — the three fallback-site wirings are the next unit.

#### 2026-08-23 — feat(llm): provider moderation refusals named, not blamed on the weather (bug 93)

_Versions: core 0.0.629, harness 0.0.552._

Ports v4 `a14a1811`'s bug-93 fix end-to-end. The new
`quilltap-core::moderation_finish_reason` module carries the ten-literal set
(incl. the hyphenated `content-filter` the v4 docblock doesn't call out),
`is_moderation_finish_reason` (trim + lowercase set membership, no substring
guessing), and `describe_moderation_refusal` with v4's sentence byte-exact.
`get_empty_response_reason` gains the moderation first branch ahead of its
five pre-existing sentences (byte-unchanged) plus the uncensored-retry
suffix; the orchestrator's empty-response arm extracts the finish reason
from the raw response, threads provider/model, and its warn payload gains
`finishReason` + `moderationRefusal`. Pinned by the new tier-1
`moderation_finish_reason_equivalence` family (35 module rows + 12
`getEmptyResponseReason` matrix rows over v4's REAL code; dropping a
literal, neutralizing the first branch, and un-threading the orchestrator
each proven red) and a new `moderation_refusal` op in the orchestrator
tier-3 corpus — v4's real `processMessage` answers the moderation sentence
in the done frame where the "known issue" copy used to appear, and v5's
spine reproduces it. v4's module docblock mis-numbers itself "bug 94"; the
port records the discrepancy and keeps the code. Also mirrors v4's
bug-88…95 docs + `bugs.md` index into `docs/v4` and banks the two help-file
sections to the `p4.9i2` row.
#### 2026-08-23 — docs(porting): the P4.D107 verification gate record

_Docs-only change._

The lane's gate appended to `status-log.md`: fmt and both clippy feature
sets clean; `cargo test --workspace` 446 binaries / 2,270 tests / 0
failed with the lane's oracle env block, the three families confirmed to
have RUN by name; the sweep driver 3/3 ok over oracles regenerated fresh
against v4 at `a14a1811`; the corpus greps and the nine-sibling
byte-identity check.

#### 2026-08-23 — docs(v4): mirror the plugin-author attachment contract

_Docs-only change._

`docs/v4/developer/PROVIDER_PLUGIN_DEVELOPMENT.md` refreshed from v4
`a14a1811`: the new attachment-contract section for plugin authors — the
two questions the host asks before an image reaches a plugin, the rule
that declaring `supportsAttachments: false` is a respectable answer while
declaring `true` and dropping the bytes is not, the honest-ledger
requirement, and the guidance against a router plugin keeping its own
vision-model list.

The NanoGPT plugin README has no mirror here (`docs/v4/` mirrors v4's
`docs/` tree only), and the bug 91–95 write-ups are left for the sibling
lanes so no two lanes write the same mirror file.

Audited tree-wide: the retired "NanoGPT chat requests are text-only in
Quilltap" sentence survives nowhere in v5 outside porting records.

#### 2026-08-23 — feat(nanogpt): serialise image_url and report the real attachment ledger

_Versions: core 0.0.630, harness 0.0.552._

v4 `a14a1811` (bug 91, NanoGPT plugin 1.1.0) stops dropping image
attachments. `build_nanogpt_body` no longer calls `collect_drop_failures`
with the blanket "text-only" sentence; the new `nanogpt_user_content`
shapes user content exactly as v4's `buildUserContent` does: a plain
string when the message carries no attachments, otherwise a content-part
array — the text part only when the content is non-empty, one
`image_url` part per forwarded image (`url` first, else the `data:` URI),
`Unsupported file type: <mime>. NanoGPT forwards images only (<list>).`
for a MIME NanoGPT does not accept, `Attachment missing data or URL` for
a row carrying neither, and a single empty text part as the floor when
every attachment failed and there was no text.

The ledger the caller reports is now what the builder actually put on the
wire — advertised and executed can no longer disagree. Both compositions
read the same `BuiltRequest.attachment_results` from the one
`build_request`, as v4 uses one `buildBody` for both modes.

No vision-model list, deliberately, carrying v4's reasoning: NanoGPT
fronts hundreds of upstream models and such a list would be stale within
the week; the host has already decided by the time a request is built.

Envelope corpus 321 → 341. The two pre-existing NanoGPT
`image-attachment` rows flip; every other pre-existing row is
byte-identical. Eight new NanoGPT rows cover the data-URI, remote-`url`,
`url`-wins-over-data, unsupported-MIME, missing-data, mixed-ledger,
empty-content, all-failed-floor and attachment-on-a-non-user-message
arms, each recorded from v4's real dist plugin in both modes; seven
mutation proofs.

Mutation testing also found a pre-existing blind spot in two SIBLING
providers: no corpus bag had ever carried a `url`, so Z.AI's and
OpenRouter's `url`-before-data precedence stayed green under a swapped
spelling. Both gained a `image-attachment-url-wins` row (corpus only — no
source change), and both swaps now go red.

#### 2026-08-23 — feat(nanogpt): the manifest attachment flip (plugin 1.1.0)

_Versions: core 0.0.629._

NanoGPT's plugin declares image attachment support as of v4 `a14a1811`
(plugin 1.1.0), and the generated provider manifest follows it:
`supportsAttachments` true, the four image MIME types, the new description,
and the `notes` field explaining the host-decides posture (NanoGPT fronts
hundreds of models and deliberately keeps no vision-model list of its own).

Regenerated through `harness/oracle/providers/gen-provider-manifests.mjs`
against the v4 checkout at the pin — never hand-edited. The generator's
augmentation table needed no extension: `notes` was already read off the
plugin's `attachmentSupport` object. The nine sibling manifests are
byte-identical.

The family that pins this block is `provider_registry_equivalence` (its
`attachmentSupport` row), not `providers_listing_equivalence` — the
listing oracle carries no attachment data at all. Proven red-first against
the pre-flip manifest, then green.
#### 2026-08-23 — feat(photos): port the auto-describe-attachment pipeline (P4.D108 unit 2)

_Versions: core 0.0.630._

`photos/auto_describe_attachment.rs` ports v4's
`lib/photos/auto-describe-attachment.ts` whole: the four pre-bytes skip
arms in v4's order (`not-found` / `not-image` / `no-sha` /
`already-described`), the bytes read behind the existing `FileBytesStore`
seam (a failure is v4's warn + `no-bytes`), the vision call behind the §C2
`ImageDescribeDriver` (the host's driver runs
`generate_image_description` whole — profile resolution, refusals, the
uncensored retry, `logLLMCall`), and the three-sink persist pass: the
`files.description` write, every blank hard link updated (per-mount
`description` + `extractedText` + the real chunk pass; kept-image links
with markdown are left untouched), and the per-mount embedding enqueue
through the recorded `SaveImageSideEffects` seam (the photo-trio
precedent — v4's differential jest-mocks the enqueue identically). The
links repo gained the dedicated `apply_auto_description` update mirroring
v4's `docMountFileLinks.update` call. Five unit tests over a provisioned
fresh two-partition instance pin the skip vocabulary (incl. the
whitespace-only-description trim quirk carried from v4), the
three-sink success path, the second-run `already-described`
short-circuit with zero vision calls, and the kept-link skip. The
module's differential (driving v4's real module with
`generateImageDescription` mocked at the §C2 level) lands with the
photo-tools family widening in unit 3.

#### 2026-08-23 — feat(tools): the describe_image catalog entry + the attach/keep/list copy rewrites (P4.D108 unit 1)

_Versions: core 0.0.629._

The tool-definition catalog regenerated at the `a14a1811` pin via
`gen-tool-catalog.mjs`: 57 → 58 entries with the new `describeImage`
definition (v4 bug 92 — the looking verb) in v4 `ALL_TOOLS` order after
`deleteAnnotation`, plus the byte-exact description rewrites v4 shipped
alongside it (`attach_image` is now explicitly a display verb that "does
NOT let you see the picture", `keep_image` is "filing, not looking", and
`list_images` names the `describe_image` arm). Both tool-definitions
oracle cases gained the new import; the count tripwire in
`definitions/mod.rs` moved 57 → 58. Red-first: the family failed on the
57 != 58 size mismatch against the fresh oracle before the regen, green
after. The handler, registration, and Librarian rewrites follow in this
lane's later units.

#### 2026-08-23 — docs(porting): the a14a1811 vision-round work orders (five lanes)

_Docs-only; no version bumps._

The next round planned against a moved oracle baseline: v4 HEAD is
`a14a1811` ("characters can look at images, and images reach vision models",
bugs 91–95), three commits past `f8973813` — the other two dispositioned
NO-PORT at planning (`65f3476e` already ruled at the f8973813 round;
`718c9ada` is pure CI/bundler, the Turbopack revert, zero lib content).
Five work orders committed: P4.D106 (the image-transport predicate +
moderation finish reasons + the three-tier attachment anchor, server),
P4.D107 (NanoGPT plugin 1.1.0 — `image_url` serialization + the real
attachment ledger + the manifest flip), P4.D108 (the `describe_image`
looking verb end-to-end, incl. the auto-describe module v5 never ported),
P4.D109 (the attachment-failure toast + the client attachment table's
stale-note retirement, SPA), and P4.57 (tri-state decode-once for
taboo/brahma-console — the P4.56 recorded lead, a no-behavior-change
consolidation). Three binding shared contracts pinned across the sibling
orders: the a14a1811 attachment-capability values (three homes), the
describer seam (file_fallback's frozen public fns), and the done-frame
`attachmentResults` wire shape (chat_events.rs frozen for the round).

#### 2026-08-23 — fix(logging): the measured duration on the streaming chat log (dogfood #100)

_Versions: core 0.0.628, harness 0.0.551._

The 2026-08-22 dogfood pass found every streamed `CHAT_MESSAGE` row logging
`durationMs = 0`, against 6,115 v4-written rows on the same instance where not
one zero appears. Three sites hard-coded the value — the streaming chat write
and both image-description writes — each behind a comment calling it a tracked
follow-up because a measured stream clock could not be diffed. That blocker had
already been lifted by `normalize_duration_ms`, which collapses any non-NULL
duration to a presence marker on both sides; the comments never caught up.

`StreamLogCtx` now carries `started_at_ms`, stamped where v4 takes its own
`startTime` — once per `streamMessage` entry, so the primary attempt, the
tool-unsupported retry, and each failover leg time only their own call.
`describe_image_with_profile` takes v4's `describeStart` at the top and both its
log sites subtract from it. Since no differential can tell a hard-coded zero
from a measured one once the column is normalized, the guard is a source census
in the `db_error_key_guard` idiom, mutation-proven red-first. Verified live: the
next streamed turn logged 9,601 ms.

#### 2026-08-22 — unify: the `f8973813` NanoGPT-caching + settings-wire round (P4.D105 ∥ P4.56)

_Versions: core 0.0.627, harness 0.0.550, web 0.0.78._

Both lanes unified; the oracle baseline stays `f8973813` (v4's one newer
commit `65f3476e` is CI/release infra + a comment-only lib edit +
standalone-tarball native linking v5 doesn't have — NO-PORT with evidence;
every gate regen ran from a pinned worktree). P4.D105: NanoGPT prompt
caching whole — the Prompt Caching options group through the manifest
generator, the `promptCaching` body key behind the strict `=== true` gate
with the TTL collapse, both-dialect cache-usage normalization with the
`??`-precedence pin measured on v4 and cache reads excluded from
prompt/total on both the non-streaming response and the streaming final
chunk (which also gains unconditional `rawProviderUsage`), plus the
`response_parse_equivalence` run line. P4.56: the B2 data-retention
present-`null` collapse fixed red-first via `double_option` behind the
harness serde-path rewire, the new `GET/PUT /api/v1/settings/data-retention`
edge (which uncovered and fixed the `BrahmaConsole` success-arm 500 standing
since P4.D57, and two leaked-DbError sentences), the groups cleared-null pin
(zero source change), `settings_wire_actions` self-containment, the
float-literal store fix, and the shared `apiKeyId`/`baseUrl` classify
readers. The §3 review found no blocking findings. Unification wire: the
NanoGPT spec-fixture transcription gained the Prompt Caching group with two
showIf render specs. Gate: 12/12 families fresh at the pin; 445 test
binaries / 2,269 / 0; clippy both feature sets; release build; ng 341 files
/ 5,056; full Playwright green (numbers in the round record).

#### 2026-08-22 — docs(porting): the P4.D105 v4 mirror refresh and lane dispositions

_Docs-only change._

Refresh `docs/v4/CHANGELOG.md` from the pinned `f8973813` worktree — the
only one of the commit's four doc files that lives inside the mirror. It
was two rounds stale, so this also brings in the `a6870c5a`
person-consistency entry.

Record in `status-log.md`: the two `help/connection-profiles.md` NanoGPT
bullets banked verbatim for `p4.9i2` (they live at v4's top-level `help/`,
outside the mirror); the spot-check of v4's audit claims (reasoning-effort
values, the `delta.reasoning` / `reasoning_content` precedence,
`stream_options.include_usage`, tool-call delta accumulation — all three
already hold in v5, nothing to port); cache-read pricing verified as a
NO-PORT (v4 has no NanoGPT pricing row at the pin); the downstream
`cacheUsage` / `rawProviderUsage` plumbing verified generic rather than
re-plumbed; and two deferrals — the live caching smoke to the dogfood
queue, and the SPA's hand-transcribed NanoGPT schema fixture, which this
lane may not touch.

#### 2026-08-22 — feat(nanogpt): cache usage and rawProviderUsage on the streaming final chunk

_Versions: core 0.0.623, harness 0.0.545._

The streaming half of v4 `f8973813`. `Flavor::NanoGpt` now derives cache
usage through the SAME `nanogpt_cache_usage` the non-streaming arm calls —
v4 has one `extractCacheUsage` serving both, and two copies here would be
two places to drift — and applies the same cache-read exclusion to the
final chunk's `promptTokens`/`totalTokens`. `build_usage` gained a `written`
counter alongside `cached`; NanoGPT is the only flavor of the five whose
gateway reports cache writes, so the other four pass `None` and their
recorded bytes are unchanged.

NanoGPT also joins Z.AI in emitting `rawProviderUsage` on the final chunk:
v4's `(usage ?? null)`, so the KEY is always present — an explicit `null`
when the stream carried no usage frame, not an absent field.

The `chat_completions_sse` corpus grows 16 → 22 cases (six NanoGPT wires:
the Anthropic dialect, write-only, the OpenAI dialect, a present-zero read,
the clamp, and a stream with no usage frame at all). All five sibling
providers' recordings are byte-identical; the five pre-existing NanoGPT
recordings change only by gaining `rawProviderUsage`. Both consumers —
`stream_decoders_equivalence` (three chunkings) and
`streaming_composer_equivalence` — are green, and the decoder differential
gains six coverage arms plus a per-case assert that v4's own recorded
`promptTokens` is `max(0, prompt_tokens - cacheRead)`.

#### 2026-08-22 — feat(nanogpt): both cache-usage dialects on the non-streaming response

_Versions: core 0.0.622, harness 0.0.544._

Port v4's new `extractCacheUsage` into the `ChatFlavor::NanoGpt` arm of
`response_parse`. It reads BOTH dialects the gateway emits — Anthropic-style
`cache_read_input_tokens` / `cache_creation_input_tokens` and OpenAI-style
`prompt_tokens_details.cached_tokens` — with v4's `??` precedence, so a
present-but-zero `cache_read_input_tokens` does not fall through to the
OpenAI key. No cacheUsage at all when neither counter is positive; the read
and write keys are independently conditional.

The house rule then applies at the call site: cache reads come out of
`promptTokens` and `totalTokens` (clamped at zero), `completionTokens`
untouched. Before this, a NanoGPT turn billed every cached input token at
full price.

Response-bodies corpus 46 → 52 rows (six NanoGPT cases recorded from v4's
real `sendMessage` with the SDK mocked below it); every pre-existing row is
byte-identical. The differential gains the missing `NANOGPT` family-presence
guard, four cache coverage arms, and a per-row assert that v4's recorded
`promptTokens` really is `max(0, prompt_tokens - cacheRead)`.

`response_parse_equivalence` also gets its self-contained run line — the
last `nothing_to_run` envelope row (`recipe_sweep.py --self-test` clean, the
family runs attributably by name).

#### 2026-08-22 — feat(nanogpt): the Prompt Caching profile options and the `promptCaching` body key

_Versions: core 0.0.621, harness 0.0.543._

Regenerate the ten built-in provider manifests against v4 `f8973813`
(NanoGPT plugin 1.0.3). Only `nanogpt.json` moves: its `optionsSchema`
gains v4's new "Prompt Caching" group — the `enablePromptCaching` boolean
(default `false`) and the `cacheTTL` enum (`5m` / `1h`, default `5m`,
gated by `showIf: { field: 'enablePromptCaching', equals: true }`) — with
the group and field help text byte-copied from the plugin. The nine
sibling manifests are byte-identical.

No Rust change: `optionsSchema` is carried as an opaque value, and the
SPA's provider-options panel already renders booleans and `showIf`-gated
enums generically (P4.D84). `providers_listing_equivalence` (which pins
`optionsSchema` byte-for-byte, field order included) and
`provider_registry_equivalence` both re-run green over oracles
regenerated fresh at the pin.

`build_nanogpt_body` then emits v4's body-level helper:
`promptCaching: { enabled: true, ttl }` between the `user` key and the
allow-listed profile params, under a STRICT `enablePromptCaching === true`
gate (a truthy `1` does not arm it) and v4's TTL collapse (only the
literal `'1h'` buys the hour). Both option keys are consumed and stay off
`NANOGPT_PROFILE_ALLOWLIST`, so neither reaches the wire verbatim —
asserted by a unit test over the list and by a corpus row carrying both.

The request-envelope corpus grows 307 → 321 rows (seven NanoGPT cases in
both modes, recorded from v4's real `buildRequestBody`); every
pre-existing row is byte-identical. The differential gains four
coverage-shape asserts (both TTLs, the caching-off arm, the
consumed-keys arm) read off v4's recorded body, so a corpus that lost
the vectors cannot pass green.
#### 2026-08-22 — docs(porting): the P4.56 lane gate record

_Docs-only change._

The settings-wire remainder lane's verification gate: the six-family regen+run
sweep from a pinned `f8973813` worktree (`{'ok': 6}`, zero SKIP), the nine new
arms grepped present in the fresh NDJSONs, `cargo test --workspace` at 445
binaries / 2,267 passed / 0 failed with zero SKIP lines, clippy on both feature
sets, the release build, and `recipe_sweep.py --self-test` at 0 failures. Also
records the Tier-3 dispositions: the SPA data-retention card needs no follow-up
(it cannot send a `null`), and the taboo / brahma-console siblings still carry
the bag-shaped wire this lane's edge demonstrates the alternative to.

#### 2026-08-22 — refactor(settings): one reader for the profile `apiKeyId` / `baseUrl` semantics

_Versions: core 0.0.624._

P4.55 fixed the missing-`else` sub-family at three sites and left the
triplication as a named cleanup. The JS SEMANTICS now live once —
`classify_api_key_id` and `classify_base_url` in `api/settings.rs` — and the
three profile-update handlers (connection, image, embedding) keep only what
genuinely differs: which patch struct, which lookup, which fixed 500 sentence.
v4's reasoning (`findApiKeyById` on a non-string, `baseUrl || null` as JS
falsiness) and the recorded terminal-but-after-side-effects divergence are
stated once instead of three times.

Behavior-neutral by construction and by measurement: all three families are
green with no NDJSON movement, and a mutation that turns either reader's refusal
arm into a clear reddens all three — so the shared reader is genuinely live at
every site, not left beside dead code.

#### 2026-08-22 — fix(memories): store integer-valued floats the JS way

_Versions: core 0.0.623, harness 0.0.547._

Zod's `.int()` is `Number.isInteger`, so `{"perCharacterCap": 200.0}` PASSES
validation; JS then has no float/int distinction and `JSON.stringify` writes the
stored cell as `200`. serde_json keeps the float, so v5's two memories-config
setters stored and echoed `200.0`. Both merged bags now go through the shared
`normalize_js_numbers` walk before they are written or echoed — genuine
fractions (`softStartFraction: 0.7`) are untouched, `1.0` collapses to `1`
exactly as `JSON.stringify` writes it.

Pinned by `housekeeping_config_set_int_float_literal` and
`extraction_limits_set_int_float_literal`, each comparing the echo AND the
re-read stored bag. Both arms deliberately bypass the family's `norm` helper:
its `canon_numbers` normalizer collapses integer-valued floats on both sides,
which is precisely the difference being measured — a new float-sensitive
`norm_float_exact` compares them instead. Measured RED first (`1.0` vs `1` on
all four comparands), green after.

#### 2026-08-22 — test(harness): `settings_wire_actions` builds the fixture it reads

_Versions: harness 0.0.546._

The family reads `/tmp/qt-settings-fixture.db` and its recipe never built it —
only `settings_routes_equivalence`'s regen invokes the builder. P4.54 measured
the consequence: with `/tmp` cleaned the family does not SKIP, it FAILS every
case, so it was green only because a sibling happened to run first. Its header
now carries the fixture build as its own regen stage (the same idempotent
builder invocation, spelled with the load-bearing `V5W=${V5W:-…}` form).

Proven from a genuinely clean slate: `/tmp/qt-settings-fixture.db` deleted, then
`recipe_sweep.py --run settings_wire_actions` rebuilds it and passes 5/5.
`--self-test` stays at 0 failures — this family was never one of the two the
self-test structurally requires to remain in the `nothing_to_run` debt list (it
had a run line all along; what it lacked was a regen).

#### 2026-08-22 — test(groups): pin the cleared-null echo on the groups side

_Versions: harness 0.0.545._

The owed groups-side counterpart to P4.55's `update_clear_description` on
projects. `description` is store-resident for a group (a markdown file, not a
slim column), so the DB patch is empty and both sides take the store-backed
update's no-DB-work branch — v4 re-reads through `applyOverlayOne(_findById(id))`,
v5 re-reads unconditionally. The fresh oracle records what v4 actually answers:
`"description": null`, present in the echo rather than omitted. v5 matches.
Zero source change, as predicted — one `store_backed.rs`, two `StoreEntity`
impls, so groups inherited the projects verdict by construction and now has its
own pin.

#### 2026-08-22 — feat(web): the data-retention REST edge (and the brahma-console edge that never worked)

_Versions: core 0.0.622, web 0.0.78._

`quilltap-web` served no data-retention route at all: v4's
`GET/PUT /api/v1/settings/data-retention` had no v5 counterpart, so the setting
was reachable only over `/api/dispatch`. The pair now exists, modeled on the
taboo edge but decoding the PUT body into `Request::DataRetentionSettingsUpdate`
through serde rather than hand-building the variant — the absent /
explicit-`null` / value tri-state is resolved by exactly one piece of code, and
a re-implementation at the edge is what made the Taboo differential blind.

Found while extending `unwrap_to_http`'s success arm: `CoreResponse::BrahmaConsole`
had been missing from that hand-maintained variant list since P4.D57, so BOTH
brahma-console edges answered 500 `Unexpected core response` on every SUCCESS.
Only the error path worked, because errors leave through `CoreResponse::Error` —
which is why the settings differential (which drives the handler) could never
see it. Fixed, and both settings pairs are now asserted on their success path by
the new `data_retention_web_routes` live wire test.

Also: the two data-retention handlers leaked the raw `DbError` text where v4's
catch answers its own fixed sentence (`Failed to fetch…` / `Failed to update…`).
Harmless while nothing served the route; the first thing an operator would have
seen now that something does.

The wire test is mutation-proven in both directions — reverting the
`double_option` field makes the explicit-`null` PUT answer 200 instead of 400,
and removing the `BrahmaConsole` arm makes its GET answer 500 instead of 200.

#### 2026-08-22 — fix(settings): an explicit `null` staleChatDays is a 400, not a silent keep

_Versions: core 0.0.621, harness 0.0.544._

`Request::DataRetentionSettingsUpdate` carried `#[serde(default)]
stale_chat_days: Option<Value>`, so serde mapped an explicit `null` to `None`
indistinguishably from an absent key; the dispatch arm then built `{}` and the
handler kept the stored value at 200. v4's Zod `.default(30)` fires only for
`undefined`, so `{"staleChatDays": null}` is a 400 `Validation error` there.
Ironically this variant's own doc comment was cited as the precedent for the
Taboo fix while still carrying the bug.

The field is now the `double_option` tri-state and the dispatch arm has the
three-arm match its siblings have: absent keeps the stored value,
`Some(Some(v))` rides raw to the handler's Zod-faithful parse, and `Some(None)`
carries the explicit `null` through to that same parse, which refuses it. Two
arms pin it — `dr_put_null` (v4's exact 400 body) and
`dr_put_null_writes_nothing` (seeded at 120, `after` refetch proves the refusal
wrote nothing). Both were measured RED against the pre-fix code through the
rewired serde path (`{"staleChatDays":30}` where v4 answers
`{"error":"Validation error"}`). Family floor 15; 141 cases matched.

The Angular card is unaffected: `data-retention-settings.ts` only ever sends a
finite in-range integer, so no client surface can now see a 400 it did not see
before.

#### 2026-08-22 — test(settings): the data-retention differential rides the `Request` serde path

_Versions: harness 0.0.543._

The settings-routes family's `dataRetention` PUT leg handed the oracle's raw
body straight to `settings::data_retention_settings_update`, bypassing the
`Request` enum's serde entirely. Every arm therefore proved the handler and
nothing about the wire — a present-`null` arm added against that leg would have
passed green while the real wire (dispatch, and the REST edge this lane adds)
collapsed `null` to key-absent. The leg now decodes the body into
`Request::DataRetentionSettingsUpdate` exactly as the wire does (schema key
retained, tagged `type` inserted, serde deciding) and maps the result to the
handler's bag the way `engine.rs` does.

Four new arms, and the seeding the family never had: `seedDataRetention` writes
a non-default `instance_settings['dataRetention']` through v4's real setter (and
the ported one) before the case runs, so "kept the current value" is
distinguishable from "reset to the schema default 30" — `dr_get_seeded`,
`dr_put_empty_merge_seeded`, `dr_put_string_body` (v4's non-object spread), and
`dr_put_invalid_writes_nothing`. `dr_put_valid` and the three seeded PUTs carry
the family's new `after: 'dataRetention'` refetch, so the persisted effect is a
comparand rather than an inference. Family floor: 13 rows. 139 cases matched.

#### 2026-08-22 — docs(porting): work orders for the `f8973813` round (P4.D105 ∥ P4.56)

_Docs-only change._

Plan the next round against the new v4 baseline `f8973813` (one commit of
drift: NanoGPT prompt caching, plugin 1.0.3; bugfix unmoved). Two disjoint
lanes: P4.D105 absorbs the drift (the two Prompt Caching profile options
through the manifest generator, the `promptCaching` body key, both-dialect
cache-usage normalization with the cache-read exclusion, plus the
`response_parse_equivalence` run-line debt), and P4.56 closes the P4.55
named remainders (the B2 data-retention present-`null` collapse with the
harness edge-mapping rewire landing first, the missing `quilltap-web`
data-retention REST edge, the groups-side cleared-null pin, the memories
float-literal nit, the shared `apiKeyId`/`baseUrl` helper, and
`settings_wire_actions` self-containment).

#### 2026-08-22 — unify: the `a6870c5a` prompts-trio round (P4.D103 ∥ P4.D104 ∥ P4.55)

_Versions: core 0.0.620, harness 0.0.542, SPA 0.5.544._

All three lanes unified; the oracle baseline moves `4cb1035e` → `a6870c5a`
and the drift debt is cleared. Server: the standing-instructions section
(project + group `instructions`) lands in the cacheable system prompt
between the Taboo section and the tool instructions (cache structure
version 3 → 4), threaded to the live turn, Carina, and self-inventory,
with the Prospero project-context whisper's duplicate section dropped;
bug 88's second-person tool reinforcement replaces the pronoun lookup;
the identity stack moves to person-consistent wording under the new
version-stamped `compiledIdentityStacks` envelope, with v4's golden table
byte-copied (v5's computed hash equals v4's registered golden — a free
cross-implementation proof); the group verbs gain `instructions` plus
both v4 validators, ported whole. SPA: the shared prompt-field label and
the byte-identical twelve-key hints table, the seven-surface migration
sweep converging the drifted create/edit copy, the Group Instructions
editor, and the round-trip beat activated at unification (its first live
run green). Maintenance: the `c8a3cf77` merge-verb silent-keep lead is
closed — the two memories config verbs validate first (they had been
persisting garbage), the autonomous-rooms settings patch refuses invalid
fields, projects update validates through v4's schema, the three
missing-`else` `apiKeyId`/`baseUrl` sites are fixed, and the store-backed
cleared-null echo measured NOT divergent. The §3 review's ten findings
were fixed on the unify branch (previous entry). Gate: the 43-family
regen+run sweep 43/43 ok zero SKIP over oracles fresh at `a6870c5a`;
`cargo test --workspace` 444 binaries / 2,266 / 0 with the 75-variable
env block; clippy both feature sets; release build; `npm test` 341 files
/ 5,054; `npm run build`; full Playwright 236/236 zero skips.

#### 2026-08-22 — fix(unify): the a6870c5a-round §3 review findings

_Versions: core 0.0.620, harness 0.0.542, SPA 0.5.544._

Ten §3 unification-review findings fixed on the unify branch before the
gate. The two that would have shipped: `group_update` parsed the body
BEFORE the existence check, answering 400 where v4's find-first order
answers 404 for a missing group with a garbage patch (fixed to v4's
order; pinned by the new `update_missing_group_invalid_body_404` arm) —
and the autonomous-rooms `title` max counted Unicode scalars where Zod
measures UTF-16 code units, silently widening the limit for astral text
(fixed; pinned by the new `update_invalid_title_astral` arm). Also:
the settings-routes `connection_profiles` stale-oracle floor raised
19 → 22 for P4.55's three new arms (the row-driven family would have
passed green with them absent); the gated group-instructions beat's
clear-save leg moved from `waitForRequest` to `waitForResponse` (the
twice-deflaked navigate-aborts-the-save class, fixed before its first
live run); the recorded-only `create_non_string_instructions_400` row
gained a status assert; the identity-compiler family gained a
sentinel-presence tripwire (fixture-agreeing-with-itself guard); the
three profile `baseUrl` comments now record the v4 terminal-failure
sequencing divergence instead of overclaiming "nothing written";
details-tab regained v4's six `htmlFor` attributes; two doc-string
corrections (the hints spec's key count, a v4 line cite).

#### 2026-08-22 — port(groups): `instructions` on the group verbs + both v4 validators, whole

_Versions: core 0.0.615, harness 0.0.537._

`groupCreate` gains an optional `instructions` (`string | null`) and persists
`instructions || null`; `groupUpdate` accepts it too. Per the round's shared
contract with P4.D104.

Both v4 schemas are ported whole rather than just the new field, because the
drift commit lands on the validators and a partial port would be worse than
none. That closes two pre-existing gaps: `group_create` hand-checked a TRIMMED
non-empty name where v4's `.min(1)` runs on the raw string (so `"   "` is a
valid name in v4 and was rejected by v5), and answered `Name is required` where
v4's middleware answers the flat `Validation error` — that sentence only ever
appears inside the `details` array. And `group_update` was a RAW passthrough
patch map with no validation at all (its doc comment claimed
`updateGroupSchema.parse`, which was stale), so it wrote unknown keys v4's
non-strict `z.object` strips. The `details` issue array remains the standing
project-wide deferral.

Fourteen new `groups_routes_equivalence` arms: create with/without, the
`|| null` empty-string normalization, the whitespace name, the empty name 400,
the cap boundary in both directions, a non-string field (refused at v5's typed
wire, which is where v5's equivalent of the ZodError lives), update
set/clear/empty-string, the update cap 400, the `name: null` 400 (`optional()`
but not `nullable()`), a bad colour, and the unknown-key strip probe. Four
mutations proven red-first.

MEASURED, and it refutes the work order's premise: a PUT with `instructions:
""` does reach the store, but `instructions` lives in a markdown file and the
overlay reader is `content === '' ? null : content`, so the round trip answers
`null` either way. The case is renamed to say what it measures.

#### 2026-08-22 — port(prospero): drop the project-instructions whisper section

_Versions: core 0.0.614, harness 0.0.536._

All five of v4 `8f868109`'s removals in the Prospero project-context writer:
`instructions` leaves `ProsperoProjectContext`, the loader stops reading it (v4's
WHY comment carried in its place), the `**Project instructions:**` block leaves
`appendProjectBodySection` along with its separator condition and return
expression, and the term leaves `projectHasContent`. The section is now injected
into the cacheable system prompt every turn, so re-whispering it would only
duplicate it in context.

The `instr-only-no-general` corpus case keeps its name as the retired-section
tripwire: a project whose only former content was instructions now has NO
content, so the whisper must come back empty. Proven load-bearing — restoring
v5's old block reddens that row with the whole whisper on the left and `""` on
the right. `post_office_prospero` and `post_office_writers_tier3` regenerated at
`a6870c5a` and grepped: the literal `**Project instructions:**` appears nowhere
in either NDJSON, or anywhere in the tree.

#### 2026-08-22 — port(prompts): Carina one-off queries carry the standing instructions

_Versions: core 0.0.613._

Mirrors v4 `8f868109`'s Carina insertion: after the scenario, before the "who
is consulting you" card, so the prompt still reads identity → world → who's
asking → what you remember. The template context is HAND-BUILT as
`{char: answerer.name, user: "User"}` — the literal string, never a resolved
user character — which is v4's own shape on this path.

The group leg follows the ANSWERER's memberships, not the chat's roster. The
carina fixture now puts the chat in an instructed project (so all 17 rows carry
the project section) and gives ONE character an instructed group, so a case
answered by anyone else carries the project section alone — which is what makes
the answerer-not-chat rule measurable. Three mutations proven red-first: the
`{{user}}` key resolved from the asker, the membership leg keyed off the chat,
and the section moved after the reference-query card.

#### 2026-08-22 — port(prompts): the standing-instructions slot in the cacheable system prompt

_Versions: core 0.0.612, harness 0.0.535._

`BuildSystemPromptOptions` gains `standing_instructions`, pushed between the
Taboo section and the tool instructions and — unlike Taboo — run through
`process_template`, so a project or group prompt can address `{{char}}`.
Absent, empty, and whitespace-only all build a byte-identical prompt to the
pre-feature layout. `PROMPT_CACHE_STRUCTURE_VERSION` 3 → 4, with v4's history
comment carried; the two sibling wording commits deliberately did not bump.

Threaded at v4's two passing call sites: `build_context` resolves before the
synchronous builder (the same shape as the Taboo read above it), and
`self_inventory`'s `build_prompt_section` includes the full section because it
is substantive conduct guidance a character should be able to introspect. The
two NON-passing sites are verified and commented rather than assumed — the
character-voiced announcer imports the shared builder but never passes the
option, and the greeting head is a separate function entirely.

`system_prompt_equivalence` gains eight standing-instructions rows (absent /
empty / whitespace byte-identity, the position between Taboo and tools, the
`{{char}}`-resolution discriminator, roleplay + tools, and an untrimmed
section pushed verbatim) plus a shape floor so a stale oracle cannot pass by
not carrying them. Three mutations proven red-first.

`build_context_tier3` gains the spine proof: the fixture bakes an instructed
project and three groups whose membership insert order fights the name sort,
and a new op puts the chat in the project. `self_inventory`'s fixture project
and group gain `instructions`. Both threading sites mutation-proven red-first.

#### 2026-08-22 — port(prompts): the version-stamped `compiledIdentityStacks` envelope

_Versions: core 0.0.611, harness 0.0.534._

Ports the compiler half of v4 `a6870c5a`. `chats.compiledIdentityStacks`
becomes `{version, stacks}` keyed to `IDENTITY_STACK_BUILDER_VERSION`: every
write is stamped, reads require STRICT equality (older, newer, and the legacy
bare map all read as "nothing cached", so the read-through path rebuilds with
current wording — newer matters on a downgrade, where a rolled-back build must
not consume stacks a later build wrote), a stale map is DISCARDED on merge
rather than blended into and re-stamped, the drop path CLEARS a stale map
instead of rewriting it back, and an empty map still writes `null` (the null
column is the "nothing cached" state and needs no version). No migration.

`identity_compiler_equivalence` gains eight `participant` rows over eight new
pre-seeded chats — current / legacy-bare / older / newer stamps on the merge
path, and all four drop-path shapes. The seeded stacks are sentinel strings no
builder emits, so a port that merges into or rewrites a stale map shows the
sentinel rather than being inferred. Five mutations proven red-first.

`chat_admin_routes_equivalence`'s minted-id normalizer moves one level down —
only the inner `stacks` keys are minted, so `version` is now a full comparand.
`chat_cast_routes_equivalence` needed nothing (it tokenizes every object key
generically). Both regenerated at `a6870c5a` and confirmed to carry the
envelope.

#### 2026-08-22 — fix(prompts): second-person tool reinforcement + identity-stack person consistency (v4 bug 88)

_Versions: core 0.0.610, harness 0.0.533._

Ports v4 `346e855f` (bug 88) and the server half of `a6870c5a` — the two
wording commits — together, since the goldens they move are the same two.

Bug 88: the last block of the assembled prompt drops its pronoun lookup
entirely and reads `When you use workspace tools, you CALL them — you do not
merely describe calling them.` v5 measurably HAD the bug: a character with no
pronouns recorded ended its prompt on `they CALLS them — they does not`, and
v5's own unit test pinned the third-person string. That assertion is flipped
and widened — a character WITH pronouns and one WITHOUT must now produce the
same final block.

Person consistency: `build_identity_stack`'s aliases, pronouns, and
physical-appearance blocks move to second person (the appearance block also
loses its markdown bullet, and keeps a second-person WRAPPER only — the body
stays noun phrases, shared with the image pipelines); manifesto, personality,
and example dialogues gain referent-fixing wrapper lines. The outward-facing
builders (public identity card, other-participants info, identity
reinforcement) stay third person — their referent is someone else.

New `IDENTITY_STACK_BUILDER_VERSION = 2`, colocated with the builder as v4
colocates it, plus v4's `IDENTITY_STACK_GOLDENS` table byte-copied as a v5
unit test that binds the constant to a hash of the builder's output in both
directions. v5 reproduces v4's registered hash `1408705ab29bb3ba` exactly — a
free cross-implementation check. The compiler that reads the stamp lands next.

`system_prompt_equivalence` regenerated at `a6870c5a`: 65 rows green and both
cache-determinism goldens move to v4's new values (`937ea8197a65d022` /
`bc37032e92411263`) with the transition recorded inline. The regenerated
NDJSON was grepped for all seven changed byte shapes (present) and all four
retired ones (absent).

#### 2026-08-22 — port(prompts): the standing-instructions module (project + group `instructions`)

_Versions: core 0.0.609, harness 0.0.532._

Ports v4 `lib/chat/context/standing-instructions.ts` (`8f868109`) as
`quilltap_core::standing_instructions`: `resolve_standing_instructions`
(project by `chat.projectId`, groups by the RESPONDING character's
memberships — never the chat), `render_standing_instructions_section`
(byte-exact `[STANDING INSTRUCTIONS]` preamble, `## Project Instructions —
<name>` / `## Group Instructions — <name>` headings, `\n\n` block joins),
and the `resolve_standing_instructions_section` one-shot. Nothing is
injected yet — the builder slot and the call sites land in the next units.

Group sources sort by `localeCompare(name)` then `localeCompare(instructions)`
— v4's doc comment says the tie-break is "(then id)", but its code tie-breaks
on `instructions`; the CODE is what is ported. Empty/whitespace instructions
contribute nothing at both layers, so an instance that never touches the
feature builds a byte-identical prompt (the Taboo contract). Every lookup
fails soft per v4's three try/catch sites.

New differential `standing_instructions_equivalence` (14 resolve + 8 render
rows) over a new `/tmp`-built fixture pair whose membership INSERT order
deliberately fights every sort. Five mutations proven red-first: byte order
instead of ICU (`apple` vs `Banana`), a reversed name sort, the dropped
`instructions` tie-break (two groups both named `Mirror`), the dropped
resolver trim, and groups-before-project.
#### 2026-08-22 — docs(porting): the P4.55 verification gate record

_Docs-only change._

The lane's gate, for the unifier: fmt clean, clippy clean on both feature sets,
443 test binaries / 2,261 passed / 0 failed with the eight-variable oracle
env block, the six families re-run by name with zero SKIP and all 26 new arms
green and present in their freshly regenerated NDJSONs, and a clean release
build. No committed fixture changed, so no sibling oracle is invalidated.

#### 2026-08-22 — docs(porting): P4.55 closes — the Tier 3 deferrals recorded by name

_Docs-only change._

The work order's status header flips to CLOSED (Tier 1 and Tier 2 both landed
whole), and the two items the lane deliberately did not take get their own
phase-4 section: the data-retention present-`null` state collapse, which is
confirmed divergent but whose fix lives in files another lane owned and whose
harness leg bypasses the wire it would need to pin; and the groups-side
cleared-null pin, which inherits the projects-side verdict by construction but
rides the next round.

#### 2026-08-22 — fix(profiles): a non-string apiKeyId or baseUrl no longer vanishes on update

_Versions: core 0.0.612, harness 0.0.535._

All three profile-update handlers (connection, image, embedding) read `apiKeyId`
as `if null … else if as_str …` with no else, so a present non-string was
silently dropped and the PUT answered 200. Measured against v4: it falls into
`findApiKeyById(<non-string>)`, which answers null for a number (no id is
spelled that way) and null for an object or array (the driver's binder throws
and `safeQuery` swallows it), so both cases are a 404 `API key not found`.

The sibling `baseUrl` read is JS falsiness — `baseUrl || null` — not a string
check. v5's `as_str()` filter collapsed every non-string to null, so a truthy
non-string silently cleared the column. Measured against v4: it assigns the
value verbatim, the row validation rejects it, and the route answers its fixed
500 with nothing written. Both reads now match, with the falsy arms (`""`,
`null`, `false`, `0`) still clearing as before.

Nine new arms across the settings-routes, image-profiles-routes and
embedding-profiles-routes families, each proven red against the old behavior.
The embedding-profiles recipe is repaired on the way: its regen wrote one
NDJSON path and its run line read another, and the .test.ts header carried a
parenthetical that is a bash syntax error, so the family could not be
regenerated through the sweep driver at all. The stale comment claiming the
embedding-profile PUT echo drops cleared nulls is corrected — it echoes them
explicitly, and has since the P4.9H2A review.

#### 2026-08-22 — fix(projects): validate the update body through v4's updateProjectSchema

_Versions: core 0.0.611, harness 0.0.534._

`project_update` passed the raw request body straight to the repository with no
validation: `{"allowAnyCharacter": "yes"}`, a 101-character `name`, an explicit
`null` on the non-nullable `backgroundDisplayMode` — all accepted, answered 200.
v4 runs `updateProjectSchema.parse(body)` and hands the repository the parsed
data, so an invalid field is a 400 `Validation error` and an unknown key is
stripped.

All fourteen fields ported constraint by constraint, in schema-declaration
order, with the `.nullable()` column carried per field so a cleared
`description` still clears while a null `backgroundDisplayMode` refuses. The
existence check runs first, as in v4, so a missing project still answers 404
even for a garbage patch. Five new arms in the projects-routes differential;
the three invalid ones proven red against the old passthrough.

The same commit settles the P4.D85 cleared-null question on the store-backed
side: v4's `update` answers `_update`'s in-memory merge overlaid, v5 re-reads,
and the new `update_clear_description` arm measures the two echoes agreeing.
No `store_backed.rs` change was needed.

#### 2026-08-22 — fix(autonomous-rooms): refuse an invalid update-settings patch instead of dropping the field

_Versions: core 0.0.610, harness 0.0.533._

`parse_settings_patch` was deliberately lenient: a present-but-wrong-typed or
out-of-range field was silently dropped and the request answered 200. A bogus
`runVisibility`, a negative or fractional cap, a 400-character `title` all
vanished without a word — and a valid field riding alongside an invalid one
still landed. v4 runs `updateSettingsSchema.parse` before the service call and
answers 400 `Validation error`, writing nothing.

The parser is now fallible across all ten fields, each constraint taken from
v4's schema: `.int().positive()` on the four integer caps, `.positive()` alone
on the spend cap, the three-member `runVisibility` enum, `title` at 300 UTF-16
units, `scheduleCron` at 120, and both booleans strict. `title` is `.optional()`
rather than `.nullish()`, so a present `null` is a refusal, not a clear. Six new
arms in the autonomous-rooms differential including a writes-nothing composite
over a room with stored caps; all seven comparands proven red against the old
leniency.

#### 2026-08-22 — fix(memories): validate the housekeeping + extraction-limits config bodies before writing

_Versions: core 0.0.609, harness 0.0.532._

The `memoryHousekeepingConfigSet` and `memoryExtractionLimitsSet` verbs merged
whatever the body carried into the stored settings bag and persisted it: a
`"yes"` for a boolean, a negative `perCharacterCap`, a string where a number
belongs — all merged, written, answered 200. v4 `safeParse`s
`housekeepingConfigSchema` / `extractionLimitsConfigSchema` before it reads
anything and answers 400 `Validation error`, writing nothing.

Both verbs now validate first, on the template the recall verb already used.
The bespoke non-object sentences ("Invalid housekeeping config body" /
"Invalid extraction limits body") are gone — a non-object body is Zod's own
root-level `invalid_type` and takes the same `Validation error` path. Six new
arms in the memories-config differential (two invalid + one writes-nothing
composite comparing the stored bag per verb), all eight comparands proven red
against the old lenient behavior.
#### 2026-08-22 — fix(prompts): the shared prompt-field label host is a block

_Versions: SPA 0.5.543._

An Angular custom-element host is `display: inline` by default, and the React
component this ports has no host element at all. In the appearance tab the
header is a DIRECT child of a `space-y-4` stack, so an inline host silently
loses its gap — `margin-top` does not apply to an inline box. `host: { class:
'block' }` fixes it everywhere at once; `display: contents` would read like
v4's zero-host render but makes `space-y-*`'s `> * + *` margin land on a
boxless element and vanish. Same class of defect as dogfood finding #97's
`qt-tab-view`. Spec-pinned and mutation-proven.

#### 2026-08-22 — feat(groups): the Group Instructions editor in the group detail view

_Versions: SPA 0.5.542._

P4.D104 unit 3 — v4 `8f868109`'s client half. The routed group editor gains
a Group Instructions markdown field, third in the form between Description
and Color exactly where v4 puts it, at v4's `minHeight="14rem"`, headed by
the shared prompt-field label over `PROMPT_FIELD_HINTS.groupInstructions`
with `optional`. Load reads `g.instructions || ''`; save sends
`instructions || null` — the server's update path is a validated
passthrough, so an emptied editor would otherwise persist `""`, and the
client is what normalizes, as v4's does.

The TS contract mirrors the Shared contract: `instructions` on
`GroupCreateRequest`, on `GroupUpdatePatch`, and on the group row DTO. The
row DTO declares it optional, exactly as v4's own shared `Group` type does —
one type serves both the list read and the detail read, and only the detail
read is contractually obliged to carry it.

The group CREATE dialog gains nothing: v4 did not touch its own.

Specs cover seeding from the loaded group, the header text drawn from the
hints table, the field's position in the form, saving an edited body, the
cleared-to-null normalization, and a group that carries no instructions at
all. Both the null normalization and the field placement were
mutation-proven. A gated Playwright beat (type → save → reload → clear →
assert the wire carries null) waits on `P4D103_SERVER_LANDED`, which the
unifier flips once the server half lands.

#### 2026-08-22 — feat(prompts): migrate every prompt editor onto the shared field label

_Versions: SPA 0.5.541._

P4.D104 unit 2 — v4's `a6870c5a` sweep mirrored across every v5 prompt
editor. Migrated: the character edit Details tab (seven fields), the
character create page (eight, incl. the singular Scenario), the
system-prompt modal's Content field (`required` plus the Markdown /
placeholder suffix), the roleplay-template modal's LLM Prompt (`required`
plus the placeholder suffix), the project Settings card's Project
Instructions, and the appearance tab, which gains ONE physicalDescription
note above all five prompt variants rather than a helper per field. The
create page's disabled "Import Template" control moves into the label row's
projection slot, v4's `actions`.

This CONVERGES the character create and edit forms, which is the stated
reason v4 wrote the component: the two had drifted apart in their
hand-rolled hint copy, and v5 had transcribed both drifted versions
faithfully. Every replaced string is deleted, not shadowed.

The scenarios block keeps its custom header — v4 deliberately left that one
hand-rolled, since the "+ Add Scenario" control and the array editor below
are not a labelled field — and gains only what the commit gave it: "the
stage, never the actor" folded into the helper and a `Written as:` line from
the shared scenario example. Fixed on the way: v5's helper had dropped v4's
"Stored in the vault's Scenarios/ folder." clause.

Pinned by a new rendered-DOM spec that asserts every migrated header is
drawn from the hints table, that create and edit render byte-identical
headers for their seven shared fields, and that none of the eleven retired
sentences survives anywhere in the rendered output. Mutation-proven:
restoring one hand-rolled helper reddened both the field assertion and the
convergence assertion.

Deferred loud, unchanged: the roleplay-template modal's "Draft formatting
instructions" button (v4 puts it in the label row's actions slot; v5 has
never had the control), and v5's "Import Template" stays disabled.

#### 2026-08-22 — feat(prompts): the shared prompt-field label + single-sourced field hints

_Versions: SPA 0.5.540._

P4.D104 unit 1. Ports v4's `components/prompt-fields/` pair (commit
`a6870c5a`): `qt-prompt-field-label`, the header-only shared component for
prompt-bearing fields (label line, helper paragraph, `Written as: <em>`
worked example, the `" (Optional)"` suffix and destructive `*`, an
`ng-content` slot standing in for v4's `actions` ReactNode), and
`PROMPT_FIELD_HINTS`, the eleven-key hint table transcribed byte-for-byte
from v4 — typographic apostrophes included.

The label component renders the header only and never wraps the editor, so
it drops above any existing input without touching its state wiring. The
whole label line is one computed string because Angular's default
whitespace collapsing would otherwise insert a space where v4's JSX
concatenates.

The hints table is pinned by a v4-client-oracle parity spec whose
expectation rows were emitted from v4's real module, so a drift edit on
either side of any of the 33 strings goes red; a mutation on one helper was
run red-then-green. No call site migrates yet — that is unit 2.

#### 2026-08-22 — docs(porting): work orders for the prompts-trio drift round (P4.D103 ∥ P4.D104 ∥ P4.55)

_Docs-only change._

Three work orders for the next round, planned against v4 `a6870c5a` (the
prompts trio — the drift the `4cb1035e` unification predicted).
`p4.d103-prompts-trio-server.md`: the standing-instructions module and its
system-prompt slot (project/group `instructions` between the Taboo section
and tool instructions, `PROMPT_CACHE_STRUCTURE_VERSION` 3 → 4), the bug-88
second-person tool reinforcement, the identity-stack person-consistency
wording, and the `compiledIdentityStacks` version-stamped envelope with the
golden-table guard — plus the groups verbs' `instructions` acceptance with
v4's validators ported whole. `p4.d104-prompts-trio-spa.md`: the
group-instructions editor, the shared prompt-field label component + the
transcribed field-hints table, the migration sweep across every v5 prompt
editor, and the gated round-trip beat. `p4.55-merge-verb-silent-keep.md`:
the `c8a3cf77` LEAD resolved to a measured site table — two unfixed
memories merge-verbs that persist garbage, the autonomous-rooms
settings patch's self-documented leniency, the projects update's missing
schema parse, the three missing-`else` `apiKeyId` sites, and the
store_backed cleared-null echo question — each fix red-first with
invalid-payload oracle arms. The data-retention present-null divergence is
recorded but deferred by file ownership. Shared contract (the group
`instructions` wire field + the `P4D103_SERVER_LANDED` beat gate) pinned
verbatim in both drift orders.

#### 2026-08-22 — unify: the `4cb1035e` image + NanoGPT drift round (P4.D100 → P4.D101 stacked ∥ P4.D102)

_Versions: core 0.0.608, harness 0.0.531, host 0.0.77, SPA 0.5.539._

All three lanes unified; the oracle baseline moves `12fe3e6f` → `4cb1035e`.
The honest image `list-models` verb replaces its refusal end to end (live
per-provider model discovery for all five image plugins, source/fetchError
labels, cache-only-live); the image-download seam lands and Z.AI image
generation becomes real (v5 measurably had v4's URL-only zero-byte bug); any
`gemini*` id now routes to generateContent; and NanoGPT arrives whole as the
tenth bundled provider — chat with the flat `reasoning_effort` allowlist and
the dual `delta.reasoning`/`reasoning_content` dialect plus bug 87's
prose-echo guard, images over the shared download seam, embeddings with the
catalogue pinned against v4's real plugin, and a thinking-turn rule through
the P4.D97 machinery. SPA: the Fetch Models control with v4's exact label
strings, the Z.AI/NanoGPT provider entries and Default Size panels, the
NanoGPT embedding surface and badge. A real v4 bug found to file upstream:
OpenRouter image discovery reads wire keys its own SDK's zod strips, so every
keyed listing throws (v5 reproduces, convergence-tripwired). Unification
wires: both gated beats flipped live; the unified gate caught and fixed a
cross-lane oracle blind spot (the routes case's plugin list) and a
first-live-run beat gesture defect (redesigned around an offline list-order
discriminator, mutation-proven). Gate: ten families fresh at the `4cb1035e`
pin zero SKIP; manifests regenerated byte-identical; 443 test binaries /
2,261 / 0; clippy both feature sets; release build; ng 338 files / 5,016;
full Playwright 235/235 zero skips. Round record: `status-log.md`.

#### 2026-08-22 — docs(porting): the P4.D101 tier-2 docs — mirror refresh + the help-doc bank

_Docs-only change._

The `docs/v4` mirror is refreshed for what the three NanoGPT commits actually
touched and v5 actually mirrors: `CHANGELOG.md`, `developer/bugs.md`, and the new
`developer/bugs/fixed/bug-87-nanogpt-reasoning-echo.md`.

The four help docs the order listed are banked for `p4.9i2` rather than ported.
They live at v4's top-level `help/*.md`, not under `docs/`, so they sit outside
the mirror entirely and no refresh applies to them; and v5 has no ported
help-doc content surface to port them into — the P4.D77 `help_doc_chunks`
substrate landed, but the Guide client half is the standing `p4.9i2` bank. Their
names and NanoGPT hit counts are recorded so that lane can find them.

The pricing measurement the order asked for is recorded with the unit-2 census:
v4 has no NanoGPT pricing row anywhere in `lib/`, so the pricing fetcher and its
fallback table are NO-PORT, and the manifest ships `"pricing": {}` — which the
generator emitted off the plugin without being told to.

#### 2026-08-22 — port(providers): the NanoGPT image arms over P4.D100's seam (v4 `781fc420`, P4.D101)

_Versions: core 0.0.608, harness 0.0.531._

Generation, discovery, orientation and auth for NanoGPT's OpenAI-compatible
images route, built on the download seam P4.D100 landed.

`response_format: "b64_json"` is PINNED on every request, carrying v4's why —
"NanoGPT defaults to b64_json already; pin it so a future default change
upstream cannot silently hand us URLs." Unlike the OpenAI builder there is no
`gpt-image` exemption, which the corpus proves with a `gpt-image-1.5` row. `size`
rides verbatim and only when supplied — v4 casts it without validating, so none
of the OpenAI path's size coercion applies; `seed` rides only when set; an absent
model becomes `hidream`, NanoGPT's own server-side default made explicit.

P4.D100's `download_zai_images` is generalized to `download_url_images`: both
plugins carry the identical loop and differ only in their two error sentences, so
the provider now selects the wording and nothing else. Z.AI's bytes are unchanged,
which its own corpus rows keep proving.

Discovery is a raw fetch to `/image-models`, so a non-ok status is NanoGPT's own
`NanoGPT image-model listing failed: HTTP <status>` rather than an SDK error. The
filter is the capability FLAG and strictly `=== true` — the listing also carries
edit-only and upscale-only entries — and the curated six are unioned in and
sorted, so the arm needs no empty-throw.

Corpus: image-dialects 65 → 82 rows, all 65 of P4.D100's byte-identical. Three
mutation proofs: dropping the b64 pin reddens; swapping in Z.AI's download
sentence reddens with the exact wording diff; and relaxing the capability filter
from `=== true` to truthy reddens on the planted `image_generation: 1` row.

Two harness generalizations were needed and both were P4.D100 shapes rather than
new ones: the `model` column is now optional (the `default_model` case
deliberately sends none), and the Z.AI-named download leg is provider-parameterized.

⚠ The regen script's concat list is separate from its run list, so adding a `run`
line alone produced a green regen with NanoGPT silently absent. Both are updated.

#### 2026-08-22 — port(providers): the NanoGPT embeddings arms (v4 `781fc420`, P4.D101)

_Versions: core 0.0.607, harness 0.0.530._

NANOGPT joins the embedding-profile provider list with no code change — the list
is manifest-driven off `capabilities.embeddings`, so the manifest landing was
enough; a unit assertion pins that it is really there and that BUILTIN stays
last.

The wire is the OpenAI-compatible `/embeddings` route with two real differences:
the base url defaults to NanoGPT's gateway, and the request carries a
`User-Agent` on both the single and batch routes where v4's single-embedding
OpenAI path omits it entirely. The runtime dispatch gains its `NANOGPT` branch,
returning the REQUEST model like the OpenAI path does.

**The differential caught a defect inspection would not have.** The first
implementation built NanoGPT's error sentence by wrapping `openai_error_message`,
which already carries its own prefix — producing `NanoGPT embedding failed:
OpenAI embedding failed: Invalid API key`. Fixed at the root: the bare
`error.error?.message || statusText` extraction is now a shared
`embedding_error_detail`, and neither provider's prefix can absorb the other's.

The seven-model catalogue is transcribed from `models.ts`, and — better than the
existing pattern — it is PINNED: `embedding_wire_equivalence` gained a
`catalogue` row driving v4's real `plugin.getEmbeddingModels()`. Extending the
same row to the other four providers was free, so OPENAI / OLLAMA / OPENROUTER /
BUILTIN are now byte-pinned against v4 too, where they had only been asserted by
count. All four were already correct. Mutation-proven: a one-word description
change or a dropped model row reddens the catalogue arm by name.

The `fetch-models` refusal note widens to name NanoGPT and record what the live
verb will owe — `GET /api/v1/embedding-models` with fallback-not-throw on a
non-ok status, an empty list, or a thrown fetch, which is the deliberate
opposite of NanoGPT's image listing.

Corpus: embedding-wire 12 → 23 rows.

#### 2026-08-22 — port(providers): the NanoGPT reasoning dialects + bug 87's echo guard (v4 `d5830439` + `4cb1035e`, P4.D101)

_Versions: core 0.0.606, harness 0.0.529._

`Flavor::NanoGpt` reads `delta.reasoning` with `delta.reasoning_content` as the
legacy fallback — the `??` precedence, which neither `OpenRouterRaw` (reasoning
only) nor the SDK flavors (reasoning_content only) have. Its terminal
`raw_response` is the OpenAI-SDK shape and still carries the accumulated run
under the LEGACY `reasoning_content` key: v4's `d5830439` changed which field is
READ off the wire and deliberately left the synthesized key alone. NanoGPT emits
a usage object even with no usage frame, derives no cache usage, and never sets
`raw_provider_usage`.

v4 bug 87's prose-echo guard is ported as decoder state. NanoGPT's gateway
sometimes re-emits the whole answer down the reasoning channel after the content
stream ends, which would repeat the reply inside a thinking fold. A reasoning run
is HELD while it is still a verbatim prefix of the prose streamed so far;
divergence commits it in full as a single chunk, and a run still mirroring the
prose at stream end is discarded from the live chunks, the final chunk, and the
`raw_response`. The guard only arms while nothing has been committed and prose
has already started, so ordinary pre-content reasoning never touches it. The
stream-end arm yields no live chunk, matching v4.

The non-streaming twin drops a run exactly equal to `msg.content ?? ''`. Exact
equality, not a prefix — a near-miss is real reasoning and is kept, which the
corpus pins.

`is_openai_sdk` and a new shape predicate were collapsed into one
`has_sdk_raw_response`, since both call sites wanted the same three flavors.

Five stream fixtures (both dialects, tool-fragment assembly, the split-chunk
echo, the diverging run), nine response bodies (both dialects, the `??`
precedence with both fields present, both echo channels, the near-miss, tool-call
normalization, no-usage), and the thinking-turn corpus gains NanoGPT's real rule
— the corpus's FIRST multi-value enabled list, and the shape where the empty
string is in neither list so "(model default)" falls through to the model's
`thinksByDefault` habit. Corpora: streams 11 → 16 cases, response bodies 37 → 46
rows, thinking-turn → 1,560 rows; all pre-existing rows byte-identical.

Four mutation proofs, each reddening by case name: disarming the streaming guard
(`nanogpt-echo-dropped` 5 chunks vs 3), dropping the main-endpoint field
(`nanogpt-reasoning-main` 3 vs 5), disarming the non-streaming guard (both
`reasoning-echo-dropped` rows), and reversing the `??` precedence
(`reasoning-both-precedence`).

#### 2026-08-22 — port(providers): the NanoGPT chat wire + the switch-table census (v4 `781fc420`, P4.D101)

_Versions: core 0.0.605, harness 0.0.528._

`build_nanogpt_body` is v4's `buildBody` verbatim — one body function for both
modes, so the streaming and non-streaming wires cannot drift. Defaults 0.7 /
4096 / 1; `stream_options.include_usage` on the streaming path only; `stop`;
`tools` with `tool_choice` defaulting to `"auto"` whenever tools are present;
both `response_format` kinds; the cache key on `user` (NOT DeepSeek's
`user_id`) and only when non-empty. The five-key profile allow-list is FLAT:
NanoGPT does not override `normalizeProfileParam`, so `reasoning_effort` is a
top-level key rather than folded into `chat_template_kwargs` the way the
OpenAI-Compatible endpoint folds it. Reasoning is never echoed back, and leading
system messages are NOT folded — NanoGPT is a hosted gateway with its own
`formatMessages`, and folding would change the bytes it receives.

`ChatFlavor::NanoGpt` carries the non-streaming reasoning read: `message.reasoning`
with `message.reasoning_content` as the legacy fallback — a precedence no other
flavor has — minus v4 bug 87's gateway echo, where a reasoning run exactly equal
to the content is dropped. Tool calls go through `normalize_oac_tool_calls`,
confirmed line-by-line to be the OAC base's `normalizeToolCalls` that v4's
NanoGPT provider actually calls, not DeepSeek's `extract_openai_tool_calls`.

The models-list fetch unions the manifest's `fallbackModels` in and removes its
`imageGenerationModels`, then sorts — one home for the curated lists rather than
a second hardcoded copy. Measured on the way: the plugin's try/catch fallback is
unreachable for a wire failure, because the OAC base's own `getAvailableModels`
swallows every transport error and returns `[]`; the union produces the same
sorted catalogue the catch would have.

The corpus grows 269 → 307 (19 cases × both modes) and all 269 pre-existing rows
are byte-identical. Mutation-proven: folding `reasoning_effort` into
`chat_template_kwargs` reddens `request_builder_equivalence` with the exact
flat-vs-folded diff.

**The switch-table census refutes four of the order's named sites.** `NANOGPT`
appears in v4's entire `lib/` tree exactly ONCE — the embedding-profile enum. It
is in none of `DEFAULT_CONTEXT_BY_PROVIDER`, `LEGACY_RECOMMENDED_CHEAP_MODELS`,
`PROVIDER_NAME_SUPPORT`, or `PROVIDER_ATTACHMENT_CAPABILITIES`, and `Provider` is
`z.string()` so no type forces an entry. Those are v4's pre-plugin fallbacks; a
plugin-era provider is served by the registry, which v5 consults first. Adding
rows would have been a v5-invented divergence, so a new guard test pins the
absence and proves the registry still answers 131072.

#### 2026-08-22 — port(providers): the NanoGPT manifest through the generator (v4 `781fc420` + `d5830439`, P4.D101)

_Versions: core 0.0.604, harness 0.0.527._

NanoGPT joins the built-in provider registry as a tenth manifest, generated
rather than hand-written. The generator's `PROVIDERS` table gains the
`qtap-plugin-nanogpt` row with its wire augmentation (base URL
`https://nano-gpt.com/api/v1`, `/chat/completions` + `/models`, bearer auth,
the chat-completions SSE decoder, no request transform); everything else —
capabilities, config requirements, the `reasoning_effort` options schema, the
`thinkingTurnRule`, the ten catalogue rows, the `:thinking` thinking facts,
message format, cheap models, chars-per-token, and the 131072 default context
window — is read off the built plugin, so the committed `nanogpt.json` is a
transcription that cannot rot away from v4.

`resolveImageGenerationModels` needed a real extension to reach NanoGPT's six
image ids. Grok and Z.AI declare `supportedModels` inside `image-provider.ts`,
where the existing reader looks; NanoGPT imports the list from `models.ts`, so
the reader now follows a same-dir relative import when the const is not
module-local, and still exits loudly and by name on any shape it does not
recognize. Hand-copying the six ids into the augmentation table would have been
the `imageGenerationModels` rot of P4.39 and the google-auth rot of P4.D78 for a
third time.

The manifest is APPENDED to `BUILT_IN_MANIFEST_JSON` rather than slotted
alphabetically. Both `provider_registry_equivalence` (the `names` row) and
`providers_listing_equivalence` (a positional zip) compare registration order,
so appending leaves all nine pre-existing rows byte-identical on both sides; the
two oracle cases' `PLUGIN_DIRS` lists carry the same append. The nine committed
manifests regenerate byte-for-byte at the pin, which is the proof the generator
change is additive.

`providers_listing_equivalence`'s exactly-two-thinking-rules shape guard moves
to three (deepseek + ollama + nanogpt) — a designed tripwire firing as designed,
not a weakened assertion — and the options-schema floor moves 8 → 9. The
NanoGPT row is mutation-proven compared: perturbing one `enumValues` label in
the committed manifest reddens the listing differential by name.

#### 2026-08-22 — port(images): the honest `list-models` verb (v4 `ca22ec45`, P4.D100)

_Versions: core 0.0.603, harness 0.0.526, host 0.0.77._

`imageProfileListModels` was a loud typed refusal; it is now v4's honest Fetch
Models action. `source` is `provider` ONLY when the provider's API was actually
queried and answered, otherwise `builtin` (the plugin's curated list) with the
live-fetch reason in `fetchError` — omitted, never null, when there is none.
The flow is v4's exactly: a falsy `provider` is 400 `Provider is required`; an
unavailable one is 400 `Provider {provider} is not available`; a dangling
`apiKeyId` is 404; and every other failure collapses into the outer catch's 500
`Failed to fetch models`. The legacy `GOOGLE_IMAGEN` alias resolves to GOOGLE's
plugin for the list, while the response and the cache key echo the RAW provider
string.

Only genuinely live-fetched lists are cached in `provider_models` (capability
`image`, `displayName` equal to the id) — a built-in list would masquerade as
provider-confirmed on later reads. The write is best-effort, warned and
swallowed exactly as v4's is, so a cache-layer problem can never turn a
successful fetch into an error page.

The discovery crosses a new `image_discovery` engine seam, wired LIVE in
`quilltap-host` over the W4.7f provider (it needs only the HTTP client and the
image-download seam, so it is built independently of the spine bundle). An
unassembled seam answers a loud not-assembled refusal — the
`image_generation` precedent — rather than silently serving a keyless list,
which is the very thing this drift set out to stop.

The routes differential grows seven arms over the v4 oracle, with the
`provider_models` table as a measured comparand on four of them. Two oracle
repairs were needed to make them honest: `jest.setup.ts` globally mocks
`@/lib/llm/plugin-factory`, so `createImageProvider` answered `undefined` for
every provider and the action 500'd on an `undefined.supportedModels` read (the
same class as the empty-provider-registry trap; un-mocking moved no other
recorded row); and the committed fixture predates `provider_models`, which v4
creates lazily and v5's boot ensure creates at startup, so the oracle now emits
v4's own `CREATE TABLE` text and the harness applies it — otherwise "nothing
cached" would be true on the v5 side for the wrong reason and the
cache-only-live rule would be vacuously green. A named stale-oracle guard fails
loudly on a pre-P4.D100 regen. Five mutations proven red: caching a built-in
list, dropping `fetchError`, caching under the wrong capability, echoing the
resolved rather than the raw provider, and losing the built-in fallback.

The P4.D33 bank note is retired with a record of what the measurement found
beyond it; it still stands for the `p4.9h` embedding-profiles surface.
`docs/v4/developer/API.md` is refreshed from the pin.

#### 2026-08-22 — port(images): keyed model discovery + the Z.AI image-download seam (v4 `ca22ec45`, P4.D100)

_Versions: core 0.0.602, harness 0.0.525, host 0.0.76._

### The five providers' keyed model discovery

v4's `ca22ec45` gave (or hardened) a keyed `getAvailableModels(apiKey?)` on all
five image plugins — openai, google, grok, z-ai, and openrouter. The contract
is uniform in one respect and deliberately asymmetric in two: no key returns
the plugin's curated static list and makes NO request at all; a live failure
throws on every provider; an empty result throws on openai / google / grok /
openrouter but cannot arise on z-ai, whose union with two static ids makes
empty unreachable.

Ported into `model/image_dialects.rs` as the same three-part split the generate
dialects use — `build_models_request`, `parse_models_page`, `finalize_models` —
composed by `RealImageProvider` over the injected `WireTransport` through a new
`ImageModelDiscovery` trait (plus an object-safe `ErasedImageDiscovery` for the
dispatch engine). `supported_image_models` transcribes the five plugins'
`supportedModels`, which are NOT the manifests' `imageGenerationModels`: google
orders imagen first and openrouter's entries differ outright, and the route
reads the plugin's list.

Endpoints, auth headers, filters, paging, dedup, sort and every error sentence
are byte-faithful: OpenAI's `/v1/models` with `/^(dall-e|gpt-image)/`; Google's
`pageSize=1000` `x-goog-api-key` page loop with the imagen-`predict` /
gemini-with-"image"-`generateContent` pair; xAI's dedicated
`/v1/image-generation-models` accepting both the `models` and `data` top-level
keys with aliases riding along as selectable ids; Z.AI's
`/^(cogview|glm-image)/i` — the exact complement of its chat filter — unioned
with the two documented ids; and OpenRouter's paged `models.list()`. The two
OpenAI-SDK providers' thrown messages (`{status} {error.message}`, falling back
to the raw body) are reconstructed because the host wire hands back a status
where v4's SDK threw; both shapes were measured against the real SDK.

**A v4 bug, faithfully reproduced and pinned.** OpenRouter's discovery reads
three WIRE key names off an object `@openrouter/sdk`'s zod has already
rewritten. `Model$inboundSchema` is a `z.object`, so it STRIPS model-level
`output_modalities` and `supported_generation_methods` outright, and remaps the
architecture's genuine `output_modalities` to `outputModalities` (plural) where
v4 reads `outputModality` (singular). All three acceptance arms therefore read
`undefined`, and at `d5830439` every keyed OpenRouter Fetch Models call throws
"OpenRouter listed no image-output models for this API key". Measured at the
pin with a schema-valid payload carrying all four signals; the port reproduces
the SDK projection so v5 answers identically, and the
`openrouter/models_live_every_signal` corpus row is the tripwire that fires
when v4 fixes the read. Same class as the P4.D33 bank note and dogfood #24.

The committed image-dialects corpus grows 24 `kind:'models'` rows recorded from
v4's REAL plugin methods at the pin, carrying every request (method, URL, and
full header set) and each page's canned answer. The differential replays them
through the whole composed path over a canned transport keyed on the exact
request signature, header-subset-asserts what v5 builds, and additionally
exercises `finalize_models` in isolation. The corpus floor now asserts SHAPE —
a no-key row and a keyed row per provider, plus eleven named contract arms —
rather than a hand count. Mutation-proven: dropping the SDK projection, grok's
dedup, openai's sort, google's `image`-in-id conjunct, grok's endpoint, and
google's auth header each go red.

One blinded comparand is named rather than papered over: OpenRouter alone
neither sorts nor dedupes, and the corpus cannot see it while v4's own bug
empties every list, so that rule is pinned by a direct unit test instead.

### The image-download seam and Z.AI URL→base64

Z.AI's Images API answers with URLs (valid roughly 30 days) while every
Quilltap consumer — the chat handler, the avatar and story-background jobs,
`tools::generate_image` — reads only the base64 `data`. v5 measurably had the
resulting bug: `parse_zai` kept both fields, and the consumer decoded
`img.data`, so a URL-only Z.AI row produced zero bytes. v4's `ca22ec45`
downloads each URL inside the provider; this ports that, in the same place.

The download crosses a NEW narrow seam, `model::image_bytes::ImageBytesFetch`,
rather than a widened `WireTransport`: the wire seam's response carries a
`String` body and no headers, and an image download needs raw bytes plus the
`content-type`. Widening the wire for one caller would have touched every
dialect. `RealImageProvider` gained a second type parameter for it, defaulting
to a `NoImageBytesFetch` that fails loudly by name; every host construction
site that can reach a Z.AI profile — the avatar job, the story-background job,
the generate-image tool runner, and the avatar-preview renderer — is wired to
the new `ReqwestImageBytes`, a bare GET with no headers of ours (v4 issues
`fetch(img.url)` with no init object at all, and the URL is a signed link the
provider just handed us).

v4's per-image rules are carried exactly: keep an existing `b64_json` and make
NO request; otherwise download, treat a non-2xx as `Failed to download Z.AI
image: HTTP {status}`, default the mime type to `image/png` and let the
response's `content-type` override it only when it starts with `image/`,
truncated at the first `;`; and reject an entry carrying neither field with
`Z.AI image entry carried neither base64 data nor a URL`.

The recorder now scripts a distinct binary answer for a provider's follow-up
request, so the corpus's z-ai rows drive the whole composed `generate_image`
against v4's real plugin: the URL-only conversion, an entry with both fields
(no download), a non-`image/` content type, a 404 download, and an entry with
neither field. The differential asserts v4's download is a bare GET with an
empty header set, and a row with no scripted download that attempts one fails
loudly. Six mutations proven red, one per rule. The pre-existing
`zai_keeps_url_and_b64` parse pin is untouched — the conversion sits above it.

#### 2026-08-22 — port(images): route any `gemini*` id to generateContent (v4 `ca22ec45`, P4.D100)

_Versions: core 0.0.600._

v4's `ca22ec45` widened `isGeminiImageModel` so ANY id beginning `gemini`
routes to the Gemini `:generateContent` endpoint, with the original exact /
prefixed / substring arms over `GEMINI_IMAGE_MODELS` preserved behind it.
Live-fetched ids the honest Fetch Models list now surfaces (e.g.
`gemini-2.0-flash-preview-image-generation`) must not fall through to the
Imagen `:predict` endpoint, which serves only `imagen-*` models. The
predicate drives both the build and the parse side of the Google dialect.

Pinned by a new `google/gemini_live_fetched_id` row in the committed
image-dialects corpus, recorded from v4's REAL `GoogleImagenProvider` at
the `d5830439` pin, plus a direct unit test over the predicate. Red-first
proven: with the pre-widening predicate the row builds
`…/gemini-2.0-flash-preview-image-generation:predict` where v4 builds
`:generateContent`.

The z-ai `url_only` corpus row is deliberately held at its pre-drift
recording in this commit — v4's Z.AI URL→base64 download (the same drift
commit) lands with the bytes seam in a later unit of this lane, and the
recorder needs a distinct download response before that row can be
regenerated honestly.
#### 2026-08-22 — port(images): the Z.AI and NanoGPT size panels, the NanoGPT options verify-spec, and two gated beats (v4 `ca22ec45` + `781fc420` + `d5830439`, P4.D102)

_Versions: SPA 0.5.539._

The image-profile modal's structured parameters editor gains its first two
cases: v4's Z.AI and NanoGPT Default Size selects, with the eight and seven
sizes, their labels, and both footnotes transcribed verbatim. Providers v4 has
no size case for keep v5's JSON textarea stand-in.

The size change spreads the existing bag rather than rebuilding it, so a
parameter the panel does not render survives being edited — the one behaviour a
structured editor most easily breaks, and mutation-proven here. The select
assigns post-render, so an off-list stored size leaves it blank as React does.

NanoGPT's options group is verified, not reimplemented: a spec drives the
existing schema-driven panel with the contract's schema and asserts the group,
the label, and all seven enum values in order, confirming v4's position that no
bespoke editor code was needed. The thinking-turn client twin gains v4's
partition arms — every non-blank value classified by exactly one side, the blank
by neither, and the blank deferring to the model's own habit.

Two e2e beats land gated on the sibling server lanes (ACTIVATE-AT-UNIFY): the
Fetch Models control's keyless-then-keyed walk, and NanoGPT arriving in the
image picker from the live registry. Both are written so a fallback render fails
them rather than quietly passing.

#### 2026-08-22 — port(embeddings): the NanoGPT embedding-provider surface (v4 `781fc420`, P4.D102)

_Versions: SPA 0.5.538._

NANOGPT joins the embedding-provider union, the metadata map (display name,
requires-a-key, description — verbatim), the badge-class map, and the
needs-an-API-key list, so a NanoGPT embedding profile without a key now shows
the missing-key badge. The `qt-badge-provider-nanogpt` CSS rule lands with it,
including v4's quirk that `--qt-badge-primary-border` has no definition at all —
the same shape as ollama's secondary border, and deliberately left unpainted.

Two order items were refuted by measurement and NOT landed. v4 has no NanoGPT
row in the connection-profile fallback provider list at this baseline — its list
is the same seven v5 already carries, and none of the round's three drift
commits touch that file — so adding one would have been a v5-invented
divergence. And NanoGPT's client attachment fall-through is achieved by ABSENCE:
v4's static table has no row for NANOGPT (nor for Z_AI or DEEPSEEK), so the
correct port is no entry, with the known-stale note updated to name it.

#### 2026-08-22 — port(images): the honest Fetch Models flow in the image-profile modal (v4 `ca22ec45`, P4.D102)

_Versions: SPA 0.5.537._

The Model row is now v4's real control instead of a `defaultModels` listing.
`imageProfileListModels` is wired through `image-profiles.api.ts` (the contract
type already existed), the modal auto-loads on provider/key change, the Fetch
Models button carries v4's disabled states and both title strings, and the
source label beneath reproduces v4's four sentences character-for-character —
the provider tally with its singular/plural, and the three-way built-in
ternary.

Faithful details worth naming: a hard request failure falls back to the
registry's `defaultModels` WITHOUT a `fetchError`, so it reads as the plain
built-in sentence rather than the "Couldn't fetch" one, matching v4's fallback
branches; the provider is normalized before the call, since the server echoes
the raw string it was sent; and any `source` that is not exactly `provider`
reads as built-in.

The Model select assigns its value post-render rather than binding it, so an
off-list name leaves the control blank as React does instead of snapping to row
zero — reachable whenever a provider has no default models.

Eight pins were mutation-tested. Two initially survived and are now covered: a
stale source label during a re-fetch (the first fetch cannot show it, so only a
second one discriminates), and the non-`provider` source coercion.

The `Validate` button stays deferred — v4 did not move it this round.

#### 2026-08-22 — port(images): the Z.AI and NanoGPT image-provider entries (v4 `ca22ec45` + `781fc420`, P4.D102)

_Versions: SPA 0.5.536._

`FALLBACK_PROVIDERS` gains v4's Z.AI and NanoGPT rows — labels, default model
lists and `apiKeyProvider` transcribed verbatim — so the image-profile modal
offers both providers in v4's order when the `list-providers` fetch fails.
`PROVIDER_DEFAULTS` gains their icon rows (`ZAI` / success, `NGPT` / primary);
without them both fell through to the generic three-character abbreviation and
rendered as `Z_A` and `NAN`.

v4's matching `PROVIDER_BADGES` rows have no v5 home — `ProviderBadge` is a
standing named deferral — so they are recorded verbatim in that deferral note
rather than dropped, for whichever lane lands the badge surface.

#### 2026-08-22 — docs(porting): work orders for the `d5830439` drift round (P4.D100 → P4.D101 stacked ∥ P4.D102)

_Docs-only change._

The next round planned against new v4 baseline `d5830439` (three commits
of drift: `ca22ec45` honest image Fetch Models + Z.AI image generation
made real; `781fc420` the NanoGPT bundled provider; `d5830439` NanoGPT
thinking options). Three orders written with fresh two-sided surveys:
P4.D100 (server — the image list-models verb, five providers' keyed
model discovery, the bytes/download seam + Z.AI URL→base64, the gemini
routing widening), P4.D101 (server, stacked on D100 — NanoGPT end to end:
manifest through the generator, chat wire + the dual reasoning dialect,
images, embeddings, switch-table census, thinking options), P4.D102 (SPA
— the whole client half, contracts pinned, beats gated). Deliberately
left out: the merge-verb silent-keep sweep (collides again with this
round's settings/provider case files), the maintenance trio
(`response_parse_equivalence` is where NanoGPT's rows land), and the owed
💸 dogfood queue (a `/dogfood` pass right after unification). Also fixed:
P4.12's stale `STATUS: OPEN` header (closed-by-P4.13 since 2026-07-23;
findings #22/#25 long confirmed gone).

#### 2026-08-22 — unify: the `12fe3e6f` thinking-turn drift round (P4.D97 ∥ P4.D98 ∥ P4.D99 ∥ P4.54)

_Versions: core 0.0.599, harness 0.0.523, host 0.0.75, SPA 0.5.535._

All four lanes unified; the oracle baseline moves `b8449b3e` → `12fe3e6f`.
v4's bugs 84/85/86 are absorbed whole — bug 84 (the tool-error sentence)
and bug 85 (the DeepSeek thinking-prefill 400) were this port's own
dogfood filings coming back fixed. Server: the thinking-turn evaluator +
registry join, the manifest substrate's first per-model facts +
`thinkingTurnRule`, the prefill `runsThinkingTurn` threading, the
model-aware DeepSeek strip, and the retire-prefill heal keyed on v4's own
`migrations_state` ledger. SPA: the browser evaluator twin, the profile
editor's three thinking-turn behaviors, and bug 84's two-layer client fix
(the reducer carried the sibling `error` nowhere before). Maintenance: run
lines for 32 of the 39 `nothing_to_run` families.

Unification wires: `P4D97_THINKING_WIRE_LANDED` flipped true (the
thinking-turn e2e beat activated) and the contract diffed name-for-name
across sides. The §3 review found no blocking findings; one documented
mechanism divergence recorded (the editor's stored-null correction is a
fired-once latch where v4's effect can re-fire — the order sanctioned the
once-only spelling).

Gate: fmt clean; clippy both feature sets; release build; 443 test
binaries / 2,253 tests / 0 failed with the round's env block; the nine
affected families fresh at a PINNED `12fe3e6f` worktree through the sweep
driver, zero SKIP, changed-bytes verified in every regenerated NDJSON;
SPA 334 files / 4,970 tests + production build; full Playwright 233/233
zero skips (the suite grew with the activated beat). ⚠ v4 moved DURING the round (`ca22ec45`,
image-provider Fetch Models + Z.AI image generation) — pin `12fe3e6f`
for every regen until that catch-up runs.

#### 2026-08-21 — docs(porting): P4.54 closes — the run-line classification, executed and recorded

_No crate versions bumped._

The successor artifact to P4.53's inventory:
`harness/tools/sweep-results/2026-08-21-12fe3e6f-p4.54-run-lines.json`. It is the
sweep driver's own results record (same shape as P4.53's) for the 29 families
this lane gave run lines, with a `p4_54_classification` block folded in carrying
all 35 in-scope rows plus the four P4.D97 owns this round. Every run-line row
carries its family's verbatim `test result:` line, because a bare exit code is
not evidence that anything ran — that is what the driver's SKIP guard exists to
say.

The vacuous-green debt goes **39 → 10**. The residual is exactly the six
wire/seam contract pins ruled correctly-headerless plus P4.D97's four envelope
families, whose run lines ride a later maintenance pass.

Recorded as new debt rather than fixed here:
`p4_6ay_workbench_wire_contract` is a sixth contract pin of the identical class
— its own header cites the `p4_6ar_wire_contract` precedent by name — that the
driver's `EXEMPT_FAMILIES` constant omits, so its refusal sentence names the
wrong reason (`no_oracle` instead of the exemption). The repair is one line of
driver logic, which this lane's Ownership forbids. A note for whoever takes it:
`--self-test`'s two end-to-end arms pick a real exempt family and a real
non-exempt `no_oracle` family out of the LIVE debt list, so the class cannot be
retired to zero until the self-test gets synthetic families of its own.

Neither the driver nor any fixture, corpus or test assertion was touched.

#### 2026-08-21 — test(harness): scoped run lines for the harness and CLI `nothing_to_run` families

_No crate versions bumped._

P4.54 item 1, second half — the ten remaining in-scope families outside
`quilltap-web`.

Eight are committed-corpus differentials (`image_dialects_equivalence`,
`moderation_wire_equivalence`, `restore_vintage_state`,
`stream_decoders_equivalence`, `streaming_composer_equivalence`,
`tool_wire_call_site`, `tool_wire_equivalence`, `web_search_wire_equivalence`).
Their by-hand RECORDING stage stays non-runnable by the driver's own design —
`stages_to_run` excludes a committed-corpus regen so a sweep can never clobber
bytes checked into the repo — and the new line adds only the cargo half.
Verified after the edit that all eight still classify as `committed_corpus`: a
run line carrying a `QT_ORACLE*=` variable would have flipped them out of that
class and re-armed the recording stage, so none does.

`web_search_runner_wire` is the P4.42 wiring proof: no oracle, no env var, a
bare scoped line.

`cli_differential` is the one family in the set that can run vacuously. It is
env-gated on `QT_V4_CHECKOUT` and prints `skipping CLI differential: …` when it
is absent, which the driver's `SKIP_PROSE` detector catches and reports as
`skipped`, exit 3. Its run line therefore carries the gate itself —
`QT_V4_CHECKOUT=~/source/quilltap-server QT_NODE=$N/node` — which is what makes
the line self-contained rather than dependent on the operator's shell:
`normalize()` rewrites the v4 path to the driver's `--v4` pin and prepends the
Node 24 `N=` assignment.

#### 2026-08-21 — test(web): scoped run lines for the nineteen `nothing_to_run` web arms

_No crate versions bumped._

P4.54 item 1, first half. Nineteen `quilltap-web` integration arms extracted to
zero stages, so `recipe_sweep.py --run <fam>` refused them as `nothing_to_run`
rather than printing a free green. Each header gains the sanctioned indented
block — `//! Run:` at the prose margin, `cargo test -p quilltap-web --test
<family>` indented past it — which satisfies both P4.45 rules at once: the
INDENTATION rule (a command sits two spaces past the marker, prose at one) and
the ATTRIBUTION rule (`--test <family>`, so a SKIP in that run can be attributed
to this family rather than a stranger).

None of the nineteen reads a `QT_FIXTURE_*` variable — measured, not assumed:
they resolve their committed fixtures through `CARGO_MANIFEST_DIR` — so the bare
scoped line is genuinely self-contained and the silent-skip hazard does not
apply to this set.

Four of them (`change_passphrase_archive_sweep`, `characters_action_route`,
`characters_wardrobe_route`, `file_content_missing_404`) already carried a
`Run:` line with the command in backticks, written at the PROSE margin, so the
driver had always read it as a sentence. Those are rewritten into the indented
form rather than duplicated.
#### 2026-08-22 — docs(porting): finding #99 is fixed — v4 at `d9c98cf2`, v5 in P4.D99

_Docs-only change._

Row #99 moves from "NO v5 CHANGE — FILED UPSTREAM as v4 Bug 84" to FIXED,
recording both halves: v4's single resolver at the render site, and v5's
two-layer version (the reducer had to carry the sibling `error` before the
vertical could read it, so a render-site-only fix would have been inert). It
also carries v4's own correction of the filing — one `imageProfileId` feeds
both the tool slate and the executor, so the recorded "offered-but-refused"
repro is not reachable that way, though the frame shape the defect turns on
is identical either way — and notes the live look owed to the dogfood queue.

The standing-notes paragraphs the finding earned (the dead-observer lesson,
and "a field the server carries on purpose is worth grepping for on the
client") are left untouched: they are lessons, not state.

The Tier-2 `docs/v4/` mirror refresh is a measured no-op. `d9c98cf2` touched
`docs/developer/bugs.md`, the bug-84 filing doc, and
`help/image-generation-profiles.md`; the mirror carries none of the three
(`docs/v4/developer/bugs/` holds only `fixed/`, and `docs/v4/help/` holds
only `database-protection.md`), so there is nothing to refresh.

#### 2026-08-22 — fix(salon): render the failing tool's own sentence in the notice and the toast (bug 84)

_Versions: SPA 0.5.531._

Both `generate_image` failure surfaces now resolve through
`resolveToolResultErrorText`, so a refusal that names its own remedy —
`Image generation is not enabled for this chat` — reaches the user instead of
`Failed to generate image` / `Image generation failed: Unknown error`. The
generic strings stay, byte-identical to v4's, as the fallback for a frame
that carries nothing worth showing.

The one-level-too-deep `(call.result ?? {}).error` read is gone; the success
branch's cast narrows to `{ images?: unknown[] }`.

The deliberate faithful-reproduction pin is retired: the spec that asserted
the generic strings as all a failure could ever say now asserts the resolved
sentence with its prefix stripped, and keeps the generic arm as the fallback.
A third case pins the WHOLE path — real frames through `reduceChatFrame`,
then the reporter, then the notice — because the existing notice specs drive
the reporter directly through a cast, which is exactly how a reducer-level
drop stayed invisible. Reverting either layer alone turns that case red
(both mutations run).

#### 2026-08-22 — port(salon): carry the tool-result error sentence through the reducer (bug 84)

_Versions: SPA 0.5.530._

`PendingToolCall` gains `errorText`, and `applyToolResult` stores the frame's
sibling `error` on the matching call. It is carried RAW — the executor's own
`Error: ` prefix intact — so the reducer stays pure and the render site
resolves it, mirroring v4's own separation.

v5 is a two-layer fix where v4 is one: v4's hook receives raw frames, while
v5 splits into a pure reducer plus a vertical reporter, and the reducer used
to drop the sibling `error` before the vertical ever saw it. Placing the
resolver at the render sites alone would be inert — the data never gets
there.

New reducer coverage feeds real frames through `reduceChatFrame`: a failing
`generate_image` result carries its sentence onto the call, and a success
frame leaves `errorText` undefined. Dropping the carry turns the first red
(mutation-proven).

#### 2026-08-22 — port(salon): the tool-result error-sentence resolver, v4's client twin (bug 84)

_Versions: SPA 0.5.529._

Lands `resolveToolResultErrorText` as a standalone client twin of v4
`app/salon/[id]/hooks/useSSEStreaming.ts` (v4 `d9c98cf2`). The SSE
`toolResult` frame carries a failing tool's human-readable text in `error`,
a sibling of `result`, because `result` itself is null on failure; the
resolver prefers that sibling, keeps the nested `result.error` read as a
fallback, strips the executor's own leading `Error: ` prefix, and returns
`undefined` for anything empty so a caller's generic string still fires.

The spec is transcribed case for case from v4's own regression test
`__tests__/unit/hooks/useSSEStreaming-tool-error.test.ts`: the real failure
shape, the prefix strip, the unwrapped sentence, the nested fallback,
sibling-wins precedence, and every empty case.

No caller reads it yet — the reducer carry and the two render sites follow.
#### 2026-08-21 — test(e2e): the gated thinking-turn prefill beat (P4.D98 tier 2, ACTIVATE-AT-UNIFY)

_Versions: SPA 0.5.532._

One gated beat in the provider-options flow: a DeepSeek profile that opts
into thinking through the schema panel un-seeds the multi-character prefill
box on model pick, warns when re-ticked, and stands the warning down on an
explicit thinking-off — gated `P4D97_THINKING_WIRE_LANDED = false` until the
server half (P4.D97) serves the rule. The beat drives the profile-choice arm;
the model-facts arm cannot run keylessly (v4 refuses a keyless model fetch
for a key-requiring provider) and is pinned at the component tier instead.

#### 2026-08-21 — feat(spa): the profile editor's three thinking-turn behaviors (v4 bug 85, P4.D98 unit 3)

_Versions: SPA 0.5.531._

The connection-profile editor now answers "will this profile run a thinking
turn?" and seeds the multi-character prefill box accordingly (client half of
v4 `97d2fcb5`): a model pick on a NEW profile re-seeds from the model's
static facts (a seed, never a clamp); a stored row that never chose (null)
is corrected ONCE when the model list lands, through both fetch sites, with
a fired-once latch so it can never fight the checkbox; and ticking the box
on a thinking profile draws v4's warning paragraph byte-for-byte. The
`fetchedModelsWithInfo` plumbing carries the wire's `modelsWithInfo` rows
the editor needs. All behaviors degrade to the provider-rule-only seed when
the wire omits the rule and facts (the pre-P4.D97 state), spec-pinned; the
correction wirings and the latch are mutation-proven red-first.

#### 2026-08-21 — feat(spa): defaultMultiCharacterPrefill learns runsThinkingTurn (v4 bug 85, P4.D98 unit 2)

_Versions: SPA 0.5.530._

The client twin of v4 `lib/llm/multi-character-prefill.ts` gains the
`runsThinkingTurn = false` second parameter from `97d2fcb5`: a profile that
will run a thinking turn seeds the prefill box off on ANY provider, before
the provider rule is consulted. Doc comments carried (including the "Resist
adding a provider here" warning); the parity spec grows v4's three new
cases 1:1. The `profileUsesNamePrefill` resolution half stays server-side
(P4.D97).

#### 2026-08-21 — feat(spa): the evaluateThinkingTurn browser twin + the thinking-turn contract fields (v4 bug 85, P4.D98 unit 1)

_Versions: SPA 0.5.529._

Ports the client half of v4 `97d2fcb5`'s shared evaluator:
`settings/providers/thinking-turn.ts` answers "will this profile run a
thinking turn?" from the provider's declared `thinkingTurnRule`, the profile's
`parameters`, and the selected model's static facts — explicit choice wins
(absent/null/`''` all read as unset), else `thinksByDefault`, else no. Parity
spec transcribed 1:1 from v4's `thinking-turn.test.ts` (10 cases). The wire
contract mirror gains `ThinkingTurnRule`, `ProviderInfo.thinkingTurnRule`,
and the two `ModelInfo` facts (`supportsThinking` / `thinksByDefault`); the
server half that serves them is P4.D97's, and everything here degrades to the
provider-rule-only answer while the fields are absent, exactly as v4's client
does.
#### 2026-08-21 — test(harness): sweep-runnable run lines for the three envelope families (P4.D97 rider)

_Versions: harness 0.0.523._

`request_builder_equivalence`, `request_builder_google_equivalence`, and
`request_builder_google_wire_equivalence` were `nothing_to_run` refusals
under the sweep driver — committed-corpus families whose recording is a
deliberate by-hand step and whose headers carried no `cargo test` run line
(three rows of the P4.53-measured vacuous-green debt; P4.54 owns the OTHER
families' run lines this round and excludes these three by name). Each
header gains the scoped run line; all three now run end-to-end by name
through the driver, and `--self-test` stays at 0 failures.

#### 2026-08-21 — docs(porting): the P4.D97 mirror refresh + dispositions (v4 bugs 84–86 docs)

_Docs-only change._

The `docs/v4` mirror refreshed at the `12fe3e6f` pin: `developer/API.md`,
`developer/PROVIDER_PLUGIN_DEVELOPMENT.md`, `CHANGELOG.md`, and — new to the
mirror — `developer/bugs.md` (whose table carries the two docs-only commits'
rows: `e04405a5` filed bug 85, `c0984bdf` filed bug 84) plus the two fixed
filings under `developer/bugs/fixed/`. The v4 root README's model-table
delta has no mirror slot (the mirror never carried the root README or plugin
READMEs) — recorded, not invented. Dispositions: `lib/startup/prettify.ts`'s
migration pretty-label is NO-PORT (v5 surfaces no migration labels anywhere
— the P4.D79/D63/D73 precedent); package/lock + plugin-types version churn
is NO-PORT (the type additions were ported as the manifest substrate). Loud
deferrals recorded in the lane record: the two help-file deltas → the
`p4.9i2` bank, and the two 💸 live proofs → the dogfood queue.

#### 2026-08-21 — feat(db): the retire-prefill-on-thinking-profiles boot heal over v4's migration ledger (P4.D97 unit 6)

_Versions: core 0.0.599, harness 0.0.522, host 0.0.75._

v4's `retire-prefill-on-thinking-profiles-v1` data migration (`97d2fcb5`),
re-homed as a boot heal — with a NEW once-only mechanism for this repo: the
pass is data-only (no column absence to key off, the P4.D79-class guard
does not transfer), and re-running it would clobber a user's re-ticked
`true`. The guard is v4's OWN `migrations_state` ledger, interoperated in
both directions: the heal skips when the row
`retire-prefill-on-thinking-profiles-v1` exists (v4 migrated first), and
after healing writes the row with v4's exact `migrations/state.ts` shapes
(lazy CREATE of both tables under the migrations_state absence check, the
row's five columns, the two `migrations_metadata` upserts) so v4's runner
skips thereafter (`isMigrationCompleted`). `quilltapVersion` records which
app stamped the row — v4 writes its package version (`4.9.0-dev.43` at the
pin), v5 stamps quilltap-core's; the id is the key. Table/column-absent
skips stamp NOTHING (v4's `shouldRun` false arm — retried next boot).
Fresh instances match v4's observable outcome: no `migrations_state` at
provisioning (D23 — nothing invented), the row appearing on the first boot
exactly as v4's runner stamps every migration on its first run.

The heal transcribes v4's FROZEN rule tables + selection + per-row
evaluation verbatim ("a migration describes the world it ran in" — the
frozen copies deliberately do NOT follow the manifests), runs after
`ensure_connection_profiles_prefill_column` in `seed_built_ins` (the column
must exist first on a pre-4.9 instance), and logs the examined/cleared
counts. New differential `thinking_prefill_heal_equivalence`
(`QT_ORACLE_THINKING_HEAL`): both sides build the same migration-vintage
table from the committed `thinking-prefill-heal.json` spec (v4's own
integration-test matrix plus modelName-NULL / parameters-NULL /
string-"true" / `''`-option rows — 17 rows, 13 examined, 6 cleared); the
oracle drives v4's REAL migration `run()` + `recordCompletedMigration()`
and proves `isMigrationCompleted` skips a re-run; the diff covers every
profile row, the ledger row (ts/version normalized), the metadata upserts,
and the result message byte-exact. The second-boot no-op and the
re-ticked-true survival are pinned in both the module tests and the family.

#### 2026-08-21 — fix(deepseek): decide the thinking strip from the model, not the request body (v4 bug 86, P4.D97 unit 5)

_Versions: core 0.0.598, harness 0.0.521._

v4 `12fe3e6f` ported: `strip_thinking_incompatible`'s predicate becomes
`deepseek_will_run_thinking_turn(body)` — the profile's explicit
`thinking.type` enabled/disabled first, then the model's own habit from the
frozen catalogue copy by exact-id match (`deepseek-v4-flash` /
`deepseek-v4-pro`, both `thinksByDefault`). Reading only the body was the
bug: a default-state profile on a V4 model was judged not to be thinking,
and temperature/top_p/frequency_penalty/presence_penalty went out on a
request that ignores them. The frozen copy is pinned against the committed
`deepseek.json` manifest's `models` field by a unit test (the two homes
cannot rot apart — v4 carries the same duplication plugin-side), and the
predicate's truth table (explicit both ways, habit by exact id,
uncatalogued/no-model/unrecognized-shape fall-throughs) is unit-pinned.

The request-envelopes corpus regenerated at the pin with three new DeepSeek
rows (both modes; 263 → 269): `thinking-default-v4-model` (the strip fires —
temperature/top_p shed from base, the two penalties never land),
`thinking-model-default-empty` ("(model default)" — the shared applier skips
`''`, so the habit decides), `thinking-disabled-v4-model` (everything
stays). All 263 pre-existing rows byte-identical — the corpus's other
DeepSeek rows ride `deepseek-chat`, an UNCATALOGUED id, so they are the
keeps-the-params leg (the order's expectation that `profile-params` /
`profile-params-skips` would shed params was WRONG about the corpus's model
id — measured, not assumed). The pre-fix red is on record: the commit-C
workspace gate ran the old predicate against the regenerated corpus and
failed at `thinking-default-v4-model[stream]` with the params present.

#### 2026-08-21 — feat(llm): thread runsThinkingTurn through the prefill default (v4 bug 85, P4.D97 unit 4)

_Versions: core 0.0.597, harness 0.0.520._

The prefill second-arg from v4 `97d2fcb5`: all three
`multi_character_prefill` functions gain `runs_thinking_turn` (checked FIRST
in the default; a stored boolean still outranks it — the tri-state is
intact), and the three call sites thread the registry join: the create
route's absent-field default now resolves
`profile_runs_thinking_turn(provider, modelName, parameters)` (the PUT arm
verified untouched, as v4 left it), and both `use_prefill` producers
(orchestrator + regenerate-swipe) join over the profile row. The prefill
oracle corpus doubled with the runsThinkingTurn axis (288 rows, full-product
shape-asserted; deleting the thinking arm reds the family). Four new
settings-routes create arms (absent-field DeepSeek-V4 default now false;
explicit thinking-disabled keeps true; Ollama enable_thinking true; a stored
true surviving) — regenerating them exposed an ORACLE HARNESS GAP: the jest
environment's provider registry was EMPTY, so v4's join silently answered
the old provider default; the case now registers the two declaring dist
plugins exactly as production startup does (a rule-less plugin and an absent
one evaluate identically, so the other arms are unaffected). Family at 132
cases; the create join mutation-proven. `orchestrator_tier3` regenerated
fresh at the pin: no movement (its profiles carry stored booleans), as the
order predicted; the producer-site wiring matches v4's own coverage level
(v4 tests the functions, not the context-builder call) — the live DeepSeek
repro rides the dogfood queue. The `connection_profiles_tier2` question
resolved by inspection: that family drives the REPOSITORY, which never
resolves defaults, so no op can observe the route default there.

#### 2026-08-21 — feat(api): serve thinkingTurnRule + model thinking facts on the wire (v4 bug 85, P4.D97 unit 3)

_Versions: core 0.0.596, harness 0.0.519._

The two wire serializations from v4 `97d2fcb5`. `provider_list` (v4
`GET /api/v1/providers`) serves `thinkingTurnRule` per provider — `?? null`,
positioned after `optionsSchema`, key order carried by the typed struct's
field order. `model_fetch` (v4 `POST /api/v1/models`) spreads
`supportsThinking` / `thinksByDefault` onto each `modelsWithInfo` echo row per
exact-id match against the manifest's model catalogue, keys omitted when the
catalogue has no entry — the merge lives at the route as v4's does, so it
covers every fetcher. Measured at the pin: the GET leg is untouched in v4 (the
cache write carries no thinking facts), so `model_list` stays as it was —
pinned by a new wire-actions test asserting the cache read stays fact-blind.
`providers_listing_equivalence` regenerated at the pin now compares
`thinkingTurnRule` byte-for-byte (key order included, the optionsSchema
precedent) with an exactly-two-rules shape assertion; removing the serializer
line reds the family (mutation-proven).

#### 2026-08-21 — feat(llm): port the thinking-turn evaluator + manifest substrate (v4 bug 85, P4.D97 units 1–2)

_Versions: core 0.0.595, harness 0.0.518._

The pure evaluator from v4's `lib/llm/thinking-turn.ts` (`97d2fcb5`):
`evaluate_thinking_turn(rule, parameters, model)` answers "will this profile
run a thinking turn?" — an explicit rule-keyed choice in the profile's
parameters wins (unset = absent/null/empty string; `disabledValues` checked
before `enabledValues`), else the model's `thinksByDefault` habit, else false.
The `ThinkingTurnRule` and `ThinkingModelFacts` types land in the
provider-manifest substrate (v5's analog of `@quilltap/plugin-types`), carried
as opaque JSON scalars so the wire re-serializes them byte-for-byte; value
matching uses JS `===` semantics (numbers compare as f64, never across
types).

The manifest substrate grows two optional fields (`schemaVersion` stays 1 —
additive optionals): `thinkingTurnRule` after `optionsSchema`, and `models`
(fact-bearing model-catalogue entries) after `fallbackModels`. The generator
learned both and all nine manifests were regenerated at the `12fe3e6f` pin —
only deepseek (rule + two V4 model entries + the bug-86 helpText rewrite,
which rides the same regen) and ollama (rule only) moved, byte-reviewed. The
generator emits the fields only where the plugin declares them, so the other
seven manifests stay byte-identical; a fact-less model entry is observably
identical to no entry on every consumer, so only fact-bearing entries are
emitted. Registry accessors `thinking_turn_rule` / `model_thinking_facts`
plus the join `profile_runs_thinking_turn(registry, provider, model, params)`
(v4's `providerRegistry.profileRunsThinkingTurn` — JS-falsy guards, exact-id
model lookup) round out the substrate.

New tier-1 differential `thinking_turn_equivalence`
(`QT_ORACLE_THINKING_TURN`, 1,134 cases over rules x parameter shapes x model
facts), with the disabled-before-enabled order and the empty-string-is-unset
arm both mutation-proven (the latter needed `''` planted inside a rule's own
disabled list — a set-but-unmatched value falls through identically
otherwise). The prefill threading, wire serializations, strip predicate, and
the heal arrive in the following units.

#### 2026-08-21 — docs(porting): plan the `12fe3e6f` drift round — four work orders (P4.D97 ∥ P4.D98 ∥ P4.D99 ∥ P4.54)

_Docs-only change._

v4 moved five commits past `b8449b3e`, all dated today: bug 84 (the
tool-result error sentence — this port's own finding-#99 filing, now fixed
v4-side), bug 85 (prefill-hostility scoped to thinking models via a new pure
`thinking-turn` evaluator, a declarative per-plugin `thinkingTurnRule`,
per-model `supportsThinking`/`thinksByDefault` facts, and a data migration
retiring the stored prefill on thinking DeepSeek/Ollama rows), and bug 86
(DeepSeek's thinking-incompatible param strip now decided from the model,
not the request body). The bugfix branch is quiet. Four orders written
against the new `12fe3e6f` baseline: P4.D97 (server half — evaluator,
manifest substrate growth including the first per-model facts, registry
join, both wire serializations, the bug-86 predicate, and the once-only
heal interoperating with v4's `migrations_state` ledger), P4.D98 (the
profile-editor client half — client evaluator twin, model-change re-seed,
stored-null correction, the warning paragraph), P4.D99 (bug 84's two-layer
client fix — the reducer currently drops the sibling `error` before the
vertical sees it), and P4.54 (run lines for the 35 in-scope
`nothing_to_run` families from P4.53's committed inventory).

#### 2026-08-21 — docs(porting): the anti-chorus dogfood pass — 18 pass, two findings, nine live proofs discharged

_Docs-only change._

The `c8a3cf77` and `b8449b3e` rounds met real data. Both multi-character anchor
routes proven at the byte level on a purpose-built three-character chat; the
direct-address rewrite behaves in production (a third-person mention no longer
arms the caution, a vocative does), and a character passed with the skip
sentinel unprompted. Per-turn conversation summaries mutation-proven ON vs OFF
over the persisted whispers. The vision send, the P4.50 log line, the bug-76
key heal, the tool-change splice-once, whispered announcements, the roleplay
quote delimiter, and the failed-import warnings all discharged.

Two v4-heuristic observations recorded, both v4-faithful: `and` is a vocative
lead-in, so a roll-call recap ending `X and Y.` re-arms the caution the
anti-chorus fix withholds; and the caution can never see the message that just
addressed the responder, because the user message is persisted after the
eligibility read in both apps.

Finding #98: the Serper key configured through v4's Settings → API Keys is
invisible to v5, which reads only `SERPER_API_KEY` — the search-provider plugin
registry is the standing P4.42 deferral, so web search is dark on a real
instance. No v5 code changed; the refusal path itself is faithful.

#### 2026-08-21 — fix(documents): Document Mode fills its workspace tab, so source mode is more than three lines

_Versions: SPA 0.5.528._

Dogfood finding #97. Opening a store document through the rail's Document Mode
entry and switching to markdown source rendered the textarea 77px tall inside a
788px pane — three visible lines of a 52,000-character document, with 700px of
empty pane below it. The WYSIWYG branch masked it: ProseMirror grows to its own
content and the pane scrolls, so nothing looked wrong until the toggle.

`StandaloneDocumentView` declared `host: { class: 'flex flex-col flex-1 …' }`,
but the workspace mounts it inside `qt-tab-view` — an Angular custom element
with no host styling, i.e. `display: inline`. `flex-1` in a non-flex parent is
inert, so the view collapsed to content height and every `h-full` beneath it
measured against that. v4 has no such element: its `TabView` renders context
providers only, so `DocumentPane`'s `flex flex-col h-full` root is a direct
child of `.qt-tab-pane` and measures the grid cell. The host class is now
`h-full`, which resolves against `.qt-tab-pane` the same way — and the same way
the Salon's `block h-full` host already did. Re-measured on the same document:
77px to 612px.

`standalone-document-view` was the only `flex-1` host mounted directly by the
tab registry; the other five all sit inside real flex parents.

Pinned by a spec asserting the rendered host class carries `h-full` and not
`flex-1` (jsdom computes no layout, so a height assertion there would be
vacuous), and by extending the existing p4.9l2 standalone-toolbar beat to
*measure* the textarea against its pane. That beat already toggled source mode
and asserted the markdown bytes, and passed throughout — which is why the bug
shipped.

#### 2026-08-21 — docs(porting): the case-folding divergence ratified

_Versions: core 0.0.594._

The human ratified P4.D96's recorded case-folding divergence (Rust Unicode
simple folding vs v4's ECMAScript canonicalize — over-detection only, on a
handful of exotic code points). The skip_signal doc comment, the order
status header, and the CLAUDE.md round bullet now carry the ratification;
a ruling record is appended to the status log. Comment-only source change.

#### 2026-08-21 — port(unify): the b8449b3e round lands — the anti-chorus drift, the memories fixture vintage, the sweep-driver follow-ups

_Versions: core 0.0.593, harness 0.0.517, SPA 0.5.527._

The three-lane `b8449b3e` round unifies onto main and the oracle baseline
moves to `b8449b3e`. P4.D96 ports v4's anti-chorus commit whole (the
direct-address `isRecentlyAddressed` rewrite in core and the SPA twin, the
grown turn-skip note, the turn-anchor restructure with the
GROUP_SCENE_DISCIPLINE block, a new tier-1 turn-anchor family; the
case-folding divergence is recorded and awaits ratification). P4.52 widens
the committed memories fixture pair to the current schema vintage and
retires the housekeeping ruled vintage row to a plain equality. P4.53 makes
sweep-recipe checkout aliases unforgeable, turns empty-stage runs into a
named refusal (39 families measured), and repairs the five clobbering
headers. Unification work: version accumulation (harness took six lane
bumps), a mid-pick Cargo.lock repair, and the round docs. Gate: fmt, clippy
both feature sets, release build, driver self-test 0 failures, the seven
affected families regenerated fresh at a pinned b8449b3e worktree (zero
SKIP), cargo test --workspace 441 binaries / 2,237 / 0, SPA 332 files /
4,936 / 0 with a clean build, and the full Playwright suite green.

#### 2026-08-21 — docs(porting): record the P4.D96 gate

_Docs-only change._

The lane's verification gate, all green: fmt; clippy plain and with
`quilltap-core/native-transport`; `cargo test --workspace` at 441 binaries /
2,237 passed / 0 failed with both lane oracle env vars set; both lane families
by name through the sweep driver against the `b8449b3e` pin, zero SKIP; the
fourteen-family spine batch at 14 ok; SPA 332 files / 4,936 tests and a clean
build. Playwright was not run — no e2e spec changed.

#### 2026-08-21 — docs(porting): the P4.D96 spine regen, the b8449b3e NO-PORT, and two corrections

_Docs-only change._

Records unit 4 (fourteen spine families regenerated at the `b8449b3e` pin
through the sweep driver — all green, exit 0, zero SKIP) with the coverage
measurement that green alone does not give: `orchestrator_tier3` is the one
family carrying all three changed byte sequences through the production spine
(56 discipline blocks, 25 turn notes, 49 identity instructions),
`regenerate_swipe_tier3` and `salon_swipe_generate` carry the discipline block
only, and the other eleven carry none. No committed corpus in the tree carries
the old bytes, so no fixture moved.

Records unit 5: v4 `b8449b3e` (the jest `--no-sparkplug` guard for bug 83) is
NO-PORT — v5 has no jest — and our own zone-legged `jest-zone-globalsetup.cjs`
chains v4's `globalSetup` rather than replacing it, so the guard still arms for
the two families that pass it. The regen venue gains from the commit; nothing is
bypassed.

Corrects two claims in this round's earlier entries, which the unit-4
measurement disproved: the turn-skip note and the turn anchor each had no
*direct* differential before this round, but both were covered transitively by
`orchestrator_tier3`. Also refreshes the `docs/v4` mirror of the nothing-to-add
spec, and records the deferrals: the two help docs to the `p4.9i2` bank, and a
live group-scene walk to the dogfood queue.

#### 2026-08-21 — feat(salon): the client skip-signal twin follows the direct-address rewrite

_Versions: SPA 0.5.527._

The Salon's client-side `isRecentlyAddressed` — which guards the human Skip
button and the turn banner — takes v4 `e22f7b36`'s rewrite as a near-verbatim
transcription: the two vocative constants and `buildDirectAddressRegex`, with
`escapeRegex` replacing the mention scan as the module's regex import. The
parity spec grows v4's own eight new cases 1:1, plus the no-usable-name null
return and its whisper-still-wins twin, which the server family also pins.

`findMentionedCharacterIds` stays in the client helpers with no consumer, as the
faithful mirror of v4's still-shipping `mentioned-characters.ts` (whose Rust
twin keeps its own consumer in `services::off_scene`); its doc comment now says
so.

#### 2026-08-21 — feat(salon): a group-scene discipline block rides every multi-character turn

_Versions: core 0.0.593, harness 0.0.513._

Ports the anchor half of v4 `e22f7b36`. `applyMultiCharacterTurnAnchor` is
restructured: it finds the system message first, builds a list of system
additions (the prose route contributes its identity instruction first; the
prefill route contributes none), always appends the new byte-exact
`GROUP_SCENE_DISCIPLINE` block, joins the list with a blank line and appends it
to the system message when one exists, and only then — prefill route — pushes
the assistant `[Name]` message. Two behavior deltas: the prefill route now
edits the system message too (it used to return before reading one), and a
prose turn appends two blocks where a prefill turn appends one.

The discipline block is the anti-chorus content rule set: no recap openings, no
agree-then-add, no reusing another character's coined phrases, no restating the
plan, speak only to change something, vary length. The identity anchor keeps a
turn attributed to one character but says nothing about its content, and with
the previous turns as the strongest style examples in context, models converge
into exactly the chorus the block forbids.

New tier-1 differential `multi_character_turn_anchor_equivalence` over v4's real
exports — no oracle drove this function *directly* before; the spine covered it
transitively through `orchestrator_tier3`. Both routes × system message
present/absent, system-message placement (not first; two systems, the first
wins), empty system content, an empty message array, and four interpolated-name
shapes, each row diffed as the full post-call message array, plus the constant
on its own. Mutation-proven on the join separator, the prefill-route system
append, the prose identity instruction, the additions order, and one byte of
the constant.

#### 2026-08-21 — feat(salon): the turn-skip signal requires direct address, and the note calls restating a pass

_Versions: core 0.0.592, harness 0.0.512._

Ports the turn-skip half of v4 `e22f7b36`. `isRecentlyAddressed` no longer
fires on any name mention: it now requires a direct address — the character's
name or an alias in a vocative position, an `@`-mention, or a targeted whisper.
A chorus-prone group scene opens every turn with a roll-call recap naming most
of the cast, so the mention-based signal marked everyone addressed forever, the
"answer rather than pass" caution fired for every character on every turn, and
nobody ever passed. The Turn note's base text gains a closing paragraph (a
reply that mostly restates or endorses what has been said is not substantive —
pass), and the caution is reworded to "directly addressed since they last
spoke".

The regex is built from the character's trimmed name and aliases, longest
first, escaped. Three JS-fidelity decisions were made by measurement rather
than assumption, each pinned by its own oracle vector: JS `\s` is written out
(Rust's `\s` excludes U+FEFF and includes U+0085, both reachable from message
content); JS's `m`-flag `^`/`$` honour CR, U+2028 and U+2029 where Rust's
`(?m)` anchors on `\n` alone, so those three are matched by consuming them
next to the token, which is equivalent for the existence of a match; and
Unicode simple case folding is a recorded divergence from ECMAScript
Canonicalize on a handful of exotic code points (U+212A, U+1E9E, U+017F),
erring toward "addressed", as `crate::mentioned_characters` already does.

The skip-signal differential grows v4's own eight new suite shapes 1:1 plus the
fidelity vectors, and gains a `turnSkipNote` kind driving v4's real
`buildTurnSkipInstruction` — the note bytes had no *direct* differential before
this round (every `build_context_tier3` op passes `turnSkip: None`); the spine
carried them only transitively, through 25 `orchestrator_tier3` rows.
#### 2026-08-20 — test(harness): widen the memories fixture pair to v4's schema vintage; retire the ruled vintage row (P4.52)

_Versions: harness 0.0.512._

The committed `memories-{main,mount}.db` pair predated seven columns v4 has
added since it was baked, and v4's `BaseRepository._update` writes
`$set: validated` — the whole validated entity — so every schema field with a
Zod default is named in the UPDATE and a column the fixture lacks is fatal.
That is why `chatSettings.updateForUser` died on `no such column:
composerEmoji` and why `housekeeping_config_set` has been pinned in
`memories_routes_equivalence` as a RULED VINTAGE ROW (v4's fixture-artifact
500 asserted alongside v5's 200) since the `c8a3cf77` unification.

Measured the gap table by table against v4's live `generateDDL` at
`b8449b3e` (the dump matches the committed `fresh_schema.json` exactly, so no
D23 drift rides along): three tables in the main partition, zero in the mount
partition. New script
`harness/oracle/fixtures/migrate-memories-fixture-columns.ts` applies v4's own
migration ALTERs, verbatim and idempotently, for `characters.archivedAt` /
`archiveFileId` / `archivedAvatarFileId`,
`connection_profiles.multiCharacterPrefill`, and `chat_settings.composerEmoji`
/ `composerUnicode` / `smartTypographySettings` — the migration shape a real
instance carries, per the `migrate-fixtures-pascal-columns` precedent. Every
pre-existing cell in both partitions is byte-identical after the widening
(pre/post row dumps of all 21 tables compared); the only delta is the seven
new columns reading v4's defaults.

Two `generateDDL` columns are deliberately still absent:
`characters.metadata` and `canChooseOutfit` are MANAGED_FIELDS that v4
`delete`s from every DB row on create and update, no v4 migration adds them,
and a real migrated instance does not carry them either.

With the fixture widened, v4 answers 200 and the tripwire fired exactly as
designed; the ruled row retires to a plain `check_body`. Mutation-proven:
dropping `composerEmoji` from a scratch copy puts v4 back on the bare 500 and
reddens the family with the same "expected an error arm" refusal the ruling
documented.
#### 2026-08-20 — fix(harness): normalize() neutralizes any checkout-alias assignment (P4.53)

_Versions: harness 0.0.514._

Repairing the five clobbering headers made the committed recipes read true; it
did not make the class unrepeatable, because the next header written
`W=${V5W:-$HOME/source/quilltap-v5}` brings the bug straight back and nothing
in the driver would notice. So the assignment no longer decides anything:
`normalize()` rewrites every `V5W=` / `WT=` / `V5=` / `W=` assignment statement
to the driver's `--v5w`, and announces the rewrite once per family — a silent
one would hide exactly the rot it neutralizes.

The notice is quiet for a rewrite that changes nothing: a value that already IS
the checkout, and the SELF-referential `V5W=${V5W:-...}` (the sanctioned
convention — the driver injects `V5W`, so the `:-` fallback never fires and the
rewrite substitutes the value it would have produced anyway). 173 families'
normalized scripts change under this rewrite and every one of them is settled,
so the notice fires zero times across the tree today. The CROSS-referential
form is not settled, and that is the whole point: the one line that fires is
the one worth reading.

The match shape is deliberately narrow. `V5W=/some/path cargo test ...` is an
ENV PREFIX, not an assignment, and rewriting it would swallow the command, so
the statement must end after its value (or its trailing `#` comment). A
`;`-joined assignment is caught; a non-alias variable, and one that merely ends
in an alias name, is not. A header that already assigns the checkout under test
is rewritten to the identical value and is not announced.

The alias INJECTION regex is widened in the same pass: it matched `$VAR` and
`${VAR}` but never `${VAR:-...}`, which is the other half of why the clobber
survived — `V5W` was never injected, so the `:-` default fired.

`--self-test` gains the mutation pins: the live defect verbatim (rewritten, and
main's path gone from the whole script), the announcement, the reference form,
two env prefixes that must survive untouched, the `;` form, the no-op silence —
and a durable regression pin that reads every `.rs` and `.ts` header in the tree
and refuses any that defaults one checkout alias from a DIFFERENT one. The
self-referential `V5W=${V5W:-...}` is the sanctioned convention and stays
allowed; the cross form is the clobber, and the backstop above hides it from
every other symptom.

#### 2026-08-20 — fix(harness): the sweep driver refuses a family with nothing to run (P4.53)

_Versions: harness 0.0.513._

`--run <family>` on a family whose recipe extracts to EMPTY stages printed
`OK: <family> recipe ran end-to-end` and exited 0 having executed nothing at
all — the driver's own uniform on the vacuous green it exists to prevent.
Every `exempt` compile-time pin and every `no_oracle` integration arm answered
that way, as did every `committed_corpus` family whose only stage is the
by-hand recording a sweep deliberately skips: 39 families in total, each of
which a gate script could name and collect a green line from for free.

`nothing_to_run` is now a refusal in the class the P4.51 unknown-family error
established — raised BEFORE any stage (nothing below it runs, not even the
stale-oracle deletion), exiting 2, and naming which class the family is in and
what to do about it. `--run-all` records the status on the row, so a batch
containing one is not green, and its results artifact gains a `nothing_to_run`
key enumerating the whole set over the entire checkout rather than just the
batch — the debt was previously recorded nowhere.

`stages_to_run` is now the single source of truth for "will anything execute?",
shared by the refusal and the execution loop, so the two cannot drift apart.
`cmd_run` also distinguishes REFUSED from FAILED: a refusal is the driver
declining to pretend, and calling it a failure sends the operator hunting for a
broken recipe.

None of the 39 is hard recipe rot — the exempt pins and committed corpora have
nothing to regenerate by design, and the no_oracle arms build their state in
process. Roughly 34 of them could gain an explicit scoped `cargo test --test
<family>` run line so `--run` can prove them; that inventory is the next
maintenance pass's, deferred by the work order.

#### 2026-08-20 — fix(harness): five oracle-case headers stop clobbering the driver's injected checkout alias (P4.53)

_Versions: harness 0.0.512._

Five case headers opened their regen recipe with
`W=${V5W:-$HOME/source/quilltap-v5}` and referenced `$W/...`. That reads as a
courtesy default and is a live clobber. The sweep driver injects the checkout
under test by PREPENDING `W="<--v5w>"` (it does so because `$W/` appears in the
body), and the header's own assignment then overwrites it — with
`${V5W:-...}` rather than `$V5W`, because the driver's prepend regex matches
`$VAR` and `${VAR}` but never `${VAR:-...}`, so `V5W` was never injected and the
`:-` default fired.

Measured on `brahma-console-routes.test.ts`, the only header where the alias is
live (its `.rs` family restores its regen from this header): a sweep run from a
lane worktree staged that family's case file AND both `QT_FIXTURE_BRAHMA_*`
databases from MAIN, then exited 0. A marker planted in the worktree's copy of
the case never reached the staged `/tmp` mirror. The other four are dormant
today only because their `.rs` headers are authoritative; they are the same
landmine and are repaired with it.

All five now use the sanctioned convention — `V5W=${V5W:-$HOME/source/quilltap-v5}`
with `$V5W/...` references — so the `:-` default finds the value the driver
injected. Header lines only; no case bodies changed, and the classifier's
report over all 412 families is byte-identical before and after.

#### 2026-08-20 — docs(porting): plan the b8449b3e round — the anti-chorus drift catch-up + two maintenance lanes

_Docs-only change._

Work orders for the next round, planned against v4 `b8449b3e` (two commits
past the `c8a3cf77` baseline; drift-checked at planning — `bugfix` carries
nothing new). P4.D96 ports the `e22f7b36` anti-chorus commit (the
direct-address `isRecentlyAddressed` rewrite in core + the SPA twin, the
turn-skip note bytes, the turn-anchor restructure with the
GROUP_SCENE_DISCIPLINE block, plus a new tier-1 turn-anchor oracle family)
and dispositions `b8449b3e` (jest Sparkplug, bug 83) as NO-PORT. P4.52
widens the committed memories-{main,mount}.db fixture pair to the current
schema vintage and retires the housekeeping_config_set ruled vintage row.
P4.53 closes the three recorded sweep-driver follow-ups (the live `W=`
header self-clobber, the `nothing_to_run` refusal, the `normalize()` alias
neutralization). The three lanes meet nowhere; the baseline moves to
`b8449b3e` at unification. The merge-verb silent-keep sweep is deliberately
deferred a round (its case-file footprint collides with both maintenance
lanes).

#### 2026-08-20 — port(unify): the c8a3cf77 round lands — per-turn summaries, the document-pane toolbar, the sweep smalls

_Versions: core 0.0.591, harness 0.0.511, SPA 0.5.526._

All three lanes unified: P4.D95 (the whole `870a57fa` drift — the per-turn
conversation-summary cadence riding the turn's one embedding), P4.9L2 (the
Document-Mode pane's formatting toolbar, closing m6 row 14b), and P4.51 (the
two `W=` recipe headers + the sweep driver's unknown-family refusal). The
oracle baseline moves to `c8a3cf77`; v4 moved again mid-round (`e22f7b36`,
anti-chorus discipline — the next round's drift), so every gate regen ran
from a detached worktree pinned at `c8a3cf77`.

The §3 review fixed one would-have-shipped divergence: an invalid
recall-config value (wrong enum, non-boolean) now answers v4's 400
"Validation error" instead of silently keeping the stored value — pinned by
three new oracle arms including a writes-nothing composite. The gate's first
by-name family run then caught two more: `housekeeping_config_set` has been
a silent standing red since v4 4.8.2 (a fixture-vintage artifact — v4's
whole-row UPDATE dies on the pre-4.8.2 committed fixture; now a ruled
vintage row with a repair tripwire, the fixture widening being a named
maintenance item), and the composite arm's `storedAfter` was being dropped
by the oracle runner's record shaper (now passed through).

Gate: seven families by name over fresh pinned oracles, zero SKIP; 440 test
binaries / 2,236 / 0; clippy both feature sets; release build; ng 332 files
/ 4,929; full Playwright 232/232 zero skips (the suite grew by the round's
three beats). Both shared version files accumulated (the identical-bump trap
hit harness AND the SPA).

#### 2026-08-20 — feat(settings): the per-turn conversation-summary toggle on the Recall Relevance card (P4.D95)

_Versions: SPA 0.5.523._

The client half of the per-turn conversation-summary cadence. `RecallConfig`
grows `perTurnConversationSummaries`, and Settings → Memory → Recall Relevance
grows v4's third checkbox ("Consult past conversations every turn") with its
body copy carried byte-for-byte. Only the toggled field travels on save — the
server merges over what it stores. Two specs cover it, and a live e2e beat
toggles it on, reloads the page to prove the value came back from the instance
settings row rather than the card's local echo, checks the sibling toggle is
untouched, and toggles it back off.

The new help-doc section v4 wrote alongside this
(`help/memory-recall-relevance.md`) is banked to `p4.9i2` with the rest of the
help family; the bank note is on `m6-screen-parity.md` row 11.
#### 2026-08-20 — feat(memory): per-turn conversation summaries riding the turn's one embedding (P4.D95)

_Versions: core 0.0.591, harness 0.0.510._

Ports v4 `870a57fa`. A new instance-wide setting,
`memoryRecall.perTurnConversationSummaries` (Settings → Memory → Recall
Relevance, off by default, no chat/project/character override), re-runs the
relevant-past-conversations search over the responding character's vault
`Conversation Summaries/` folder on every turn and folds the result into the
consolidated Commonplace Book whisper's `relevantConversations` part. Until now
that list refreshed on three cadences only — the chat-start / character-join
recap, each summary fold, and retrospective turns — so between folds it stood
still while the conversation moved on.

It costs no extra embedding call. `search_memories_semantic` now reports the
vector it embedded through a capture out-param (fired the moment the embedding
lands — before the dimension guard, never for the extra probes, never on the
text-search fallback), and `search_vault_conversation_summaries` accepts it as
`precomputed_embedding`. The proactive pre-compute path threads its vector
through `proactive_recall_task` → the orchestrator → `build_context`, so the
reuse holds on both memory paths; with no vector available the cadence sits the
turn out rather than embedding on its own.

Dedup: the per-turn list filters against the standing fold-posted
`relevant-conversations` whisper (read backwards, stopping at the first match —
the fold refresh sweeps a target's prior whispers when it posts a fresh one),
the retrospective mini-recap now filters against both lists, and the cadence
stands down entirely on the turn the recap runs. The whisper target scope is
computed once and shared, so a list can never be filtered against one scope and
whispered to another. The four list-length ramp constants moved next to the
search so both cadences read the same numbers.

Also here: `get_memory_recall_settings` returns a struct rather than a
`(scope_policy, expand_related)` tuple (a tuple that grows a third field
silently re-orders every destructuring that reads it), and the recall-config
GET/POST verbs carry the third field with v4's byte-exact merge and response
bodies.

Two venue repairs ride along, neither of them a port. `memories-config.test.ts`
now mocks the background-job processor host: under jest the fork of
`child-entry.ts` dies instantly and its crash-and-respawn loop disturbed
whichever case happened to be writing. Each case also gets its own
`QUILLTAP_DATA_DIR`, because one shared `data/quilltap.lock` raced a case's
close against the next case's open. The `memories_routes_equivalence` recipe
header, which produced only one of the two NDJSONs the test needs, now carries
both regen stages and both env vars.

Not fixed here, diagnosed and documented in the family header:
`housekeeping_config_set` is a pre-existing red. v4's `chat_settings` UPDATE
names every column, and the committed `memories-*.db` fixture predates the three
columns v4 added in the 4.8.2/4.8.3 round, so it dies on `no such column:
composerEmoji`. Reproduced against an unmodified oracle case at `c8a3cf77`. The
repair is a fixture-vintage one and belongs to a maintenance order — that `.db`
pair is also read by two e2e specs.
#### 2026-08-20 — test(e2e): walk the document toolbar in both of its hosts

_Versions: SPA 0.5.525._

`salon-documents-flow` gains a beat that seeds a roleplay template carrying one
`wrap` delimiter, hangs it on the chat through `chatUpdate` (reading the chat
back, since a dispatch verb answers 200 on an ignored field), opens a document,
and asserts the pane's toolbar carries exactly ONE delimiter button with v4's
`getDelimiterTooltip` string — no synthesized "Nar", which the composer shows
for the same template. It then presses the delimiter on a selected line, reads
the document's bytes through the toolbar's own source toggle, and presses a
markdown button against the raw textarea. The chat is put back on no template
and the template deleted.

`workspace-document-standalone-flow` gains the mirror: the chat-less pane's
toolbar is present with the markdown inventory and both pickers, carries NO
delimiter buttons, and its Bold reaches the saved bytes. It deletes the document
it created.

Row 14b of `m6-screen-parity.md` is struck, and the `GAP (named, P4.9L)`
paragraph in `document-pane.ts` was retired when the mount landed. One further
divergence the survey turned up is recorded, not fixed: v4's source mode shows
the whole file and hides the frontmatter table, where v5 shows the body and
keeps the table — the saved bytes are unaffected, but a transform at offset 0
lands in a different place.

#### 2026-08-20 — feat(salon): thread the chat's roleplay delimiters into the document pane

_Versions: SPA 0.5.524._

`salon-conversation.ts` passes the template delimiters it already fetches into
`qt-document-pane`, so a Salon-hosted document shows the template's delimiter
buttons — v4's `SalonView.tsx:1577/1630/1847` passing `chat?.roleplayTemplateId`
down through `DocumentPaneBinding`, in v5's resolved-delimiters spelling (the
toolbar takes the resolved entries rather than fetching the row a second time —
the P4.9L mechanism divergence).

`standalone-document-view.ts` deliberately passes nothing, matching v4's
`StandaloneDocumentView.tsx:381`, which mounts `DocumentPane` with no
`roleplayTemplateId`: the chat-less pane's toolbar shows the markdown buttons
and no delimiter rail. The omission is now named at the mount site.

#### 2026-08-20 — feat(documents): the Document-Mode pane wears v4's formatting toolbar

_Versions: SPA 0.5.523._

The pane (`documents/document-pane.ts`) mounts `qt-formatting-toolbar` in a
`.qt-doc-toolbar` row above the editor, where v4's `DocumentPane` mounts its own
`FormattingToolbar` (`DocumentPane.tsx:323-355, :686-693`): the markdown
buttons, the indent controls, the code-block toggle, both character pickers, the
source toggle, and the active roleplay template's delimiter buttons. v4's
`DocToolbar` passes no `narrationDelimiters`, so this toolbar shows no
synthesized "Nar" button — the binding is deliberately absent, and the omission
is pinned against the two recorded v4 vectors.

Wiring: format buttons through `formatCommand`, delimiters through
`applyDelimiterCommand`, both falling to their source-mode transforms when the
raw textarea is showing; pickers insert into the editor, or at the textarea
caret in source mode. Source-mode transforms run through the pane's own
`onEditorInput` seam, so a markdown file's frontmatter block is still recombined
onto the saved bytes.

The toolbar's source toggle and the pane's header button now share one
`showSource` signal through one `toggleSourceMode()` handler, which flushes on
the way out as v4's does (`DocumentPane.tsx:487-494`; v4's `onFlushSave` is its
`onBlur` at both call sites). The header control itself has no v4 counterpart —
it was added when this pane had no toolbar — and is recorded as a divergence.

`document-pane.toolbar.spec.ts` (12 cases) pins the mount shape, the no-Nar
shape against the recorded corpus, the shared signal from both controls, the
flush direction, and both routing branches. Six mutations red.
#### 2026-08-20 — docs(porting): the P4.51 lane gate — both riders discharged, three follow-ups recorded

_Docs-only change._

The lane's gate numbers (fmt, clippy both feature sets, the harness self-tests,
the full workspace suite at 440 / 2,236 / 0, the spelling guard, the driver
self-test, and both carina families end-to-end through the driver from the
worktree), the drift check at `c8a3cf77`, and the three things the lane
deliberately left open: the fifth `.ts` header whose family really does restore
from it (`brahma_console_routes_equivalence`), the driver's empty-stage vacuous
`OK`, and the `normalize()` hardening that would make the header class
unforgeable.

#### 2026-08-20 — fix(harness): the sweep driver refuses an unknown family name loudly, before any stage runs (P4.51 unit 2)

_No crate versions bumped._

`recipe_sweep.py` used to leave an unrecognized family name with a bare
`unknown family: x` on stderr and exit 1 — the same code an ordinary recipe
failure carries, so a typo in a gate script could not be told apart from a real
red. Inside `--run-all` it was worse: the death came AFTER the family banner had
printed, so the results artifact carried no record for the name it died on, and
an unknown `--exclude` name was silently ignored, running the family you meant
to skip.

An unknown family is now an operator error in the driver's own refusal class:
every family name — `--show`, `--run`, and both of `--run-all`'s
`--families`/`--exclude` lists — is validated BEFORE any stage executes, and an
unrecognized one exits 2 naming the family, the closest known spellings
(`difflib`), and the checkout that was scanned (a wrong `--v5w` being the other
way to get here). `--self-test` grew four end-to-end arms that drive the driver
as a subprocess plus two unit arms; both a swallowed exit code and a removed
validation redden them.

#### 2026-08-20 — fix(harness): the two carina recipe headers stop clobbering the sweep driver's worktree injection (P4.51 unit 1)

_Versions: harness 0.0.510._

`carina_query_tier3_equivalence` and `carina_memory_extraction_tier3_equivalence`
declared their v5 checkout as `W=${V5W:-$HOME/source/quilltap-v5}`. The sweep
driver injects the checkout it was asked to test by PREPENDING `W="<path>"` to
the script, so the header's own assignment ran second and overwrote it — with
`$HOME/source/quilltap-v5`, i.e. main — every time the driver ran either family
from a worktree. The run still went green, against main's case file and main's
fixture builder: the failure mode is invisible. Both headers now use the
documented immune spelling (`V5W=${V5W:-…}`, where the injected `V5W` wins the
`:-` default) and reference `$V5W`; nothing else in either header moved.

Proven by running both families end-to-end through the driver from the lane
worktree (fresh fixtures, fresh oracles, both diffs green), and by a
both-directions provenance probe: a marker appended to the worktree's copy of
`carina-query-tier3.test.ts` is ABSENT from the staged `/tmp` mirror under the
old spelling and PRESENT under the new one.

#### 2026-08-20 — docs(porting): plan the c8a3cf77 round — P4.D95 ∥ P4.9L2 ∥ P4.51

_Docs-only change._

Drift-checked v4 at planning: two commits past the `9125f492` baseline —
`870a57fa` (per-turn conversation summaries with embedded vector reuse, a
behavior change on the ported memory/context spine; no schema change) and the
version-only `c8a3cf77` (NO-PORT). Three work orders written, all disjoint:
P4.D95 (the whole drift — the new `memoryRecall.perTurnConversationSummaries`
setting end-to-end, the `captureQueryEmbedding` hook, `precomputedEmbedding`
on the vault summary search, the proactive vector thread, and the
build-context per-turn cadence with its dedup/stand-down rules; six harness
families named), P4.9L2 (the DocumentPane formatting toolbar — the m6 §4
row-14b named gap, SPA-only, two live beats), and P4.51 (the two `W=`
self-clobbering carina recipe headers + the sweep driver's
exit-0-on-unknown-family wart). Round-plan paragraph added to phase-4.md; the
oracle baseline moves to `c8a3cf77` at unification.

#### 2026-08-19 — port(unify): P4.50 lands — the DbError::Key catch-all split (finding #96 FIXED)

_Versions: core 0.0.590, harness 0.0.509, host 0.0.74, web 0.0.77._

The solo stacked lane unified onto main; the oracle baseline stays `9125f492`.
`DbError::Internal(String)` (bare-message Display) replaces the catch-all use
of `DbError::Key` at 243 of its 246 construction sites — the two genuine
key-derivation wraps and the Display arm keep the prefix, held there by an
executable census guard (`db_error_key_guard`). No observable byte moved: no
`From<DbError>` shim matched `Key` explicitly, so the new variant inherits
every mapping, and `db_error_response` still answers `ErrorKind::Internal`.
The `system_restore_state` leaked-prefix mask is retired, so restore warnings
now byte-compare against v4's whole sentences. The §3 review audited the
migration mechanically (every hunk a pure variant rename; the string-literal
multiset moved by exactly one literal, the retired prefix-strip helper whose
rendered bytes are identical) and found no blocking issues.

Gate: fmt/clippy clean both feature sets; release build; 440 test binaries /
2,236 / 0 with the restore oracle regenerated fresh at the pin; both moved
families by name zero SKIP; SPA 331 files / 4,915 / 0; full Playwright
229/229 zero skips. Deferred loud: the three per-domain taxonomy candidates
named not built; the live combined.log look on a real failed turn joins the
dogfood queue.

#### 2026-08-19 — fix(core): a failed provider call stops claiming key derivation (P4.50, finding #96)

_Versions: core 0.0.590, harness 0.0.509, host 0.0.74, web 0.0.77._

`DbError::Key`'s `Display` prepends `"key derivation failed: "`, which is a
claim about the cause of the failure. The variant was also the crate's
general-purpose message carrier: of 246 construction sites workspace-wide,
exactly two derived a key. Every other one printed a cipher-flavoured lie in
front of its real sentence — including the one an operator meets after a
failed turn, which read `key derivation failed: primary stream failed:
HTTP 500 …` in `combined.log`.

Adds `DbError::Internal(String)`, whose `Display` is the bare message (the
shape `StoreUnavailable` already uses), and migrates all 243 non-key
construction sites onto it. The two genuine sites — `runtime::Db::open` and
`Writer::open_writable`, both wrapping `dbkey::pepper_b64_to_key_hex` — keep
`Key`, so the prefix keeps its diagnostic value instead of becoming noise.

Nothing observable moves on any v4-pinned surface: no `impl From<DbError>`
shim matched `Key` explicitly, so `Internal` inherits every catch-all arm,
and `db_error_response` maps it to `ErrorKind::Internal` exactly as before.
The one place the prefix reached a v4-comparable string was the restore
warning — `system_restore_state` carried a `LEAKED_PREFIX` strip so the rest
of the sentence could be compared. That strip is retired: those warnings now
byte-compare against v4's whole sentence, which is strictly stronger.

A new harness guard (`db_error_key_guard`) walks `crates/**/*.rs` and holds
every `DbError::Key(` occurrence against a census, so the catch-all cannot
silently regrow; the allow-list is the census. Two unit pins cover both
variants' `Display` bytes and two more cover the api-body surface.

#### 2026-08-19 — port(unify): the 9125f492 drift round lands — bugs 81/82 + the Lantern uncensored target (P4.D93 ∥ P4.D94)

_Versions: core 0.0.589, harness 0.0.508, host 0.0.73, SPA 0.5.522._

Both drift lanes unified onto main; the oracle baseline moves to `9125f492`
and the drift debt is cleared. P4.D93 absorbs v4's fix for the two bugs this
port filed from its own 2026-08-19 dogfood walk: a provider may now accept an
API key without requiring one (`acceptsApiKey` on the manifest substrate, the
predicate pair with one fallback home, the `resolve_connection_profile_api_key`
gate+lookup composite at both Brahma sites with a dangling id refusing loudly
even where the key is optional, and the settings SPA offering the
OpenAI-Compatible key unstarred and optional), and local endpoints fold their
leading system-message run at request-build time (Ollama + OAC builders only;
DeepSeek's three blocks survive on the wire as the recorded regression guard;
request-envelopes corpus 257 → 263 rows, all pre-existing rows byte-identical).
The spine measurement answered that v5 never had bug 81's server-spine half —
the host key scan is capability-blind — and that reading is pinned. P4.D94
absorbs the Lantern change: the story-background crafter selects candid vs
concealment intimacy guidance per call (the prompt split into seven generated
constants with the concealed path proven byte-identical at 5114 UTF-16 units),
the flag threads through the empty-response retry unchanged, and a post-hoc
moderation reroute re-crafts the prompt candidly for its target through a new
seam on the shared reroute machinery (the avatar handler passes the no-op; its
family regenerated green as the guard), with five new dangerous-chat fixture
cases and seven red mutation proofs across the lane.

Gate: fmt/clippy clean both feature sets; release build; 439 test binaries /
2,231 / 0 with the round's env block over oracles regenerated fresh at the
pin; the eight moved families re-run by name, zero SKIP; SPA 331 files /
4,915 / 0; full Playwright 229/229 zero skips. The §3 unification review read
the whole combined diff and found no blocking issues; the round record in
`status-log.md` carries the details, including the unifier's own
caught-and-amended conflict-marker incident.

#### 2026-08-19 — docs(porting): the P4.D93 lane gate record

_No crate versions bumped._

Gate numbers for the bug-81/82 lane: 439 test binaries / 2,227 passed / 0
failed with the lane's env block, clippy clean in both feature configurations,
the six moved differential families re-run by name over oracles regenerated
fresh at `9125f492` with zero skips, SPA 331 files / 4,915 tests, and the full
Playwright suite 229 passed / 0 failed / 0 skipped with the bug-81 walk live.

#### 2026-08-19 — test(e2e): walk the optional OpenAI-Compatible key, and repair two self-clobbering recipe headers

_Versions: harness 0.0.506, SPA 0.5.522._

The bug-81 live walk joins the existing settings wizard beat rather than
standing alone: it needs exactly what that beat has just produced, a saved
OpenAI-Compatible profile to attach a key to. Open Add API Key — the provider
list now offers OpenAI-Compatible and still omits Ollama — create a key of that
provider, edit the wizard's profile, find the key field present with an
unstarred label and the `None — the endpoint needs no key` placeholder, attach
the key, save, and re-open to see it held.

Running the Brahma one-shot family through `recipe_sweep.py --run` went red for
a reason unrelated to the port: both Brahma headers opened with
`W=${V5W:-$HOME/source/quilltap-v5}`, and the driver injects the worktree path
as `W` before that line, so with `V5W` unset the header clobbered the injection
and rebuilt the fixture from main's spec file. Converted to the documented
`V5W` spelling, where the injected value survives the `:-` default. Two more
families carry the same broken spelling and are recorded for a maintenance pass.

#### 2026-08-19 — fix(settings): the OpenAI-Compatible key field, offered and optional (v4 bug 81, SPA)

_Versions: SPA 0.5.521._

The client half of v4 `9125f492`. A new `api-key-support.ts` holds the two
predicates as a pure module with no other imports, the v5 twin of v4's
`lib/llm/api-key-support.ts`, so the Add-New-API-Key filter, the profile form's
key field, and the form's outbound gate all read the same question through the
same function rather than each testing `requiresApiKey` by hand.

The Add-New-API-Key provider list is filtered on whether a provider *may* hold a
key; OpenAI-Compatible now appears in it and Ollama still does not. The profile
modal shows the key field whenever the provider takes one, starred only when it
demands one, with the placeholder option reading `None — the endpoint needs no
key` in the optional case. `outboundApiKeyId` — the one gate every outbound site
reads — now refuses on "accepts no key" rather than "requires none", which is
what actually let the attached key leave the form.

The Fetch Models gate and the Connect guard stay on `requiresApiKey`
deliberately, as v4's do: a provider that merely accepts a key must not be
blocked for lacking one. The wizard and the embedding-profile modal are
untouched — their question is requires-shaped and v4 left them alone.

Seven specs mirror v4's added cases: its one api-key-modal case and its five
`profile-modal-optional-api-key` cases, plus the starred-label counterpart.

#### 2026-08-19 — fix(providers): fold the leading system run for local endpoints (v4 bug 82)

_Versions: core 0.0.587._

Ports v4 `9125f492`'s `collapseLeadingSystemMessages`. The context builder
deliberately emits the head of a turn as up to three consecutive `system`
messages so a cache breakpoint on the first survives churn in the others. A
hosted provider accepts that; a local runtime applies the model's own chat
template, and the Qwen family — plus several Llama- and Gemma-derived templates
— raises on any system message after index 0. The opening greeting sends one
block and worked; every turn after it was refused with a 500.

The repair is at request-build time, leaving the context assembly and its
caching design untouched. The fold takes the maximal leading run of `system`
messages, drops empty contents before joining the rest with a blank line, keeps
the first block's other keys, and hands back an array of fewer than two leading
system messages unchanged and un-reallocated.

v5 has no provider subclassing, so v4's `acceptsRepeatedSystemMessages` property
has no counterpart: v4's one override is on the local-endpoint OpenAI-Compatible
plugin, which is exactly v5's `OpenAiCompatible` kind, and the hosted subclasses
have their own builder functions here. Both v5 folds are therefore
unconditional, applied at the two sites v4 folds at, and every hosted builder is
byte-identical on the wire because it never calls the fold at all.

The request-envelope corpus gained six rows recorded from v4's real plugins: a
three-leading-system turn for Ollama and for OpenAI-Compatible on both the
streaming and non-streaming paths, and the same turn for DeepSeek — the
regression guard, with all three system messages still on the wire. The 257
pre-existing rows are byte-identical. Request prefix hashes were re-checked at
the pin rather than assumed unaffected, and are unchanged.

#### 2026-08-19 — fix(providers): forward the key a profile attached, and refuse a dangling one (v4 bug 81, server)

_Versions: core 0.0.586, host 0.0.73._

Ports v4's `resolveConnectionProfileApiKey`. Four v4 paths gated the key
*lookup* on `requiresApiKey`, so an OpenAI-Compatible profile's attached key
would never have reached the wire even once the form could hold it. The v5 twin
lives in `services::api_key_service` beside the two capability predicates, which
moved there from their two duplicate homes (`api::settings` and the Brahma
console) — one v4 function had become three copies.

The order of its three gates is load-bearing: a provider that accepts no key is
never looked up, so a stale row cannot fail its turn; a missing `apiKeyId`
refuses only where a key is required; and a present `apiKeyId` is always
followed, so a dangling one refuses even where the key is optional — the user
attached it on purpose, and going out unauthenticated instead is the
silent-wrong-answer kind of failure.

Both Brahma sites route through it, keeping their byte-different sentences (the
one-shot service's are lower-case, the orchestrator's are not — a pre-existing
v4 asymmetry). The one-shot tier-3 corpus gained three arms recorded from v4's
real code: a dangling key on an accepting provider refuses, an accepting
provider with no key proceeds, and a keyless provider ignores a dangling stale
id. The orchestrator's are composition-level tests over its committed fixture.

The chat-message spine needed no port: v5 resolves keys host-side through a
provider scan with no capability gate on it, so a stored OAC key has always been
forwarded, and the manifest's `auth` scheme is what keeps a keyless endpoint
bare. That reading is now pinned and recorded on the seam.

#### 2026-08-19 — feat(providers): a provider may accept an API key without requiring one (v4 bug 81, substrate)

_Versions: core 0.0.585, SPA 0.5.520._

`ProviderConfigRequirements` gains an optional `acceptsApiKey`, ported from v4
`9125f492`. `requiresApiKey` was answering two questions — "must this provider
hold a key?" and "may it?" — which are the same question for a wholly hosted or
wholly local provider and genuinely different for OpenAI-Compatible, whose one
plugin serves an unauthenticated llama.cpp on localhost and a hosted endpoint
behind a bearer token.

`ConfigRequirements::accepts_api_key` is the single home for the fallback rule:
omitted means "the same answer as `requiresApiKey`", so the eight manifests that
do not declare it keep exactly their present behavior. The manifest generator
extracts the field from the plugin config only when the plugin declares it,
mirroring v4's own `manifest.json`; regenerating all nine manifests changes
exactly one line, in `openai_compatible.json`.

`provider_list()` emits `acceptsApiKey` into `configRequirements` immediately
after `requiresApiKey` and only when the manifest carries it — v4's route passes
`plugin.config` through whole, so the key is present exactly where the plugin
declares it. The providers-listing oracle now spreads that whole config object
instead of hand-picking six fields: the hand-picked comparand was blind to any
config key v4 adds, which is exactly how `acceptsApiKey` would have passed green.
#### 2026-08-19 — port(lantern): a moderation reroute re-crafts the story prompt candidly

_Versions: core 0.0.586, harness 0.0.507._

v4 `decd8ef9`, second half. When an image provider post-hoc rejects a generated
image for content moderation, the Concierge reroutes to the configured
uncensored image profile — and was resending the prompt verbatim, cinematic
concealment and all, to a provider that never asked for it. It now re-crafts
candidly for the reroute target first, re-running the step-9b
character-enumeration pass over the replacement, and is best-effort: any failure
keeps the prompt already in hand so the reroute still produces an image.

v4 does this inline in the story handler's catch. v5 shares the reroute
machinery between the story and avatar handlers, so it lands as a seam: a
`RerouteRecraft` trait on `image_job_common`, invoked with the RESOLVED target's
provider once a reroute exists and before the profile/orientation resolution —
exactly where v4's block sits. The story handler passes `CandidRecraft`, guarded
on `!uncensored_image_target` (an already-candid prompt is left alone), crafting
through v4's `uncensored ?? cheap` selection into a context carrying the
target's provider and `uncensored_image_target: true`, re-running the
enumeration pass over the hoisted non-participant list, and logging v4's info
and warn sentences with its fields. The avatar handler passes
`NoRerouteRecraft`: v4's avatar path is unchanged. All three downstream
consumers moved onto the re-crafted base prompt — the reroute prompt-hint concat
and both `llm_logs` request projections.

v5's crafter returns a result rather than throwing, so v4's two best-effort arms
(a soft empty result and a thrown error) collapse into one `None` return.

Three tier-3 cases cover it, over an image profile whose `blocked-model` throws
a moderation error: the re-craft (two craft calls, flags false then true, the
reroute prompt carrying the candid text AND the re-run enumeration), the
re-craft failing (the reroute reuses the concealed prompt and still produces an
image), and an already-candid prompt (ONE craft call, no re-craft). Four
mutations proven red: skipping the enumeration re-run, ignoring the re-craft at
the prompt site or in the log projections, and dropping the already-candid
guard. `avatar_job_tier3_equivalence` was regenerated fresh at the pin and re-run
by name — no seam leak.

#### 2026-08-19 — port(lantern): the story-background crafter picks its intimacy guidance per call

_Versions: core 0.0.585, harness 0.0.506._

v4 `decd8ef9`, first half. The story-background crafter's "DEPICTING INTIMATE
OR UNCLOTHED STATES" section translates narrative nudity into cinematic
concealment (drapery, silhouette, foreground occlusion). That exists to clear
image-provider moderation, but it was baked into the system-prompt constant and
applied unconditionally — so a dangerous-marked chat with a Concierge
uncensored image profile got accurate appearance text (the appearance sanitizer
already steps aside for exactly that case) fed into a crafter that then draped a
sheet over it, to clear moderation the target provider does not perform.

The prompt is now assembled per call. `gen-image-scene-prompts.mjs` learned
v4's seven-constant shape (a shared head and tail, two intimacy blocks, two
worked examples, the closing line) and `prompt_text.rs` was regenerated from it;
`build_story_background_prompt(uncensored_image_target)` performs v4's
five-element `"\n\n"` join. The concealed (default) path is byte-identical to
the pre-split constant — checked against the previously generated constant's
exact bytes, and pinned at its old 5114 UTF-16 code units. Four Rust unit tests
mirror v4's five prompt-content cases, assertion substrings verbatim.

`StoryBackgroundPromptContext` gains `uncensored_image_target` (the
`generate_image` crafter and the avatar builder take no flag, as in v4), and the
story-background handler computes `is_dangerous_chat &&
has_uncensored_image_provider` — the same pair the appearance sanitizer reads,
so the two layers agree. The empty-response retry carries the flag unchanged:
swapping in the uncensored TEXT profile does not change the IMAGE target.

The tier-3 corpus grew two cases: a dangerous chat WITH an uncensored image
profile (candid) and a dangerous chat WITHOUT one (still concealed — the `&&`
pin). A chat-settings row is per-user and the corpus has one user, so a case can
carry its own `dangerousContentSettings` bag, which both sides apply to their
fresh copy before the run. The oracle's completion mock now branches on the
candid marker in the system message, so the selected variant reaches the image
key and both `llm_logs` projections. Three mutations proven red: dropping either
conjunct of the signal, and hard-coding the retry's flag to `true`.

#### 2026-08-19 — docs(porting): plan the `9125f492` drift round — orders P4.D93, P4.D94, P4.50

_Docs-only change._

v4 shipped three commits past the `c6ff8051` baseline, two of them fixing the
bugs this port filed from its own 2026-08-19 dogfood walk. The round is
planned as two parallel drift lanes plus one stacked maintenance lane, each
with a fresh survey (both sides, dated, file:line):

- **P4.D93** (`docs/developer/porting/work-orders/p4.d93-oac-api-key-and-system-fold.md`)
  — v4 `9125f492`, bugs 81/82: the `acceptsApiKey` manifest flag and
  predicate pair, the shared connection-profile key resolver at the two
  Brahma sites (the help-chat leg banked to `p4.9i2`; the chat spine's
  divergent key seam measured, not assumed), the leading-system-message
  fold in the Ollama and OpenAI-Compatible request builders only (with a
  DeepSeek no-fold regression row in the request-envelopes corpus), and
  the settings SPA half.
- **P4.D94** (`.../p4.d94-lantern-uncensored-target.md`) — v4 `decd8ef9`:
  the story-background crafter's candid-vs-concealment intimacy selection
  through the prompt-text generator (concealed path pinned byte-identical),
  the handler flag + retry carry, and the moderation-reroute candid
  re-craft as a story-only hook on the shared reroute machinery, with the
  story fixture grown its first dangerous-chat coverage.
- **P4.50** (`.../p4.50-db-error-kind-split.md`) — dogfood finding #96:
  split the `DbError::Key` catch-all (246 construction sites) to a
  bare-message variant with a per-site census and a regrowth guard; runs
  stacked after the drift lanes unify because it touches their files.

Also fixed in passing: `phase-4.md`'s standing regen note was stale at
`979652a9` (the `c6ff8051` unification updated the candidate list but
missed that paragraph), and the phase plan now records the round.

#### 2026-08-19 — build(spa): `npm run build` / `npm test` actually return the shell

_SPA 0.5.519._

`ng build` and `ng test` complete their work, print their summary, and then
never exit, so anything chained behind one silently never starts — the failure
mode `.claude/commands` and the memory notes have been working around for
months. Measured this time rather than worked around: after the completion
line `process.getActiveResourcesInfo()` reports exactly
`["PipeWrap","PipeWrap","ProcessWrap"]` and nothing else — the
`esbuild --service=... --ping` child and its two stdio pipes, still ref'd — and
killing that one child makes `ng build` exit instantly with code 0. It is not
esbuild's bug (its shim unrefs the service child at spawn and re-refs it only
during an in-flight operation); `@angular/build` 21.x defers `result.dispose()`
into a generator `finally` on the non-watch path. Fixed upstream in
`@angular/build` **22.0.4**, which disposes eagerly before yielding, and never
backported — 21.2.21's `src/` tree is byte-identical to 21.2.19's, and taking
22.x means Angular 21 -> 22 plus TypeScript 5.9 -> 6.0. That upgrade is tabled
as its own lane.

New `apps/web/tools/ng-run.mjs` (Node, zero dependencies, so it works in the
Docker image too) runs ng as a detached child, streams its output through
untouched, waits for the command's terminal marker — Angular's own
`Application bundle generation complete./failed.`, or vitest's `Duration` line
plus the `Test Files` verdict — then reclaims the shell and exits with the real
status, killing the process group so the esbuild grandchild goes with it. All
four paths measured: successful build -> 0, broken build -> 1, passing tests ->
0, failing tests -> 1, zero stray processes each time. `--watch` and `serve`
pass straight through untouched. `npm run build` and `npm test` now route
through it (`build:raw` / `test:raw` remain as escape hatches), which also
removes a latent hang from the Dockerfile's `RUN npm run build`. `/dogfood`,
`/unify`, `/carryout`, the SPA README and `docs/developer/running.md` all point
at the npm scripts and say plainly never to invoke `ng` directly.

#### 2026-08-19 — docs(dogfood): the /dogfood skill drives the walk itself, pausing only for the human build + unlock

_Docs-only change._

The `/dogfood` command is restructured around a new division of labor: Claude
executes the walk through the Browser pane instead of scripting it for the
human. The research phase now emits a living checklist document under
`docs/developer/porting/dogfood-walks/` with per-step owner
(`CLAUDE`/`HUMAN`), status, gesture, and verification method; the skill then
pauses with verbatim build (`cargo build --release`, `ng build`) and rsync
instructions for the `~/qt-dogfood-friday` refresh, launches `quilltap-web`
itself after confirmation (passphrase unlock stays the human's step), and
drives every `CLAUDE`-owned test — deferring only expensive or
judgment-bound steps to a short human remainder at the end. Per-step result
recording, the diagnose-against-v4 fix loop, and the close-out trail
(findings rows, status-log record, 💸 queue update) carry over from the old
form.

#### 2026-08-19 — docs(porting): the c6ff8051 round record — unified, baseline moved, both lanes closed

_Docs only._

The `c6ff8051` drift round (P4.D91 ∥ P4.D92) unified onto main: v4's
bugs-78/79 convergence absorbed (pin retirements, the five named import
warnings, the preflight refusal, the equipped-slot coercion point) and the
bug-80 project story background landed through the workspace backdrop with
a live e2e beat. The unification review added the JS template-literal
interpolation fix. Gate: clippy both feature sets; release build; 439 test
binaries / 2,208 / 0 with the round's env block; the four affected families
regenerated fresh from a PINNED `c6ff8051` worktree (the v4 checkout moved
to the `release` branch mid-unify — the first, unpinned regen went red and
was discarded); ng test 331 files / 4,908; ng build clean; full Playwright
229/229 zero skips. The oracle baseline moves to `c6ff8051`.

#### 2026-08-19 — fix(import): interpolate a malformed item's name the way v4's template literal does

_Versions: core 0.0.584._

The five named import warnings quote the item's `name` read off the raw
payload, and the arm fires exactly when the item failed to parse — so the
field can be any JSON shape, or absent. v4 renders it through a JS template
literal (`undefined`, `null`, `7`, `a,b`, `[object Object]`); the port read
it as a string and rendered anything else as empty. The five warning arms
now share one helper with the JS interpolation semantics, pinned by a unit
test. The create-path name fallbacks are untouched — they feed written
data, not a sentence. Caught at the unification review.

#### 2026-08-19 — fix(import): name the five silent per-item failures and the preflight refusal (bug 79)

_Versions: core 0.0.583, harness 0.0.505._

Ports v4 `275cd7bc`'s second half. Five import arms — tags, roleplay
templates, and the three profile kinds — logged a per-item failure and
dropped the item without putting anything in the result's `warnings`, so a
user whose items vanished had no way to know. They now push v4's exact
`Failed to import <kind> "<name>": <error>` sentence, from both the
per-item catch and the typed-deserialization refusal that stands in for
v4's create-time validation. The preserve-ids preflight, which aborts the
whole import, likewise answered `success: false` with an empty warnings
array; it now pushes `Import refused before anything was written:
<message>`, so a refused import always says why.

v4's mechanism for the other half of its fix — an AsyncLocalStorage scope
that suspends the repository layer's degrade-to-fallback behavior for the
duration of an import — has no counterpart here and needs none: this port's
reads are typed results that propagate by construction, which P4.48 already
made the import honor at every site.

Two new differential arms carry the change. `execute_named_item_failures`
imports five deliberately malformed items, one per newly-named arm, since
no committed archive can express an item that fails to import — a real
export imports. `execute_preserve_ids_unvalidatable_row_refuses` plants a
destination row that SQLite returns happily and v4's schema rejects, which
is the one plant that reaches v4's strict scope: v4 refuses naming the
validation failure, this port refuses naming the collision it can still
see, and neither writes a row.

The P4.48 `preview_planted_unreadable_tags_table` divergence was
re-measured and its recorded mechanism corrected. It is not `safeQuery`
swallowing a read error: v4's `ensureCollection` rebuilds a dropped table on
first repository access, so the read succeeds against a table v4 has just
created. The oracle now emits whether the table came back, and the arm
asserts the asymmetry on both engines, so the claim is a comparand rather
than a comment.

#### 2026-08-19 — fix(wardrobe): normalize equipped slots on the way out of the column (bug 78)

_Versions: core 0.0.582, harness 0.0.504._

Ports v4 `275cd7bc`'s first half. `chats.equippedOutfit` is unconstrained
JSON and slots were added over time, so a chat row written before the hair
slot carries four keys. `get_equipped_outfit` now maps every character's
entry through the slot normalizer, which makes it the single coercion point
out of the column: the chat-outfit API, the previous-chat outfit carry, and
`set_equipped_outfit`'s read-modify-write all see five slots, and a write
that re-reads the state repairs its siblings' bags on the way through.
`wardrobe_create`'s hand-rolled read of the column moved to the repository
for the same reason.

This port was never exposed to the crash v4 fixed — its slot reader
defaulted a missing key to empty from the start, which is what the retired
`legacy_four_key_equipped` divergence pin recorded. That pin fired on the
first regeneration, as designed, and is now a plain equality: v4 completes
the avatar job and writes the avatar, as this port always did. New unit
tests diff the normalizer against v4's `normalizeEquippedSlots` case for
case, including the malformed-bag salvage and the non-object shapes.
#### 2026-08-18 — fix(prospero): keep v4's legacy per-view background layer on the routed project page

_Versions: SPA 0.5.518._

v4's fix for bug 80 added the backdrop report without removing the older
per-view CSS variable, because the project page still renders outside the
workspace on its own route, where the `::before` layer is the only thing that
paints. Quilltap-v5 serves that route too whenever the workspace-tabs flag is
off, so it carries the variable as well: set when a background resolves,
absent when none does, so the `:not([style*=…])::before` rule keeps the layer
hidden.

Inside the workspace this is inert by design — the layer is suppressed there
in favor of the single arbitrated backdrop, exactly as in v4.

#### 2026-08-18 — test(e2e): walk a project's story background onto the workspace backdrop

_Versions: SPA 0.5.517._

A live browser beat for v4 bug 80's fix. It deep-links straight onto a
project — the case v4's two competing reporters used to lose — sets "Latest
chat background", and checks that the arbitrated backdrop paints the image
resolved from the project's own chat, over the real dispatch and the real
byte route. It then confirms the per-view background layer really is
suppressed inside the workspace, so the pixels came from the reporter and
not from a surviving `::before`. Finally it switches the project to "Theme"
and asserts the backdrop is absent — Quilltap-v5's honest shape, since it has
no subsystem image to fall back to.

The display mode is driven through the UI rather than seeded in SQL. The
first run of this beat found out why: `backgroundDisplayMode` is a
document-store overlay property, so the `projects` column is shadowed by the
project's `properties.json` and a direct table UPDATE is invisible to every
reader. Driving the select exercises the real write path and leaves the
shared test instance in the mode it shipped with.

#### 2026-08-18 — fix(prospero): a project's story background reaches the workspace backdrop (v4 bug 80)

_Versions: SPA 0.5.516._

A project set to "Latest chat background" (or "Project background", or a
static upload) showed nothing. The setting saved and the server returned the
right image — the page simply never painted it.

The tabbed workspace replaced each view's own background layer with one
arbitrated backdrop that views must report to, and suppressed the per-view
`::before` layer inside `.qt-workspace`. The project detail was never
converted, so its background reached the screen by neither route. It now
reports the resolved image to the backdrop registry under its own tab id and
clears the entry when the view goes away, with v4's passive-poll gate
(poll only when the mode is not "Theme"; the fetch itself always runs,
because the server is what resolves "Theme" to no image).

Two deliberate differences from v4's fix, both recorded in the component.
v4 falls back to the theme's Prospero subsystem image for "Theme" mode;
Quilltap-v5 has no subsystem-background machinery at all, so that mode
reports nothing and the backdrop is absent — the standing divergence, whose
newest instance this is. And v4 had to move the projects list's own
subsystem reporter into a shell that unmounts while a detail is shown,
because two live reporters raced over one tab key; the v5 list reports
nothing, so the detail is already the only reporter and the deep-link case
cannot lose.

#### 2026-08-18 — fix(prospero): read a project's resolved story background (bug 80, part 1)

_Versions: SPA 0.5.515._

`ProjectBackgroundDto` described a body the server does not send. The core
verb `project_background_get` answers v4 `handleGetBackground`'s bare
`{backgroundUrl, displayMode, sourceChatId?}`, and `backgroundUrl` is
already the id-keyed byte route (`/api/v1/files/{id}`) — the client type
declared `{url, sourceChatId}` instead, so `fetchProjectBackground` read a
key that is never present and resolved every project's background to null.
Nothing consumed it yet, which is why it went unnoticed.

The DTO now matches the wire, the resolver maps all three fields, and
`projectKeys` gains a `background(id)` sibling of the detail key (spelled in
this file's own `['projects', <kind>, id]` idiom, so the `projectKeys.all`
prefix still covers it). This is the read half of bug 80's port; the
reporter that puts the value on screen follows.

#### 2026-08-19 — docs(changelog): restructure into per-commit headers, split by month

_Docs-only change._

Restructured this changelog. The single flat "Recent Changes" body — 1,479
paragraphs, ~19,400 lines, recording nearly every commit since Phase 1 with
no headers at all — is now one H4 section per originating commit, derived
mechanically from `git blame`: each header carries the short commit hash,
the commit date, and the commit subject, and the line beneath it records
the crate versions that commit bumped (or notes a docs-only change). The
file is split by month, with June and July 2026 archived under
`docs/changelog/` (July in two halves; it alone held 895 entries). No
paragraph text was altered: an order-insensitive line diff against the old
file came back identical. Going forward, new entries use the same header
shape but without the hash — a commit's hash doesn't exist when its entry
is written, and amending would invalidate it — per the amended
`.claude/commands/commit.md` §7 and `unify.md` §6.

#### `6334b193` — 2026-08-18 — docs(readme): a front page that says what this repository is

_Docs-only change._

Planned the `c6ff8051` drift catch-up round and wrote its two work orders.
v4 moved three commits past the `979652a9` baseline: a docs-only commit
filing bugs 78/79 (the port's own findings), the fix for both (v4 converging
onto behavior v5 already pinned — the equipped-slot normalization and strict
import reads, plus new named import warnings v5 must adopt), and the bug-80
fix (a project's story background reaching the workspace backdrop — a
display half v5 never ported). P4.D91 (`work-orders/
p4.d91-import-wardrobe-convergence-server.md`) owns the crates side:
retiring the fired convergence pins, the five import warning arms plus the
preflight warning byte-for-byte, and the healed-reader sweep. P4.D92
(`work-orders/p4.d92-project-backdrop-spa.md`) owns the SPA side: the
project background query, the detail-tab backdrop reporter in v4's fixed
one-reporter shape, and a live e2e beat, with the theme-mode subsystem
fallback as a measured tier-2. The bugfix branch gained only the 4.8.4
version marker (no port).

Gave the README a real front page. It had been a short banner carrying v4's
badges — v4's GitHub release, v4's Docker Hub tag, v4's npm package — none of
which this repository produces. Those three are replaced by a port-status
badge, a stack badge, and one badge that names v4 as the version you can
actually install, and the page now says in its first paragraph what it is:
the unfinished native port, with the Quilltap you can run today living in
quilltap-server.

The rest states the terms. v5 is feature-identical and API-identical to v4 at
release, an existing instance directory opens as it stands with no export or
conversion step, and this repository becomes the project when v4 makes its
last release. Two exceptions are called out rather than left to be discovered
later: v4's plugins will not run in v5 and no compatibility shim is planned,
because a Rust binary has no Node process to load an npm package into — a
replacement extension system is being designed and it will not be JavaScript
— and the v3/v4 virtual-machine appliance is retired, with a locked-down
Docker deployment as the sandbox and the reasoning given in full. The page
also explains how each ported unit is checked against v4's running code
rather than against a description of it, names the four ways in at first
release (macOS, Windows, Linux, Docker) and mobile as unpromised but
deliberately unblocked, credits the libraries the new stack is built on, and
points readers at quilltap.ai, the Folio, and the porting docs. It answers
"should you use this today" with "no."

#### `07b87e84` — 2026-08-18 — docs(porting): rule the product version scheme and order PB1

_Docs-only change._

Settled how v5 will be versioned and scheduled the work. The first real
release is 5.0.0; until then the product version is the semver prerelease
5.0.0-dev.N, one canonical string in the workspace manifest with every
platform form derived from it. Today no such version exists: the About
badge says so outright, a single build answers with four different numbers
depending on whether you reach it through the server, the desktop shell or
the CLI, and this changelog has no version anchors. The change is ordered
as PB1, the first pre-beta work order, to run when parity work is winding
down and before any build reaches an outside tester. Nothing ships yet;
signing, publishing, the updater, multi-arch Docker and cross-platform CI
stay deferred.

Removed a stale version badge from the README. It showed a v4 version
number and linked to a root package.json this repo does not have.

#### `674ee84a` — 2026-08-18 — docs(porting): the 979652a9 round record — unified, baseline moved, all five orders closed

_Docs-only change._

Unified the 979652a9 drift round: five parallel lanes, all closed. The
wardrobe gains its fifth slot — hair, holding a hairdo rather than hair —
threaded through every tool, prompt, avatar branch, import, export, and
dialog, with an empty hair slot deliberately saying nothing anywhere (empty
means unstyled, never bald). Importing a .qtap with composite outfits now
remaps their component references, so imported outfits arrive whole instead
of hollow. An API key no longer follows a connection profile onto another
provider, and an already-poisoned profile heals on its next ordinary save.
The image-generation notice above the composer now owns its own lifetime: it
appears when generation starts, reports the outcome, dismisses itself after
six seconds, and carries a close button — no route out of a turn can strand
it. Workspace tabs refresh their data when you navigate back to them, with
live surfaces and unsaved editors deliberately left alone. And the server
now writes combined.log and error.log into the instance's logs directory
with rotation and a startup sweep for iCloud and Finder conflict files, so
warnings survive the terminal.

#### `e253c4db` — 2026-08-18 — fix(import): remap composite componentItemIds on .qtap import (P4.D87, v4 Bug 75 / 40d507cc)

_Versions: core 0.0.581, harness 0.0.503._

Fixed .qtap character import leaving composite outfits hollow (v4 Bug 75
ported; v5 had the same defect). Import re-mints wardrobe item ids, but
composite componentItemIds kept referencing the export's original ids.
The importer now pre-assigns every new id, creates leaf items before the
composites that bundle them, remaps the references, and drops a dangling
reference with a warning instead of leaving it pointing at nothing.
Proven by a new committed composite-chain .qtap fixture whose items are
read back through both sides' real vault readers and compared by
relationship.

#### `b76c4729` — 2026-08-18 — feat(wardrobe): port the hair slot's server half (P4.D87, v4 4423ad10)

_Versions: core 0.0.580, harness 0.0.502._

Ported the server half of v4's hair wardrobe slot (P4.D87, v4 4423ad10).
Wardrobe slots now number five — top, bottom, footwear, accessories, hair
— with hair holding a hairdo, not hair: an empty hair slot means unstyled,
never bald, so no report, prompt, or tool result ever mentions hair when
the slot is blank (two per-slot dumps that printed a literal
"hair: (empty)" line are fixed at all three v5 sites). Nudity semantics
compute over clothing slots only; a hair-only pick still counts as "chose
nothing to wear"; avatars carry the hairdo on both the dressed and
bare-top branches (the bare-top guard keyed on accessories alone and
would have silently dropped it); undressing keeps the hairstyle in scene
state. The ten duplicated slot lists collapse onto one registry
(WARDROBE_SLOT_META) with per-slot label/clothing/report-when-empty
metadata; three deliberately frozen legacy lists stay at four. The outfit
hash gains the hair key unconditionally (each chat re-derives its cached
clothing summary once). No schema or migration change: absent hair keys
read as empty. Thirty-one differential families regenerated fresh from a
pinned 979652a9 v4 worktree, all green, plus a new outfit_hash_equivalence
tier-1 family. Found and pinned upstream: v4 itself crashes avatar
generation on any pre-hair chat row (raw four-key equipped state reaches
the five-slot resolver with no default) — v5 tolerates the legacy shape
by design, pinned both directions with a convergence tripwire.
Quilltap now writes its logs to files again, the way the Node version
did: `logs/combined.log` holds every record and `logs/error.log` holds the
errors, both as one JSON object per line, rotating into numbered backups
once a file passes its size limit. On startup the log directory is also
swept of the debris a synced folder collects — iCloud conflict copies like
`combined 2.log`, Finder duplicates, and leftovers from an older rotation
naming — while terminal transcripts and the launcher's own stdout/stderr
files are left strictly alone. Until now the native build wrote only to the
terminal it was launched from, so a warning worth acting on was gone the
moment the window scrolled. Found by dogfooding (finding #93).

#### `9c8414a5` — 2026-08-18 — feat(logging): wire the file transport into the log surface, defaulting to both

_Versions: web 0.0.75, tauri 0.0.7._

Both destinations are on by default now. `LOG_OUTPUT` still chooses
between `console`, `file` and `both`, and `LOG_FILE_PATH`,
`LOG_FILE_MAX_SIZE` and `LOG_FILE_MAX_FILES` work as they always did; a
value that cannot be read falls back to the default and says so in the log
rather than refusing to start. A setup that asks for files but has nowhere
to put them keeps writing to the terminal instead of going quiet.

#### `718e747d` — 2026-08-18 — docs(logging): rule the CLI's twin by measurement — reader, not writer

_Versions: cli 0.0.10._

The `quilltap` command-line tool still logs only to the terminal, as it
always has — it reads the log files rather than writing them, and a
short-lived command must not rotate a log file out from under a running
server.
Added a browser walk for the hair wardrobe slot — create a hairdo, see it
badged, wear it in the Hair slot, and find it again after reopening the
dialog. It stays inert until the server accepts the fifth slot.

#### `265277c6` — 2026-08-18 — test(wardrobe): parity specs for the slot registry, grouping, and the preview

_Versions: SPA 0.5.507._

Added tests covering the new hair wardrobe slot: the slot registry's rows,
the item editor's grouping, the rose badge on a chosen hairdo, and an
outfit payload saved before hair existed still rendering.

#### `918c3f24` — 2026-08-18 — fix(profiles): an api key no longer follows a profile onto another provider (v4 bug 76)

_Versions: SPA 0.5.509._

Added the "hair" wardrobe slot to the app. It holds a hairdo — braids, an
updo, marcel waves, a wig — not hair itself; colour, length, and texture
stay in the physical description. Hair appears wherever the other four
slots do: the wardrobe dialog's slot filters and equipped rows, the item
editor's types and component groups, the project wardrobe, the
"same as last conversation" preview, and the Green Room's outfit preview,
all wearing a rose badge. The app now reads one slot registry instead of
five copies of the slot list, and an outfit saved before the hair slot
existed still opens, with hair empty.
Fixed an API key following a connection profile onto a provider that
cannot use it. Switching a profile from, say, Anthropic to Ollama left the
stored key in place while the API Key control disappeared, and the save
was then refused with "API key provider does not match profile provider" —
naming a field the dialog no longer showed, with no gesture anywhere that
cleared it. Switching between two hosted providers was worse still: the
control read blank while the wire carried the old provider's key. A
profile now sends only a key its current provider could actually display,
and clears the column otherwise, so a profile already saved that way —
or imported that way — heals on its next ordinary save. Found by
dogfooding (finding #90).

#### `e644a08a` — 2026-08-18 — fix(salon): give the tool-execution notice its own lifetime (v4 bug 77)

_Versions: SPA 0.5.510._

Fixed the "Generating image..." notice above the composer never going
away. It had exactly one teardown, at the end of one of the several routes
a turn can finish by, so any other ending — a tool chain's intermediate
turn, continue mode, either error arm — left it pinned above the composer
for the rest of the session, with no close control to escape it. The
notice now owns its own lifetime: it stays up while the image is still
generating, reports the outcome for six seconds and then dismisses itself,
is dropped if the turn ends without ever producing a result, clears at
once when you stop a turn, and carries a close button of its own.
Two end-to-end walks now cover the returning-to-a-tab refresh: leaving
the chat list and coming back re-reads it, and returning to the
Scriptorium re-lists its stores without the page blinking through its
loading state.

#### `3d41a51f` — 2026-08-18 — feat(screens): views that fetch outside the query cache refresh on return

_Versions: SPA 0.5.513._

The workspace tabs that load their data outside the query cache now
refresh on return too: My Photos, Scenarios, the Scriptorium (list and
store detail), Generate Image's character list, and a character's stats,
conversations and memories. Each refresh happens in place — the page you
came back to stays on screen while fresh data arrives, instead of
blinking through its loading state.

#### `afff539e` — 2026-08-18 — feat(workspace): refresh a tab's cached reads when it is navigated back to

_Versions: SPA 0.5.512._

Returning to a workspace tab now refreshes what it shows. Because tabs
stay mounted, a tab you came back to still displayed whatever it had
loaded when you left it. Each tab kind now declares which cached reads
go stale on re-activation — Home refreshes the dashboard plus chats,
projects and characters; Characters, the chat list, Projects, Files,
Generate Image, Pascal's tools, your profile and Settings each refresh
their own. Live surfaces (a conversation, a terminal, Document Mode,
the Brahma console) and editors holding unsaved work are deliberately
left alone, and the chat sweep never touches a conversation's own data.

#### `8ecf2d73` — 2026-08-18 — feat(workspace): a tab subtree knows whether it is visible

_Versions: SPA 0.5.511._

Workspace tabs now know whether they are the tab you are actually looking
at. Each mounted tab's subtree gets a visibility signal, and a new
`onTabActivated` hook runs a callback on every hidden-to-visible
transition — never on the first activation, never when a tab is hidden,
and never outside the workspace. Nothing uses it yet; the refresh it
enables lands next.

#### `f1471616` — 2026-08-18 — fix(providers): google's model list, on the field name the wire actually sends

_Versions: core 0.0.579._

Fixed Fetch Models returning nothing for Google. Two faults compounded:
the model list was filtered on a field name the Google SDK invents when it
reshapes the response, which the API itself never sends, so every model was
discarded; and the fallback list Quilltap keeps for exactly this case was
missing, so the empty result reached the screen instead of being covered.
Both are restored, including the distinction that Google falls back on an
empty result as well as on an outright failure, where Anthropic falls back
only on failure. Found by dogfooding (finding #91).

#### `ca27521c` — 2026-08-18 — fix(build): the amalgamation path is resolved at build time, not baked in

_No crate versions bumped._

Fixed a release build failing in the main checkout after a lane worktree
was deleted. The SQLite3MC build script resolved its vendor directory
with a compile-time path, and the compiled build script is cached
indefinitely because that crate's version is deliberately pinned — so a
worktree sharing the same target directory could leave its own path
baked in, pointing at a directory that no longer existed. The path is now
read at build time.

#### `cfd202e0` — 2026-08-17 — fix(providers): the models list carries the provider's declared auth headers

_Versions: core 0.0.578._

Fixed Fetch Models returning a fixed list of eleven Claude models for
Anthropic instead of the models your key can actually reach. The models
request was sent without the `anthropic-version` header Anthropic
requires, so the API rejected it and the code fell back to its built-in
list without reporting anything. Providers declare fixed headers like
that in their manifests, and the two hand-built wire calls — the models
list and the generic connection probe — now send them. Found by
dogfooding (finding #89).

#### `1355c23c` — 2026-08-17 — docs(porting): the d123658d round record — baseline moves to d123658d, both lanes closed

_Docs-only change._

Unified the d123658d connection-profile-editor round (P4.D85 server ∥
P4.D86 SPA). The oracle baseline moves to d123658d and the drift debt is
cleared; v4's one newer commit (9c01fa99, sample-prompt content) is
classified no-port. The unification review caught one cross-lane
staleness before merge — the SPA's enriched-tag type documented a
narrowing its sibling lane had closed in the same round — fixed with the
type widened to the full tag row. The gated profile-tag e2e beat was
activated and passed its first live run. Final versions: core 0.0.577,
harness 0.0.501, SPA 0.5.505; host/web/cli/tauri unchanged.

#### `fdd1b8ef` — 2026-08-17 — docs(porting): the P4.D86 lane record — closed, nothing deferred

_Docs-only change._

Closed work order P4.D86, the SPA half of the `d123658d` round. Docs only; no
code changed.

#### `905bfdbf` — 2026-08-17 — feat(spa): the banked attachment-support line, and v4's own verification walk

_Versions: SPA 0.5.502._

The connection-profile editor names what a provider accepts as attachments
again, under the provider dropdown, matching v4.

#### `ea258b9f` — 2026-08-17 — feat(spa): connection-profile tags, in their fixed form (v4 bug 74, client)

_Versions: SPA 0.5.501._

Connection profiles can be tagged. The editor's tag box adds and removes tags as
you go, and the tags now show their names on the profile card instead of drawing
as empty pills. Ports the client half of v4's bug-74 fix, where tagging a profile
had never worked at all.

#### `193c371e` — 2026-08-17 — fix(spa): a hidden base URL no longer follows the profile (v4 bug 73)

_Versions: SPA 0.5.500._

Fixed a base URL following a connection profile onto a provider that neither
shows nor takes one. Selecting Ollama fills in localhost:11434; switching to a
hosted provider hid the field but kept the value, and every connection test,
model fetch and save still sent it, so the profile could not connect with
nothing on screen to explain why and no way to clear it. The value now stays in
the form (switching back restores it) but never reaches the wire, and a save
always sends the field so an already-broken profile heals the next time you save
it. A provider the app has not loaded keeps its stored URL. Ports v4's bug-73
fix.

#### `05716b08` — 2026-08-17 — fix(spa): a cleared numeric provider option keeps its own draft (v4 bug 72)

_Versions: SPA 0.5.499._

Fixed a numeric provider option in the connection-profile editor putting the
schema default straight back when you clear it, so the next digit appended to it
and a wrong value was stored (clear 300, type 5, get 3005). The box now keeps its
own draft while you edit, and an unset option shows its default as a placeholder
rather than a value, so "leave blank for the default" is a state you can see
yourself reach. Ports v4's bug-72 fix.

#### `17bd67ba` — 2026-08-17 — port(profiles): connection-profile tags — the shared flat resolver and the three verbs (v4 Bug 74, d123658d)

_Versions: core 0.0.576, harness 0.0.500._

Connection profiles can be tagged. The three actions v4 grew in its bug-74 fix
— read a profile's tags, add one, remove one — are now verbs, and the two tag
shapes that had drifted apart in v4 each have one owner here: the flat shape the
tag editor reads, shared by the character and connection-profile answers, and
the enveloped shape the list endpoints send. The enveloped one now carries the
whole tag record, as v4's does; it had been narrowed to an id and a name, which
nothing could catch because no profile in the test corpus had ever had a tag.

#### `ac25de4a` — 2026-08-17 — fix(profiles): a cleared field comes back explicitly empty, as v4 sends it

_Versions: core 0.0.577, harness 0.0.501._

Clearing a connection profile's base URL, model class, max context, or API key
now answers with that field explicitly empty, as v4 does, instead of leaving it
out of the reply altogether. A client reading the saved profile straight back
off the response could not tell a cleared field from one the server had
declined to send. Found by giving a test profile a stored base URL — with every
column already empty, the two behaved identically and the difference could not
be seen.

#### `d6f1f4fc` — 2026-08-17 — docs(porting): work orders for the d123658d connection-profile-editor drift round (P4.D85 ∥ P4.D86)

_Docs-only change._

Planned the `d123658d` connection-profile-editor drift round and committed its
two work orders: P4.D85 (server — the profile tag verbs, the GET action gate,
and the `resolveEditorTags` convergence from v4's bug-74 fix) and P4.D86 (SPA —
the bug-72 number-field draft machinery, the bug-73 `outboundBaseUrl`
chokepoint with the always-send save body, and the profile tag surface). v4's
`d123658d` fixes bugs 72 and 73, which this port's own 2026-08-16 dogfood walk
found and filed, plus bug 74 (profile tagging had never worked). The sibling
commit `d81ccc17` is docs-only, no port. Docs only; no code changed.

#### `56196c2c` — 2026-08-16 — docs(dogfood): the 93ed8abf walk's coverage through Part B — three live proofs discharged

_Docs-only change._

Recorded the 93ed8abf round's dogfood coverage through Part B. Three of the
round's four owed live proofs are discharged on real Friday data: Max Tokens
and Top P reaching a local wire along with the keep-alive sentinels, the
per-profile request timeout firing on a cold model, and the provider-options
panel driven on real profiles. OpenAI-compatible tool calling against a local
llama-server is still owed.

#### `3b7cdbcf` — 2026-08-16 — tools(dogfood): a byte-faithful wire tap for local providers

_No crate versions bumped._

Added `harness/tools/wire-tap.py`, a byte-faithful TCP tap for reading what a
local provider actually receives. Neither the LLM Inspector nor the logs can
answer that — the log stores a summary of the request, and the IO layer traces
nothing — so a claim that the profile's parameters reach the wire had no way to
be checked by hand. Point a connection profile's base URL at the tap and every
request body prints while the bytes relay untouched. Dev tooling only; nothing
in the app changed.

#### `c753a62a` — 2026-08-16 — docs(dogfood): findings #87 and #88 — two faithfully ported v4 bugs, filed upstream

_Docs-only change._

Recorded two dogfood findings from the 93ed8abf round's walk, both diagnosed as
faithfully ported v4 bugs and filed upstream rather than fixed here. Clearing a
numeric provider option puts the schema default straight back with the caret
after it, so the next digit appends to it and a wrong value reaches the server.
And a base URL picked up from Ollama or an OpenAI-compatible endpoint survives a
switch to a provider that hides the field, breaking every connection test and
saving onto the profile row. Both were measured by driving v4's own components,
not by reading them. No product code changed.

#### `6024652b` — 2026-08-16 — docs(porting): the 93ed8abf drift-round record — baseline moves to 93ed8abf, all three lanes closed

_Docs-only change._

Unified the 93ed8abf drift catch-up round: the context budget honors the
profile's Max Context end-to-end (single-sourced window resolution, the unified
safe-input-limit formula the builder and validator both read, tool schemas and
spliced system messages measured and reserved before the context is built, and
the tool-change notice now actually reaches the model — a pre-existing deferral
closed), the profile's sampling knobs reach every completion path through one
resolver (five call sites including Carina; Top P gained a seat in the
non-streaming params; absent Max Tokens stopped meaning zero), the
profile-parameters passthrough covers Ollama and OpenAI-compatible endpoints
(sampler options, keep-alive, thinking effort, request timeout; OAC gains tool
calling on both paths), each provider's connection-profile options schema is
served and rendered by a schema-driven panel in the profile editor (replacing
the hardcoded Enable Thinking row), and the tool-use seed hint plus the vision
re-seed landed. The oracle baseline moves to 93ed8abf; the drift debt is
cleared. Gate: 437 test binaries / 2,147 / 0 with the round's env block, 26
families regenerated fresh at the pin with zero skips, clippy both feature
sets, release build, ng test 4,792 / 0, full Playwright 222 / 0 / 0 with the
options round-trip beat live on first activation. Versions: core 0.0.575,
harness 0.0.499, host 0.0.72, SPA 0.5.498.

#### `9e841c4f` — 2026-08-16 — unify(review): the §3 findings — the kwargs array arm, OAC non-streaming tool calls, the client-facing pre-send validation, the reroute budget profile, the provider estimator rate

_Versions: core 0.0.575, harness 0.0.499._

The 93ed8abf unification review's fixes. A profile storing its OpenAI-compatible
chat_template_kwargs as a JSON-array string now reaches the wire as the parsed
array, matching the reference (objects only, before). The OpenAI-compatible
non-streaming path parses tool calls back with the reference's own filter. The
pre-send context validation now tells the client — an unconditional validating
status and a warning status on overage, the user's only signal a payload may be
rejected — instead of logging only. A danger-rerouted turn budgets its context
to the rerouted profile's Max Context (it read the original's), and the
turn-extras reservation uses the provider's own chars-per-token rate (Google is
3.8, not the flat 3.5). Doc placement fixed on the transport policy composers;
the sampling and timeout harness assertions gained their missing symmetric
directions.

#### `75b88d4e` — 2026-08-16 — port(providers): serve each provider's connection-profile options schema — P4.D83 unit 7

_Versions: core 0.0.574, harness 0.0.497._

The providers listing now carries each provider's connection-profile options
schema — the fields, labels, help text and enum choices a profile editor draws
for that provider. It was always null, so the panel had nothing to render;
eight of the nine built-in providers declare one, and the ninth (Google) still
answers null because it declares none.

#### `c044678d` — 2026-08-16 — port(ollama): the profile's request timeout on both call shapes — v4 d89babc4, P4.D83 unit 6

_Versions: core 0.0.573, harness 0.0.496, host 0.0.72._

An Ollama connection profile can set its own Request Timeout. A turn was
bounded by the shared five-minute default with nothing in the UI to change it,
and loading a large model off disk plus reading a long prompt both happen
before the first token — a big model on a busy machine could cross the ceiling
and leave no reply at all. The profile's `request_timeout_seconds` now sets
that budget on both the streaming first-token wait and the whole non-streaming
request. Blank, absent or unparseable falls through to 300 seconds, so nothing
changes for a profile that never touches it, and a caller that has already
decided what the work is worth waiting for still wins.

#### `b32513cd` — 2026-08-16 — port(oac): tools on both paths, the profile allow-list, the template-kwargs fold — v4 93ed8abf, P4.D83 unit 5

_Versions: core 0.0.572, harness 0.0.495._

An OpenAI-compatible endpoint can now call tools, and reads the profile's
settings. It sent no tools on either path and never looked at the parameters
blob, so a local llama-server with function calling could not be used for tool
work and reasoning effort was unreachable. Tools and tool_choice now go on the
body and tool calls are parsed back on both paths, with streamed argument
fragments accumulated by index. The provider forwards its own allow-list —
top_k, min_p, the three penalties, seed, cache_prompt — and folds reasoning
effort into chat_template_kwargs, which is how llama-server reaches a
template's arguments; a flat key parses and is never seen by the template. The
tool-use capability stays off: it seeds the checkbox on a new profile and never
disables it.

#### `e3ca1596` — 2026-08-16 — port(ollama): the profile's sampler options, keep_alive and thinking effort — v4 93ed8abf, P4.D83 unit 4

_Versions: core 0.0.571, harness 0.0.494._

Ollama now sends the sampler settings a connection profile saves. It read two
keys — the context window and the thinking switch — beside a hardcoded options
literal, and dropped every other one in silence, so no local model could run at
its publisher's recommended settings. The options table now carries top_k,
min_p, the three penalties, the seed and the mirostat trio; Keep Model Loaded
rides the top level (sending nothing at all by default, leaving OLLAMA_KEEP_ALIVE
in charge, and sending its two sentinels as numbers because the server refuses
them as duration strings); and Thinking Effort folds into the thinking field as
a level. The three control keys the provider reads for itself never reach the
wire.

#### `31868bb4` — 2026-08-16 — port(providers): one profile-parameter applier with a per-key hook — v4 93ed8abf, P4.D83 unit 3

_Versions: core 0.0.570, harness 0.0.493._

Z.AI no longer sends a reasoning effort to a model that ignores it. A profile
setting Reasoning Effort had it forwarded to every GLM, including the older
models where the field is not honored; it is now sent only to the models that
support it, matching what the rest of the stack already did. The
profile-parameter passthrough itself became one shared mechanism with a
per-key hook, so a provider can reshape a stored value or send it under a
different key without hand-rolling the copy loop.

#### `aa1b28a9` — 2026-08-15 — port(llm): the profile's Max Tokens and Top P reach the model — v4 d89babc4, P4.D83 unit 2

_Versions: core 0.0.569, harness 0.0.492._

A connection profile's Max Tokens and Top P now reach the model. Both were
stored and displayed and never sent: the Salon's main path, the greeting
(including its uncensored retry) and Carina's reference query all read them
under camelCase names the profile editor does not write, so two of the three
sampling knobs came out empty on every turn and the provider fell back to its
own defaults. Regenerate/swipe read the snake_case names but never read Top P
at all, so the original reply and a regeneration of it disagreed. All five
sites now go through the one resolver, and the image-description fallback
sends Top P as well — the non-streaming path had nowhere to carry one. A
profile that names no Max Tokens now leaves the field off the request, where
regenerate and the image description used to send a literal zero.

#### `2dd6e3ef` — 2026-08-15 — port(llm): one reader for a profile's sampling knobs — v4 d89babc4, P4.D83 unit 1

_Versions: core 0.0.568, harness 0.0.491._

A connection profile's sampling knobs now have one reader. `resolve_sampling_params`
maps a profile's free-form `parameters` blob to the three per-request fields —
canonical snake_case (`temperature` / `max_tokens` / `top_p`) first, camelCase
tolerated for a hand-edited or imported blob, absent knobs left unset so nothing
is invented. Strings go through the same number coercion the rest of the port
uses, and a key that is present but unusable falls through to the other
spelling rather than ending the search.

#### `465e3d54` — 2026-08-15 — port(chat): reserve the turn's extras before building context; tell the model its tools changed — v4 f933ba9c, P4.D82 unit 4

_Versions: core 0.0.567, harness 0.0.490._

The turn now reserves room for what it adds after the context is built, and
tells the model when its tool roster changed. A chat whose tool settings the
operator edited set a flag that nothing ever consumed or cleared, so the
model was never told and the flag stayed set forever; that turn now carries
the tool-change notice and clears it. The pre-send payload check measures
the messages plus the tool schemas against the same ceiling the builder
packed to.

#### `f1d50ed6` — 2026-08-15 — port(tokens): count the tool schemas, and build the turn extras in one place — v4 f933ba9c, P4.D82 units 2-3

_Versions: core 0.0.566, harness 0.0.489._

Tool schemas now count toward the context budget. `count_tool_schema_tokens`
measures the serialized slate (plus per-tool framing), and a new
`turn_extras` module builds and measures everything a turn adds after the
context is built — the tool schemas, the agent-mode instructions, and the
tool-change notice — in one place, so the room reserved for them and the
text that fills it cannot drift apart.

#### `8ab6ff1e` — 2026-08-15 — port(context): the context budget honors the profile's Max Context — v4 f933ba9c (bug 70), P4.D82 unit 1

_Versions: core 0.0.565, harness 0.0.488._

The context budget now honors the connection profile's Max Context (v4 bug
70). A profile pointed at a model no lookup table knows — any `hf.co/...`
Ollama tag, any custom OpenAI-compatible endpoint — was budgeted at the
8192-token provider default while the compression trigger, which does read
the profile, worked from the real window; history was trimmed to fit the
small figure on every turn. `resolve_context_window` is now the single
source of the window (the profile wins, the name lookup is the fallback, a
zero or negative setting falls through), and the allocation, the safe-input
limit, `calculate_max_available` and the self-inventory last-turn section
all route through it. The builder and the pre-send validation now share one
ceiling (`safe_input_limit` = window less response reserve less a 10%
estimator margin), which `ContextBudget` carries alongside the margin
itself; the builder used to pack 10% past the line that then warned about
it. The context builder also accepts `reserved_outgoing_tokens` — room held
back for what the caller adds after the context is built — and says so by
name when the fixed payload leaves no room for conversation history at all.
Documented the providers listing's options-schema field on the SPA contract now
that the profile editor consumes it.

#### `0aece250` — 2026-08-15 — test(spa): the provider-options e2e walk, two beats live (P4.D84 unit 4)

_Versions: SPA 0.5.497._

Added an end-to-end walk for the connection-profile editor's provider-driven
surfaces: the tool-use hint and the vision re-seed run live, and the
schema-driven options round trip is written and waiting on the wire half of
this round.

#### `e52d6a4d` — 2026-08-15 — port(spa): the tool-use seed hint and the vision re-seed (P4.D84 unit 3)

_Versions: SPA 0.5.496._

The connection-profile editor now explains itself when a provider does not
advertise tool support: the box still starts off, but a note under it says an
endpoint that really does speak native function calling can be switched on
regardless. Changing the provider on a new profile also re-seeds the vision
checkbox from that provider's attachment support, which it previously left
alone.

#### `b89d9281` — 2026-08-15 — port(spa): cut the profile modal over to the options panel (P4.D84 unit 2)

_Versions: SPA 0.5.495._

Cut the connection-profile editor over to the schema-driven options panel.
The hardcoded Ollama "Enable Thinking" row is gone; Ollama's own schema draws
it along with thinking effort, keep-alive, the request timeout, and the whole
sampling group, and every other provider's options appear the same way.
Clearing a numeric option now removes the key instead of leaving a hole,
OpenRouter's "Use Custom Model ID" switches the model box to free text, and an
old profile's nested OpenRouter data-collection preference is translated to the
flat zero-data-retention flag the panel shows.

#### `ade78dcb` — 2026-08-15 — port(spa): the schema-driven provider-options renderer (P4.D84 unit 1)

_Versions: SPA 0.5.494._

Added the schema-driven provider-options renderer to the connection-profile
editor (`qt-provider-options-panel`), a port of v4's `ProviderOptionsPanel`.
It draws boolean, enum, multi-enum, string, and number fields from the schema
each provider plugin declares, honors `showIf` guards and group headings, and
writes each key into the profile's parameters bag one at a time so keys with
no control still survive a save. Not yet wired into the modal.

#### `5df3938e` — 2026-08-15 — docs(porting): the 93ed8abf drift-round work orders — P4.D82 (bug-70 context budget), P4.D83 (profile-params wire, stacked), P4.D84 (SPA options panel)

_Docs-only change._

Planned the 93ed8abf drift catch-up round and committed its three work
orders: P4.D82 (bug 70 — the context budget honors the profile's Max
Context, with the new turn-extras tool-schema token accounting), P4.D83
(the profile-parameters wire — resolveSamplingParams at all four call
sites, the Ollama options/keep_alive/thinking-effort/request-timeout
table, OPENAI_COMPATIBLE tool calling, and optionsSchema carried onto
the provider manifests; stacked on P4.D82), and P4.D84 (the SPA
schema-driven provider-options panel replacing the hardcoded Enable
Thinking row, plus the tool-use seed hint and the supportsImageUpload
re-seed). Docs only; no crate or SPA source touched.

#### `a6cd94f5` — 2026-08-15 — docs(porting): the aa464abf drift-round record — baseline moves to aa464abf, all five lanes closed

_Docs-only change._

Unified the aa464abf drift catch-up round: the whole Ollama-thinking wire
(inline think-block stream parsing on both channels, the think and num_ctx
request fields with a one-shot retry when a model refuses the think
parameter, the toolUse capability flip), the per-profile multi-character
[Name] prefill column end to end with greeting reasoning persisted onto the
first message, the profileParams consolidation (which also fixed three
pre-existing v5 defects: the Salon primary stream sent no per-model
parameters at all, the Carina answer read temperature from a nonexistent
key, and the profile editor dropped every non-sampling parameters key on
save), the archivedAt chat-GET enrichment with the archived-badge beat
flipped live, the archive rehydrate self-heal for digest rows damaged by
v4's pre-4.9 watcher, and the import preflight's read-error propagation
(a ruled divergence — v4 swallows those errors — with both-directions
tripwires). The oracle baseline moves to aa464abf; v4's bug-70 commit is
the queued next drift. The unification review found no blocking findings;
the gate repaired three harness recipes. Full gate green: 435 test
binaries / 2,125 tests, the 28 affected differential families regenerated
fresh at the pinned baseline, ng test 4,741, and the full Playwright suite
(numbers in the round record). Versions: core 0.0.564, harness 0.0.487,
host 0.0.71, SPA 0.5.493.

#### `e83b133e` — 2026-08-15 — docs(porting): the P4.D78 lane gate and the pinned-oracle re-proof

_No crate versions bumped._

Re-proved every P4.D78 oracle against a v4 worktree pinned at aa464abf after
the v4 checkout went dirty mid-lane with unrelated WIP: all eight (the two new
oracles, the registry and listing oracles, the generated manifests, the ollama
stream recording, and the request-envelope and response-body corpora) are
byte-identical. Docs and a recipe-header correction only.

#### `a62831fd` — 2026-08-15 — feat(model): the Ollama retry-without-think + the toolUse manifest flip (P4.D78 units 5-6)

_No crate versions bumped._

Ollama declares tool use, and the retry-without-think lands (P4.D78 units
5-6, v4 d9c5a1c7): a non-ok Ollama response whose body mentions "think" now
re-sends once with the parameter deleted, on both the streaming and the
non-streaming path, proven by fake-transport quartets and a new tier-3 arm
driving v4's real plugin with fetch mocked below it. The regenerated
manifests flip `OLLAMA.capabilities.toolUse` to true.

Fixed the provider-manifest generator's stale google auth entry: P4.47
corrected the committed manifest to the `x-goog-api-key` header but not the
generator's augmentation table, so the next regen would have silently
reverted it (the P4.39 class of rot).

#### `ae46c8f9` — 2026-08-15 — feat(decoders): route Ollama reasoning through the stream decoder (P4.D78 unit 4)

_No crate versions bumped._

The Ollama stream decoder now routes reasoning (P4.D78 unit 4, v4
d9c5a1c7): native `message.thinking` deltas and inline `<think>` interiors
land on one cumulative `reasoningContent`, reasoning-only chunks carry empty
content, the parser's flush releases its tail as a content chunk before the
terminal one, and the terminal `rawResponse` content is think-free. Seven new
recorded stream vectors; every pre-existing vector byte-identical. The
composer differential's stale ollama byte-chunking exclusion is retired — all
three chunkings now run there too.

#### `0c8cdddd` — 2026-08-15 — feat(model): route both Ollama reasoning channels on the non-streaming parse (P4.D78 unit 3)

_No crate versions bumped._

The non-streaming Ollama parse now routes both reasoning channels (P4.D78
unit 3, v4 d9c5a1c7): `parse_ollama` splits `message.content` through the
think parser and concatenates the native `message.thinking` ahead of the
inline reasoning, attaching `reasoningContent` only when the result is
non-empty. Seven new Ollama cases in the response-body corpus; every
pre-existing row byte-identical.

#### `517b1b3c` — 2026-08-15 — feat(model): Ollama think + options.num_ctx on the wire (P4.D78 unit 2)

_No crate versions bumped._

Ollama requests now carry the thinking switch and the context window
(P4.D78 unit 2, v4 d9c5a1c7): `build_ollama_body` emits a top-level `think`
on every body (false when the profile's `enable_thinking` is off) and
`options.num_ctx` when the profile bag coerces to a finite positive number.
The request-envelope corpus regenerated at the aa464abf pin — 14 new Ollama
cases in both modes, every non-Ollama vector byte-identical.

#### `680e507d` — 2026-08-15 — feat(model): the Ollama inline-<think> stream parser (P4.D78 unit 1)

_No crate versions bumped._

Ported the Ollama inline-`<think>` stream parser (P4.D78 unit 1, v4
d9c5a1c7): `ThinkTagStreamParser` plus the one-shot `extract_think_blocks`,
carrying the partial-tag holdback, the swallowed-opening-tag rule and its
emitted-visible cutoff, the flush semantics, and the JS-whitespace sanitize.
New tier-1 differential `ollama_think_parser_equivalence` drives v4's real
think-parser.ts over a committed 339-case table that enumerates every split
point of both tags.
Closed the connection-profile parameters key-order seam on the update path
(P4.D79 tier 2). The corpus already pinned a non-sorted multi-key bag on
create; the replace path now has its own arm, because the module's
"constrained to {} or a single key" note stopped describing reality when the
SPA began writing enable_thinking beside temperature. Insertion order
round-trips both ways. The module doc was swept for the new column and the
retired constraint. Versions: core 0.0.557, harness 0.0.479.

#### `3760c342` — 2026-08-15 — feat(chat): capture the greeting's reasoning and persist it

_Versions: core 0.0.556, harness 0.0.478._

Greeting generation now captures a thinking model's reasoning and persists it
on the opening message (P4.D79 unit 6, v4 23af7146). Providers emit
reasoningContent cumulatively — the full thinking-so-far on every chunk — so
it is accumulated by assignment, not concatenation, and an empty chunk does
not clear what came before; the value is trimmed alongside the content and
carried through all four greeting attempts and the Concierge reroute. It is
display-only, and a scripted first message stores NULL because it never
touched a model. The greeting differential grew a reasoning-carrying case
(plus one that pins the empty-chunk rule) and the chat-create capstone now
diffs the persisted value. Versions: core 0.0.556, harness 0.0.478.

#### `42af3ad0` — 2026-08-15 — feat(export): carry multiCharacterPrefill and regenerate the key-order table

_Versions: core 0.0.555._

Carried multiCharacterPrefill through the .qtap export (P4.D79 unit 5, v4
23af7146). schema-key-order.json was regenerated from v4's live schemas rather
than hand-appended, so the key lands in its schema slot after pseudoToolMode
instead of silently at the end of every exported profile record; a unit test
pins the slot. The connection-profile net read also gained per-column
tolerance: a table that predates the column selects a literal NULL instead of
failing outright, which is how pre-migration instances and every fixture built
before the drift stay readable. Versions: core 0.0.555.

#### `d0b2d6f7` — 2026-08-15 — feat(api): multiCharacterPrefill through the connection-profile routes

_Versions: core 0.0.554, harness 0.0.477._

Wired multiCharacterPrefill through the connection-profile create and update
routes (P4.D79 unit 4, v4 23af7146). Create resolves the provider default when
the client omits the field and stores it, so a create never writes the
tri-state NULL; both routes 400 with v4's exact sentence on a present
non-boolean, an explicit null included. The repo's create OMITS the column
when the value is absent rather than writing NULL — measured against v4, whose
INSERT names only the columns the parsed document carries, so on a fresh
instance the omission lands NULL and on a migrated one the DEFAULT 1 lands 1;
writing NULL would have passed the differential and diverged on upgraded
instances. The restore path carries the field; the .qtap import carry is
STOPPED by the round's ownership tripwire, with the two-line edit recorded at
the site. Versions: core 0.0.554, harness 0.0.477.

#### `f40a10ea` — 2026-08-15 — feat(services): the multi-character turn anchor becomes per-profile

_Versions: core 0.0.553, harness 0.0.476._

Made the multi-character [Name] turn anchor per-profile (P4.D79 unit 3, v4
23af7146). The hardcoded "Anthropic gets prose, everyone else gets the
prefill" branch is replaced by applyMultiCharacterTurnAnchor over the
profile's own multiCharacterPrefill choice, resolved through the tri-state
resolver; the prose sentence is byte-unchanged. The connection-profile net
read carries the new column (a NULL reads as an absent key, matching v4's
SQLite hydration). The orchestrator tier-3 corpus gains two cases that invert
the old provider rule — an OpenRouter profile with the prefill off takes the
prose route, an Anthropic profile with it on takes the prefill — and the
danger-reroute case pinned that the anchor resolves from the EFFECTIVE
profile, not the original. Versions: core 0.0.553, harness 0.0.476.

#### `3facb590` — 2026-08-15 — feat(core): consolidate profileParams and port the Ollama num_ctx injection

_Versions: core 0.0.552, harness 0.0.475._

Consolidated every profile-parameters construction site onto the shared
profileParams helper and ported its Ollama num_ctx injection (P4.D79 unit 7,
v4 d9c5a1c7). The helper gains the injection (Max Context becomes
options.num_ctx for Ollama profiles that do not already pin it); the eight v4
call sites' v5 twins now all route through it, which fixes three measured
divergences on the way (regenerate-swipe forwarded a non-object parameters
cell verbatim, the image-description fallback and the greeting both dropped
an array bag). A new 900-case tier-1 differential compares the result both
structurally and as literal JSON text, so key order is a comparand.

Two larger gaps surfaced while converting, both pre-existing and both fixed:
the Salon's primary stream had no modelParams twin at all, so a profile's
temperature, maxTokens, topP and the whole parameters bag were silently
dropped on every turn; and the Carina answer read its temperature from a
top-level profile key that does not exist. The orchestrator tier-3 corpus
gave its Primary profile a real parameters bag and the oracle now records
the modelParams reaching the wire, which is what made both visible.
Versions: core 0.0.552, harness 0.0.475.

#### `40172c98` — 2026-08-15 — feat(services): the multi-character [Name] prefill resolver

_Versions: core 0.0.551, harness 0.0.474._

Ported the multi-character prefill resolver (P4.D79 unit 2, v4 23af7146's
lib/llm/multi-character-prefill.ts): the provider default (off for Anthropic,
on everywhere else) and the tri-state resolution, where a stored null means
"never chosen" and falls back to the default — so an Anthropic profile
imported from a pre-4.9 bundle cannot come back with the prefill on. A new
144-case tier-1 differential over providers by stored state, mutation-proven.
Versions: core 0.0.551, harness 0.0.474.

#### `e30acf4e` — 2026-08-15 — feat(db): the multiCharacterPrefill column — D23 re-dump + boot ensure

_Versions: core 0.0.550, host 0.0.71._

Added the connection_profiles.multiCharacterPrefill column (P4.D79 unit 1,
v4 23af7146): the D23 fresh-schema re-dump at the aa464abf pin, plus a boot
ensure that gives an existing instance v4's migration shape. The two v4
shapes differ here — generateDDL emits a bare INTEGER (the Zod field has no
default), the migration emits INTEGER DEFAULT 1 and backfills Anthropic
profiles off. The ensure's guard is at the column level, not per statement,
because v4's backfill runs exactly once: re-running it every boot would
clobber a user's explicit choice on an Anthropic profile. Versions: core
0.0.550, host 0.0.71.
Closed work order P4.D80. Docs only.

#### `6d4285ca` — 2026-08-15 — port(P4.D80): bug-66 archivedAt enrichment + bug-69 rehydrate self-heal

_Versions: core 0.0.550, harness 0.0.474._

Ported the server half of v4's aa464abf. The chat GET's character enrichment
now carries archivedAt on both of its return paths, so the Salon sidebar can
badge an archived seat on a fresh load instead of only after a participants
action (v4 bug 66 — the bug this port filed upstream, now fixed on both
sides). Rehydrating a character archive also self-heals a bundle row whose
recorded digest was overwritten with the digest of its encrypted bytes: when
the recorded digest is provably the digest of the file as stored, the bundle
is intact and the record is repaired with a warning; any other mismatch is
still refused as corrupt (v4 bug 69, reachable on instances a pre-4.9 v4
damaged). The character-archive differential grows to 20 cases and gains a
digest-classification comparand, both mutation-proven.
Re-ran the neighbouring import, restore and archive differentials against
freshly regenerated oracles to confirm the import read-failure change moved
no green-path byte. Six of seven green; the one red is v4 drift on a
connection-profile column another lane is porting, not a regression here.

#### `4a2d4e85` — 2026-08-15 — test(import): pin the read-failure behavior with three planted differential arms

_Versions: harness 0.0.474._

Pinned the import read-failure behavior with three differential arms that
plant a failure in the database itself and compare against v4's real import
and preview. Two legs, measured rather than assumed: an unavailable document
store sinks both engines identically (the refusal message compared byte for
byte, and the preflight proven to write nothing across all three
partitions), while an unreadable table is swallowed by v4 and refused by v5
— recorded as a deliberate divergence with tripwires in both directions.

#### `943b9d27` — 2026-08-15 — fix(import): a failed existence read refuses the import instead of reading "id free"

_Versions: core 0.0.550._

The `.qtap` import no longer mistakes an unreadable database for an empty
one. Every existence check in the preserve-ids preflight, the per-item
importers, and the import preview used to swallow a read failure and report
the id as free; a failed read now refuses the import (or sinks the preview)
instead of going on to attempt id-carrying inserts into a database it could
not read. Twenty-three sites in all — the escalation had counted ten,
missing the multi-line ones. A fresh survey of v4 refuted the premise that
v4 propagates here: v4 swallows the same read errors, so this is a
deliberate divergence under the import/restore ruling, except on the
unavailable-document-store leg, where v4 genuinely throws and v5 was simply
wrong.
Recorded the P4.D81 lane's deferrals and gate: the provider option-schema
renderer stays unported, a live Ollama thinking proof is owed, and the archived
badge beat's gate is the unifier's to flip.

#### `c8f6d2c5` — 2026-08-15 — docs(composer): v4 converged on the source-view send, and the greeting fold pin (P4.D81 unit 5)

_Versions: SPA 0.5.492._

Recorded that v4 adopted Quilltap-v5's source-view send behavior in 4.9: a send
made with the raw-Markdown view open ships what the writer can see. No behavior
change here — comments and test notes only, plus a spec pinning that Send lights
for text typed only in source view.

#### `12cff6b7` — 2026-08-15 — test(archive): flip the archived-seat badge beat, and pin the pass-through (P4.D81 unit 4)

_Versions: SPA 0.5.491._

Turned the archived-seat sidebar beat around: it now expects both the Absent
and Archived badges on a fresh load, gated until the server projection that
makes that possible lands beside it.

#### `ba3e410e` — 2026-08-15 — feat(settings): Ollama's Enable Thinking, and the parameters bag that leaked (P4.D81 unit 3)

_Versions: SPA 0.5.490._

Added an Ollama "Enable Thinking" option to the connection-profile editor,
writing `enable_thinking` into the profile's parameters. Editing a profile no
longer drops the parameter keys the editor shows no control for — `num_ctx`,
OpenRouter's provider preferences, and the rest now survive a save.

#### `c18d919d` — 2026-08-15 — feat(settings): the multi-character prefill checkbox (P4.D81 unit 2)

_Versions: SPA 0.5.489._

Added the multi-character prefill checkbox to the connection-profile editor:
"Announce the speaker in multi-character scenes ([Name] prefill)", seeded from
the stored value or, when a profile has never chosen, from the provider
default. Switching provider re-seeds it on a new profile and leaves a saved
one alone. Ticking it on Anthropic warns that recent models reject a request
handed over mid-turn. The value ships on create and update, including the
Courier's, which renders the same assembled context.

#### `5c79c004` — 2026-08-15 — feat(settings): the multi-character prefill default, client twin (P4.D81 unit 1)

_Versions: SPA 0.5.488._

Ported v4's multi-character prefill default into the SPA
(`defaultMultiCharacterPrefill`): off for Anthropic, on for every other
provider, on when the provider is absent. It seeds the connection-profile
editor's new checkbox; the stored value's resolution stays server-side.

#### `c16f69ac` — 2026-08-15 — docs(porting): the aa464abf drift-round work orders — five lanes planned

_Docs-only change._

Planned the aa464abf drift catch-up round and committed five work orders:
P4.D78 (the Ollama-thinking provider wire — think-tag stream parsing, the
think/num_ctx request fields with the retry-without-think, the toolUse
manifest flip), P4.D79 (the multiCharacterPrefill column through the D23
re-dump and boot ensure, the per-profile turn anchor, greeting reasoning
capture, and the profileParams consolidation that also fixes three measured
v5 divergences), P4.D80 (the bug-66 archivedAt enrichment and the bug-69
rehydrate self-heal arm, with the round's three no-port dispositions),
P4.D81 (the SPA riders: the prefill and Enable Thinking checkboxes, the
archived-badge beat flip, the bug-67 convergence records), and P4.48 (the
escalated import-preflight read-error propagation at ten sites). Docs only;
the oracle baseline moves to aa464abf at the round's unification.

#### `0dad9719` — 2026-08-14 — docs(porting): v4 bugs 66 and 67 filed — the round's two v4-side findings discharged

_Docs-only change._

Filed the round's two v4-side findings upstream as v4 bugs 66 and 67 (the
archived-seat badge that cannot light on a fresh load, and the raw-source
view's send discarding source edits) — a docs-only commit in the v4
checkout, left unpushed beside the in-flight Ollama work.

#### `f6cd55d6` — 2026-08-14 — docs(porting): the help-drift round record — baseline moves to 24633026, all four lanes closed

_Docs-only change._

Unified the help-drift round: the server half of v4's section-level help
search (the help_doc_chunks table, chunking, the sync's re-slice and upgrade
backfill, the embedding job's chunk pass, the reindex/reapply riders, and
best-section ranking in help_search), the archive-family coverage remainder,
three maintenance smalls (the settings Zod arms, the google api-key header
fix, the sweep driver's staging-dependency class), and the composer
formatting toolbar with v4's composer layout. The oracle baseline moves to
24633026. The unification review fixed two wire-status divergences, a
disabled-state gap, and a mis-claimed editor divergence before merge; the
gate repaired two sweep-driver defects and restored a briefly-clobbered
recorded corpus. Full gate green: 431 test binaries / 2,082 tests, the
round's differentials regenerated fresh at the pinned baseline, ng test
4,711, and the full Playwright suite (numbers in the round record).
Versions: core 0.0.549, harness 0.0.473, host 0.0.70, SPA 0.5.487.

#### `cf1ec06f` — 2026-08-14 — fix(harness): restore the sweep-clobbered google-wire corpus; the driver never runs a committed corpus's recording stage

_Versions: harness 0.0.473._

Restored the recorded google-wire corpus after the unification gate's sweep
clobbered it: running the family through the sweep executed its RECORDING
script against the pinned v4 worktree, where the google plugin's runtime
deps are absent, so all eighteen recordings refused and the committed bytes
were overwritten with refusal rows (then swept into a commit by a broad
add). The sweep driver now never runs a committed-corpus family's recording
stage — recording is a deliberate by-hand act — and warns loudly whenever
any family's stages leave tracked fixture bytes modified.

#### `e633a113` — 2026-08-14 — fix(review): the help-drift round's §3 findings — settings error statuses, the composer disabled gate, the list-divergence pin, and four comment corrections

_Versions: core 0.0.549, harness 0.0.472, SPA 0.5.487._

Fixed two wire-status divergences the unification review's new error-status
assert caught in the settings routes: a validation failure whose message
carries no "Invalid" (a bare threshold out of range) now answers 500 exactly
as v4's catch does, and creating or renaming a connection profile onto an
existing name now answers 409 Conflict, not 400. Bodies were already
identical; only the statuses moved. The settings differential now asserts
the recorded status on every error row.

Fixed the Generate Image composer button staying clickable while the
composer is disabled (v4 disables it), and recorded — with a pinning spec —
the deliberate divergence that a narration/OOC button pressed inside a list
rewrites only the caret's item where v4 flattens the whole list into one
text run. Corrected two help-chunk code comments that misstated v4's
safe-query error semantics (the behavior was faithful; the stated mechanism
was inverted), refreshed the stale "no reindex-all handler" note in the
help-doc sync header, and recorded the reindex clear-pair's transaction
shape beside the backfill's existing note.

#### `3d3e60a1` — 2026-08-14 — fix(harness): the help-sync-guards recipe stages its own fixture (the P4.47 handoff wire)

_Versions: harness 0.0.471._

The help-sync guards differential's regen recipe now stages its own fixture
into a family-specific /tmp path instead of silently reading the sync family's
— the staging-dependency defect the new sweep-driver class flags, flagged by
the P4.47 lane and repaired at unification per the round's shared contract.

#### `b35ecea1` — 2026-08-14 — docs(help): record the backfill's transaction-shape divergence, and close the P4.D77 lane record

_Versions: core 0.0.546._

Recorded, in the code, why the section backfill's slicing loop is a single
transaction where v4's is not: a failure part-way through rolls the whole pass
back here and the next boot retries it, where v4 keeps what it wrote and waits
for a reindex. Comments only.

#### `b2494b62` — 2026-08-14 — test(help): pin that help sections keep no bookkeeping of their own, and bank the unported Guide search (P4.D77 tier 2, v4 24633026)

_Versions: core 0.0.545, harness 0.0.464._

Pinned the rule that section embeddings keep no bookkeeping of their own: the
dimension reconcile and the embedding-status rows count help documents, never
sections. Tests only.

#### `b1f8c92f` — 2026-08-14 — feat(help-search): rank a help page by its best section, and lead the result with it (P4.D77 unit 6, v4 24633026)

_Versions: core 0.0.544, harness 0.0.463._

`help_search` now ranks a help page by its best matching section as well as by
the page as a whole, and leads the result with that section rather than with the
first thousand characters of the file — which, on a long settings page, is a
table of contents and a preamble and never the answer. A page whose sections
have not been embedded yet still ranks exactly as it did.

#### `acea31dc` — 2026-08-14 — feat(embedding): embed help sections in the HELP_DOC job, and clear/re-fit them with their document (P4.D77 units 4-5, v4 24633026)

_Versions: core 0.0.543, harness 0.0.462._

Help sections are now embedded. The same job that embeds a help document fills
in its section vectors, skipping any that already have one, and a section whose
embedding call fails is logged and passed over rather than failing the whole
document. A full reindex clears section vectors alongside the document ones,
and re-fitting an embedding profile re-fits the sections too. Search does not
use them yet.

#### `075f809d` — 2026-08-14 — feat(help): slice help documents into section chunks on sync, and backfill upgraded instances (P4.D77 units 2-3, v4 24633026)

_Versions: core 0.0.542, harness 0.0.461._

Help documents are now sliced into sections for search. Each slice keeps the
nearest heading above it, and the slices are deliberately smaller than the
Scriptorium's — a settings page covers a dozen unrelated subsystems, and one
chunk that swallows four of them defeats the purpose. Nothing embeds or
searches them yet.

#### `07b2957f` — 2026-08-14 — test(import): the archive-bundle sweep and the preserveIds duplicate corner (P4.D65 item 5b/5c)

_Versions: harness 0.0.467._

Added the `help_doc_chunks` table — the storage for section-level help search.
Fresh instances get it from the re-dumped schema; existing instances get it on
the next boot. Nothing reads or writes it yet; the slicing, embedding, and
search follow.
Added tests for wiping archived-character bundles. Deleting all data spares
archived bundles by default; the explicit request to destroy them too was
untested, so nothing checked that the option was read at all. Also pinned how
imports behave when told to preserve incoming ids — an id that already exists
refuses the whole import before any duplicate-handling runs, which is what makes
the outcome predictable. Tests only; no shipped behavior changed.

#### `27ca83f2` — 2026-08-14 — test(archive): the banked archived-character refusals and the setParticipantStatus wrapper (P4.D65 item 5d/5e)

_Versions: harness 0.0.466._

Closed the last untested character-archive guards. Sending or listing mail as an
archived character, sending mail to one, asking an archived character what it
can reach, and picking one to answer a turn all refuse — and every one of those
refusals now has a test that fails if it is removed. Previously the guards were
in place but no test fixture contained an archived character, so nothing checked
them. Tests only; no shipped behavior changed.

#### `770de89a` — 2026-08-14 — test(archive): the four corpus-undriven arms (P4.D65 item 6) — and a stale oracle mock that was starving v4's import

_Versions: harness 0.0.465._

Closed the four corpus-undriven arms the character-archive review had recorded
as owed. The archive test fixture gained a default embedding profile, a
per-chat avatar-override face, and a standalone avatar thumbnail, and the
differential gained four cases: an archive refused because the instance
passphrase has not been entered, a rehydrate of a bundle sealed under an
earlier passphrase, an incomplete prune, and a rehydrate of a tombstone left by
the older archive revision. Regenerating the comparison also found that the
test harness had been stubbing out the default embedding profile, so the
reference implementation had been queueing no embedding work for restored
memories at all; with that removed, both implementations queue the same nine
jobs. Tests only; no shipped behavior changed.
Fixed Google Gemini requests sending the API key in the URL instead of the
`X-Goog-Api-Key` header. Google accepts both, so nothing was visibly broken, but
keys in URLs are the shape that ends up in proxy and server logs — and Quilltap's
own image-generation requests were already using the header, so the two paths
disagreed. Found by newly asserting the request headers the Google wire test had
been recording and ignoring.

#### `53737074` — 2026-08-14 — fix(harness): the sweep driver's staging-dependency class, made honest (P4.47 C)

_Versions: harness 0.0.469._

Developer tooling: the harness recipe sweep now detects a test family whose
oracle-regeneration recipe reads a scratch file no stage of that recipe builds —
the failure mode that leaves a family working on one machine and dead on another.
Four families that leaned on a sibling's scratch build now stage their own inputs
and were re-proven to run from nothing.

#### `f2d3cda2` — 2026-08-14 — fix(settings): the three sibling Zod arms answer v4's real 400 bodies (P4.47 A)

_Versions: core 0.0.547, harness 0.0.468._

Fixed three chat-settings fields answering an invented error message instead of
the one the server actually produces. Saving an out-of-range or wrong-typed
value under Answer Confirmation, Cheap LLM or Dangerous Content used to come
back as a single flat sentence; it now returns the same detailed, per-field
report v4 returns, listing every offending key at once. Dangerous Content also
now checks that the two uncensored-profile ids are real UUIDs, and the cheap-LLM
bag is checked at the point v4 checks it, so a request with more than one bad
field reports the same one v4 reports.
Documented a known gap: the Document Mode pane still has no formatting toolbar,
where v4 gives it the same one the composer has.

#### `7e9051c7` — 2026-08-14 — feat(salon): v4's composer layout, and retire the dogfood #75 band-aid (P4.9L units 4-5)

_Versions: SPA 0.5.485._

Fixed the Salon composer's layout (dogfood finding #75). The message-level tools
are back in the compact two-column block beside the message box rather than
strung out in one long row, so the box keeps the width it needs and the
"Type a message…" placeholder no longer clips. The interim fix that wrapped the
whole tool row below the box is gone with it.

#### `54f7f331` — 2026-08-14 — feat(salon): mount the formatting toolbar in the composer (P4.9L unit 3)

_Versions: SPA 0.5.484._

The Salon composer has its formatting toolbar. Switch a chat to composition mode
and the bold/italic/heading/list/quote/code buttons, the indent controls, the
emoji and symbol pickers, the roleplay template's delimiter buttons, and the
"Edit markdown source" toggle all appear above the message box — the same
toolbar the form fields have had, with the delimiters v4 shows only in a chat.

#### `313feede` — 2026-08-14 — feat(composer): the toolbar's roleplay-delimiter section (P4.9L unit 2)

_Versions: SPA 0.5.483._

The formatting toolbar now shows a roleplay template's delimiter buttons — the
narration button first, then the template's own, minus any that mark text the
same way narration does. Form fields, which have no template, are unchanged.

#### `3f522662` — 2026-08-14 — feat(composer): port v4's roleplay-delimiter toolbar transforms (P4.9L unit 1)

_Versions: SPA 0.5.482._

Ported v4's roleplay-delimiter toolbar transforms — the half of the composer
formatting toolbar the shared form-field toolbar never had. The narration button
a template synthesizes from its narration characters, the tooltip strings, the
line-prefix and tag-prefix transforms, and the rich-editor command that applies
them all now match v4 byte for byte, against vectors recorded from v4's own
toolbar, transforms and command handler.

#### `88adbbee` — 2026-08-14 — docs(porting): the 24633026 round's work orders — P4.D77 help-chunks drift, P4.47 maintenance smalls, P4.9L composer toolbar; P4.D65 remainder assigned

_Docs-only change._

Planned the next porting round against the new v4 baseline `24633026`
(section-level help embeddings and Guide content search). Wrote three new
work orders — the P4.D77 help-doc-chunks drift catch-up (server), the
P4.47 maintenance smalls (settings Zod-collapse arms, google-wire header
asserts, the sweep driver's staging-dependency class), and the P4.9L
composer formatting toolbar (SPA) — and assigned the open P4.D65
remainder (items 5–6) to the round. Docs only; no code changes.

#### `2c748fbc` — 2026-08-14 — docs(dogfood): the 2026-08-14 walk record — 38 steps, findings #79-#86, the wardrobe-tier and passphrase proofs discharged

_Docs-only change._

Dogfooded the 4.8.2/4.8.3 round against a copy of a real instance: 38 steps
across Setup, the instance lock, smart typography, both composer typeaheads,
the group wardrobe tiers, and the passphrase chain. Two defects found and
fixed (see the entries below); everything else behaved as v4 does. The
passphrase walk confirmed that changing it rewrites every archived-character
bundle — both archives rehydrated afterwards from bundles sealed under the old
passphrase.

#### `0477623e` — 2026-08-14 — fix(spa): typed backslashes doubled on the wire, and the typeahead arrows died under the mouse (dogfood #84, #85)

_Versions: SPA 0.5.481._

Fixed the arrow keys appearing to do nothing in the composer's emoji and symbol
menus (dogfood finding #85). They always worked with the mouse away from the
menu; with the pointer resting on it — where it usually is, since the menu opens
at the caret you just clicked — every keypress was undone by the row list being
rebuilt underneath the cursor. The rows are now updated in place when only the
highlight moved.

Fixed typed backslashes doubling on their way out of the editor (dogfood finding
#84). `$\alpha$` written in the composer reached the message as `$\\alpha$` —
broken LaTeX for KaTeX and a doubled backslash for any model reading the
transcript — and the same went for Windows paths and any other literal
backslash. The markdown serializer escaped the backslash where v4's never does.
Text written by a character was never affected; it does not pass through the
editor.

#### `f8924e60` — 2026-08-14 — fix(spa): the paused notice said something pause does not do (dogfood #83)

_Versions: SPA 0.5.480._

Corrected the paused-chat notice in the Salon (dogfood finding #83). It claimed
"the next character won't speak until you resume", which is not what pause does
in either app: pause stops the auto-chain between characters, and a message you
send is still answered once by whoever's turn it is. The notice is a v5-only
affordance (v4 shows none), so the wording was v5's alone — and it read one
ordinary reply as a broken pause. The engine behavior was already v4-faithful
and is unchanged.

#### `4ace3b2a` — 2026-08-14 — fix(spa): Enter leaves a fenced code block (dogfood #82)

_Versions: SPA 0.5.479._

Fixed a fenced code block being a one-way door in the chat composer (dogfood
finding #82). Typing ``` opens a code block and nothing in the markdown dialect
closes one, so every subsequent Enter only added another line to it — a writer
who fenced a snippet mid-message could never get back out to prose. v4 has an
escape v5 never got: Enter on a blank trailing line trims that line away and
opens a paragraph after the block. Ported with v4's conditions one for one, and
scoped to the composer, the only host v4 gives it to.

#### `5b25c54f` — 2026-08-14 — docs: the 4.8.2/4.8.3-round unification record — baseline → 48396682, all seven orders closed, the two review finds, gate numbers; order status headers; phase-4 candidates; CLAUDE.md round bullet + baseline (old paragraph archived)

_No crate versions bumped._

Unified the 4.8.2/4.8.3 drift catch-up + lock-order round (P4.D71 ∥ P4.D72 ∥
P4.D73 ∥ P4.D74 ∥ P4.D75 ∥ P4.46 ∥ P4.D76 — all seven closed) onto main; the
oracle baseline moves to `48396682` and the drift debt is cleared (v4 HEAD
`11553944` = 4.8.4 is tests+docs only, NO-PORT). Landed: the group wardrobe
tiers + bundle dissolution on both sides (dogfood finding #78 closed — v4's
Bug-61 fix ported with a deterministic race beat), the three composer features
whole (smart typography, `:` emoji + `\` Unicode typeaheads and pickers over
v4's code-identical engines and byte-copied corpora/datasets; bugs 62 + 63
fixed), the three chat_settings columns (D23 re-dump + boot ensure + Zod-exact
PUT arms), the P4.D68 open-before-lock escalation discharged (lock before any
partition open on boot/unlock/setup) with setup hardening and .dbkey
unknown-field preservation, and the SDK wire re-check (neutral outside the
self-dating version markers). The unification review caught and fixed two
would-have-shipped bugs: the empty typeahead menu swallowed Enter/Tab/arrows
(v4 falls through), and first-run Setup failed on a not-yet-created data dir
(the lock reorder outran the dir creation; every test had masked it). Gate:
430 test binaries / 2,067 / 0 with the round's env block; the 25 affected
oracle families regenerated fresh; ng test 319 files / 4,614 / 0; full
Playwright 212/212 zero skips. Versions: core 0.0.540, harness 0.0.460, host
0.0.69, web 0.0.72, SPA 0.5.478.

#### `9fdb4b14` — 2026-08-13 — test(wardrobe): prove the chat-start group tier where it actually runs (P4.D71 unit 5)

_Versions: core 0.0.534, harness 0.0.457._

Closed a coverage hole in the group wardrobe tier (P4.D71 unit 5, tests and
fixtures only — no behavior change). The chat-start pool's own group read and
the per-character tier in default-outfit resolution had shipped with no
differential able to see them: deleting either left every family green. The
chat-dialogs fixture gained a group holding two characters, with a default the
group supplies outright, a copy that shadows a Quilltap General item under a
different slot, and a personal opt-out of a group default — which together make
the whole precedence chain (character > group > project > general) visible in
the stored outfit rather than only in prompt text. Four v4 merge-rule unit
tests ported alongside.

#### `84418b46` — 2026-08-13 — feat(wardrobe): the group-scope read + transfers out of a group (P4.D71 units 3-4, v4 8600c83f)

_Versions: core 0.0.533, harness 0.0.456, web 0.0.71._

Finished the group wardrobe tier's two remaining surfaces (P4.D71 units 3–4).
`GET /api/v1/characters/{id}/wardrobe?scope=group` now serves the group tier —
the shared items in the `Wardrobe/` folder of every store belonging to a group
the character is a member of — as a standalone read for the client-side tier
merge; the dispatch verb `characterWardrobeList` takes the same optional
`scope`. The REST path itself is new to v5: the SPA reads this resource over
the dispatch channel, so the documented URL had never been served and would
have answered 404 to any client following the API reference. Wardrobe transfers
can also take an item back OUT of a group now: the source lookup scans the
source character's group stores between the project store and Quilltap General,
and a group source deletes by mount point like a project source does. Without
that, an item moved into a group was stuck there.

#### `01076672` — 2026-08-13 — feat(wardrobe): the group tier + bundle dissolution, server side (P4.D71 units 1-2, v4 8600c83f + 61574563)

_Versions: core 0.0.532, harness 0.0.455._

Ported the group wardrobe tier and bundle dissolution into the Rust core
(P4.D71 units 1–2; v4 `8600c83f` + `61574563`). Wardrobe items living in a
group store's `Wardrobe/` folder are now visible and resolvable everywhere
the other shared tiers are, with precedence character > group > project >
general — `wardrobe_list`, `wardrobe_read`/`_wear`/`_update`/`_archive`,
the chat outfit route's six item-resolving modes, chat-start default
resolution and the cheap LLM's pick, the outfit summary's cast-wide union,
the equipped reads in chat creation, the Aurora outfit whisper, story
backgrounds, image generation, buildContext's outfit cache, and the avatar
prompt. Group stores follow the character, never a co-participant; the
chat outfit summary is the one documented exception and reads the union of
the participants' memberships. Two v4 fixes ride along: `wardrobe_create`
now keys component resolution on the RECIPIENT of a gift, and its
equip-now path passes tiers at all (a new bundle's shared components did
not resolve before). Separately, wearing a resolvable bundled outfit now
dissolves it into its leaf garments at write time on every wear path
(wear / replace / add_to_slot, default outfits, the cheap LLM's chat-start
pick), so equipped state holds garments rather than one opaque card over
empty slot rows. Dissolution is recursive and fail-safe: a bundle whose
components cannot be resolved is stored whole exactly as before. No schema
or export-shape change; no migration.
P4.D73 unit 3 — the chat-settings PUT arms for the three 4.8.2 keys. Two
boolean arms at v4's schema-ordered positions after `composerSpellcheck`, with
its exact `Invalid composerEmoji value (must be boolean)` sentences, and a
route-level `SmartTypographySettingsSchema.parse` that defaults each absent key
on a partial bag. The reject bodies are v4's whole `ZodError.message` —
`JSON.stringify(issues, null, 2)`, which v4's route lets escape to
`getErrorMessage` and its `.includes('Invalid')` test turns into a 400 — so the
Zod issue shape is ported byte-for-byte rather than collapsed to a summary
sentence. `settings_routes_equivalence` gained a ten-case `composer_settings`
family (GET defaults, full/partial/empty bags, both wrong-typed booleans, a
null bag, a nested wrong type, a non-object, and the create branch) with its own
stale-oracle count guard; 72 cases match. A new `quilltap-web` wire test drives
the three keys through `POST /api/dispatch` over an instance whose columns were
dropped before boot, pinning the boot ensure and the raw-bag wire — an explicit
`null` must reach the handler as present-and-invalid, not read as an absent key.

#### `50ca5f0b` — 2026-08-13 — feat(db): the chat_settings composer/typography boot ensure (P4.D73 unit 2)

_Versions: core 0.0.536, host 0.0.67._

P4.D73 unit 2 — the `chat_settings` composer/typography boot ensure. v4
adds the three 4.8.2 columns through its migration runner; v5's runner is a
locked deferral, so they are re-homed as a boot repair over the main partition
(the P4.d7 / P4.D41 / P4.D63 precedents), each `ALTER TABLE ... ADD COLUMN`
carrying v4's exact type and DEFAULT clause and guarded by its own
column-presence check. Load-bearing rather than cosmetic: measured on an
un-ensured instance, the read tolerates (the settings screen renders with the
Zod defaults) but the PUT answers `500 sqlite error: no such column:
composerEmoji`, since `update_for_user`'s update branch is a plain `SET` —
the same class as the third Friday dogfood sighting. A fresh instance already
has the columns from the re-dump. v4's three migration pretty labels have no
v5 analog: recorded NO-PORT.

#### `0871733b` — 2026-08-13 — feat(settings): the D23 re-dump + v4 4.8.2's three chat_settings columns through the data layer (P4.D73 unit 1)

_Versions: core 0.0.535, harness 0.0.458._

P4.D73 unit 1 — adopted v4 4.8.2's three new `chat_settings` columns
through the data layer and the D23 fresh-schema re-dump. `fresh_schema.json`
and `chat_settings_seed.json` re-dumped from v4's live `generateDDL` at
`48396682` (31 -> 34 seed columns; the diff is exactly `composerEmoji INTEGER
DEFAULT 1`, `composerUnicode INTEGER DEFAULT 1`, `smartTypographySettings TEXT
DEFAULT '{"displayQuotes":false,"dashes":true,"ellipsis":true}'`).
`schema-key-order.json` regenerated and proven byte-identical (no exported
entity moved). All six `chat_settings.rs` sites carry the three columns, with
the new `SmartTypographySettings` struct in v4 Zod declaration order; the read
defaults each absent column to its Zod default rather than erroring or emitting
null, and the restore-facing `ChatSettingsCreate` deserialize defaults them so
a pre-4.8.2 archive still restores. The tier-2 corpus grew the three cells on
both write paths plus a surviving create op -- the create arm's cell values
were previously unobservable, because the only created row was deleted by a
later op in the same sequence.
Recorded the dispositions for v4's two 4.8.3 lifecycle fixes (docs only, no
code): bug 64 (first-run setup wedging every DB connection) is a NO-PORT — v5
caches no database handle above `Db`, lock/unlock rebuilds the world, and there
is no post-setup plaintext-to-cipher conversion to sequence because v5
provisions encrypted from byte zero — and bug 65 (the inert version guard) is a
NO-PORT because v5 has neither a version guard nor a migration runner. Both
records carry the evidence, and both real defects the survey turned up on the
way are fixed above. Left deliberately unfixed and flagged for a ruling:
`instance_settings.highest_app_version` is read by the Almanack but never
written by v5, so a v5-provisioned instance renders that premise null and
carries no downgrade tripwire — writing v5's own version into v4's semver guard
would be meaningless at best and lock-out-inducing at worst.

#### `bce6d432` — 2026-08-13 — fix(dbkey): a passphrase change preserves .dbkey fields v5 does not model (P4.46 tier 2)

_Versions: core 0.0.539, harness 0.0.460._

A passphrase change no longer strips fields Quilltap v5 does not model from
`quilltap.dbkey`. v4's version guard writes `minServerVersion` into the key file
by read-modify-write so an older binary is refused before the database is opened
at all; v5's re-wrap was a full replace, so the first passphrase change on a
v4-authored instance removed the floor. The re-wrap is now read-modify-write at
the JSON level — the ten modelled fields are replaced in place, everything else
survives, and the file keeps its key order. This is a deliberate divergence from
v4, measured from v4's own code in the cross-compat oracle: v4's
`changePassphrase` drops the field too, but v4 rewrites it every startup and v5
never would.

#### `f327baf9` — 2026-08-13 — feat(host,core): lock the instance before ANY partition opens — boot, unlock, and first-run setup (P4.46, the P4.D68 escalation)

_Versions: core 0.0.538, SPA 0.5.455._

The instance lock now covers every partition open — boot, unlock, AND
first-run setup (P4.46, the standing P4.D68 escalation). v4 acquires its
single-instance lock inside `connect()`, ahead of `new Database`; v5 acquired
it at the head of the host assembler instead, which is after `Db::open` has
opened all three partitions writable and, on first-run setup, after the whole
DDL replay and the baseline seed have been written. Against a contended
instance that meant writing to databases another live process believes it holds
exclusively — the class v4's bug-58 fix closes. Acquisition moves to a new
`EngineAssembler::pre_open` seam the engine calls at all three entrances
(re-entrant per PID, so setup's own claim does not block its later open);
release stays exactly at shutdown and the heartbeat / lock-lost semantics are
unchanged. Proven by a new contended-start test that plants a live foreign lock
and asserts the refusal arrives with the partitions byte-for-byte untouched and
nothing created; mutation-checked by reverting the ordering (all three go red).

First-run setup no longer eats the display-once encryption key, and a failed
setup can no longer brick the instance on retry. A late failure — the key
written, the instance provisioned, but the databases would not open — now
returns the pepper anyway with `requiresRestart` (v4 bug 64's contract: the key
is displayed exactly once, so it is never withheld behind an error), and the
setup wizard renders the key plus a restart notice instead of a bare error. A
second setup against a data directory that already carries `quilltap.dbkey` or
any partition refuses by name instead of minting a new pepper over the old one
(v5 hardening — v4's conversion-shaped setup has no analog); provisioning
enforces the same precondition its doc comment already claimed.

#### `ce1412b8` — 2026-08-13 — P4.D76: the 4.8.2 SDK refresh moved v4's provider wire not at all

_No crate versions bumped._

Fixed an ordering-dependent break in the SPA test suite: a clipboard stub was
installed as a non-writable property, so whichever spec next touched
`navigator.clipboard` in the same worker threw. Test-only.
Re-checked the provider wire against v4's 4.8.2 dependency refresh (P4.D76):
openai 7.2.0 → 7.4.0 and `@openrouter/sdk` 1.2.2 → 1.2.32 moved v4's outgoing
requests not at all. All four recorded corpora were regenerated against the
new SDKs (proven installed: the plugin and root lock files match their
node_modules exactly, 188 + 1,023 packages, zero mismatches): every request
body, url, method, input, and the one recorded refusal are byte-identical,
and the only differences are the self-dating version markers the corpora
started recording in P4.44 — `x-stainless-package-version` on the 80
openai-family rows and the speakeasy user-agent on OpenRouter's 13 SDK-path
rows. Anthropic's rows are byte-identical whole (its SDK did not move), as
are the google-request and response-body corpora. The six consuming
differentials pass by name against the fresh corpora, including the
OpenRouter pricing family with the real 1.2.32 SDK in the oracle loop — the
SDK's key remap and its 500-row page loop both survived the bump. The
provider manifests regenerate byte-identical against the 14 rebuilt plugins.
The google-wire corpus also gained the request headers it should have gotten
in P4.44 (that lane regenerated only its sibling); the bodies are unchanged
and nothing asserts those headers yet. No engine code changed.
P4.D72 unit 5 — the round's smaller wardrobe riders. The character editor's
"Aurora's Core whisper" card now stacks: label and description span the card
with the full-width dropdown beneath, instead of a two-column row where the
select took nearly the whole card and wrapped the label into a sliver (v4 4.8.2
`fe63547a`). The Salon sidebar's `.qtap` chat export anchor-clicks through the
download helper instead of setting `window.location.href` — the same fix v4
4.8.2 made for Electron, and v5 had the same exposure in the Tauri `qtap://`
webview, where the old line navigated the app window onto the API route rather
than downloading. Dogfood finding #78 is CLOSED (v4 fixed Bug 61; v5 ported the
fix), the wardrobe e2e's now-stale comment block is rewritten, and the walk
gains a beat that holds `chatOutfitGet` open across a Wear click and asserts
the edit survives the seed and reaches one `set_all` with both items — red
against the pre-fix code, green after.

#### `bd803042` — 2026-08-13 — port(spa): the group wardrobe tier in the loader (v4 4.8.2 `8600c83f`)

_Versions: SPA 0.5.458._

P4.D72 unit 4 — the SPA wardrobe loader reads a fourth tier: the shared
wardrobe of every group the character belongs to (v4 4.8.2 `8600c83f`, where
items moved into a group were invisible to everyone because nothing read them
back). `characterWardrobeList` gains an optional `scope: "group"` field per the
round's shared contract, the loader fires four parallel tier reads instead of
three, and the merge precedence becomes personal > group > project > general —
so a group's livery shadows a project's copy while a personal copy shadows
both, including the isDefault-false personal copy a character uses to opt out
of a shared default. Group items arrive with a null characterId like every
other shared tier, so they are wear-only with no extra labeling. The group read
fails soft, so the SPA still works against a server whose group arm has not
landed.

#### `2166296b` — 2026-08-13 — port(spa): client bundle dissolution (v4 4.8.2 `61574563`)

_Versions: SPA 0.5.457._

P4.D72 unit 3 — ported v4 4.8.2's client-side bundle dissolution into the SPA:
putting a bundled outfit on now stores its PARTS, never the bundle's own id, so
the wardrobe no longer shows one "Man in Black" card above four slot rows that
all read Empty. New `dissolve-bundles.ts` (dissolveBundleToLeaves /
layLeavesIntoSlots / dissolveBundlesInSlots) over a new `expand-composites.ts`,
with `wearItemIntoSlots` / `replaceItemIntoSlots` / the new `addItemToSlot` /
`computeDisplacedSlots` / `buildDefaultOutfit` widened to take the item lookup.
Threaded through the dialog's five wear sites and the outfit store's three
optimistic paths. Fail-safe throughout: no lookup, or a bundle whose parts do
not resolve, stores it whole exactly as before, and read-time expansion still
covers outfits equipped before the change. `replace` clears the union of the
slots the bundle designates and the slots its pieces land in. v4's 347-line
dissolve suite ported case-for-case (minus its persisted-repo describe, which
is P4.D71's) plus the default-outfit case; mutation-proven at both levels —
inverting the dissolve guard fails 16, dropping the lookup at the call sites
fails 4 (dialog) and 1 (store). The server stays authoritative for persisted
slots; this is the optimistic mirror.

#### `d8678abc` — 2026-08-13 — port(spa): the bug-61 staged replay in the wardrobe dialog (v4 4.8.2 `07d4ccce`)

_Versions: SPA 0.5.456._

P4.D72 unit 2 — wired v4 4.8.2's Bug 61 fix into the SPA wardrobe dialog: a
Wear clicked before the worn snapshot arrives is no longer lost. The gesture
(not its result) is recorded in a per-character pending-mutator queue, the
seeding effect drains that queue and rebases it onto the true worn slots
instead of cloning them, and the Done flush classifies through
`classifyStagedOutfits` — a character staged against no baseline is now put to
the operator ("Word of what {names} is presently wearing never reached us…")
instead of being silently counted as clean and closed as if saved. Declining
keeps the dialog open so a late snapshot can still seed and save. v4's race
suite is ported case-for-case (4 tests, held-open `chatOutfitGet`, asserting
ONE `set_all` carrying both the already-worn shirt and the just-clicked hat)
and mutation-proven: restoring the three pre-fix parts fails three of four.
Closes dogfood finding #78 in the SPA.

#### `8feb94b6` — 2026-08-13 — port(spa): the staged-live-outfits pure module (v4 4.8.2 `07d4ccce`, bug 61)

_No crate versions bumped._

P4.D72 unit 1 — ported v4 4.8.2's new `lib/wardrobe/staged-live-outfits.ts`
into the SPA as `app/wardrobe/staged-live-outfits.ts`: `rebaseStagedSlots`
(replays gestures staged before the worn snapshot arrived onto the real
slots) and `classifyStagedOutfits` (separates "nothing changed" from "we
never learned what clean was" — the Bug 61 silent loss). `equippedSlotsEqual`
moved from `equipped-slots.ts` to the new module, matching v4's own move out
of the dialog. v4's unit suite ported case-for-case (11 tests) and
mutation-proven: reverting either helper to its pre-fix shape fails four
cases. No behavior change yet — the dialog wiring is unit 2.
Closed P4.D74 (the 4.8.2 smart-typography + renderer drift, SPA): the
rule engine, render-time quote curling, type-time dashes and ellipsis,
bugs 62 and 63, the katex pin, the settings card, and a live Playwright
walk. Gate: ng test 304 files / 4,283 tests, ng build clean, full
Playwright 206 passed / 1 gated skip.

#### `06dd13bb` — 2026-08-13 — fix(spa): the settings service must not write signals while being constructed (P4.D74 unit 8)

_Versions: SPA 0.5.467._

Fixed a render-breaking regression in the smart-typography settings
service (P4.D74 unit 8): it wrote its signals while being constructed,
which in a zoneless app happens inside the parent template's reactive
context — Angular threw NG0600, the render unwound, and the Salon chat
list came up empty after navigating back out of a chat. The initial
values are seeded at field level now and every later write is untracked.
Caught by the full Playwright suite, which is the only place it was
visible.

#### `0994c21e` — 2026-08-13 — test(spa): the smart-typography live walk + v4's spec mirrored (P4.D74 unit 7)

_Versions: SPA 0.5.466._

Walked smart typography live (P4.D74 unit 7): four Playwright beats —
the dash ladder and the ellipsis typed into the real composer, the
one-Backspace revert and its one-keystroke window, the code-fence bail,
and the settings card's try-it box. The display-quotes beat waits on the
settings column and is gated by a named constant. v4's feature spec is
mirrored under `docs/v4/`; its help-doc section is banked for `p4.9i2`,
which is where v5's help surface still lives.

#### `5f026785` — 2026-08-13 — feat(spa): the Smart Typography settings card (P4.D74 unit 6)

_Versions: SPA 0.5.465._

Added the Smart Typography settings card (P4.D74 unit 6), between Text
Replacement and Token Display in Settings → Chat: the curly-quote
display toggle, the two type-time rules, and a try-it box that runs the
same engine the composer does, so the preview cannot disagree with the
real thing. Each save sends the whole bag, so flipping one rule cannot
drop another.

#### `c49df581` — 2026-08-13 — port(spa): smart typography Part B — type-time dashes and ellipsis (P4.D74 unit 5)

_Versions: SPA 0.5.464._

Ported smart typography Part B into the composer and the Document Mode
editor (P4.D74 unit 5): typing `--` gives an en dash, `---` an em dash
and `...` an ellipsis, over the same engine v4 uses, with v4's revert
semantics — one Backspace puts the literal characters back, one
Cmd/Ctrl+Z undoes the substitution and nothing else, and any other key
closes the window. Nothing fires in code blocks, inline code, source-mode
editors or during IME composition, and pasted text is left alone. Each
rule follows its own toggle, live.

#### `c268189a` — 2026-08-13 — fix(spa): bug 63 — typing aids no longer rewrite code (P4.D74 unit 4)

_Versions: SPA 0.5.463._

Fixed bug 63 in the composer (P4.D74 unit 4): text replacements fired
inside fenced code blocks and inline code runs, because a code block is
a textblock like any other and the inline code mark was never consulted.
Both typing aids now bail through one shared `code-context` helper —
which also covers the case where the code mark is armed but nothing has
been typed yet — so their bail lists cannot drift apart again.

#### `49751f9c` — 2026-08-13 — port(spa): smart typography Part A — render-time quote curling (P4.D74 unit 3)

_Versions: SPA 0.5.462._

Ported smart typography Part A into the SPA renderer (P4.D74 unit 3):
`remark-smartypants` curls straight quotes at render time when
`smartTypographySettings.displayQuotes` is on, at v4's exact position in
the pipeline and behind v4's two cached processors. Stored content is
never modified. Code, math and link targets are excluded structurally,
and a roleplay template that claims a quote character as a delimiter —
or a dialogue detection naming a straight quote without its curly forms
— suppresses curling for the whole render. The renderer reads the
setting itself, as v4 does, so every message surface agrees; the render
memo now keys on it so a toggle repaints what is already on screen. The
v4-captured parity corpus grows 51 → 67 vectors.

#### `562fdcee` — 2026-08-13 — fix(spa): bug 62 — curly-quoted dialogue is highlighted again; katex → 0.18.4; corpus recaptured (P4.D74 unit 2)

_Versions: SPA 0.5.461._

Fixed bug 62 in the SPA renderer and recaptured the v4 parity corpus
(P4.D74 unit 2): the default dialogue pattern and the paragraph-level
detection chars now carry the curly double quotes as v4 does since
`c7892132` (in `\\u` escape form, deliberately), the `katex` pin moved to
v4's resolved 0.18.4, and `markdown-fixtures.json` was recaptured from
v4's real renderer at `48396682`. Exactly one vector's bytes moved —
curly-quoted dialogue is highlighted now, where before it was not.

#### `9ec51bbe` — 2026-08-13 — port(spa): the smart-typography rule engine twin + v4's fixture corpus (P4.D74 unit 1)

_Versions: SPA 0.5.460._

Ported the smart-typography rule engine into the SPA (P4.D74 unit 1):
v4's stdlib-only `lib/smart-typography/engine.ts` copied byte-identical
below its docblock, its 15-vector fixture corpus copied byte-for-byte,
and v4's engine test suite ported case-for-case to vitest (23 tests).
The engine has no callers yet — the renderer, the composer plugin and
the settings card follow in later units of the same order.
P4.D75 gate: SPA unit suite 310 files / 4,420 tests green, `ng build` clean with
the two character datasets served as static assets and absent from every JS
bundle, and the lane's four e2e beats green against the real server.

#### `84538e45` — 2026-08-13 — test(e2e): four live beats for the character-insertion feature, and the v4 docs mirror (P4.D75 unit 6)

_Versions: SPA 0.5.475._

Added four end-to-end beats for the character-insertion feature (`:smile:` in
the live composer, the menu's Enter/Escape, `\to ` keeping its space, and a
toolbar picker inserting into a markdown field), and mirrored v4's two feature
specs into `docs/v4/`.

#### `8519d42e` — 2026-08-13 — port(spa): the two Composer settings toggles for the emoji and symbol shortcuts (P4.D75 unit 5)

_Versions: SPA 0.5.474._

Added the two Composer settings toggles for the emoji and symbol shortcuts
(`composerEmoji` / `composerUnicode`, both on by default). They govern the
automatic `:` / `\` triggers only; the toolbar buttons are never gated.

#### `789a0267` — 2026-08-13 — port(spa): the emoji and symbol pickers — the shared panel, both popovers, the two toolbar buttons (P4.D75 unit 4)

_Versions: SPA 0.5.473._

Added the emoji (☺) and symbol (Ω) pickers to the editor's formatting toolbar:
a shared browsable panel with search, a recently-used row, and a grid grouped by
emoji category or Unicode block, over the same match engine the typeaheads use.
Neither button is gated by the composer settings — those govern the automatic
`:` / `\` triggers only. In raw-source mode the pick lands at the textarea
caret, where v4 loses it.

#### `dc6f747c` — 2026-08-13 — port(spa): mount the typeaheads in the composer and Document Mode, gated by the two chat settings (P4.D75 unit 3)

_Versions: SPA 0.5.472._

Mounted the composer emoji/Unicode typeaheads in the two hosts v4 mounts them
in — the Salon composer and the Document-Mode editor — gated by the
`composerEmoji` / `composerUnicode` chat settings (default on, read from the
shared settings query). Form-field editors do not get them. The pickers' insert
path landed on the editor handle.

#### `1466d436` — 2026-08-13 — port(spa): the char-typeahead adapter — the caret-anchored menu ProseMirror had no precedent for (P4.D75 unit 2)

_Versions: SPA 0.5.471._

Ported the composer's `:` emoji and `\` Unicode typeahead into the SPA's
ProseMirror editor (v4's `CharTypeaheadPlugin`): the menu-free commit
keystrokes (`:smile:`, `\to `), the caret-anchored menu with its flip/align
geometry, the code-block / inline-code / math-span / IME bails, the lazy
dataset fetch, and v4's one-undo contract. v4's two plugin suites are ported
case-for-case (56 cases) over a real editor and the real dataset, plus 11 cases
for the menu surface. Not yet mounted in a host — that is the next unit.

#### `ebfb104c` — 2026-08-13 — port(spa): the char-insert engine twin — v4's `lib/char-insert/` verbatim, its four corpora and both datasets byte-copied (P4.D75 unit 1)

_Versions: SPA 0.5.470._

Ported v4's character-insertion engine (`lib/char-insert/`) into the SPA at
`apps/web/src/app/editor/char-insert/` — the shared logic behind the composer's
`:emoji` and `\unicode` typeaheads and both toolbar pickers. Code-identical to
v4's (doc comments aside); the four behavior corpora and both dataset assets
are byte-identical copies, and v4's four unit suites are replayed case-for-case
(174 cases). The datasets are served from `public/` at v4's paths and fetched
lazily, so they never enter the bundle.

#### `595fb678` — 2026-08-13 — docs: trim CLAUDE.md under the per-turn size limit — archive the 2026-07-10→07-30 round bullets and the superseded oracle-baseline chain to claude-md-status-history.md (verbatim, diff-verified); CHANGELOG

_Docs-only change._

Trimmed CLAUDE.md back under its per-turn size limit (202KB → 73KB, docs
only, no code): the round bullets from 2026-07-10 through the `5cc76688`
round (2026-07-30) and the whole superseded oracle-baseline history chain
moved verbatim (diff-verified) to
`docs/developer/porting/claude-md-status-history.md`, replaced in
CLAUDE.md by a compressed digest bullet and a pointer. Rounds from P4.D29
(2026-07-30) onward and the current `03154b72` baseline paragraph stay in
place. Second application of the 2026-07-10 precedent (the unit journal's
move to `status-log.md`); going forward, displaced baseline paragraphs
and aged-out round bullets migrate to the archive instead of chaining.

#### `cb0c4aab` — 2026-08-13 — docs: plan the 4.8.2/4.8.3 drift catch-up round — seven work orders (P4.D71–P4.D76 ∥ P4.46), round record, CHANGELOG

_Docs-only change._

Planned the 4.8.2/4.8.3 drift catch-up round and wrote seven work orders
(docs only, no code): v4 main moved `03154b72` → `48396682` with releases
4.8.2 + 4.8.3 — the group/shared wardrobe tiers + bundle dissolution +
v4's fix for Bug 61 (the staged-edit race this port filed), the three
composer features (smart typography, emoji, Unicode) with their three new
`chat_settings` columns, the bug-64/65 lifecycle fixes, and an SDK
refresh. Orders: P4.D71 (wardrobe server) ∥ P4.D72 (wardrobe SPA, closes
dogfood finding #78) ∥ P4.D73 (settings columns + D23 re-dump) ∥ P4.D74
(smart-typography/renderer SPA + bugs 62/63 + katex) ∥ P4.D75
(char-insert SPA) ∥ P4.46 (lock-before-open reshape + setup hardening —
the standing P4.D68 escalation, widened by survey to unlock and Setup) ∥
P4.D76 (provider SDK wire re-check). Round record in the status log; the
oracle baseline moves to `48396682` when the round unifies.

#### `601371d0` — 2026-08-12 — docs: the 4.8.1-drift round record — baseline → 03154b72, all three orders closed, the open-before-lock escalation ordered next, gate numbers

_Docs-only change._

Unified the `03154b72` 4.8.1-release drift catch-up round (P4.D68 ∥
P4.D69 ∥ P4.D70, plus the parallel wardrobe-flow deflake that was sitting
uncommitted in its worktree) onto main — the oracle baseline moves to
`03154b72` and the drift debt is cleared. v4 released 4.8.0 and 4.8.1 and
now develops on two branches (main + bugfix); drift-checks widen to both.
Landed: the bug-60 one-file dbkey port (the phantom
`quilltap-llm-logs.dbkey` write shed; the cross-compat oracle grown to
both directions with mutation-proven cross-side tripwires), the bug-59
measured convergence + fail-closed seed pin, the bug-58 NO-PORT with the
writable-open lock enumeration — whose one escalation (v5 boots the
databases writable BEFORE taking the instance lock) is now the top named
next-round candidate — the repo-wide spelling sweep wired into the
workspace gate, the `db characters` shell completions (Tier R red-first →
188/0), the standalone streaming indicator + About release-freshness
mirror, and the wardrobe `set_all` beat deflaked (the lost-edit race
measured at 3 ms, kept v4-faithful, filed upstream as v4 Bug 61). The §3
review found no blocking defects. Gate: 426 test binaries / 2,017 tests /
0 failed with the round's env block (families by name, zero SKIP, fresh
at the pin); clippy both feature sets; release build; ng test 298 files /
4,142 / 0; ng build clean; full Playwright 202/202 zero skips (5.1 m).
Versions: core 0.0.531, harness 0.0.454, cli 0.0.9, SPA 0.5.454.

#### `f5e2ac7b` — 2026-08-12 — docs(about): mirror v4's 4.8.0 release-freshness sweep of the feature list (P4.D70 unit 2)

_Versions: SPA 0.5.453._

P4.D70 unit 2: mirrored v4 `5fdd7bed`'s About release-freshness sweep into
the SPA's Key Features list — the Docker bullet notes the filesystem
document stores bound through to the container, a new "The Workspace"
bullet lands before Aurora, Aurora gains encrypted-bundle archiving,
Pascal is rewritten around four state registers plus user-authored custom
tools and the Workbench, and the Foundry gains the Almanack. Every named
feature is ported in v5, so the new text is truthful here. Pinned by spec
(static template prose has no other guard against rot); v5 keeps reporting
its own version, not v4's.

#### `aefbcc56` — 2026-08-12 — fix(salon): give the streaming thinking indicator room above a tool block (P4.D70 unit 1)

_Versions: SPA 0.5.452._

P4.D70 unit 1: re-ported v4 `fed5b5da` — the streaming "still working" quill
now takes a line of its own (with the tool block's own top spacing) when the
trailing prose segment is empty and a tool batch precedes it, instead of
rendering inline where the quill's feather crowds the block above it. Inline
placement mid-prose is unchanged. v4 splits the live prose into an
interleaved parts array to ask that question; v5 renders one prose blob
followed by the batches, so the predicate reads the same batch offsets
directly. Both arms pinned by spec, mutation-checked.

#### `7881e5f0` — 2026-08-12 — fix(cli): teach shell completions the db-characters sub-subverbs (P4.D69, v4 db195fba)

_Versions: cli 0.0.9._

P4.D69 (the 4.8.1 CLI shell-completions drift): mirrored v4 `db195fba`'s
`db characters` completion arms into all three shell templates
(bash/fish/zsh) in `quilltap-cli` — the five sub-subverbs
(status/archives/archive/rehydrate/export) and their flags, including
zsh's early `return` after the subverb `_describe`. Byte-copied from a
worktree pinned at v4 `03154b72`; Tier R flipped exactly the three
`completion <shell>` cases red before the fix (188 cases / 3 failures)
and runs green after (188 / 0). quilltap-cli 0.0.8 → 0.0.9.

#### `4f8778a2` — 2026-08-12 — docs(v4): mirror the 4.8.1 bug write-ups + dbkey doc corrections; record the baseline-move neutrality proof (P4.D68 unit 5)

_Docs-only change._

P4.D68 unit 5 — the baseline-move neutrality proof + the docs mirror.
The two seed-transitive oracle families (`reset_builtins_equivalence`,
`seed_avatars_equivalence` — the only cases importing v4's
`seed-initial-data`) regenerated and re-ran green at the `03154b72` pin
through the sweep driver, proving the bug-59 drift happy-path-neutral;
the oracle baseline moves to `03154b72` at unification. Mirrored into
`docs/v4/`: the three 4.8.1 bug write-ups (a new `bugs/fixed` mirror
subtree), the bug-60 corrections to DATABASE_ENCRYPTION / DDL /
BACKUP-RESTORE / DEPLOYMENT, and `help/database-protection.md` (a new
`help/` mirror subtree), with that help doc joining the `p4.9i2` bank
by name.

#### `516586da` — 2026-08-12 — feat(tooling): the repo-wide Quilltap spelling sweep, wired into the workspace gate (P4.D68 unit 4)

_Versions: harness 0.0.454._

P4.D68 unit 4 — mechanical spelling enforcement. The v5 analog of v4
4.8.1's repo-wide checker: `harness/tools/check_spelling.py` fails on
any case-insensitive quilt-based misspelling of "Quilltap" in tracked
text files outside a reasoned allowlist (rule-stating docs, the `docs/v4`
mirror, a correctly-spelled line-exception marker), and a new
`quilltap-harness` test (`spelling_guard`) runs it under
`cargo test --workspace`. The first sweep found five closed work orders
quoting the misspelling to state the rule — allowlisted, text untouched.

#### `d5872b69` — 2026-08-12 — fix(seed): pin the bug-59 fail-closed gate; record the bug-58 lock enumeration + escalation (P4.D68 units 2-3)

_Versions: core 0.0.531._

P4.D68 units 2–3 — the bug-59 and bug-58 dispositions, measured. Bug 59
(v4 4.8.1: a failed read must not trigger first-startup seeding) is a
structural convergence: v5's sample-content gate already fails closed
(`character_count` returns `Result`; the `Err` arm warns and returns
without seeding) and the embedding-profile seed runs only inside fresh
provisioning — no code change; a new regression pin
(`failed_gate_probe_seeds_nothing`) ports the intent of v4's
probe-throws test (a populated-but-unreadable database seeds nothing).
Bug 58 (migrations bypass the instance lock) is NO-PORT — v5 has no
migration runner; the lock-coverage enumeration confirmed every v5
writable entrance sits behind the instance lock (host/web/tauri boot,
the spine's in-process writers, the CLI's no-override write lock, the
CLI archive verbs through the running server), and v5's lock semantics
match v4's contract (re-entrant same-PID, same-host dead-PID reap, VM
heartbeat freshness). ONE escalation recorded in the order header: the
boot path opens the databases writable (journal-mode header writes)
BEFORE the lock is acquired, where v4 locks before opening — needs its
own small order.

#### `aa55e100` — 2026-08-12 — fix(dbkey): one pepper, one file — change_passphrase sheds the phantom quilltap-llm-logs.dbkey (v4 4.8.1 bug 60, P4.D68 unit 1)

_Versions: core 0.0.530, harness 0.0.453._

P4.D68 unit 1 — the bug-60 port: `change_passphrase` writes one `.dbkey`
file. v4 4.8.1 removed the vestigial `quilltap-llm-logs.dbkey` write (the
remnant of a per-database-key design that was never built; nothing ever
read it, and it could hold a stale wrapping), and v5 follows: the second
write in `quilltap-core::dbkey::change_passphrase` is gone, a pre-existing
stale file on disk is left untouched (v4 parity — no deletion), and the
doc comment now carries v4's one-pepper-one-file reasoning. Proven at the
new `03154b72` pin by the extended dbkey cross-compat differential, now
BOTH directions: v4's REAL `changePassphrase` (driven fresh in the pinned
worktree via the new `QT_DBKEY_V4_OUT` leg of
`verify-dbkey-crosscompat.ts`) writes one file that v5's reader unlocks
(new `reads_v4_changed_passphrase_dbkey`), and v5's rewrap writes one
file that v4's real code unlocks — with the one-file outcome asserted on
both sides of the fence and mutation-proven (re-adding the second write
turns four Rust arms and the oracle assertion red). The
`archive_reencrypt_tier2_equivalence` family regenerated fresh at the pin
(its comparands never observe the data-dir file set — recorded; the
one-file arm lives in the dbkey vehicle) and the
`change_passphrase_archive_sweep` wire test re-ran green.

#### `32bde325` — 2026-08-12 — docs: work orders for the 03154b72 4.8.1-release drift catch-up round (P4.D68 ∥ P4.D69 ∥ P4.D70)

_Docs-only change._

Planned the `03154b72` 4.8.1-release drift catch-up round and committed
its three work orders (docs-only; no code moved). v4 released 4.8.0 and
4.8.1 and its main moved eight commits past `de9f70bf`; the effective
lib/app drift is v4's bugs 58–60 (instance lock, fail-closed seeding, the
phantom `quilltap-llm-logs.dbkey` — which v5's `change_passphrase`
reproduces faithfully and must now shed), the `db characters` shell
completions, and two client fixes (the standalone streaming indicator
above a tool block; About release-freshness). Orders:
`work-orders/p4.d68-dbkey-onefile-seed-lock-drift.md` (server; owns the
baseline move to `03154b72`), `p4.d69-cli-completions-archive-drift.md`,
`p4.d70-streaming-indicator-about-spa-drift.md`. Operational hazard
recorded in all three: the v4 checkout now sits on the `bugfix` branch
(4.8.2-bugfix.0), so every oracle regen this round pins a detached
worktree at `03154b72`, and future drift-checks must watch both v4
branches.

#### `e8e90e12` — 2026-08-11 — docs: the P4.D65-finish + sweep-rot round record — baseline → de9f70bf, P4.D63 + P4.45 closed, gate numbers

_Docs-only change._

Unified the P4.D65-finish + sweep-rot round (P4.D65-resumed ∥ P4.45) onto
main — the oracle baseline moves to `de9f70bf` and the Bug-57 drift debt is
cleared (v4 converged onto this port's twice-linked-blob rehydrate dedupe;
the divergence pins retired to plain equalities, with the fixture-driven
equality arm mutation-proven). P4.D63 and P4.45 are CLOSED; P4.D65 stays
open on items 5–6 only (the banked round-1 tier-2 arms and the owed corpus
arms). The unification review fixed two things before merge: the
re-encryption sweep's upload-failure reason now carries v4's `uploadRaw`
wrapper sentence (it leaked the bare backend error into a string the
settings UI surfaces verbatim — pinned by a failing-upload unit test), and
the archive-holder lookup's error arm now answers v4's fixed
`Failed to delete file` 500 instead of raw database error text. Gate: 425
test binaries / 2,013 tests / 0 failed with the round's env block; the 20
affected differential families regenerated fresh at `de9f70bf` and run by
name through the repaired sweep driver, zero skips; clippy both feature
sets; release build; ng test 298 files / 4,138; full Playwright 202/202
zero skips. Versions: core 0.0.529, harness 0.0.452, host 0.0.66, web
0.0.70; cli, tauri, SPA unchanged.

#### `746a7cb1` — 2026-08-11 — fix(harness): the green proof over all 39 repaired families, and the four broken recipes it found (P4.45 unit 4)

_Versions: harness 0.0.451._

Proved all 39 repaired differential families runnable end-to-end through the
sweep driver — regenerate the oracle from v4, then run the diff, one clean
invocation each — and committed the per-family results. The run exposed four
more broken recipes and fixed them: one that annotated a command line with a
parenthesis (bash died on it), one that expected a temp-built fixture to
"already exist" and so was dead the first time the temp directory was cleaned,
and two whose regeneration step was silently dropped because it invoked its
tool by full path, which the extractor did not recognize as a command. A fifth
suspected class — a recipe reading a fixture no step of it builds — is recorded
with its candidates rather than half-detected. Developer tooling only.

#### `300708f3` — 2026-08-11 — fix(harness): the sweep driver's stale-oracle deletion covers jest families too (P4.45 unit 3b)

_Versions: harness 0.0.450._

Closed a hole in the sweep driver's own stale-oracle guard: it deleted a
family's previous oracle only when the recipe wrote it through a shell
redirect, which is the tsx convention. Every jest-based family writes through
`QT_ORACLE_OUT=`, so its old oracle survived — and a regen that quietly
produced nothing would let the run pass against a previous round's file. That
covers the majority of the database-level and mocked-LLM families.

#### `de19bafe` — 2026-08-11 — fix(harness): the sweep driver refuses an unattributable run line (P4.45 unit 3)

_Versions: harness 0.0.449._

Made the sweep driver refuse an unattributable recipe outright: a family whose
run line does not name its own test binary is now reported as broken and will
not execute, so the skip-masquerade cannot be reintroduced silently. A
positional test-name filter does not count as a scope — it matches across
every binary in the crate. Mutation-proven by reverting a repaired header in
place and watching the refusal fire.

#### `d19d915d` — 2026-08-11 — fix(harness): scope every differential recipe's cargo-test line to its own binary (P4.45 unit 2)

_Versions: harness 0.0.448._

Scoped every differential family's `cargo test` recipe line to its own test
binary. Thirty-two families' run lines read `cargo test -p quilltap-harness`
with no `--test <family>`, so following the recipe compiled and ran EVERY
harness test binary with only one family's oracle env var set: every sibling
family found its own env var missing, printed its skip notice and passed. A
run like that proves nothing about the family whose recipe it is, and the
sweep driver's fail-on-skip guard could not tell whose skip it was seeing —
which is why three consecutive rounds had to re-run these families by hand.
Developer tooling only — no test body, fixture or oracle case changed.

#### `674a27c4` — 2026-08-11 — fix(harness): the sweep driver reads recipe lines by INDENTATION, not by first word (P4.45 unit 1)

_No crate versions bumped._

Fixed the sweep driver's prose-leak class at its root: a differential
family's regeneration recipe is now recognized by INDENTATION (a command
sits two spaces past `//!` or `*`; prose sits at the marker's margin)
rather than by guessing from the line's first word. Twenty-one doc
sentences opening with a command word were being extracted as shell, six of
them into regen scripts that would die on a bash syntax error. The one
header that used a markdown code fence instead of indentation
(`memory_weighting_equivalence`) was converted to the tree's convention.
Developer tooling only — no shipped behavior changed.

#### `012d0301` — 2026-08-11 — feat(archive): the passphrase-change re-encryption sweep, the held-bundle delete guard, and the export archive keys

_Versions: core 0.0.528, harness 0.0.447, host 0.0.66, web 0.0.70._

Finished P4.D65's resume list. Changing the instance passphrase now
re-encrypts the archive library with it: the sweep runs as phase two of
`POST /api/v1/system/unlock?action=change-passphrase`, and the response
carries an `archives` summary naming any bundle left holding the old
passphrase. A failed sweep does not fail the passphrase change. Deleting a
held archive bundle is refused (`ARCHIVE_BUNDLE_HELD`, naming the character
whose only copy it is) unless `force=true`. Archived characters no longer
appear in the export picker. Exported character records carry the three
archive columns in v4's schema slot — the committed key-order table had
been stale since the columns landed, so the keys were being appended at the
end of every character record. Two real defects were caught by the new
differentials: the re-encryption sweep would have panicked the moment it
was called from the async dispatch arm (it blocked on the writer channel),
and the export key order above. New: a six-case archive re-encryption
differential over v4's real sweep (planted plaintext / foreign-passphrase /
missing-bytes bundles, plus one the archive service really wrote), a
live web-edge test that archives, changes the passphrase and rehydrates,
and five new arms on the archive differential. v4's Bug-57 fix
(`de9f70bf`) converged onto this port's twice-linked-blob dedupe, so the
divergence markers retire and the fixture grew the shape as a
plain-equality arm; the oracle baseline moves to `de9f70bf`.

#### `6917fc9b` — 2026-08-11 — docs: plan the P4.D65-finish + sweep-rot round (the de9f70bf Bug-57 convergence folds into P4.D65; P4.45 written)

_Docs-only change._

Planned the P4.D65-finish + sweep-rot round (docs only). v4 fixed its Bug
57 at `de9f70bf` — converging onto this port's twice-linked-blob rehydrate
dedupe — so the drift folds into P4.D65's resumed lane as a Round-3
addendum (pin retirement, a plain-equality fixture arm, import-graph
regens, baseline move to `de9f70bf`; zero v5 source change needed —
verified at planning). A new parallel maintenance order, P4.45, repairs
the sweep-driver recipe rot: the six turn families' unscoped run lines
(the SKIP-masquerade), the `diff`-prose classifier trap, self-test growth,
and the 30-file census. Round-planned record in the status log.

#### `5ecaff36` — 2026-08-11 — fix(cli): §3 review — propagate swallowed SQL errors, the falsy default sub, the Tier R wire-parity assertion; round docs

_Versions: cli 0.0.8._

Unified round 2 of the character-archive catch-up (P4.D65 ∥ P4.D66 ∥
P4.D67) onto main — the oracle baseline moves to `ed8934f1` and the Bug-56
drift debt is cleared. The archive service is LIVE end-to-end: archive
packs a character into an encrypted bundle, verifies it, commits the
tombstone and prunes the vault in place; rehydrate brings them back at
their original ids; both dispatch verbs and the SPA's four action beats
run live (the archive e2e spec is 10/10). The CLI gained the whole
`db characters` family (status / archives / archive / rehydrate / export
with offline bundle decrypt), Tier R 136 → 188 cases against v4's real
launcher. The Bug-56 base-path-availability module landed with byte-exact
diagnosis sentences, the folder-create 409, and the store-create warning
rewrite. The unification review caught and fixed, before merge: the
round's cross-lane blind spot (no lane served the CLI's
`POST /api/v1/characters/{id}?action=` URL on v5's server — a thin REST
edge now delegates into the dispatch arms, pinned by a live web-edge
test); a missing character answering 500 where v4's route answers 404;
four CLI sites swallowing SQL errors v4 propagates; the archive
differential's blindness to `background_jobs`; and one deliberate
divergence with the v4-side fix queued — v4 cannot rehydrate a character
whose vault links the same bytes twice (per-link blob export duplication ×
an undeduped preflight list), while v5's preflight now dedupes carried
blob ids. Gate: 423 test binaries / 2,010 tests / 0 failed; the round's
differentials fresh at `ed8934f1`; clippy both feature sets; release
build; ng test 4,138; full Playwright green with the four archive action
beats active. P4.D66 and P4.D67 close; P4.D65 stays OPEN at its resume
list (re-encrypt wire, files-delete guard, export filter, banked arms).
Versions: core 0.0.526, harness 0.0.446, cli 0.0.8, web 0.0.69,
SPA 0.5.451.

#### `c0d0cd97` — 2026-08-11 — feat(archive): the character-archive service + the two dispatch verbs (P4.D65 unit 1)

_Versions: core 0.0.523, harness 0.0.444._

Ported the character-archive service (P4.D65 unit 1): `archiveCharacter`
and `rehydrateCharacter` now work end to end, and the two dispatch verbs
`characterArchive` / `characterRehydrate` no longer refuse. Archiving
packs the character into an encrypted `.qtap` bundle, verifies it by
decrypting the bytes that will actually be persisted, commits the
tombstone, and prunes the vault in place — keeping the ten managed
documents, the wardrobe and the avatar links, so an archived character is
still a readable page and old messages keep their faces. Rehydrating
restores the pruned material at its original ids, un-flags the tombstone,
brings absent chat seats back, and re-chunks the vault. New committed
`character-archive-{main,mount}.db` fixture family and an eight-case
tier-2 differential against v4's real archive service, diffing the
returned result, the DECRYPTED bundle, and every table in both
partitions. (Ciphertext is never compared — a fresh salt and IV per
bundle make it nondeterministic on both sides.)
P4.D67 closed — the `ed8934f1` (Bug 56) base-path-availability drift
catch-up is complete and clears the drift debt; the oracle baseline
moves to `ed8934f1` at unification and the `d553f72a` mount-points
regen pin retires.

#### `63e773da` — 2026-08-11 — docs(running): the Docker bind-mount property for filesystem stores (P4.D67 tier 2)

_Docs-only change._

P4.D67 tier 2 — documented the Docker bind-mount property in
`docs/developer/running.md`: filesystem and Obsidian document stores are
invisible inside a container unless bound in at creation, the failure is
quiet (the folder listing comes from the cached mount index, so only
byte-touching operations notice), and the Bug-56 409 diagnosis is the
runtime symptom. Notes the same-path-both-sides requirement, the
non-root user, Docker's fabricated bind sources, and that v5 has no
equivalent of v4's bind planner (it banks with the standing
`quilltap docs` CLI deferral). Docs only.

#### `4bf6ea35` — 2026-08-11 — port(mount-index): wire the base-path check into all three consumers (P4.D67 unit 2)

_Versions: core 0.0.524._

P4.D67 unit 2 — wired the base-path-availability check into the three v4
consumers (`ed8934f1`, Bug 56). Creating a folder in a filesystem store
whose own root is unreachable now answers 409 with the diagnosis instead
of letting the recursive mkdir walk up to the topmost missing ancestor
and fabricate the entire chain; `verify_base_path` is deleted, and
store creation's warning is the diagnosis plus "The store was created,
but scanning will fail until the path is reachable." — so a store on a
real directory now creates with no warning at all, where v5 previously
warned unconditionally. Both mount differentials gained planted
reachability arms (missing / denied / not-a-directory / available) and
were regenerated at `ed8934f1`: mount-points-routes 15 → 19 cases,
mount-ops 39 → 42.

#### `543ae5e2` — 2026-08-11 — port(mount-index): the base-path-availability module (P4.D67 unit 1)

_No crate versions bumped._

P4.D67 unit 1 — ported v4 `ed8934f1`'s new
`lib/mount-index/base-path-availability.ts` as
`services/mount_index/base_path_availability.rs`: the never-failing
`check_base_path_availability` (missing / denied / not-a-directory), the
`assert_base_path_available` refusal carrying the diagnosis, and the
byte-exact operator-facing sentences including the containerized
variants (pinned by unit tests — neither test environment is a
container). No caller yet; the wire lands with the route arms.
Docs — P4.D66's order status header closed out: what landed (the whole
`db characters` family, Tier 1 and Tier 2 both), the one deferral that is not
from this family (`docs docker-mounts`), and the two places the lane
deliberately departs from the order's text (v4's launcher has its own
passphrase ladder; the archive/rehydrate arms need no ACTIVATE-AT-UNIFY gate
because one stub answers both CLIs). Docs only; no version bumps.

#### `1a547c3c` — 2026-08-11 — cli(characters): port the whole `db characters` family (v4 ed8934f1)

_Versions: cli 0.0.7._

CLI — ported the whole `quilltap db characters` family (v4 `ed8934f1`):
`status`, `archives`, `archive`, `rehydrate`, `export`. This is the first
`db` verb v5 ships at all, so it also lands v4's verb-path entrance
(resolve → instance hint → dbkey unlock → dispatch) and its error contract
(`Error: <message>`; exit 2 on an ambiguous name). `status` carries the full
vault-readiness report including the divergence pass; `archives` lists the
shelf with loose bundles; `export` decrypts an archived character's bundle
offline through core's archive crypto — the only way to reach packed-away
mail, photographs and summaries without rehydrating — and proxies live
characters to the server. `archive`/`rehydrate` proxy the same v4 URLs.
Two v4 quirks ported deliberately and pinned: `characters --json status`
runs the default sub with JSON off, and `db --json characters` reports an
unknown subcommand. The Tier R differential goes 136 → 188 cases, 0
failures, over a new 17-character fixture with eight distinct archived-export
arms; the new arms are mutation-proven and carry a permanent coverage guard.

#### `592fcbea` — 2026-08-11 — cli(drift): re-capture the db/docs help + completion templates at v4 ed8934f1

_Versions: cli 0.0.6._

CLI — absorbed v4's `ed8934f1` text drift into `quilltap-cli`. `db --help`
picked up the fourteen `characters archives|archive|rehydrate|export` lines
from the round-1 archive commit; `docs --help` and all three shell-completion
templates picked up `docs docker-mounts` from the Bug-56 commit. All five
files re-captured byte-for-byte from v4's real launcher and templates. The
`docs docker-mounts` verb itself is recognized and refuses loudly by name (its
bind planner is unported); `--format` is parsed so that flag reaches the
refusal. The Tier R CLI differential goes 136 cases / 7 failures → 0 at the
new baseline.

#### `6bdb9e9c` — 2026-08-11 — Docs: plan the character-archive round-2 + Bug-56 drift round (P4.D65 ∥ P4.D66 ∥ P4.D67)

_Docs-only change._

Planned the character-archive round 2 + the `ed8934f1` (Bug 56)
drift-catch-up round: three work orders committed under
`docs/developer/porting/work-orders/` — P4.D65 (the archive service,
verbs, re-encrypt wire, export carry, and the round-1-banked oracle
arms; closes P4.D63 on completion), P4.D66 (the CLI `db characters`
family, Tier R), and P4.D67 (the Bug-56 base-path-availability port
slice, which clears the drift debt and moves the oracle baseline to
`ed8934f1` at unification). Round record in `status-log.md` ("Round
planned — character-archive ROUND 2 + the `ed8934f1` Bug-56 drift
catch-up"). Docs only; no version bumps.

#### `02b8ce57` — 2026-08-11 — Docs: the character-archive round-1 record, order status headers, CHANGELOG, phase-4 + CLAUDE.md baseline move to d553f72a

_Docs-only change._

Unified round 1 of the character-archive drift catch-up (P4.D62 ∥ P4.D63
∥ P4.D64) onto main — the oracle baseline moves to `d553f72a`. Server:
the whole `.qtap` preserveIds substrate (vault-carrying character
exports with carried row ids, the 16-kind refuse-on-collision preflight
with the rehydrate-only skip-if-present mode, the Bug-52 avatar remap,
Bug-54 sha256 dedup, Bug-55 typed missing-content 404s), the three
archive columns (D23 re-dump + boot ensure + read tolerance), the write
guard + the `archived=` list chokepoint + every turn/tool/mail refusal
arm, the archive bundle crypto (byte-exact format, 17-arm tier-1
differential) with the engine-held runtime passphrase cache, the
wipe/restore spare-bundle options, and the two round-2 verbs defined
refusal-armed. SPA: the whole client surface — roster toggle/badges/
sort, the read-only tombstone page with both dialogs, group and seat
badges, and the four settings surfaces — with six tombstone-read e2e
beats live over a seeded archived character and four action beats gated
for round 2. The unification review fixed six findings before merge
(the headline: the one-default embedding rule had leaked into help-doc
sync, where v4 keeps the first-profile fallback), and the beats' first
live runs surfaced a v4-side bug now recorded for filing upstream (the
archived-seat sidebar badge cannot light on a fresh load in v4 — the
chat GET's enrichment never got `archivedAt`). The archive service
itself, the API actions, and the CLI subcommands are round 2; the
`ed8934f1` (Bug 56) drift catch-up is owed. Gate: 421 test binaries /
1,997 / 0; 25 oracle families fresh at the `d553f72a` pin by name; clippy
both feature sets; release build; ng test 4,138 / 0; full Playwright 198
passed / 4 gated skips / 0 failed. Versions: core 0.0.522, harness
0.0.443, web 0.0.68, host 0.0.65, SPA 0.5.450.

#### `a78e8f90` — 2026-08-10 — Widen system-data-* so the archive substrate's arms are measured (P4.D62)

_Versions: core 0.0.521, harness 0.0.442._

Widened the shared test fixture so the archive substrate's new behavior is
actually measured rather than merely present: a nested vault folder, a
document shared by two characters' vaults, a real portrait blob with the
avatar pointers that exercise the remap, and archive-category files. No
shipped behavior changed, but one real port bug surfaced with it — the export
PREVIEW was excluding archive bundles where v4 still lists them.

#### `89d86972` — 2026-08-10 — Answer 404 when a file row outlives its bytes (P4.D62, v4 bug 55)

_Versions: core 0.0.520, host 0.0.64, web 0.0.67._

Serving a file whose bytes have gone missing now answers 404 instead of 500
(v4 bug 55). A `files` row can outlive its content — a deleted mount point, a
storage key with nothing at that path — and answering a server error on every
render invited retries that could never work while burying genuine storage
faults. Both file routes now tell the two apart; every other failure still
500s, unchanged.

#### `677f1ea5` — 2026-08-10 — feat(archive): the character-archive schema, guards, chokepoint and crypto (P4.D63 units 1-6, 8-11)

_Versions: core 0.0.522, harness 0.0.443, host 0.0.65, web 0.0.68._

Ported the `.qtap` export/import substrate the character archive is built
on (v4 `01e481f6` + `d553f72a`, work order P4.D62). A `characters` export now
carries each character's whole vault — mount point, folders, documents and
blobs — so a cross-instance import no longer lands a faceless, mail-less
character; export records carry their source row ids; character-archive
bundles and anything under `/archives` are excluded from exports alongside
backups; and the export preview reports the vault weight a bundle will add.
The import side gained the `preserveIds` path: a bundle can be restored at
its original ids, refusing the whole import on any collision, or — for
rehydration only — skipping ids that already exist inside the target
character's own vault. Content rows and blobs settle that by content hash
first, so a conversation summary shared with a group chat no longer blocks a
rehydrate (v4 bug 54). Imported characters are repointed at the vault their
bundle carried and the placeholder vault is discarded, and avatar pointers
are remapped through it — a dangling default image is cleared and an
unresolvable per-chat override is dropped, each with a warning (v4 bug 52).
Imported memories are now embedded under the default profile whatever its
provider, instead of being left unembedded under the built-in one. Folder
parents on an ordinary import resolve by path rather than keeping a source
id that never existed here.
Landed P4.D63 units 1-6 (the character-archive schema, guards, chokepoint and
crypto) against v4 `d553f72a`. Adopted the three new `characters` columns via a
D23 re-dump of v4's live generateDDL — `archivedAt`, `archiveFileId`,
`archivedAvatarFileId` — with a boot repair pass that adds them to any existing
instance and pragma-guarded read tolerance so a pre-drift database still opens.
An archived character is now a tombstone: the repository refuses every write to
one except the single-key unarchive, the wardrobe write path refuses rather than
silently falling back to the legacy table, and the read overlay deliberately does
NOT short-circuit (an archived character hydrates exactly like a live one, since
archiving prunes the vault in place). Added the `archived=` list filter as the
single chokepoint — every picker and roster excludes tombstones by default, with
`include`/`only` to opt in — plus `archivedAt` on list items, chat participants
and group members, refusals for adding an archived character to a group or
roster, and a refusal for exporting one. The turn selection drops an archived
seat even if its status somehow stayed active, and the participant resolver,
Carina probe, character resolver, self-inventory, both mail tools, both mail chat
actions and the doc-edit self-vault resolver all refuse or skip archived
characters. Conversation summaries are no longer written into an archived
character's vault. Ported the archive bundle crypto byte-for-byte (PBKDF2-SHA256
at 600k iterations into AES-256-GCM, under the instance passphrase and never the
database pepper, so a bundle stays readable after a restore onto a new instance)
with its four typed errors, and added the runtime passphrase cache the engine had
never had, deposited at all four `.dbkey` chokepoints and cleared on lock.

Also landed the rest of P4.D63's server surface. Deleting all data and
restoring in replace mode now spare archived-character bundles by default —
the character rows go, so what survives is a loose importable bundle, and only
an explicit `keepArchivedCharacterBundles: false` wipes them too; the delete
summary reports how many were on hand and whether they were kept. Defined the
`characterArchive` and `characterRehydrate` verbs, which refuse loudly by name
until the archive service lands in round 2, and added an exact-match
`category` filter to the files list. Tightened the embedding rule at all five
sites that pick a profile: only a profile actually marked default counts, with
no fallback to an arbitrary one, so chunks wait rather than embedding into a
different vector space than everything else. **Still owed under this order:**
the passphrase-change re-encryption sweep is ported but not yet wired to the
change-passphrase response, so a passphrase change does not yet rewrite
archive bundles — see the work order's resume list.
P4.D64 unit 6: end-to-end beats for the archive, plus the fixture that feeds
them. Global setup seeds a tombstoned character (Marchpane) with a group and
a conversation that hold it, so the read surfaces — the roster toggle,
badges, the read-only page, the group's can-speak line, the archived seat —
have something real to walk. The seeder probes for the archive column and
writes nothing without it, keeping it inert until the schema lands. Six read
beats activate at this round's unification; four action beats stay gated for
round 2.

#### `eff6ce32` — 2026-08-10 — feat(archive): the settings surfaces account for archive bundles (P4.D64 unit 5)

_Versions: SPA 0.5.449._

P4.D64 unit 5: the settings surfaces account for archive bundles. The
Encryption Passphrase card counts the bundles sealed under the current
passphrase and warns that each will be rewritten, then reports what the
change actually did — all rewritten, or the ones left behind named with
their reasons. Delete All Data reports the bundles on hand and offers to
leave them on the shelf (ticked by default), with a note on the completion
screen when any survive. The export wizard, on reaching its options step for
a characters export, asks what the vaults add to the trunk and says so —
advisory only, so a failed read simply says nothing. A replace-mode restore
can spare the bundles from the wipe that precedes it.

#### `f95f8436` — 2026-08-10 — feat(archive): archived members and seats say so (P4.D64 unit 4)

_Versions: SPA 0.5.448._

P4.D64 unit 4: archived characters are visible where they still belong. A
group whose roster includes an archived member now reads "3 members / 2 can
speak (1 archived)" and badges that member's row; an ordinary group's
subtitle is unchanged. In a conversation's cast, an archived seat carries an
"Archived" badge alongside "Absent" — both can show, since an archived
character is normally absent too. Every character picker was verified to
inherit the server's exclude-by-default, with a spec as the tripwire.

#### `cc3933e9` — 2026-08-10 — feat(archive): the archived character's page, read-only, with both dialogs (P4.D64 unit 3)

_Versions: SPA 0.5.447._

P4.D64 unit 3: an archived character's page is readable but inert. A banner
above the header explains what was packed away and what was kept; every tab
still renders, inside a disabled fieldset, and the Edit Character door hides
itself (a disabled fieldset cannot inert a link). The header forks: an
archived character offers one Rehydrate button in place of the live action
cluster, and a live one gains Archive at the end of it. Two new dialogs —
the archive confirmation, which itemizes what goes and what stays, and the
post-rehydrate bundle disposal, whose destructive arm is deliberately the
secondary button. All six toasts carry v4's sentences. The archive and
rehydrate actions answer P4.D63's not-yet-available refusal until round 2.

#### `694c5586` — 2026-08-10 — feat(archive): the roster shows and sorts the archive (P4.D64 unit 2)

_Versions: SPA 0.5.446._

P4.D64 unit 2: the character roster shows the archive. A "Show Archived"
toggle leads the toolbar (folder icon, v4 labels and tooltips both ways);
its two states fetch and cache separately, and every character mutation now
refreshes both. Archived characters sort to the very end of the shelf ahead
of every other ordering rule, wear an "Archived" badge dated to when they
were put away, lose the favorite / Carina / control toggles, and trade Chat
and both export actions for one inert "Resting in the archive" note —
Delete stays. Sort rule 0 and the distinct-cache-key refetch were
mutation-proven.

#### `1bf6159c` — 2026-08-10 — feat(archive): mirror the character-archive client contract + data layer (P4.D64 unit 1)

_Versions: SPA 0.5.445._

P4.D64 unit 1: mirrored the character-archive client contract (v4
`d553f72a`) and added the archive data layer. `core-contract.ts` gains the
`archived` list filter, `archivedAt` on the character list/detail DTOs plus
`archiveFileId`, `archivedAt` on group members and chat-participant
characters, `keepArchivedCharacterBundles` on delete-data and
restore-execute, the files-list `category` filter, the two new
`characterArchive`/`characterRehydrate` verbs, and the
`ArchiveReencryptSummary` / `ExportVaultPreview` shapes; the stale "v4 has
no export-preview route" comment is reconciled. `characters.api.ts` gains
`archiveCharacter`, `rehydrateCharacter`, `deleteArchiveBundle`,
`countArchiveBundles`, the archived list filter, and distinct cache keys per
archived-filter state. The two verbs answer P4.D63's not-yet-available
refusal this round; round 2 fills them.

#### `e85adb7c` — 2026-08-10 — Docs: plan the character-archive drift catch-up, round 1 of 2 (P4.D62 ∥ P4.D63 ∥ P4.D64)

_Docs-only change._

Planned the character-archive drift catch-up (v4 `f6eac168` →
`d553f72a`): round 1 of 2, three work orders committed —
`p4.d62-export-import-archive-substrate.md` (export fidelity, preserveIds,
Bugs 52/54/55), `p4.d63-archive-schema-guards-crypto.md` (the D23
three-column re-dump, write guards, the `archived=` chokepoint, archive
crypto + re-encrypt, wipe/restore options), and `p4.d64-archive-spa.md`
(the whole client surface, action beats gated). The archive service, API
actions, and CLI are round 2 (scope recorded in the status log). Docs
only; no version bumps.

#### `2d38535b` — 2026-08-08 — Docs: the P4.D60∥P4.D61∥P4.44 round record, order status headers, CHANGELOG entry, phase-4 + CLAUDE.md baseline move to f6eac168

_Docs-only change._

Unified the `f6eac168` drift catch-up round (P4.D60 ∥ P4.D61 ∥ P4.44) onto
main — all three orders closed; the oracle baseline moves to `f6eac168`
(v4 Bugs 47-51, filed from this port's own dogfood walk). Server: the
fair-rotation first-responder pause (a fresh user send now pauses for
another user-driven seat instead of forcing the sole LLM to answer every
human turn), the Brahma budget-exhaustion salvage in both paths (a run
that spends its budget now always answers and signals completion), the
chat GET projects `impersonatingParticipantIds`/`activeTypingParticipantId`
(a reload restores impersonation state), and the five-copy
impersonating-ids reader consolidation. SPA: impersonating hands the seat
the current turn (client turn override, cleared on send), the speaking-as
follows the current user-driven turn (latch-keyed), and the persisted
speaking-as seeds once instead of clobbering live state. P4.44: the
conversation-chunks upsert create arm pinned, eager per-delete/overwrite
thumbnail cleanup over StorageBackend (bug 43 tier 2 closed), and the
provider request-header pin (corpus regenerated byte-identical, 8-provider
coverage floor). The unification review fixed two spec defects before
merge: a false-green seed-once parity spec (TanStack structural sharing
kept the stub reference; now mutation-proven both directions) and the
reload beat's gesture + over-claiming assertion (the turn-follow
legitimately supersedes the seed; the beat now pins the deterministic
payoffs). Gate: 419 test binaries / 1,978 / 0; the round's seven
differentials by name zero SKIP over fresh `f6eac168` oracles; clippy both
feature sets; release build; ng test 296 files / 4,065; ng build clean;
full Playwright 192/192 zero skips. Versions: core 0.0.518, harness
0.0.440, SPA 0.5.444.

#### `059fb611` — 2026-08-08 — refactor(chat): unify the five impersonating-ids readers (P4.D60 rider)

_Versions: core 0.0.517._

Refactor: consolidated the five duplicate `impersonatingParticipantIds` JSON
extractions to one shared `db::chats_impersonation::read_impersonating` reader
(the P4.D56 §3 style note). Pure de-duplication, no behavior change.

#### `386568b9` — 2026-08-08 — port(salon): chat GET projects impersonation overlay (bug 51 server, unit 4)

_Versions: core 0.0.516, harness 0.0.437._

Fixed (v4 Bug 51, server half): the chat GET now projects
`impersonatingParticipantIds` and `activeTypingParticipantId`, so a reload (or a
mid-session server restart) restores the impersonation overlay instead of
snapping every seat back to LLM-controlled. Both keys are always present (`[]` /
`null` when unset), matching the mutation replies.

#### `b2b77330` — 2026-08-08 — port(brahma): budget-exhaustion salvage in both paths (bug 47, server unit 3)

_Versions: core 0.0.515, harness 0.0.436._

Fixed (v4 Bug 47): the Brahma Console no longer hangs silently when its turn
budget runs out. The forced final turn runs no tools, so a model that answered
it with another tool call left an empty response — saving no message and
signaling no completion. Both Brahma paths now salvage an explanatory answer
folding in the last tool result: the streaming orchestrator always finalizes
(message + done event) even with no tool data; the one-shot @Brahma path
salvages when tool data exists and otherwise falls through to its existing empty
-response failure.

#### `98ce0eb1` — 2026-08-08 — port(salon): fair-rotation pause guard in the chat spine (bug 50, server unit 2)

_Versions: core 0.0.514._

Fixed (v4 Bug 50): in a room where the human drives two or more seats alongside
a single LLM, the LLM answered every human turn. The chat spine now runs a
fair-rotation pause guard before resolving a responder — when the rotation's
next speaker after a human post is another seat the human drives, it persists
the message and pauses for that seat (emitting the existing `user_turn`
chain-complete frame) instead of forcing the sole LLM to speak out of turn.
Single-user-seat rooms and whisper/nudge/continue turns are untouched.

#### `5ae85165` — 2026-08-08 — test(salon): impersonation take-the-turn + reload e2e beats, and AllLLMPause opener coverage (P4.D61)

_Versions: SPA 0.5.444._

Added `select_next_speaker_after_user_message` (v4 Bug 50 fair rotation): the
pure helper that projects the turn rotation one step past a user's just-typed,
unpersisted message, so a multi-seat room's first responder honors the full
roster instead of an LLM-only shortlist. Tier-1 differential extended
(`select-speaker` gains eight `select-after` cases against v4's real helper).
Tests (P4.D61): extended the impersonation e2e walk to assert Bug 48's
take-the-turn (after Speak-as the user-turn banner names the impersonated seat),
and added a reload-restores-impersonation beat gated ACTIVATE-AT-UNIFY behind
`P4D60_CHAT_GET_PROJECTION_LANDED` (it needs the sibling server lane's chat-GET
projection). The P4.D54 AllLLMPause live-opener e2e beat stays deferred (the
committed fixture has no all-LLM chat and the pause threshold needs real LLM
turns) — its opener → take-over → Bug-48 handoff is instead covered
deterministically at the unit level. SPA 0.5.444.

#### `4f2ffa61` — 2026-08-08 — fix(salon): the composer speaking-as follows the current user-driven turn (v4 Bug 49)

_Versions: SPA 0.5.443._

Fixed (v4 Bug 49; P4.D61): the composer's speaking-as now follows the current
user-driven turn — when the rotation lands on a seat the human drives (their own
character or one they are impersonating) and that seat changes, the speaking-as
defaults to it, so on the impersonated character's own turn you speak as them
without a manual switch. It is a latch keyed on the turn seat: a deliberate
same-turn Speaker choice still sticks, and a non-user/absent next speaker clears
it. Client-only (forwarded per send as `speakingAsParticipantId`; no per-turn
persistence). SPA 0.5.443.

#### `dfc5a3b5` — 2026-08-08 — fix(salon): impersonating hands the character the current turn (v4 Bug 48)

_Versions: SPA 0.5.442._

Fixed (v4 Bug 48; P4.D61): impersonating a character now hands them the current
turn — unless an LLM is mid-generation — so the composer's user-turn banner
names them and a typed message lands in turn. v5's turn is server-authoritative
and auto-refreshed, so this is a client turn override that layers above the
server-queried turn and is cleared when a message is sent (matching v4, which
recomputes the turn from history once a message is sent). Both impersonate entry
points (the sidebar and the AllLLMPause take-over) inherit it. SPA 0.5.442.

#### `d3a73428` — 2026-08-08 — test(harness): pin the provider request headers (P4.44 item 3)

_Versions: harness 0.0.440._

Fixed (v4 Bug 51, client half; P4.D61): the impersonation overlay
(`impersonatingParticipantIds` / `activeTypingParticipantId`) is now held as a
client-local mirror that the chat record only SEEDS, never overrides live. The
impersonating list re-seeds from the record only when non-empty (transitions,
including → empty, are owned by the mutation replies), and the persisted
speaking-as is seeded once while still unset — so once the sibling server lane
(P4.D60) projects those fields on the chat GET, a refetch no longer resurrects a
just-stopped impersonation nor snaps the composer back to the stale persisted
seat after each turn. The sidebar's active-typing indicator reads the same local
source (v4 feeds `impersonation.activeTypingParticipantId`, not the record). SPA
0.5.441.
P4.44 item 3 — pinned the provider request HEADERS (the P4.D55 deferral: the
vision/transport path's headers were unpinned by any family). The request-
envelope recorder now captures the outbound headers, and
`request_builder_equivalence` compares them at the post-`apply_auth` point (v5's
real `transport_headers` + `apply_auth` via `execute_completion`), a subset check
over the headers v5 models (User-Agent, HTTP-Referer/X-Title, content-type, auth,
anthropic-version), normalizing the version-bearing User-Agent and the auth
secret. The corpus widening is additive (every pre-existing method/url/body
byte-identical). One documented OpenRouter-only divergence: v4's `@openrouter/sdk`
send path overrides the User-Agent and omits X-Title, where v5's single reqwest
transport keeps both. The abort/timeout-arming half stays a loud deferral (it is
wall-clock, unobservable in the fetch-args corpus, and proven unit-tier in
`model::transport`). quilltap-harness 0.0.437.

#### `bf6d1f2f` — 2026-08-08 — feat(files): eager per-delete/overwrite thumbnail cleanup (P4.44 item 2)

_Versions: core 0.0.518, harness 0.0.439._

P4.44 item 2 — eager per-delete/overwrite thumbnail cleanup (closes the bug 43
tier-2 deferral). Deleting or overwriting a library file now eagerly removes its
cached thumbnails (`_thumbnails/{fileId}_{120,150,300}.webp`) over the host's
disk `StorageBackend`, matching v4's `cleanupThumbnails` — best-effort,
DB-invisible, never failing the delete/upload, and gated on
`canGenerateThumbnail(mimeType)`. `file_delete`/`file_upload` take an optional
backend, wired at the engine from `qtap_file_storage()`; a diskless host has no
thumbnails to reap. The daily orphan-thumbnail sweep stays the reaper for strays
that leave by any other route. The chat-file routes run no cleanup in v4, so
their twins stay unwired. quilltap-core 0.0.513, quilltap-harness 0.0.436 (the
`files_routes_equivalence` call sites pass the new optional backend).

#### `b4a05f40` — 2026-08-08 — test(harness): pin the conversation-chunks upsert CREATE arm (P4.44 item 1)

_Versions: harness 0.0.438._

P4.44 item 1 — pinned the conversation-chunks `upsert` CREATE arm. The tier-2
family only ever exercised the update arm (every corpus upsert hit an existing
`(chatId, interchangeIndex)` row); two new upserts on unseeded pairs now take
the create arm (with and without a supplied embedding). The harness placeholders
the create arm's minted id/createdAt/updatedAt and re-sorts the dump by
`(chatId, interchangeIndex)` so the comparison is independent of v4's random
minted id. No production behavior change (test + fixture only). quilltap-harness
0.0.435.

#### `1356dd06` — 2026-08-08 — Docs: plan the f6eac168 drift catch-up round — work orders P4.D60 ∥ P4.D61 ∥ P4.44

_Docs-only change._

Planned the `f6eac168` drift catch-up round (v4 Bugs 47-51) and committed
three work orders: P4.D60 (server — the fair-rotation first-responder pause,
the Brahma budget-exhaustion salvage, and the chat-GET impersonation
projection, plus the `impersonating_ids` consolidation rider), P4.D61 (SPA —
impersonate-takes-the-turn, the speaking-as turn-follow, seed-once
reconciliation, plus the AllLLMPause live-opener beat rider), and P4.44
(three pinning follow-ups: the conversation-chunks upsert create arm,
per-delete thumbnail cleanup over StorageBackend, and the vision-path
headers pin). v4 `f521fc0c` classified docs-only NO-PORT. Docs only; no
version bumps.

#### `20881bc5` — 2026-08-08 — fix(salon): let the human speak as their own character while impersonating (dogfood #77)

_Versions: SPA 0.5.440._

Fixed (dogfood #77): while impersonating a character, the human could not switch
back to speaking as their own character — the Speaking-As selector stayed hidden
and every typed message went to the impersonated seat. v5's `controlledCharacters`
listed only genuine user seats, where v4 includes impersonated seats too, so the
selector never reached its two-seat threshold. It now includes impersonated seats
(matching v4), and `onSelectSpeaker` applies the server reply to the local
speaking-as/impersonation mirrors so a pick persists (the chat GET projects
neither field). SPA 0.5.440.

#### `442bfed5` — 2026-08-08 — fix(salon): apply activeTypingParticipantId from the impersonate reply (dogfood #76)

_Versions: SPA 0.5.439._

Fixed (dogfood #76): while impersonating a character, the composer speaking-as
portrait kept showing your own character (not the impersonated one) and a
just-sent message was optimistically attributed to your character before the
server corrected it to the impersonated seat. The chat GET projects no
`activeTypingParticipantId` (v4-faithful), and v5 had no local mirror for it —
so the speaking-as resolution fell back to the owner seat while impersonating.
Added the `activeTypingLocal` mirror, applied from the impersonate/stop replies
like v4, and folded it into the active-speaker resolution. SPA 0.5.439.

#### `937ea82f` — 2026-08-08 — fix(salon): give the composer editor room; wrap the toolbar below (dogfood #75)

_Versions: SPA 0.5.438._

Fixed (dogfood #75, interim): the Salon composer editor collapsed to its width
floor and the "Type a message…" placeholder clipped to "Type a", because v5's
one-row gutter cluster (a p4.9l shortcut) plus the speaking-as avatar crammed
into the composer's max-w-4xl cap. As a band-aid pending the p4.9l composer-
toolbar port, the action cluster now takes a full-width row and wraps below the
editor+avatar line, so the editor keeps the dominant width. The proper 2-column
layout is routed to p4.9l. SPA 0.5.438.

#### `0b373829` — 2026-08-08 — fix(brahma): make the Console header sticky (dogfood #74)

_Versions: SPA 0.5.437._

Fixed (dogfood #74): the Brahma Console header (model picker + New conversation)
scrolled off the top of the workspace tab with the transcript instead of staying
put. The message-list component host defaulted to `display: block` with no flex
sizing — in v4's React DOM the `flex-1 overflow-y-auto` root is the node itself,
but Angular wraps it in the host element, so the inner scroll container had no
bounded parent and the whole tab scrolled. Gave the host `flex flex-col flex-1
min-h-0` and the inner scroller `min-h-0`; the transcript now scrolls within the
list and the header is sticky. SPA 0.5.437.

#### `ac5f457d` — 2026-08-08 — Docs: the P4.D57∥D58∥D59 round record, order status headers, CHANGELOG entry, phase-4 + CLAUDE.md baseline move to 1bed814f

_Docs-only change._

Unified the `1bed814f` drift catch-up round (P4.D57 ∥ P4.D58 ∥ P4.D59) onto
main — all three orders closed; the oracle baseline moves to `1bed814f` and
the drift debt is cleared. The Brahma Console turn budget is live end-to-end
(server accessors/resolver/verbs/REST edge + the Settings → Chat card), the
salon impersonation reconcile closes dogfood findings #71/#72 (banner gate,
optimistic-bubble attribution, the speaking-as composer portrait), and the
About-backdrop change is a recorded no-port. Unification wires: the two
brahma-console request variants folded into the SPA contract name-for-name
and the gated settings e2e beat activated. Gate: fmt/clippy both feature
sets/release build clean; full workspace tests with the round's oracles
regenerated fresh at `1bed814f` (the three families re-run by name, zero
SKIP); ng test 296 files / 4,046; ng build clean; full Playwright green zero
skips. Final versions: core 0.0.512, harness 0.0.434, web 0.0.66, SPA
0.5.436.

#### `9e344d20` — 2026-08-07 — P4.D57 unit 6: regenerate the two Brahma tier-3 differentials at the new cap

_Versions: harness 0.0.434._

P4.D57 (Brahma Console turn budget, server): regenerated the two Brahma tier-3
differentials at the new baseline — the agent-mode system prompt now records the
default-50 turn cap (was 25), and both the one-shot and orchestrator ports
reproduce it byte-for-byte. harness 0.0.434.

#### `5403bdad` — 2026-08-07 — P4.D57 unit 5: settings-routes differential for the brahma-console budget

_Versions: harness 0.0.433._

P4.D57 (Brahma Console turn budget, server): extended the settings-routes
differential with 12 brahma-console GET/PUT cases (default, seeded, valid,
boundaries, empty-merge, and the four 400 arms + null + non-object body), driven
against the reference route and green over an oracle regenerated at `1bed814f`.
harness 0.0.433.

#### `59c6d890` — 2026-08-07 — P4.D57 unit 4: REST edge GET/PUT /api/v1/settings/brahma-console

_Versions: web 0.0.66._

P4.D57 (Brahma Console turn budget, server): added the REST edge
`GET / PUT /api/v1/settings/brahma-console` (byte-faithful to the reference
route: GET returns `{maxAgentTurns}`, PUT merges over current, validates,
persists, echoes; a malformed body 500s, an invalid value 400s). web 0.0.66.

#### `5cde24b4` — 2026-08-07 — P4.D57 unit 3: brahma-console settings dispatch verbs + handler

_Versions: core 0.0.512._

P4.D57 (Brahma Console turn budget, server): added the dispatch surface the
settings card consumes — `brahmaConsoleSettings` (GET, `{maxAgentTurns}`) and
`brahmaConsoleSettingsUpdate` (PUT, merge-over-current, validate, echo the
stored value; 400 on out-of-range/non-integer/null). core 0.0.512.

#### `04cc1e18` — 2026-08-07 — P4.D57 unit 2: both Brahma paths read the operator-set turn budget

_Versions: core 0.0.511._

P4.D57 (Brahma Console turn budget, server): both Brahma paths — the streaming
orchestrator and the one-shot `@Brahma` — now read the operator-set agent-turn
budget through a shared resolver (`resolve_brahma_max_agent_turns`) instead of a
hardcoded 25. A deep ledger investigation can now run to the configured budget
(default 50) before it is forced to answer; the duplicate/stale-query guard is
unchanged and still short-circuits a stuck loop. core 0.0.511.

#### `30365398` — 2026-08-07 — P4.D57 unit 1: brahmaConsole instance-setting accessors

_Versions: core 0.0.510._

P4.D57 (Brahma Console turn budget, server): added the
`instance_settings['brahmaConsole']` accessors to the Rust core — the
agent-turn budget (default 50, bounds 5–200) both Brahma paths read. Read
falls back to the default on a missing/unparseable/out-of-range value; write
validates and refuses out-of-range without storing. Portable by default
(rides `.qtap` export and full backups). core 0.0.510.

#### `9c0d4638` — 2026-08-07 — P4.D58 unit 5: assert the speaking-as portrait in the impersonation e2e beat

_No crate versions bumped._

Extended the impersonation end-to-end test to check that the "speaking as"
composer portrait appears and names the character in play. (Test-only.)

#### `d5e3e0e1` — 2026-08-07 — P4.D58 unit 4: the SpeakingAsAvatar composer cue (bug 46b)

_Versions: SPA 0.5.436._

Added a "speaking as" portrait to the Salon composer: a persistent full-height
picture of the character your typed message will be attributed to, seated just
left of the send controls. It brightens when the floor is yours and dims while a
reply is streaming, and it hides on the narrowest screens. Resolved the same way
the server attributes a message, so it always names whom you're actually speaking
as — including a character you're impersonating. SPA 0.5.436.

#### `32fb9e4f` — 2026-08-07 — P4.D58 units 2-3: reconcile Salon impersonation attribution to the overlay

_Versions: SPA 0.5.435._

Reconciled the Salon's impersonation attribution to the reference app's updated
client. The composer turn banner now announces an impersonated seat's own turn
(offering "type as them" and Skip), where before it stayed silent because the
seat keeps its LLM control column under the impersonation overlay. And a
just-sent message's optimistic bubble is now attributed to the same seat the
server will resolve it onto, so it no longer flickers to the wrong author on
refetch. SPA 0.5.435.

#### `d4792c50` — 2026-08-07 — P4.D58 unit 1: client impersonation-overlay participant filters (turn-order.ts)

_Versions: SPA 0.5.434._

Added client-side impersonation-overlay participant filters (`isUserDrivenSeat`,
`findUserParticipant`, `findActiveUserParticipant`) to the chat turn-order
module — the browser mirror of the reference app's turn-manager utilities and
the differential-proven engine filters. These resolve who the human is driving a
seat as, honoring the impersonation overlay (an impersonated seat keeps its LLM
control column). Groundwork for the composer turn banner and message-attribution
reconciliation. SPA 0.5.434.

#### `8d4f30a5` — 2026-08-07 — feat(settings): add the Brahma Console budget card to Settings → Chat (P4.D59)

_No crate versions bumped._

Added the Brahma Console budget card to Settings → Chat (between Data
Retention and Autonomous Rooms). One number field caps how many tool-use
turns the Console — and every one-shot @Brahma consultation — may take
before it must answer, adjustable 5–200 (default 50). It commits on blur
or Enter, reverts an out-of-range entry without nagging, and skips the
round-trip when unchanged. This is the SPA half; the setting itself is
served by the sibling server change. The About workspace-backdrop
reference-app change is a deliberate no-port: this app ships no About
background image, so the bug it fixes cannot occur here.

#### `1b5ab7ef` — 2026-08-07 — Write P4.D57/D58/D59 work orders: the 1bed814f drift catch-up round

_Docs-only change._

Planned the next porting round: a drift catch-up on three reference-app
changes (work orders P4.D57/P4.D58/P4.D59). It ports the Brahma Console
turn-budget instance setting, reconciles the impersonated-seat message
attribution and turn banner (with a new "speaking as" composer portrait),
and dispositions the About workspace-backdrop change. Work orders only;
no behavior change yet.

#### `3435055c` — 2026-08-07 — Revert dogfood #70: v5's turn banner matches v4's (keys on the bare controlledBy)

_Versions: SPA 0.5.433._

Reverted the previous change to the impersonated-seat turn banner. The
reference app does not announce an impersonated seat's turn in the
composer banner either (it keys on the seat's control column, which the
impersonation overlay leaves unchanged), so lighting the banner up for
impersonated seats was a divergence. The turn banner is back to matching
the reference behavior. The underlying rough edge — that while
impersonating, the banner and the message attribution can disagree with
no on-screen cue — is a shared reference-app issue and will be addressed
there first. SPA 0.5.433.

#### `0a646da8` — 2026-08-07 — Dogfood #70: surface the type-as-them/Skip banner on an impersonated seat's turn

_Versions: SPA 0.5.432._

Dogfood fix: an impersonated character's paused turn now shows the
"type as them, or skip" prompt and its Skip button. Since impersonation
became a pure overlay, the seat keeps its LLM control column while the
server correctly reports the turn as the user's; the Salon was still
deciding whose turn it was from the bare column, so an impersonated
seat's turn surfaced neither the prompt nor the Skip control. The turn
banner now consults the impersonation overlay the same way the engine
does. SPA 0.5.432.

#### `b2885484` — 2026-08-07 — Close the P4.D56 Bug 44 overlay round: round record, CHANGELOG, order header CLOSED, phase-4 candidates, CLAUDE.md baseline move to 62c63dc3

_Docs-only change._

Unified the Bug 44 impersonation-overlay round (single lane): the
reference-app baseline moves to 4.8.0-dev.178 and the drift debt is
cleared. Impersonation is a pure overlay end to end — starting or
stopping never writes the seat's control column or recompiles identity
stacks, the turn-resolution gates consult the overlay, and the
participant card's Stop-Impersonate button is back in solo-shaped
casts, proven by the re-gestured end-to-end walk driving Stop through
the card itself. Gate: 419 test binaries with 1,956 tests and no
failures, all twenty-four affected differential families regenerated
fresh against the reference app and green, the Angular suite at 4,015
tests, and the full end-to-end suite at 189 passing with zero skips.
Versions: core 0.0.509, harness 0.0.432, SPA 0.5.431.

#### `c158d2e3` — 2026-08-07 — Re-gesture the impersonation e2e beat to the Bug 44 overlay

_Versions: SPA 0.5.431._

Re-gestured the salon impersonation walk to the Bug 44 overlay: it now
checks that the seat's control column does not flip, that the
Stop-Impersonate button stays on the participant card, and that stopping
through that button returns the Speak-as affordance. No client component
changed — the healed Stop button falls out of the server no longer
flipping the column.

#### `3ae4708d` — 2026-08-07 — Port Bug 44: impersonation overlays the seat instead of mutating controlledBy

_Versions: core 0.0.509, harness 0.0.432._

Impersonation is now a pure overlay instead of mutating a participant's
control column (Bug 44, ported from the reference app). Starting to
speak as a character no longer writes controlledBy to user or recompiles
identity stacks, and stopping no longer writes it back to llm; recording
the seat in impersonatingParticipantIds is the whole change. A shared
is_user_driven_seat helper is consulted at the turn-resolution gates
(attribution and who-responds); owner-seat readers keep reading the
column, which restores the Stop-Impersonate button on the participant
card with no client change. The stop flow's new-profile arm is now a
plain connection-profile reassignment. Twelve differential families were
regenerated against the reference app and re-run; the neutrality set is
unchanged.

#### `25348fe3` — 2026-08-07 — Plan the Bug 44 impersonation-overlay drift round: the P4.D56 order (single lane), the round-planned record, the drift classified (62c63dc3 = the round; cc0bbebf/3fa36825 NO-PORT)

_Docs-only change._

Planned the Bug 44 impersonation-overlay drift catch-up (work order
P4.D56, single lane): the reference app has now implemented the ruled
correction — impersonation becomes a pure overlay and the seat's
controlledBy column is never written — so this port absorbs it as the
pre-announced drift round. The order carries the site-by-site survey
(the change-list and, just as binding, the keep-list of owner-seat
readers that stay on the column), the twelve moving differential
families plus the neutrality set, and the e2e re-gesture that returns
the Stop-Impersonate button to the participant card.

#### `94fdc646` — 2026-08-06 — Point the #39 records at v4 Bug 44 (catalogued upstream, v4 3fa36825, docs-only; baseline unchanged)

_Docs-only change._

The #39 mechanism correction is now catalogued upstream as v4 Bug 44
(reference-app docs commit `3fa36825`): the full account of what is
wrong with the mutate-and-restore impersonation mechanism and the
ruled overlay fix, specced at the exact sites. This app's records
point at it; the oracle baseline is unchanged (docs-only upstream).

#### `b2ca2781` — 2026-08-06 — Record the #39 impersonation-mechanism ruling: the overlay design stands; v4's bug-27 mutate-and-restore is ruled a mistake, correction queued v4-first

_Docs-only change._

The impersonation-mechanism ruling (finding #39) is recorded: the
overlay design stands, and the reference app's recent impersonation
fix — which rewrites who controls a character's seat and restores it
afterward — is ruled a mistake to be corrected there first, since it
is the oracle. Until that correction lands, this app faithfully
matches the shipped behavior; the ruling, its reasoning, and the two
gate sites the correction touches are recorded in the porting docs.

#### `47a2851a` — 2026-08-06 — Close the f4955e0e convergence round: round record, CHANGELOG, CLAUDE.md baseline move, the stop-affordance v4-side note

_Docs-only change._

Unified the `f4955e0e` found-bugs convergence round — six lanes, all
closed, the oracle baseline moved to v4 4.8.0-dev.175. The reference
app's coordinated bug sweep (every catalogued v4 bug 1-43 is now fixed
there, much of it adopting fixes this port made first) is absorbed
whole: roughly twenty-five deliberate-divergence pins across seven
differential families retired into plain equalities, and the four
genuinely new pieces landed. Long conversation interchanges now
sub-chunk at a 24,000-character budget so they can actually embed and
be searched (with the boot pass re-rendering previously stranded
chats once); the all-LLM pause finally has its dialog, opening
automatically with take-over buttons for the cast; OpenRouter sends
images (a dedicated non-streaming vision path, plus the capability map
now saying so); Grok accepts text attachments; almost-base64 text
attachments arrive as their actual bytes; Ollama streaming no longer
drops content split across network reads; the chat record projects
five more fields so the sidebar's controlled selects survive a reload;
staff-signed announcements reach the model with their author's name;
impersonation genuinely hands the seat over (and back); tool cards
initiated by the operator wear the operator's face; self-targeted
whispers read "whispered to you"; deleted chats sweep their
annotations; orphaned thumbnails get a daily reaper; corrupt
character-vault edits are refused in both apps now; and Memory
Deduplication + Regenerate Conversation Summaries went live in
Settings, closing the embedding-profiles order whole. Gate: 419 Rust
test binaries / 1,951 tests, every round differential fresh at the
new baseline with zero skips, ng 294 files / 4,015, full Playwright
189/189 zero skips. Versions: core 0.0.508, harness 0.0.431, host
0.0.63, web 0.0.65, SPA 0.5.430.

#### `01097dab` — 2026-08-06 — P4.D51: stale-comment sweep + bug-43 tier-2 loud deferral

_Versions: core 0.0.494._

P4.D51: stale "queued v4-side" / "deliberate divergence" prose in the
store-delete, import, delete-all, and character-vault comments is corrected
now that the reference app has converged (bugs 8/9/10/11). The per-delete
eager thumbnail cleanup (bug 43 tier 2) is deferred with a loud note — the
landed daily orphan-thumbnail sweep covers those strays until a storage
backend is threaded through the file delete/upload paths.

#### `4bb2703b` — 2026-08-06 — P4.D51 bug 38: attach native-text documents instead of 404ing

_Versions: core 0.0.493, harness 0.0.420._

P4.D51 (bug 38): a native-text document (a `.md`/`.txt`/`.json` written into a
document store, held with no image blob) can now be attached to a chat and
reaches the model as text, instead of failing with "Mount-point file blob not
found". The attach path, the file list, and the send-time attachment loader
all fall back to the document row for these files, serving their text with the
`/files/` route.

#### `817fe88a` — 2026-08-06 — P4.D51 bug 43: orphaned-thumbnail sweep + the maintenance-summary reshape

_Versions: core 0.0.492, harness 0.0.419, host 0.0.62._

P4.D51 (bug 43): the daily maintenance pass now sweeps orphaned thumbnails —
`_thumbnails/{fileId}_{size}.webp` cache entries whose source file left by a
route that never cleaned them up (a restore, a delete-all, an out-of-app
edit) — deleting the strays and skipping unparseable names. Adds a `list`
capability to the storage backend (the local-disk backend enumerates the
directory; backends that cannot list report zero). The maintenance summary is
reshaped to the reference app's order and shape (the store-children reaper now
runs before the orphaned-files sweep, and reports `{links, folders,
documents}`; the thumbnail sweep reports `{scanned, deleted, unparseable}`).

#### `8e116d1a` — 2026-08-06 — P4.D51 bug 9: retire the store-delete cascade-leak divergences

_Versions: harness 0.0.418._

P4.D51 (bug 9): the reference app now deletes a document store's children
in one transaction, leaking no orphaned document bodies or group links —
the cascade fix this port made first. The store-delete differential's
both-directions leak divergences are retired to plain equalities; the
boot orphan-reaper arm stays (a fix still unique to this port).

#### `783e5178` — 2026-08-06 — P4.D51 bug 12: reshape the restore dedup carve-out for v4's partial convergence

_Versions: harness 0.0.417._

P4.D51 (bug 12): the reference app now preserves an archived document
store's link IDs across a second-generation restore — the carried-file
dedup this port made first. Its convergence is partial: the reference app
kept its earlier file-restore phase order, so it still loses the "restored"
folder on one archive shape and, on a store file larger than 3 MB, invents
a phantom duplicate copy the archive never contained. Both remain deliberate
divergences where this port is ahead. The restore differential's dedup
carve-out is reshaped: the gen-2 archive is now a plain equality, and the
remaining phase-order divergences are pinned in both directions with a
self-retiring tripwire.

#### `79438835` — 2026-08-06 — P4.D51 bug 10 (delete-all half): retire the conversation_annotations divergence

_Versions: core 0.0.491, harness 0.0.416._

P4.D51 (bug 10, delete-all half): the reference app now clears
`conversation_annotations` on "delete all my data" — a privacy fix this
port made first. The delete-data differential's both-directions divergence
carve-out is retired; the annotation count is compared as a plain equality
like every other table.

#### `d89a5619` — 2026-08-06 — P4.D51 bug 11: retire the import store-identity + folder-clear divergences

_Versions: core 0.0.490, harness 0.0.415._

P4.D51 (bug 11): the reference app now recognizes an imported `.qtap`
archive's document store by its ID (not its display name), preserves that
ID on create, and clears folders on overwrite — all fixes this port made
first. The import-execute differential's both-directions divergence
carve-outs (store identity, folder clear) are retired to plain equalities.
One table stays carved out with a self-retiring tripwire: the per-chat
`conversation_annotations` sweep on chat delete is a sibling lane's (P4.D53)
change, so the two converge at unification.

#### `010c8b37` — 2026-08-06 — P4.D51 bug 15: retire the reindexLinkGroupSiblings divergence carve-out

_Versions: harness 0.0.414._

P4.D51 (bug 15): the reference app now re-chunks hard-linked file siblings
when one location is rewritten, where before its sibling-reindex pass was
dead code and the other locations served stale search chunks. This port
was already correct, so the both-directions divergence carve-out in the
mount-file-links differential is retired — the chunk table is now a plain
row-for-row equality.

#### `3c3aeef1` — 2026-08-06 — P4.D51 bug 13: guard gc_orphaned_file_row against a blobless mount index

_Versions: core 0.0.489._

P4.D51 (bug 13): garbage-collecting an orphaned mount-index file row no
longer crashes on an index that never held a blob (a document-only,
restored, or hand-built store). The blob and document payload deletes are
now guarded behind a table-existence check, so the second write to a path
succeeds instead of failing with "no such table: doc_mount_blobs".

#### `e54e3192` — 2026-08-06 — P4.D51 bug 18: widen the help-sync prune guard against blank-content wipes

_Versions: core 0.0.488, harness 0.0.413._

P4.D51 (bug 18): the help-doc sync no longer wipes the in-app Guide when
the only Markdown on disk is blank. A `help/` directory whose files are
all whitespace-only walks non-empty but produces no usable content;
previously that pruned every existing row. It now refuses the destructive
prune when nothing on disk parsed to usable content while the table is
populated, matching the reference app, and leaves the rows for the next
healthy sync.

#### `8ac1bb25` — 2026-08-06 — P4.D51 bug 8: converge the corrupt-properties.json character-vault refusal

_Versions: core 0.0.487, harness 0.0.412._

P4.D51 (bug 8): a character whose `properties.json` is present but
corrupt now fails a save loudly instead of clobbering the six vault-only
fields — a fix this port made first (finding #47), and the reference app
has since adopted it, so the two now refuse identically. The refusal
message matches the reference app's character-vault wording.
Corrected a stale scope note on the conversation-chunks repository doc
(it described `upsert` and its siblings as unimplemented, though the
Scriptorium slices landed them). Comment cleanup only.

#### `ae69b883` — 2026-08-06 — Count failed embedding rows in the Almanack + retire converged pins

_No crate versions bumped._

The Almanack's embedding-pipeline census now counts failed embedding
rows instead of a status value nothing writes (v4 Bug 19). The "Failed
rows" line — renamed from "Permanently failed rows" — now reflects the
real FAILED count for the current profile. The cast-size histogram and
the wardrobe-permission counts, which this port had fixed ahead of the
reference app, are now matched by the reference app too, so their
differential divergence pins are retired to plain comparisons (v4 Bugs
20 and 21).

#### `70efff36` — 2026-08-06 — Preserve the gate's memory links in the fold pass (bug 26)

_No crate versions bumped._

The fold-episode pass no longer discards the memory links its own gate
just created (v4 Bug 26). When a consolidated episode is written as a new
memory linked to a related memory, folding in the per-turn fragment
links used to overwrite the whole related-memory list, dropping the
gate's link. It now preserves the gate's links and adds the fragments on
top; a plain insert still starts empty.

#### `e7e3e1c4` — 2026-08-06 — Reconcile arm C (bug 17) + read mount points from the mount DB (bug 16)

_No crate versions bumped._

The startup reconcile now re-renders a conversation whose only stuck
chunk is over the per-chunk budget but under the transport cap (v4 Bug
17 arm C) — exactly the oversize chunk that could never embed before
sub-chunking, and that the FAILED-status skip otherwise leaves stranded.
Re-rendering splits it into embeddable in-context chunks; the pass stays
self-limiting and stale-gated.

The boot embedding-dimension reconcile's mount-chunk count now reads the
mount-index database where `doc_mount_points` actually lives (v4 Bug 16).
It had guarded on the main database, where that table does not exist, so
the count was dead — non-conforming chunks on an enabled document mount
were never counted toward a mismatched-dimension reindex. The
embedding-remainder differential fixture was regenerated to seed the
arm-C window chunk, and the old dead-code tripwire is retired.

#### `4ae88dd8` — 2026-08-06 — Null a conversation chunk's stale embedding on content change (bug 17)

_No crate versions bumped._

A conversation-chunk re-render now nulls a chunk's embedding when the
text at that interchange position changed (v4 Bug 17). Before, a
re-render preserved the stored vector unconditionally, so when an
oversize interchange split into sub-chunks — shifting every later chunk
to new content at an existing index — those chunks kept the previous
occupant's vector and were never re-embedded. The chunk upsert now nulls
a stale vector (the re-embed enqueue gate is "no embedding"), while a
caller that supplies a fresh embedding still wins. Unchanged text keeps
its vector. The conversation-chunks differential grew a three-op upsert
family (supplied-embedding overwrite / content-change null / identical-
content preserve), mutation-proven.

#### `0b785afc` — 2026-08-06 — Port bug 17's interchange sub-chunking into the Scriptorium renderer

_No crate versions bumped._

The Scriptorium conversation renderer now sub-chunks an oversize
interchange (v4 Bug 17). An interchange whose rendered text exceeds a
24,000-character budget is split into sequential in-context chunks — at
message boundaries first, then within a single very long message at
natural boundaries (paragraph, sentence, whitespace, and a hard cut only
when a single token itself overflows) — so a long conversation turn no
longer produces one chunk the embedding model can never accept and that
stays unsearchable forever. Anything under budget is unchanged, byte for
byte. All budget arithmetic counts UTF-16 code units, matching the
reference app. The conversation-markdown differential corpus grew four
over-budget cases (message-boundary split, a single-message boundary
walk, a split interchange 0 carrying the metadata header, and astral
content for the UTF-16 proof), all byte-exact against the reference
renderer.
Fixed a cluster of chat-state and impersonation bugs (v4 bugs 22, 23, 24, 27,
36, 37). "Speak as an AI character" now attributes correctly: starting it flips
the chosen character to user-controlled (and hands it back to the LLM on stop),
so the operator's next message lands under the right speaker and the badges are
truthful. A `controlledBy` change to a participant no longer short-circuits the
rest of the update, and removing an impersonating participant returns the chat
as it stands after cleanup (no more stale "still impersonating" state until a
refetch). The chat view now remembers saved values for the timeline mode,
lantern-image alerts, show-thinking, and answer-confirmation selects across a
reload, exposes the all-LLM pause count, and reports whether a connection
profile permits tool use (so the tool-settings dialog can warn when it doesn't).

#### `2bc03dfe` — 2026-08-06 — P4.D53 unit 3 — bug 28: attribute staff-signed announcements (finding #54)

_No crate versions bumped._

Staff-signed announcements now carry their author's name into the model's
context (v4 bug 28). An Insert-Announcement line signed as the Host (or any
Staff voice) used to reach every character as an anonymous block of prose, so
the model had to guess who spoke; it now arrives tagged with the resolved staff
name — and the tag lands on the opaque body too, so an opaque-anywhere chat
stays attributed. Ordinary Staff whispers (image notices, tool bubbles) are
left alone, since they name themselves in their own prose.

#### `60b96a40` — 2026-08-06 — P4.D53 unit 2 — bug 10: sweep per-chat annotations on chat delete

_No crate versions bumped._

Deleting a conversation now also sweeps its per-message annotations (v4 bug
10). Annotations sat on no cascade, so a deleted chat left its annotation rows
behind — harmless day to day, but a later restore of a migrated instance could
collide on the unique constraint. The sweep is scoped to the deleted chat.

#### `5507bd3b` — 2026-08-06 — P4.D55 bugs 31/33/34: OpenRouter vision send, Grok text/PDF, base64 round-trip

_No crate versions bumped._

Widened the `Content-Disposition` `filename*` escaping to cover the full RFC
8187 stray set — `' ( ) * !` are now all percent-encoded, matching the
reference app (v4 bug 41). v5 previously escaped only the apostrophe; v4 has
since caught up to the full set, so the two now agree and the deliberate
divergence is retired. Filenames with parentheses, asterisks, or exclamation
marks beside non-ASCII characters download with their real names intact.
OpenRouter non-streaming image sends now reach the model (v4 bug 31): a
completion carrying an image routes around the SDK — whose client-side
validation silently rejected image content — to a direct chat-completions
request, so regenerate and continuation legs with a picture no longer send
nothing. Grok text and PDF attachments are handled (v4 bug 33): text files
embed inline and PDFs get an honest "requires the Files API" message instead
of a blanket "unsupported file type" rejection. Text attachments that are
plain text (not base64) now ship verbatim instead of garbled, on both Grok
and Anthropic (v4 bug 34).

#### `b6fb5ab4` — 2026-08-06 — P4.D55 bug 35: the Ollama stream splitter buffers across reads

_No crate versions bumped._

The Ollama streaming decoder now buffers across network reads (v4 bug 35):
a JSON object split across two reads reassembles instead of being silently
dropped, a multi-byte UTF-8 character split across reads is no longer
corrupted, and a CRLF line terminator parses. The decoder is now
push-boundary-insensitive and joins the full three-chunking stream-decoder
equivalence.

#### `dfb18622` — 2026-08-06 — Land the P4.9H2A maintenance remainder (P4.43): memory-dedup + conversation-summaries regeneration

_Versions: web 0.0.64._

OpenRouter now reports its four image MIME types (JPEG, PNG, GIF, WebP) in
the client-side attachment-capability map instead of "unsupported" (v4 bug
32), so a new OpenRouter connection profile that omits the flag defaults to
image upload enabled — the client vision gate onto the now-working
non-streaming vision path.
Landed the memory-maintenance remainder (P4.43): the Settings → Commonplace
Book "Memory Deduplication" and "Regenerate Conversation Summaries" cards now
work. Analyze Memories finds semantically duplicate memories across all
characters (grouped by embedding width, clustered by cosine similarity), keeps
the best of each cluster, folds unique details from the discards in as
footnotes, and — on Run — removes the duplicates; preview costs nothing and
changes nothing. Regenerate Conversation Summaries re-mirrors every summarized
chat's summary into its participant vaults (the files the Commonplace Book reads
before a character's turn), deduping to one run at a time. Both are live over
the dispatch verbs and v4's `/api/v1/system/tools` and
`/api/v1/system/conversation-summaries` URLs.
P4.D54 (Salon + UI-polish SPA drift, `f4955e0e` round — in progress):
- Bug 29: a composer/Run-Tool result card no longer wears the last
  speaker's face. The standalone-tool avatar helper is now
  `resolveToolRowAttributionMessage`, which heads an `initiatedBy: 'user'`
  row as the operator; character-initiated rows keep the positional
  borrow.
- Bug 30: a private user-initiated run reads "whispered to you" instead
  of "whispered to unknown" — the whisper label resolves the operator's
  own userId to "you" (both the bubble label and the Staff header tag).
- Bug 22: the four controlled chat-sidebar selects (Story's Clock,
  Lantern image announcements, Thinking visibility, Answer Confirmation)
  now survive a reload instead of snapping back to their defaults — the
  server projects the columns (P4.D53) and the sidebar seeds from them.
  The `core-contract.ts` mirror gains `showThinking`,
  `answerConfirmationOverride` and `allLLMPauseTurnCount`; the three
  write-only-select divergence records are retired.
- Bug 36: the LLM Tool Settings dialog now shows its "tools disabled by
  connection profile" warning when an LLM participant's profile has tool
  use turned off — the projected `connectionProfile.allowToolUse` (P4.D53)
  makes v4's long-dead-code condition computable, and the binding fires it.
- Bug 42: toasts slide up and fade in on appearance (a 0.3s entry
  animation with a reduced-motion guard), matching v4's now-live
  `slideInUp`.
- Bugs 39/40: two v5-first fixes v4 has now converged on — the
  `.qt-text-danger` alias and the search-dialog body portal — so their
  "deliberate v5 divergence" comments are retired to convergence notes
  (no behavior change).
- Bug 37: an all-LLM room that auto-pauses (at 3, 6, 12, 24… turns) now
  explains itself with the All-LLM Pause dialog — continue, stop, or take
  control of a character — instead of stalling silently. The opener keys
  off the projected `isPaused`/`allLLMPauseTurnCount` (P4.D53) and rides
  the existing chain-complete refetch, so it also surfaces on loading an
  already-paused room.
- Tier 2: an e2e beat proving a Story's-Clock choice survives a reload
  is authored, gated behind `PROJECTION_ROUNDTRIP_SERVER_LANDED` until
  P4.D53's projection lands. The all-LLM-pause live-opener beat is
  deferred loud (needs a seeded paused all-LLM fixture).

#### `8cb86df1` — 2026-08-06 — Plan the f4955e0e found-bugs convergence round: six work orders + the round record

_Docs-only change._

Planned the `f4955e0e` found-bugs convergence round: six committed
work orders (P4.D51 guards/backup/mount convergence, P4.D52
scriptorium/memory/almanack, P4.D53 chat-API/attribution server,
P4.D54 Salon/UI-polish SPA, P4.D55 provider attachments/streaming,
P4.43 memory-maintenance remainder) absorbing v4's eleven-commit
"bugs 8–43" batch — at the new baseline every catalogued v4 bug is
fixed, much of it v4 adopting fixes this port made first, so the
round is largely convergence-pin retirement plus four genuine ports
(interchange sub-chunking, the AllLLMPauseModal, the OpenRouter
vision path, the thumbnail sweep). Docs only; no code changed.

#### `90ed1743` — 2026-08-06 — Close the fallback + wire + embedding-profiles round: docs, headers, round record

_Docs-only change._

Unified the fallback + wire + embedding-profiles round — four lanes, all
landed. OpenAI conversations no longer wedge when the provider forgets a
prior response: a failed chained request now retries once with the full
conversation, matching the reference app (finding #69). Web search is
connected end to end: with a Serper key set, the search tool actually
runs on every surface (chats, autonomous rooms, Carina, the Brahma
Console, Run Tool), and what the tool picker advertises is now derived
from the same source that executes, so they can never disagree.
Embedding profiles are manageable from Settings → Commonplace Book:
create, edit, delete, set default (with the update rules that queue the
right re-embedding work — full reindex, vocabulary refit, or the local
Matryoshka re-apply, which now has a live job handler taking a backup
before any rewrite). The Memory Deduplication and Regenerate
Conversation Summaries cards are built and answer an honest
not-yet-available message until their server halves land. Chat cards
grew the three-state Scriptorium badge with click-to-re-render. The
unification review fixed one wire defect before it shipped (an
explicit-null profile update dropped keys from the echo) and aligned
every error sentence with the reference app. Gate: 417 test binaries /
1,931 tests / 0 failed with all round differentials fresh at the pinned
baseline; SPA tests 292 files / 3,956; full Playwright 185 passed + 2
gated skips (the one flake is the documented wardrobe intermittent,
green in isolation). Versions: core 0.0.486, harness 0.0.411, host
0.0.61, web 0.0.63, SPA 0.5.422.

#### `520226a5` — 2026-08-06 — Restore the OpenAI conversation-chaining fallback (P4.41, finding #69)

_Versions: core 0.0.483._

Restored the OpenAI conversation-chaining fallback (finding #69). When a
multi-turn OpenAI chat sends a prior-response reference the server can no
longer find — routine, because both apps send unstored responses — the
streaming provider now retries the turn once with the full conversation
instead of failing it. Before this, such a turn errored and left the chat
wedged on the same dead reference. Matches the reference app's behavior;
the retry is a second request only on failure. Version: core 0.0.483.

#### `e5991a00` — 2026-08-06 — Pin the chaining fallback's retry bytes to a non-chained build (P4.41)

_Versions: harness 0.0.409._

Added a wire-byte regression test pinning the chaining fallback's retry
request to a from-scratch non-chained build of the same call (no
prior-response reference, the full conversation). Version: harness
0.0.409.

#### `2e687f9c` — 2026-08-06 — Add the tier-3 chaining-fallback differential (P4.41, finding #69)

_Versions: harness 0.0.410._

Added a differential test proving the chaining fallback recovers a turn
exactly as the reference app does: both run their real code (the
reference app's provider with its SDK made to fail once then succeed; our
streaming provider with the transport made to do the same) and the
recovered stream and retry pattern are compared. Version: harness
0.0.410.
Defined the memory-deduplication and conversation-summary-regeneration
maintenance actions in the request dispatcher; they currently return a
clear "not yet available" response pending their implementation.

#### `80a9b55b` — 2026-08-06 — P4.9H2A unit 4: EMBEDDING_REAPPLY_PROFILE handler + reapply differential

_Versions: core 0.0.485, harness 0.0.411, host 0.0.61._

Implemented the embedding re-apply job: when a profile's Matryoshka
dimensions are narrowed, the stored vectors are re-sliced and
renormalized in place (with a safety backup of each database taken
first) instead of re-embedding from scratch. The narrow arm of the
profile-edit trigger now does real work.

#### `5241a38a` — 2026-08-06 — P4.9H2A unit 3: embedding-profiles REST edges (quilltap-web)

_Versions: web 0.0.63._

Exposed the embedding-profiles management API at its standard web
addresses (in addition to the shared request endpoint), so the settings
screens can reach it by URL.

#### `e8d30e91` — 2026-08-06 — P4.9H2A unit 5 (routes): embedding-profiles routes differential + fixture family

_Versions: harness 0.0.410._

Added a differential test and committed test fixtures covering the
embedding-profiles management API against the reference app, including
the re-embed trigger matrix. Test-only.

#### `15930e05` — 2026-08-06 — P4.9H2A unit 2: embedding-profiles management verbs + handler + PUT matrix

_Versions: core 0.0.484._

Added the embedding-profiles management API: listing profiles (with
their API key, tags, and — for the built-in model — vocabulary and
embedding-progress stats), viewing, creating, updating, and deleting a
profile, and the manual re-embed / vocabulary-refit / re-apply actions.
Editing the default profile's model, provider, or dimensions now
re-embeds the whole corpus; narrowing a Matryoshka profile's dimensions
re-applies locally instead. Available embedding providers and their
models are listed for the settings screens. Wired into the request
dispatcher; the browser screens land in the companion change.

#### `ba9cdf2a` — 2026-08-06 — P4.42: wire the web-search provider into the running engine

_Versions: harness 0.0.409._

Extended the embedding-profiles data layer with the query helpers the
management surface needs: looking a profile up by name (for the
duplicate-name check when creating or renaming), clearing the default
flag across a user's profiles, and clearing a profile's optional
numeric or key fields back to empty. Internal groundwork for the
embedding-profiles management screens; no user-visible change yet.
Began wiring the web search tool (P4.42). Added an optional endpoint-URL
override to the Serper web-search provider so a test can point it at an
in-process mock; by default the request is unchanged and goes to the
real Serper endpoint byte-for-byte. No behavior change on its own — the
provider is still not connected to the running engine yet. Versions:
core 0.0.483, host 0.0.61.
Connected the web search tool to the running engine (P4.42). The
`search_web` tool was fully built and verified but never wired in, so
it refused at runtime even with a search key set, while the tools list
advertised it as available. Now, when `SERPER_API_KEY` is set, the host
builds the Serper provider once and threads it into every tool-running
surface — the in-chat turn, the character/ask-carina and Brahma Console
engines, and the operator Run Tool modal — so searches actually run
(one Serper call each). The tools list's "web search configured" flag
now derives from the very same provider, so it can never again claim
the tool is available while the tool refuses. The plugin-registry path
(a search plugin supplying the key) stays deferred; the environment-key
path is what this connects. Also added an optional endpoint-URL override
(default: the real Serper endpoint, byte-for-byte unchanged) so a test
can point the request at an in-process mock. A new wiring test proves a
runner built the production way actually runs a search (and refuses with
today's message when unconfigured), and pins the advertised-vs-executed
consistency both ways. An end-to-end browser beat runs a search through
a mock Serper server and shows the result card. Versions: core 0.0.483,
host 0.0.61, harness 0.0.409, SPA 0.5.417.
Added the P4.9H2B e2e beats: a live Salon walk clicking the Scriptorium
badge to enqueue a conversation render (asserting the dispatch + toast,
not job completion), plus the gated Commonplace Book management walk
(profile create → rename → delete on a non-default BUILTIN profile,
dedup preview zero-state, summaries enqueue) that activates once the
P4.9H2A server verbs land. SPA 0.5.422.

#### `d7df1f9e` — 2026-08-06 — P4.9H2B unit 5: the p4.9o Scriptorium-badge rider

_Versions: SPA 0.5.421._

Made the chat-card Scriptorium badge clickable to queue an on-demand
conversation render (P4.9H2B rider, p4.9o). The three-state badge
(not-rendered / rendered / rendered-and-embedded) is now a shared
component on both the Salon list card and the character Conversations
card; clicking it POSTs the render-conversation action, toasts, and
wakes the toolbar queue badges. The Salon card previously showed no
badge at all; the character card's was static. SPA 0.5.421.

#### `d3e49ac7` — 2026-08-06 — P4.9H2B unit 4: the Regenerate Conversation Summaries card

_Versions: SPA 0.5.420._

Ported the Regenerate Conversation Summaries card into the Settings →
Memory tab (P4.9H2B), last in v4's card order on the
`conversation-summaries-regenerate` deep link: a single-click enqueue
that re-mirrors every summarised chat into its participants' vaults, an
in-flight line that polls every 5s while a run drains and stops at zero,
and swallowed status-read failures (the button still works). This
completes the Commonplace Book tab — every card the reference app ships
now renders. SPA 0.5.420.

#### `eb70f139` — 2026-08-06 — P4.9H2B unit 3: the Memory Deduplication card

_Versions: SPA 0.5.419._

Ported the Memory Deduplication card into the Settings → Memory tab
(P4.9H2B), between Recall Relevance and Regenerate Memories on the
`memory-deduplication` deep link: the similarity-threshold slider
(0.70–0.95, default 0.80) and the Analyze → preview → Run dialog with
the per-character table, the totals, the zero-removable Run disable, and
the exact success toast. Preview failures show inline only, never as a
toast. SPA 0.5.419.

#### `b6ab160e` — 2026-08-06 — P4.9H2B unit 2: the Embedding Profiles card

_Versions: SPA 0.5.418._

Ported the Embedding Profiles card into the Settings → Memory tab
(P4.9H2B), first in v4's card order on the `embedding-profiles` deep
link: the profile list with provider and missing-key badges and
embedded-status counts, the four default-gated maintenance actions
(refit / re-embed everything / Matryoshka re-apply / re-embed
mismatched) with their two-step confirms and inline "View Tasks Queue"
success banners, and the create/edit modal with the BUILTIN model pin,
the validity gate, and the "Re-embed Everything?" follow-up on a save
that newly sets the default. The Matryoshka truncation inputs are
deliberately absent, matching the reference app (an API-only matrix
there). SPA 0.5.418.

#### `ae114b56` — 2026-08-06 — P4.9H2B unit 1: the §1 embedding-profiles + maintenance wire mirror

_No crate versions bumped._

Added the client-side wire contract for the embedding-profiles
management surface (P4.9H2B, §1): the eleven profile verbs (list / get /
create / update / delete / refit / reindex / reapply / list-providers /
list-models / fetch-models) and the four memory-maintenance verbs
(dedup preview + run, conversation-summaries status + regenerate), as
CoreClient methods over `dispatchData` plus the request/DTO types.
Server half is the parallel P4.9H2A lane. SPA 0.5.417.

#### `f160203c` — 2026-08-06 — Plan the fallback + wire + embedding-profiles round: four work orders

_Docs-only change._

Planned the next porting round and committed four work orders (docs
only, no code change): restore the OpenAI conversation-chaining
fallback that finding #69 showed was dropped in the port (P4.41),
connect the already-ported Serper web search to the running engine so
the search tool stops advertising while refusing (P4.42), and port the
embedding-profiles management surface — profile CRUD with the update
trigger matrix, the profile re-apply job, memory deduplication, and
conversation-summary regeneration — as a server lane and an SPA lane
(P4.9H2A/P4.9H2B, the SPA lane also carrying the chat-card Scriptorium
badge). Round record in the status log; no version bumps.

#### `972e4a25` — 2026-08-06 — Dogfood #66/#67/#68: fix two Almanack ledger-collector bugs on real Friday data

_Versions: core 0.0.482, harness 0.0.408._

Fixed two defects in the system report ("The Almanack"), found while
reviewing the report against real data. The "Cast sizes" table now
groups conversations by their number of participants instead of listing
one row per conversation. The character summary's "may dress themselves"
and "may create outfits" counts now reflect the effective permission — a
character left at its default is permitted, and only an explicit opt-out
is excluded — where before they counted only characters explicitly
switched on and so read zero on most instances. Both changes match how
the reference app behaves at runtime rather than how its report counted;
they are recorded as intentional differences that revert automatically
if the reference app adopts the same fix. Versions: core 0.0.482,
harness 0.0.408.

#### `8633cbd1` — 2026-08-06 — Close the Taboo + maintenance round: round record, order headers, phase plan, baseline move

_No crate versions bumped._

Unified the Taboo + maintenance round — three lanes, all closed. The
system report ("The Almanack") is now fully live: its collectors are
verified against the reference implementation by a new 72-check
equivalence suite over a committed four-database fixture family, the
host wiring makes the report reachable in production, timestamps in the
plain space-separated database vintage render correctly, and the
end-to-end walk (compile, per-phase progress, viewer, download, delete)
runs on every test pass. The reference app's newest feature, Taboo, is
absorbed whole: an instance-wide list of phrases no character may
utter, normalized on save, rendered into the cacheable prefix of every
character's system prompt (verified byte-for-byte including both cache
goldens), served over new settings endpoints, and edited from a new
Settings → Chat card. The maintenance sweep repaired the two
differential oracles that could no longer regenerate (a corpus defect,
not a port bug — and the same defect turned out to explain a
long-standing red in a third suite), fixed a flaky tracing test at its
real cause, hardened two intermittent end-to-end checks, and taught the
recipe sweep driver to regenerate safely while the reference app is
mid-drift. The unification review caught and fixed a real bug before it
shipped — an explicit null sent to the new Taboo endpoint would
silently keep the stored list where the reference app rejects it — plus
five report-fidelity nits (user scoping, error wording, number shape, a
dropped id-format gate, an unknown-provider skip) and three rotted
regeneration recipes. Gate: 414 Rust test binaries / 1,911 tests / 0
failed with eleven equivalence families re-proven fresh; SPA 3,926
tests / 0; full Playwright 183 passed / 0 failed / 0 skipped. Versions:
core 0.0.481, harness 0.0.407, host 0.0.60, web 0.0.62, SPA 0.5.416.

#### `efdc58fa` — 2026-08-06 — P4.37: activate the Almanack e2e walk; fix the inline-host click bug it caught

_Versions: SPA 0.5.413._

The system-report card's end-to-end test walk is active: compile,
per-phase progress, viewer, download, delete. Activating it caught two
real issues fixed here: the report card's and the settings tab
container's custom elements had no display rule, so their inline boxes
did not enclose their content and automated clicks on the compile button
were read as landing on the tab container (the same class of bug as the
chat sidebar's Add Character footer); and the test's viewer assertion
was ambiguous once a real report rendered, since the dialog's
description repeats the report title.

#### `4aec0307` — 2026-08-05 — P4.37: wire the Almanack host seam - the report is live in production

_Versions: host 0.0.60._

The Almanack is now reachable in production: the server supplies the
report pipeline its database paths, backups directory, honest runtime
facts (host version, platform, uname, total memory, process uptime,
timezone), the passphrase-protection flag, the application version, a
clock, and the disk storage layer. Until now the four report actions
answered a clear "not assembled" refusal even though the collectors were
fully built. The runtime section describes the actual host process — a
documented difference from the reference app's Node facts.

#### `487f7fc4` — 2026-08-05 — P4.37: parse the space-form date vintage in the Almanack's formatters

_Versions: core 0.0.478, harness 0.0.405._

The system report's date columns now render the plain space-separated
timestamps older database rows carry ("2026-08-05 09:07:03") instead of
"N/A". The reference app's JavaScript date parser has always accepted
that form; our strict ISO parser did not, so any report column fed such a
stamp went blank. The acceptance is scoped to the report's two formatters
only, and a new render-oracle case pins the behavior against the
reference app byte-for-byte.

#### `3adcf77f` — 2026-08-05 — P4.37 unit 12: the almanack-* fixture family + the tier-2 differential

_Versions: core 0.0.477, harness 0.0.404._

The new system report ("The Almanack") is now verified against the
reference app end to end: a new committed fixture family exercises every
report section non-trivially over all three databases, and a differential
drives the reference app's real collectors and route handlers against
ours — comparing the report data, the rendered markdown byte-for-byte,
the four report actions with their download link, the per-phase progress
frames, and the rows both apps persist when a report is filed. Both
attribution arms are proven (a modern call log with the per-profile
columns and a legacy one without). One fix fell out: the report's
library entry no longer invents a description on the stored copy the
reference app leaves blank. Two knowingly accepted differences are
pinned so any drift trips the test: our provider list omits the built-in
TF-IDF entry (no plugin loader), and JSON parse-failure wording follows
the JSON engine.

#### `bda0d2f0` — 2026-08-05 — Port the Taboo list into the system prompt (P4.D50 unit 1)

_Versions: core 0.0.481, harness 0.0.407._

Ported the collectors, orchestrator and API surface of the new system
report ("The Almanack"): the seven-phase census across all three
databases, the four report actions (generate / list / get / delete) with
the download link, and per-phase progress on the live event stream.
NOT YET VERIFIED against the reference app and NOT YET REACHABLE: the
report's equivalence test is still to be built, and the server answers a
clear "not assembled" until the host supplies its paths and version.
Characters can now be told which phrases they may never utter. An
instance-wide Taboo list is stored in instance settings and rendered
into the universal, cache-stable portion of every character's system
prompt on conversational turns, between the math-notation note and the
per-turn tool instructions. Saving normalizes the list — each entry
trimmed, blanks dropped, case-insensitive duplicates removed keeping
the first — while deliberately preserving the order you arranged, so
the cached prompt prefix only shifts when you actually edit the list.
An empty list renders nothing at all, so an instance that never touches
the feature produces a byte-identical prompt to one built before it
existed. The prompt-structure cache version moved from 2 to 3
accordingly. The list travels: it exports with instance settings and
rides along in full backups. Introspection (the self-inventory tool)
deliberately omits the section, matching the reference app. The list is
read and written over `/api/v1/settings/taboo`, which merges a partial
body over the stored value so an incomplete request can never wipe it,
and is edited from a new Taboo card on Settings → Chat, between
Dangerous Content and Data Retention. The card sends the whole list on
every add and remove and renders back exactly what the server stored,
so the normalization is visible where you made the edit. The Taboo help
page from the reference app is not carried over — this build has no help
surface to show it on.
Fixed a check in the oracle-recipe sweep tool that was flagging
correct recipes as broken. It warned whenever a recipe pointed the
test runner at a directory ending in the standard layout — which the
correct, staged form also ends in — so sixteen already-correct
comparisons were reported as debt and the tool refused to run them
from a working branch. All sixteen now run there, proven by running
them. Nine recipes that really were in the old form were converted,
and two doc comments that opened a sentence with a command word (and
so were being executed as shell) were reworded. No product code
changed.

#### `8e51f3da` — 2026-08-05 — Work the recipe-rot tail down; pin the sweep at a v4 worktree (P4.40 unit 4)

_No crate versions bumped._

Repaired the last broken oracle-regeneration recipes and gave the
sweep driver the one option a catch-up round needs: a way to point
every recipe at a pinned copy of the reference app, so a sweep run
while the reference app is ahead of us cannot quietly bake in changes
we have not absorbed yet. Re-running the known-broken set from that
pin found fifteen of sixteen already healthy. The sixteenth — a
comparison two earlier rounds had argued over and left red — turned
out to have exactly the same cause as the two oracles repaired
earlier today, and is now green. No product code changed.

#### `97340a71` — 2026-08-05 — Deflake the terminal-typing and rename-toast e2e beats (P4.40 unit 3)

_No crate versions bumped._

Hardened two end-to-end checks that could fail on a busy machine
without anything being wrong with the app. The terminal check typed
its "exit" the moment the terminal appeared, but the connection
underneath opens a moment later and silently discards anything sent
before it does — so on a slow run the command was never sent and the
check waited out its clock for a shell that had not heard it; it now
waits for the shell's own first output first. The rename check looked
for a "Chat renamed" notice by text alone, and when an earlier notice
had not yet faded there were two on screen and the lookup became
ambiguous; it now waits for the save round trip and matches the
success notice specifically. Both failures were reproduced on demand
before being fixed, and neither check asserts less than it did.

#### `bac876f0` — 2026-08-05 — Deflake failed_job_emits_a_tracing_event (P4.40 unit 2)

_No crate versions bumped._

Fixed a test that could report the job runner had stopped logging
failures when it had not. The check installed its log capture only for
its own thread, but the logging library decides once, process-wide,
whether a given log statement is worth evaluating — and a sibling test
running first, with no capture installed, could switch that statement
off for everyone. Under a parallel run the check failed roughly two
times in three. The capture is now installed process-wide with
per-thread buffers, which is what makes the statement reachable no
matter which test ran first. Nothing it proves changed: the failure
event must still be a warning on the same log target, and the job row
must still end up marked failed.

#### `467c038a` — 2026-08-05 — Repair the two unregenerable tier-3 oracles (P4.40 unit 1)

_No crate versions bumped._

Repaired two differential test oracles that could no longer be
regenerated. Both drive the reference app's real memory and
conversation-summary code, and both had started failing outright
because their test data used placeholder connection-profile names
where the reference app now requires real identifiers — so every
call-log row it tried to write was rejected, silently, and the
comparison had nothing to read. The test data now uses well-formed
identifiers and both comparisons run again, with a note in each
recording why. No product code changed.

#### `2a0c0b81` — 2026-08-05 — Plan the Taboo + maintenance round (P4.37-resumed ∥ P4.D50 ∥ P4.40)

_Docs-only change._

Planned the next porting round and committed its work orders — three
lanes. The first resumes the held Almanack server work from its
preserved branch: the report collectors get the equivalence test they
were held back for, the host wiring that makes the report reachable in
production, and a date-format arm for the space-form timestamps real
instances carry. The second absorbs the reference app's newest
feature, Taboo — an instance-wide list of phrases no character may
utter, stored in instance settings, normalized on save (trimmed,
deduplicated case-insensitively, order preserved), rendered into the
cacheable prefix of every character's system prompt, and edited from a
new Settings → Chat card. The third is a maintenance sweep: repairing
the two differential oracles that can no longer regenerate, curing a
flaky tracing test and three intermittent end-to-end beats, and
working down the remaining broken regeneration recipes. Documentation
only — the implementation is the two new work orders plus the resume
assignment added to the Almanack order.

#### `a892b153` — 2026-08-05 — Measure real durationMs on both image-path llm_logs sites

_Versions: core 0.0.475._

Image-generation calls now record how long the provider actually took.
Two internal call-log sites (the avatar/story-background job path and
the in-chat image tool) were writing a duration of zero on every image
call, which made those rows read as unmeasured in the system report's
latency figures; both now measure the real wall clock around the
provider call, on the primary attempt and the Concierge reroute alike,
matching the reference app. Unit tests pin that the logged duration
actually brackets the provider call.

#### `4b23e1ee` — 2026-08-05 — Close the f7f1a956 round: round record, order headers, phase plan, baseline move

_Docs-only change._

Unified the `f7f1a956` Almanack round (partially): the reference app's
system-report rewrite is absorbed on every already-ported surface, and
the new report's foundations are in. The LLM call log gains the two
per-profile attribution columns (fresh schema follows the reference;
existing pre-migration instances stay readable, and the write refuses
exactly as the reference does), call sites now record which profile
served a call and how long it took, and a token-accounting bug is
fixed that had made autonomous-room daily token budgets a silent no-op
— rooms with a daily budget now actually pause or end when they spend
it. Restores remap the new columns. The Almanack's byte-exact markdown
renderer, its phase manifest, and a progress "phase" event landed with
a seven-case differential; the full settings card, report viewer, and
a shared segmented progress bar (also adopted by the Proving Bench and
search-importance meters) are live in the SPA — but the server-side
report collectors were deliberately held back pending their
equivalence test, so the card lists no editions yet. The provider
manifest generator was repaired (regenerating manifests no longer
drops the image-model lists) and perl was purged from the Docker
image. The unification review caught a stale test-schema copy that was
silently costing log coverage, restored the report viewer's heading
typography, and documented one layout divergence. Gate: 413 Rust test
binaries / 1,867 tests / 0 failed with twelve differential families
re-proven over oracles pinned at the reference baseline; SPA 3,914
tests / 0; Playwright 180 passed with only the deliberately gated
Almanack beat skipped. Versions: core 0.0.474, harness 0.0.403, host
0.0.59, web 0.0.61, tauri 0.0.6, SPA 0.5.412.

#### `36a4e485` — 2026-08-05 — P4.D49 units 7+9: the blast-radius sweep, two stale DDLs, and the inspector coverage

_Versions: core 0.0.472, harness 0.0.402, host 0.0.59, web 0.0.61, tauri 0.0.6._

The LLM Inspector shows which profile served each logged call, on
instances whose log database carries the new columns; older rows and
older instances read exactly as before.

#### `3b076024` — 2026-08-05 — P4.D49 unit 8: `f7f1a956` is NO-PORT, and its jest-TZ clobber is defused

_Versions: harness 0.0.401._

Harness only: the reference app now pins its test timezone to UTC before
forking test workers, which silently overrode the timezone our two
zone-legged oracle regenerations pass on the command line. Both now apply
their zone from a hook that runs early enough, prove it took, and record
it so a future override fails loudly instead of quietly re-recording the
wrong leg.

#### `9e3ca4f7` — 2026-08-05 — P4.D49 unit 6: the llm-logs profile ids join the new-account UUID remap

_Versions: core 0.0.471._

Restoring a backup into a new account now carries LLM-log profile
attribution across: a log row's connection-profile and image-profile ids
are remapped alongside the profiles themselves, so restored rows keep
naming a live profile instead of one that no longer exists.

#### `af4ecf8b` — 2026-08-05 — P4.D49 units 4-5: the token-usage un-zero and the autonomous daily budget it revives

_Versions: core 0.0.470._

Autonomous rooms' daily token budget now works. The query behind it
filtered on a condition SQL can never satisfy, so the spend it summed was
always zero and a configured daily budget never bound on anything; the
reference app found and fixed this and v5 had reproduced it faithfully.
A room with a daily token budget will now grant its grace turn and then
pause at instance-local midnight rollover, as it was always meant to.
Rooms without one are unaffected.

#### `5d4e361d` — 2026-08-05 — P4.D49 units 2-3: the llm_logs profile columns through the write spine and the six call sites

_Versions: core 0.0.469, harness 0.0.400._

LLM call logs now record which profile served the call. Every text call
made through a connection profile stamps its id; every image call stamps
its image profile id, including Concierge reroutes. The shared cheap-LLM
path and the content gatekeeper now also measure how long the provider
took — both had been writing no duration at all, which left every
latency average hollow. Logs written before this change read exactly as
before, and an instance whose log database predates the new columns
still opens and reads.

#### `0a26dadc` — 2026-08-05 — P4.D49 unit 1: the D23 re-dump for the llm-logs profile columns

_Versions: core 0.0.468._

Adopted the reference app's new LLM-log schema: a freshly provisioned
instance now creates the call log with the two profile-attribution
columns (`connectionProfileId`, `imageProfileId`), re-dumped from the
reference app's live schema generator rather than hand-edited. No
indexes changed.
Generalized the progress side-channel so any long operation can narrate
named phases, not just chat creation. Existing progress messages are
unchanged on the wire.

#### `f9816ad5` — 2026-08-05 — P4.37 units 1-2: the Almanack's pure core + the tier-1 render differential

_Versions: core 0.0.473, harness 0.0.403._

Added the pure core of the new system report ("The Almanack"): the
seven-phase manifest, the report data model, and the markdown renderer,
all byte-identical to the reference app. The renderer is verified against
the reference app's real code over seven inputs covering every branch,
including empty sections, an unreachable document store, and locale
number and date formatting. No user-visible surface yet — the collectors
and the report API follow.
Added browser tests for the Almanack card and marked the parity
checklist row done. The card and its deep link are covered now; the
full compile-and-view walk is written and waits on the server half.

#### `b2cb084f` — 2026-08-05 — Port the Almanack card and its report viewer onto the Providers tab

_Versions: SPA 0.5.411._

Added the Almanack card to the AI Providers settings tab, with its
report viewer. Compiling a report shows a phase-by-phase progress bar
while it runs and opens the finished report when it's done; previous
editions can be viewed, downloaded, or deleted. The card answers the
same `?section=capabilities-report` deep link older bookmarks use.

#### `cf060b98` — 2026-08-05 — Move the Proving Bench and search-importance meters onto qt-progress

_Versions: SPA 0.5.410._

Moved the two existing meters — the Proving Bench outcome shares and the
search results' importance bar — onto the shared progress styles. Both
were painted with hardcoded colors no theme could reach; they now draw
from the same variables as every other bar in the app.

#### `b8df4641` — 2026-08-05 — Add the shared segmented progress bar and the qt-progress CSS family

_Versions: SPA 0.5.409._

Added the shared progress bar and its themeable `qt-progress` style
family. The bar shows one segment per phase of a long operation, sized
by how long each phase usually takes; the running segment stops at 90%
so a slow phase reads as still working rather than finished and stuck.

#### `44ee9c5d` — 2026-08-05 — Purge perl-base from the Docker runtime image

_No crate versions bumped._

Mirrored the Almanack's client contract into the web app: the four
dispatch verbs (generate, list, get, delete), the new `phase` progress
frame on the shared event stream, and the seven-phase manifest whose
labels and timing weights must match the server's exactly.
Purged `perl-base` from the Docker image. Debian's slim base ships it as
an Essential package carrying a set of critical and high CVEs with no fix
available, and nothing in the image is perl — the runtime is two compiled
Rust binaries. The TLS root bundle and the timezone database, the two
things the server actually needs from the base image, are untouched;
verified by building the image and walking a fresh container through
setup, the seeded home dashboard, a restart, the in-container CLI, and
`QUILLTAP_TIMEZONE=America/Chicago` resolving. Installing packages inside
a running container is no longer reliable, which is an acceptable trade
for a dev-grade image.

#### `3be673f5` — 2026-08-05 — Teach gen-provider-manifests.mjs the imageGenerationModels field

_No crate versions bumped._

Repaired the provider-manifest generator so regenerating the nine
built-in provider manifests no longer silently drops each provider's
image-generation model list. The list had been added to the committed
manifests by hand and never taught to the generator, so a regen deleted
it from five of them — google, grok, openai, openrouter, z_ai — and the
only symptom would have been an empty model dropdown when configuring an
image profile. The generator now reads the list off each built plugin
where the plugin exposes it, and out of the plugin's own image-provider
source for the two that do not; an unrecognized source shape stops the
run with a named error instead of emitting a stale manifest, and nothing
is written until all nine build. Regenerating now reproduces all nine
committed manifests byte for byte.

#### `1585456f` — 2026-08-05 — Plan the f7f1a956 Almanack round: four work orders + the drift classification

_Docs-only change._

Planned the `f7f1a956` Almanack round and committed its four work
orders: the reference app rewrote its system capabilities report into
The Almanack, adding per-profile attribution columns to the LLM call
log and fixing a token-accounting bug that had made autonomous-room
daily token budgets a no-op. One order absorbs those changes on the
already-ported surfaces (schema, call-site logging, the budget fix,
backup remapping), two port the new Almanack report itself (server
collectors/renderer/API, and the settings card with a new shared
progress-bar component), and a fourth repairs the provider-manifest
generator so regenerating manifests no longer silently drops the
image-model lists. Planning docs only; no code changed.

#### `9d62dd51` — 2026-08-05 — Close the 7189a968 round: round record, order headers, phase plan, baseline move

_Docs-only change._

Unified the `7189a968` round: the reference app's import/export overhaul
is fully absorbed. Exports no longer carry memory embeddings (99.7% of a
real 791 MB archive; the importer re-embeds what it inserts, one queued
job per memory), the export picker reaches all fifteen types (files with
their bytes, prompt templates, provider models, plugin settings with
password-key redaction, instance settings minus the instance-local
keys), a long-standing ordering bug that silently dropped every group's
linked document stores on import is fixed, and an opt-in compact backup
leaves the search indexes behind and rebuilds them after restore. The
import preview now explains per-item notes ("secrets withheld…"), and
the backup dialog gained the compact checkbox. Separately: the reference
app's Anthropic SDK jump was proven byte-neutral on the wire; the Docker
container now honors QUILLTAP_TIMEZONE/TZ so scheduled rooms and
same-day recall stop running on UTC; and a stale harness mock that had
been mis-reporting a context-summary divergence was retired. Gate: 412
Rust test binaries / 1,854 tests / 0 failed with twelve differential
families re-proven over oracles pinned at the reference baseline; SPA
281 files / 3,870 / 0; full Playwright green with the round's gated beat
live (numbers in the round record). Versions: core 0.0.467, harness
0.0.399, web 0.0.60, SPA 0.5.407.

#### `881dc3a0` — 2026-08-05 — Port compact backup + the restore tail (P4.D46 u5); pin the drifted v4

_Versions: core 0.0.467, harness 0.0.399, web 0.0.60._

Compact backup lands end to end (P4.D46 unit 5, reference `7189a968`):
opt-in via `compact: true` on the backup-create call (body optional, a
malformed one treated as absent, only literal true engages), memory
embeddings nulled at their schema slot, the six derived embedding
collections omitted from the archive outright — absent is what shrinks
it — and `manifest.compact` stamped only when true, so a full backup's
manifest stays byte-identical. Restore gained the matching tail: a
compact archive enqueues a full re-index before the embedding reconcile
(so the reconcile's dedupe sees it — proven by the oracle, whose
reconcile reports no second enqueue), and every restore now ends with
the dimension reconcile, its outcome riding the summary as
`embeddingReconcile` with the reference app's warning sentences. A new
committed compact archive, built by the reference app's real
createBackup, drives the new differential arms; the eleven existing
restore cases regenerated green with the tail, and the two compact
cases extend the ruled replay-dedupe divergence by name. Found on the
way: the reference app's restore-oracle environment had its vector
store globally mocked, which made the whole reconcile throw into its
catch the moment a restored corpus carried real vectors — the oracle
now uses the real module, same class as the embedding-service mock.

#### `9666907d` — 2026-08-05 — Port the 7189a968 export/import server surface (P4.D46 u2-u4, u6 tri-state)

_Versions: core 0.0.466, harness 0.0.398, web 0.0.59._

The five new export types land server-side (P4.D46 unit 4, reference
`7189a968`): files (folder tree, metadata, and bytes as chunked base64
through the same counted-arrivals reassembly the document-store blobs
use), prompt templates (built-ins never travel), the provider-model
catalogue (a regenerable cache, exportable for air-gapped instances),
plugin configs (every password-typed manifest key redacted, the whole
config withheld when the manifest can't be resolved — v5 carries a
static transcription of the bundled manifests, generator included), and
instance settings (minus the five non-portable keys). The entity
listing and both previews cover all fifteen types, and the importers
land with all four conflict strategies: file bytes are written back
through the same storage bridges the backup restore uses (storage keys
never transfer; post-bridge mime/size win; dangling links dropped so
cascade-delete can't be fooled), prompt templates dedupe by name,
provider models upsert by provider and model id, plugin configs merge
so a redacted key can't clobber a local secret, and instance settings
overwrite unconditionally. The plugin-config upsert gains the reference
app's tri-state `enabled` flag, passed by restore and import both, so a
plugin the user had switched off doesn't come back on. Differentials:
the export family grew to 57 cases, the import-read family to 25, and
the import-execute state family to 23 — including a four-strategy
files matrix whose first run caught two real gaps (the mount-stats
refresh v5's bridge leaves to callers, and the upload-failure message
wrapper) and one oracle artifact (v4's fire-and-forget stats refresh
caught mid-air by the state dump; the oracle now settles pending tails
before dumping, which is also what keeps them from poisoning the next
case).

Embeddings no longer travel in `.qtap` exports, matching the reference
app's `7189a968` overhaul (P4.D46 units 2–3). The writer strips the
field at both memory emit sites, the NDJSON reassembler drops it from
older archives that still carry it, and the legacy import path drops it
again before validation. Every memory an import creates now gets its own
`EMBEDDING_GENERATE` job queued after the reconcile — previously nothing
re-embedded and semantic search stayed broken until the next restart —
with the reference app's exact bail-out warnings when no default
embedding profile is configured or the default is the built-in TF-IDF
one. Document stores also import before the group→store link step; they
ran dead last, so in a mixed archive every group's linked stores were
silently dropped (proven by mutation: moving the step back turns the
differential red on `group_doc_mount_links`). The entity listing gains
`groups` and `document-stores` and the import preview gains the
`documentStores` array, both from the same reference commit. The
import-execute oracle also stopped lying: the jest setup file's global
embedding-service mock had `getDefaultEmbeddingProfile` pinned to null,
so the oracle claimed the reference app never enqueued — the
`doMock(requireActual)` antidote plus a processor-wake stub fixed it,
and the job rows the state diff now compares are real on both sides.

#### `0c6b9ee3` — 2026-08-05 — Widen the system-data fixture for the export/import drift port (P4.D46 u1)

_No crate versions bumped._

Widened the shared `system-data-*` test fixture for the export/import
drift port (P4.D46 unit 1): a second database store hard-linked to the
group, the Quilltap Uploads mount with its `userUploadsMountPointId`
pointer, two files with real bytes stored through the reference app's
uploads bridge (one project-less text file, one project-bound binary
over the 3 MiB chunk size), an embedding-bearing memory, a built-in
prompt template, a switched-off bundled-plugin config, and four more
instance-settings rows. No production code changed; every consuming
differential regenerates over the rebuilt DBs as the round's units land.
The Export Data picker now offers all fifteen kinds of data the writer
can produce, not seven. Prompt templates, projects, character groups,
document stores, files and folders, provider models, plugin settings,
and instance settings were all exportable and none of them could be
reached from the screen. The list is now exhaustive by construction —
a kind without a label no longer compiles — and the picker's order and
wording match the reference app exactly. None of the eight new kinds
needs an extra wizard step: each is a two-choice, four-step flow, and
the names the server composes are shown exactly as they arrive.

#### `0ff2a759` — 2026-08-05 — Show the import preview's per-item notes and new sections

_Versions: SPA 0.5.405._

The import preview lists those new kinds too, and now carries the
server's per-item notes: a file whose contents did not travel says so
before you import it, and a plugin's withheld secrets are named so you
know what you will have to type back in.

#### `045c95e7` — 2026-08-05 — Offer a compact backup, off by default

_Versions: SPA 0.5.406._

Create Backup offers a compact archive, off by default. It leaves the
search indexes behind — a restore rebuilds them — for a considerably
smaller file. Full fidelity stays the default on purpose: a backup
usually returns to the same instance, where those indexes are still
good, and rebuilding costs time and money at the worst possible moment.

#### `c26a0c93` — 2026-08-05 — Honor QUILLTAP_TIMEZONE / TZ in the container (P4.D48 unit 2)

_No crate versions bumped._

The browser walk for all of the above is written and waits on the
server half of the same change; it starts running the moment that lands.
Fixed the timezone in Docker. A container has no timezone, so it ran on
UTC — and that was not just cosmetic, because rooms that wake on a
schedule, the daily token allowance that turns over at midnight, and
"today" for same-day recall all read the clock directly. A room set for
7am fired at 2am. Worse, setting `QUILLTAP_TIMEZONE`, the one variable
the docs mentioned, fixed only the printed timestamps and left the
schedules on UTC, so it looked solved. Now either `QUILLTAP_TIMEZONE` or
`TZ` sets the whole process, whichever you supply fills in the other, and
`QUILLTAP_TIMEZONE` wins if the two disagree — matching the reference
app. The value must be an IANA name (`America/Chicago`, or `UTC`); an
abbreviation like `CDT` is refused with a warning instead of being
forwarded and silently falling back to UTC. `docs/developer/running.md`
documents it.

#### `83315ca5` — 2026-08-05 — Prove the be2c9cbb Anthropic-SDK jump wire-neutral (P4.D48 unit 1)

_No crate versions bumped._

Checked that the reference app's Anthropic SDK jump — 27 minor versions,
0.88 to 0.115 — did not change a single byte on the wire, and it did not.
All four recorded corpora that drive the Anthropic plugin (request
envelopes, response bodies, tool wire, streaming frames) were re-recorded
against the new SDK and compared byte for byte against the committed
copies: identical, including the streaming event types the upgrade put at
risk. The seven differentials that consume them pass unchanged. Nothing
to fix, but the check is the point: the last time an SDK was upgraded on
a claim of "no wire change," the proving run turned up two real bugs.
The same differential now also compares what that pass writes — the dated
episode memories it consolidates out of a folded stretch of conversation,
and the links it makes between an episode and the individual turns it came
from — rather than only checking that both apps asked the model the same
question. Doing so turned up a second stale mock: the test harness had
been standing in for the vector store with one that always returned no
matches, which meant the reference app's duplicate check never actually
ran. With that removed, both sides agree.

#### `346acc17` — 2026-08-05 — Un-mock the fold-episode pass in the context-summary oracle

_No crate versions bumped._

Retired a stale mock in the context-summary differential (test-only; no
shipped behavior changed). The fold-time episode pass — which turns a
just-folded stretch of conversation into dated episode memories — runs in
the reference app on every fold, and it runs here too. But the test's
canned model had no answer for the episode prompt, so on both sides the
call died as "unrecognized prompt" and the pass was suppressed in all but
name; the resulting row-count mismatch had been sitting as an unexplained
red. The canned model now answers it, so the pass runs to completion on
every fold in the corpus and both sides are compared on what it does.

#### `de624c15` — 2026-08-05 — Plan the 7189a968 round: four work orders for the export/import drift, the SDK wire check, and the stale-mock retirement

_Docs-only change._

Planned the next porting round (docs only). The reference app shipped
seven commits, headlined by an import/export overhaul on already-ported
surface: embeddings no longer travel in exports, five new export types
plus an exhaustive type listing, and an opt-in compact backup. Four work
orders cover it — the server port (`p4.d46`), its SPA half (`p4.d47`),
a wire-neutrality check on the reference app's Anthropic SDK jump plus
container-timezone handling (`p4.d48`), and retiring a stale harness
mock on the context-summary differential (`p4.36`). Also corrected two
stale status cells on dogfood findings #58/#59, which had stayed "open"
after their fixes landed.

#### `bb935688` — 2026-08-04 — Let a real export be imported, and say so when a preview fails

_Versions: web 0.0.58, SPA 0.5.402._

Large exports can be imported again. A `.qtap` over 100 MB — a real
character library runs to hundreds of megabytes once vault images are
included — was refused at the edge before any of the import code ran,
with an unhelpful "Payload Too Large". The ceiling now matches the
reference app's own, which is 10 GB and always was; the wrong one of its
two configured limits had been copied. Note that a very large import is
now possible, not cheap: the whole archive is held in memory while it is
read, so a multi-gigabyte file will cost multi-gigabyte memory until the
import path learns to stream.

The import wizard also stops going blank when a preview fails. It
recorded the reason and drew nothing — an empty "Step 2 of 5" with
working buttons and no explanation, in both apps. It now shows what went
wrong, a deliberate departure from the reference app in the
backup/restore/import/export family, where a lost explanation is worst
precisely when something has already gone wrong.

#### `eb045d97` — 2026-08-04 — Run P4.34's phase-2 handoff at unification and adjudicate the round's one red

_Docs-only change._

Unified the 7fe9fe40 drift-catch-up round onto the main line: four
parallel lanes, all closed, and the oracle baseline moves to the
reference app's head — no drift debt remains. The New Chat form gains
its Roleplay Template dropdown: pre-selected with what the chat would
have gotten anyway (project default, then your global default, then No
Template), hidden when no templates exist, and an explicit choice —
including a deliberate "No Template" — now beats both defaults at
creation, with an unresolvable template refused outright. Staff
announcements and the tool-execution rules stop teaching models
asterisk-delimited narration, which also retires a long-stale wording
gap in the native tool prompt. Importing a document store now matches
by the store's identity rather than its display name (renames can no
longer redirect an overwrite onto an unrelated store), imports preserve
archive store ids so re-imports recognize their own stores, and an
overwrite claims the whole store, folders included. The differential
harness's recipe rot is repaired rather than re-measured — most of the
"unrunnable" recipes turned out to be a measurement venue artifact, the
sweep driver gained a self-test and a batch mode whose results survive
the round, and the one remaining red is precisely diagnosed (a stale
oracle mock, not a port bug) and queued as its own small order. Gate
results, versions, and the unification review's findings are in the
round record.

#### `19b4279c` — 2026-08-04 — Repair the build-context fixture builder, its TZ pin, and its seed

_Versions: harness 0.0.396._

Repaired the differential harness's build-context fixture builder, which
had stopped building at all: the blob table that the reference app's
orphan collector now requires was created after character provisioning
rather than before it, and provisioning is itself what first reaches the
collector. Its regeneration recipe also gained the timezone pin it always
needed — without it the recall signature ring baked the host machine's
offset into the generated oracle — and now stages its case and corpus in
a scratch mirror so the recipe runs from a worktree checkout as well as
the main one. The seeded Commonplace Book message in that corpus was
updated to the new announcement wording, so no fixture teaches the old
shape.

#### `1531d941` — 2026-08-04 — Stop teaching models asterisk narration (the 7fe9fe40 re-port)

_Versions: core 0.0.463._

Staff announcements and the tool-execution rules no longer teach models
asterisk-delimited narration. Under a roleplay template that uses a
different narration delimiter, the asterisks in those strings taught the
wrong format outright. Thirteen strings across the Aurora wardrobe
announcements, the Aurora core whisper, the seven Commonplace Book
sections, and the Suparna mail whisper (including the blank-letter
placeholder) are now plain declarative lines: the wording is unchanged,
only the delimiters are gone, and a trailing dash becomes a colon where
the line introduces content. The native tool prompt's first rule now
illustrates itself in unquoted prose instead of quoted asterisk spans;
that block goes into the system prompt on every tool-enabled turn, so it
was the one unconditional source. This also retires a stale wording gap
in that same rule, left over from an earlier upstream change.
A browser walk covers the new template picker end to end: seed a
template, open New Chat, confirm the pre-selection is what the chat would
have gotten, pick the template, create, and read the created chat back to
confirm it carries the id that was on screen.

#### `32df594e` — 2026-08-04 — Pick a roleplay template when creating a chat

_Versions: SPA 0.5.400._

The New Chat form gained a Roleplay Template dropdown, beneath Play As.
Previously the template was decided silently at creation and could only
be seen or changed afterward from the chat's sidebar. The dropdown is
pre-selected with what the chat would have gotten anyway — project
default, then your global default, then No Template — with that option
marked "(default)", and it is hidden entirely when no templates exist.
Adding a character or switching projects re-seeds the default only until
you pick one by hand. The form sends the value it displayed, and omits it
entirely when the reads it depends on failed, so a read error can never
masquerade as a deliberate "no template".

#### `a334b095` — 2026-08-04 — Accept an explicit roleplay template when a chat is created

_No crate versions bumped._

Chat creation now accepts an explicit roleplay template. The create
request carries a tri-state `roleplayTemplateId`: present — including an
explicit `null`, which means "no template" — beats both the project
default and the user's global default; omitted keeps the old default
chain; an id that resolves to nothing is rejected with "Roleplay template
not found". The chat-creation differential's fixture gained three
roleplay templates and both defaults, and five new cases cover the five
resolution arms. The fixture change also gave every pre-existing case a
real template to carry, and that exposed a hole in the differential
itself: the id-normalizing diff could not tell one template id from
another, so two of the new arms passed even with the resolution
deliberately broken. The template id is now compared literally.
Audited every place the code could reach a document store by its display
name, confirming that nothing resolves a character's vault that way. The
one remaining name lookup is the startup repair that re-adopts a vault
whose link was lost, where there is no ID left to look up; it is now
labeled as such in place.

#### `342e8939` — 2026-08-04 — P4.33 arm 2: a document store's identity on import is its ID

_Versions: core 0.0.464, harness 0.0.397._

Import now identifies a document store by its ID rather than its display
name. Overwriting or skipping matches the store the archive actually came
from, so renaming a store — on either side — can no longer redirect an
overwrite onto an unrelated store that happens to wear the name, and an
archive from elsewhere that merely shares a name is created alongside
instead of claiming yours. For that to work, importing a store now keeps
the archive's store ID rather than minting a new one, so re-importing
the same archive updates the store it created the first time instead of
multiplying it; the display name is still made unique when it collides,
and "import as duplicate" still mints a fresh ID. This is a deliberate
difference from the reference app, which still matches by name.

#### `2f7a0be8` — 2026-08-04 — P4.33 arm 1: the import overwrite-clear claims the folders too

_Versions: host 0.0.58._

Fixed a test-suite race in the maintenance-sweep cadence check: it waited for
the terminal-session cleanup and then read the sweep timestamp, which is
written separately just afterward, so a heavily loaded full-suite run could
read it too early. Test-only; no product behavior changed.

Overwriting a document store on import now clears its folders too.
Previously an overwrite replaced every file in the store but left the old
folder tree standing, so folders the archive no longer mentioned became
permanent empty husks, and re-importing the same archive failed to
restore its own folders at all — every one collided with a survivor. The
store is now emptied completely before the archive's tree is written
back, which is what makes an import a faithful round trip. A real export
always carries the store's whole folder tree, scaffolding included, so
nothing is lost by clearing first. This is a deliberate difference from
the reference app, which still keeps the husks.
Recorded the post-fix autonomous-rooms sweep result alongside the
pre-fix one, so the committed evidence shows both states.

#### `3a97952a` — 2026-08-04 — Repair the rotten oracle recipes and pin the ruled failed-call divergence

_No crate versions bumped._

Repaired the differential harness's rotten oracle-regeneration recipes.
Re-measuring the families a previous sweep had written off found that most
of them were never broken — they only fail when the sweep runs from an
agent worktree, which the reference app's test runner ignores. The ones
that were genuinely broken had four distinct causes, each fixed: a recipe
that used a scratch directory it never created, one that copied into a
directory it never made, one whose complete recipe the driver had been
dropping on the floor, and a fixture builder that created a table the
reference app needs *after* the step that needs it. Nine more recipes now
stage their files outside the worktree so where you run them stops
mattering, two stopped pointing at temporary directories that no longer
exist, and four stopped running the entire test suite when they meant to
run one test.

The context-compression differential is green again: it was permanently red
only because v5 deliberately records a log row for failed cheap model calls
where the reference app records nothing, and that ruled difference is now
pinned explicitly — asserted in both directions, so it fails loudly if the
two ever agree again instead of hiding it.

Also fixed the autonomous-rooms oracle, which had been silently spawning a
child process that cannot start under the test runner; its crashes were
intermittently emptying the database mid-run.

#### `52112051` — 2026-08-04 — Fix the sweep driver's four defects and give it a durable batch mode

_No crate versions bumped._

Fixed the differential harness's recipe-sweep driver, which had been
misreporting working recipes as broken. Doc prose that happens to start
with a shell word no longer leaks into the generated script; the
skip detector no longer mistakes an environment variable name ending in
"_SKIP" for a skipped test; recipes naming a temporary reference-app pin
directory (which never survives the round that made it) are now flagged;
and recipes that hand the test runner a path inside an agent worktree —
where the reference app's test runner ignores everything — are flagged
and refused with an explanation instead of failing mysteriously. The
driver also gained a batch mode that writes its results to a file after
every family, so a sweep's per-family findings stop dying in temporary
storage, plus its own self-tests.

#### `f699ffae` — 2026-08-04 — Plan the 7fe9fe40 round: four work orders (P4.D44 ∥ P4.D45 ∥ P4.33 ∥ P4.34)

_Docs-only change._

Planned the next porting round and committed its work orders — four
lanes. Two absorb the reference app's newest drift: the New Chat form
gains its roleplay-template dropdown (pick the template at creation,
with the default chain pre-selected and an explicit "No Template"
honored), and the staff announcement strings plus the tool-execution
rules stop teaching models asterisk-delimited narration (which also
retires a long-stale wording gap in the native-tool prompt). The third
lane is the previously ruled import-overwrite repair, now slotted into
the round. The fourth is a maintenance pass over the differential
harness's regeneration recipes: the sweep driver's known defects, the
mechanically-unrunnable recipe repairs, pins for the deliberately
divergent error-row families, and a durable results artifact so sweep
classifications stop dying with the round. Documentation only — the
implementation is the four work orders.

#### `27219d98` — 2026-08-04 — Rule the import-overwrite escalation and order it as P4.33

_Docs-only change._

Ruled and ordered the import-overwrite repair the last round escalated:
overwriting a document store on import now means overwriting all of it,
folders included; a store's identity for import matching is its ID rather
than its display name (renames can no longer redirect an overwrite onto
an unrelated store, and imports will preserve archive store IDs so
re-imports recognize their own stores); and an audit ensures every
character-vault reference in the code resolves by ID. Documentation only
— the implementation is the new P4.33 work order.

#### `f3aa2d8f` — 2026-08-04 — Close the gate's two late catches: a doc-lint and the convert beat's weak toast locator

_Versions: core 0.0.462, SPA 0.5.399._

Unified the 49769ec4 drift catch-up and store-delete round onto the main
line: four parallel lanes, all closed. Every model request is now bounded
— a provider that accepts a call and never answers is abandoned in
seconds-to-minutes instead of wedging a turn for ten silent minutes
(45s/180s per cheap-task attempt, a 60s ceiling on the memory recap, a
300s default at the transport, and streaming carefully bounded only until
the first byte so a long answer is never cut off mid-generation). The
custom-tool run dialog gained presets: save the current parameter values
under a name into the character's vault, load them back from a dropdown,
reset to defaults — ordinary vault files, visible in the Scriptorium,
riding backup and export. Deleting a document store now takes all of the
store's rows with it in one transaction (the reference app leaks
documents and group links there — deliberate, pinned divergences), and a
startup pass reaps the orphans existing databases have already
accumulated, including the 43 links and 118 folders measured on the real
instance. The document-editing test oracle now runs the reference app's
real chunk-on-write, so the two long-red equivalence checks are green and
edited documents are positively proven chunked for search. The
unification review caught four real defects before they shipped — the
worst a fail-soft gate that would have silently disabled the orphan
repair forever on text-only databases. Gate: 412 Rust test binaries /
1,848 tests / 0 failed with fourteen oracle families regenerated from a
pinned reference checkout and re-run by name; Angular tests 278 files /
3,829 green; the full browser suite 177 passed / 0 failed / 0 skipped
with the new preset walk live.

#### `6a79f0ee` — 2026-08-04 — Repair the six doc-edit families' regeneration recipes (P4.32)

_No crate versions bumped._

Rewrote the regeneration recipes carried in all six document-editing
equivalence checks. The old ones pointed the reference app's test runner
at a path it silently ignores, so a regeneration matched nothing, left
the previous output sitting on disk, and let the check pass against a
stale comparison — which is how the chunk-on-write difference stayed
hidden. Each recipe now stages its case where the runner can see it,
names its target exactly, and was executed start to finish to prove it
works. Documentation inside the tests only.

#### `2e7c7cf0` — 2026-08-04 — Un-mock the doc-edit oracle reindex and prove the chunk pass (P4.32)

_No crate versions bumped._

Repaired the two long-red document-editing equivalence checks. The test
oracle had been silencing the reference app's own chunk-on-write step
along with a separate, still-unported trigger, so the reference looked
like it never indexed an edited document while the port did — a
difference in the test rig, not in either app. The oracle now runs the
real indexing code and seams only the piece the port genuinely omits,
and the checks additionally prove, on named files, that an edited
document really is chunked for search on both sides. All six
document-editing families' regeneration recipes were rewritten to
commands that actually run, since the broken ones are why this sat
unnoticed. Test-harness only; no application behavior changed.

#### `8a52de57` — 2026-08-04 — Rule the doc-edit oracle un-mock and order it as P4.32

_Docs-only change._

Ruled and ordered the repair for the two long-red document-editing
equivalence checks: the test oracle had been silencing the reference
app's own chunk-on-write step, making the reference look like it skipped
indexing when it doesn't. The oracle now runs the real indexing code, so
the comparison also proves edited documents get chunked for search. The
repair runs as a fourth lane alongside the round already in flight.
Checked the new request ceilings against the reference app across
seventy-five comparison suites, and confirmed the change moves the bytes
sent to model providers not at all. The check found one defect in the new
work — two test harnesses ran on a clock that had been switched off — and
a backlog of comparison suites that can no longer be re-run at all, which
is recorded for its own repair. No product behavior changed here.

#### `5c138336` — 2026-08-04 — Put a 60s phase ceiling on the memory recap (P4.D42 unit 3)

_Versions: core 0.0.455._

Put a one-minute ceiling on the whole memory-recap step of a turn. The
recap makes two network calls in a row, each already deadlined; this is
the backstop that keeps a turn from sitting on "Recalling…" no matter
which leg misbehaves. It sits above the per-call budget on purpose — a
recap that is merely slow in two places is still working — and when it
does fire the turn simply continues without the recap, which is optional
flavour rather than something the reply depends on.

#### `6e1a77de` — 2026-08-04 — Give every cheap-LLM attempt a deadline (P4.D42 unit 2)

_Versions: core 0.0.454._

Gave every background model call a deadline. Memory recaps, titling,
compression and extraction now abandon a provider that goes silent after
45 seconds (three minutes for a local model, where slow is not the same
as stalled), and each attempt gets its own fresh deadline. The provider
is handed a hard budget five seconds inside that one, so a stalled
request is closed at the socket rather than left running. An abandoned
call now says so in the log, with the provider, model, task and elapsed
time — the whole reason the original ten-minute stall was so hard to
find is that it said nothing at all.

#### `ccde0e93` — 2026-08-04 — Close dogfood #58's root cause: the store-delete cascade + the orphan reaper

_Versions: web 0.0.57._

Put a ceiling on every provider request. A model call that a provider
accepts and then never answers used to run all the way to the ten-minute
SDK default; without a caller-supplied budget it now fails after five
minutes, and a caller can hand down a shorter budget for one attempt,
which is never retried past. Streaming is bounded differently on
purpose: the ceiling there covers only how long a provider may take to
*start* answering, so a long reply is never cut off mid-sentence.
Deleting a document store now takes its contents with it, in one
all-or-nothing step: the files, folders, links, chunks, document bodies,
image data and the group and project links that pointed at it. Before
this, several of those were left behind with nothing referencing them —
invisible, unreachable, and impossible to clean up — which is what made
restoring a backup report dozens of raw database errors. A file that two
stores share is still spared: only the last store to let go of it takes
the contents down. Startup now also sweeps up the leftovers a database
has already accumulated, and the daily maintenance pass does the same
for servers that run for weeks without restarting.

#### `e4b71a7e` — 2026-08-04 — Measure and pin the import overwrite-clear folder gap, and escalate it

_Versions: harness 0.0.391._

Recorded, with a test that will notice if it ever changes: importing an
archive over an existing document store replaces every file but leaves
the old folders standing, so folders the archive no longer contains
survive forever. The obvious repair -- clear the folders too -- turned
out to delete a character vault's own scaffolding (Outfits, Prompts,
Scenarios, Wardrobe), so the behavior is unchanged pending a decision
about which folders an overwrite may claim.
Added an end-to-end walk for run presets against a real server and a
real vault — save a preset, change the values, deal the preset back,
reset to defaults — and mirrored the reference app's design note for
the feature.

#### `3b8aaecf` — 2026-08-04 — P4.D43 unit 3: the run dialog's Presets section

_Versions: SPA 0.5.396._

The custom-tool run dialog can now keep named presets. Set a tool's
parameters the way you like them, give the arrangement a name, and it
is filed in the running character's vault as an ordinary JSON document
— visible in the Scriptorium, hand-editable, and carried along by
backup and export. A dropdown deals a saved preset back into the form,
and a Reset button returns to the tool's declared defaults. Loading is
forgiving on purpose: a preset saved against an older version of a tool
fills in whatever still applies and leaves the rest alone, rather than
refusing to load. The section only appears where it can do something —
a tool that takes parameters, run as a character with a vault.

#### `095df46e` — 2026-08-04 — P4.D43 unit 2: the tool-presets naming contract, ported verbatim

_No crate versions bumped._

Ported the naming rules for custom-tool presets: a preset is a plain
JSON file in the character's vault, named for the tool and the preset
together, and the name is locked to lowercase letters, digits, dashes
and underscores as it is typed. The reference app's own tests for those
rules came across case for case.

#### `8836356b` — 2026-08-04 — P4.D43 unit 1: the roster listing carries the running character's vault

_No crate versions bumped._

The reference app's custom-tool run presets now work here too, first
half: the composer's run dialog is told which vault belongs to the
character a tool would run as, which is what decides whether the preset
controls appear at all. A character whose vault could not be read says
so plainly rather than going missing, so the dialog can hide the
controls instead of guessing.

#### `9d7c2a34` — 2026-08-04 — Plan the 49769ec4 drift catch-up + store-delete round (P4.D42 ∥ P4.D43 ∥ P4.31)

_Docs-only change._

Planned the next porting round and committed its three work orders: a
re-port of the reference app's new provider-request bounding (so a
stalled model call can no longer wedge a turn for ten minutes), a
re-port of the new custom-tool run presets (named parameter hands saved
into the character's vault), and a repair for the long-standing bug
where deleting a document store left its files, folders, and links
orphaned in the index — plus a startup pass that cleans up the orphans
existing databases have already accumulated. Documentation only; the
work itself runs in the round's three parallel lanes.

#### `57524682` — 2026-08-03 — Record the round: docs, order headers, phase plan, and the baseline move

_Docs-only change._

Unified the hard-link-groups drift and restore-remainder round onto the
main line: four parallel lanes, all closed. Hard links between document
stores are now real in v5 exactly as the reference app just made them —
a file linked into two places stays one file when either side is edited,
copies stay independent, orphaned content left behind by old edits is
collected on every write and swept once at startup, links survive export
and import as links, and the command-line file listing counts deliberate
links rather than coincidentally identical bytes. Along the way the port
found the reference app's own sibling-reindex half of that feature is
dead code (its fix is queued upstream) plus a crash it can hit on old
databases. The backup family closed out the latest walk's findings:
wiping data now clears conversation annotations, restoring an archive
with orphaned rows skips them with plain-English sentences instead of
fifty raw database errors, a backup that cannot collect a file now says
so at backup time, and background jobs hold still while a restore or
delete-all is rewriting the shelves. A new migration-vintage test
fixture — the reference app's real migration chain replayed from
nothing — makes these vintage-shaped faults provable for the first
time. The remaining screens owed notification messages got them
(ninety-two sentences, byte-for-byte), and every conversation now
renders with its own roleplay template's patterns instead of the
defaults. The unification review caught and fixed a twin-write-path gap
that would have left hard-linked siblings stale on the main editing
surfaces, sealed the round's deliberate two-vintage oracle seam, and
pinned every new deliberate divergence in both directions.

#### `9283008b` — 2026-08-03 — Dump the link group, and regenerate what the collection moves

_Versions: harness 0.0.386._

Editing a document in a store no longer leaves its previous contents behind
for good. Every edit stores the new version under a new entry and repoints the
file at it; the entry it left was never collected, so a long-lived instance
accumulated dozens of abandoned document and image bodies. They are now
collected as they are abandoned, and any backlog is cleared at startup.

#### `3536fee3` — 2026-08-03 — Count deliberate links in the CLI, not identical bytes

_Versions: cli 0.0.5._

The command line no longer reports a file as linked into three dozen places
when nothing was linked at all. Its links column counted every file that
happened to store identical bytes — an empty file, a shared boilerplate header
— because that is all a shared store row means. It now counts links actually
made, and lists only those under each file. An instance carried forward from
an older version reports one link per file rather than refusing the listing.

#### `8cea7fcb` — 2026-08-03 — Carry hard-link groups through export and import

_Versions: core 0.0.449, harness 0.0.385._

Backups and exports now carry hard links, so a file linked into two stores
comes back linked rather than as two documents that drift apart on the next
edit. Importing the same archive twice cannot merge the two copies into one
file: the link is re-made from scratch each time, never reused by name.

#### `045c5dfd` — 2026-08-03 — Bind a hard-link group when linking, never when copying

_Versions: core 0.0.448, harness 0.0.384._

Linking a file into a second store and copying it there are now different
things, as they read. A link makes one file with two paths: edit either and
both change. A copy is its own file from the moment it is written, even though
it shares storage with the original until then.

#### `1c7aad74` — 2026-08-03 — Keep hard-linked document-store files linked on write

_Versions: core 0.0.447, harness 0.0.383._

A file hard-linked into a second document store now stays one file. Editing
either side used to fork them apart silently: the write moved the edited
location onto fresh contents while the other kept the old ones, so the second
place served the previous revision indefinitely — to the file browser, to
search, and to characters. A write through any member of a deliberate link now
moves every member, and rebuilds the search index for each. Unlinking the
second-to-last member leaves an ordinary independent file behind. Contents
nothing points at any more are collected as they are abandoned, not left to
accumulate.

#### `8d263517` — 2026-08-03 — Record deliberate hard links in the document-store schema

_Versions: core 0.0.446._

Started making hard links between document stores real. A fresh instance now
carries the field that records a deliberate link, and an instance carried
forward from an older version gains it on startup. Startup also clears out
stored file contents that nothing points at any more — every edit to a
document in a store leaves its previous contents behind, and until now they
were never collected, so a long-lived instance accumulated dozens of them.
Backup and Restore now raise the same notifications the original does — a
confirmation when a backup downloads or a restore finishes, and the failure
message as a notification as well as on the dialog.

#### `8635012d` — 2026-08-03 — Measure the vintage gap instead of reasoning about it

_Versions: harness 0.0.384._

Checked, against a database built the way real ones are built, that
restoring a backup never asks for a column the database in front of it does not
have. It does not — and the check found that the difference between a fresh
database and a carried-forward one is wider than assumed: forty columns across
four tables. One case is still not handled, and is recorded with a test that
fails as soon as it is.

#### `a215870a` — 2026-08-03 — Hold the pump still while the shelves are being emptied

_Versions: core 0.0.448._

Background jobs are now held still while a restore or a delete-all runs.
They used to keep claiming work and writing to tables the operation was in the
middle of emptying and refilling. They start again afterwards however the
operation ends, including when it fails — unless you had stopped them yourself,
in which case they stay stopped.

#### `d81e5546` — 2026-08-03 — Refuse to insert a row whose parent the backup never carried

_Versions: core 0.0.447, SPA 0.5.381._

Restoring a backup no longer fails on document-store rows whose store is
gone. A store deleted at some point in the past can leave its file links and
folders behind; the backup copies them out faithfully, and putting them back
used to fail with a bare database error — once per row, naming a filename and
nothing else. Restore now checks that a row's parent is in the backup before
trying, and says which rows it skipped and why. Nothing about how backups are
written has changed, so archives you already have benefit too.

Creating a backup now tells you when it could not read one of your files.
Those files were skipped in silence, and the only sign was an error at restore
time, possibly months later. The names now come back with the backup, and the
skips are written to the server log as well.

#### `b6539324` — 2026-08-03 — Sweep the margin notes away with everything else

_Versions: core 0.0.446, harness 0.0.383._

Deleting all your data, and restoring a backup over the top of an
existing library, now also clear the notes characters leave on individual
messages. That table was never cleared, so the old notes stayed behind and a
restore could fail trying to put the backed-up ones back — on databases carried
forward from older versions, once per note. The original leaves the same rows
behind; this version does not.
A hands-on walk now covers the whole path end to end: a template with its
own narration marks is made, hung on a conversation, and the line already on
screen re-dresses itself without a reload — then goes plain again when the
template is taken away.

#### `9fdae5b5` — 2026-08-03 — Render each conversation with its own roleplay template

_Versions: SPA 0.5.382._

Conversations now render their messages using their own roleplay template's
patterns. Until now nothing ever read the template a conversation was set to,
so every message everywhere was drawn with the built-in marks — a template
that dressed narration or dialogue differently had no effect on anything you
could see. Its patterns now reach settled messages, the reply being typed as
it streams, a character's shown reasoning, and an opened announcement alike,
and they follow a template swapped mid-conversation without a reload. A
template that has been deleted, or that supplies no patterns of its own,
quietly falls back to the built-in marks, as the original does.

#### `23a427dd` — 2026-08-03 — Teach the render parity corpus about roleplay templates

_Versions: SPA 0.5.381._

Widened the test that holds message rendering to the original's, so it now
also covers conversations whose roleplay template supplies its own patterns
rather than the built-in ones. Eleven new cases were captured from the
original app itself, covering custom patterns, custom dialogue marks, and
both of the ways a template can fall back to the defaults. Every one of the
forty existing cases came back unchanged. No visible behavior changed here;
this is the measuring stick the next change is checked against.
Added an automated test that walks three real actions end to end — saving
a character, allowing any character into a project, and creating a
folder in Files — and checks that each one shows its confirmation
message.

#### `2433acb0` — 2026-08-03 — Close out Tier 1: API key associations + tag-create toasts (P4.29 unit 12)

_Versions: SPA 0.5.389._

Gave the API key dialog a confirmation message for each connection
profile it automatically links to a newly created key, matching the
original. Gave the character tag editor an error message when creating a
brand-new tag fails.

#### `f7abb286` — 2026-08-03 — Give New Chat its v4 toasts, retiring the inline banner (P4.29 unit 10)

_Versions: SPA 0.5.388._

Gave the New Chat screen confirmation and error messages for every
outcome: loading its data, every validation refusal (no character picked,
an autonomous room needing two LLM characters and no user, a missing
connection profile, no LLM-controlled character at all), and both
creating a chat and creating an autonomous room. An inline banner v5 had
added for all of these is gone, matching the original, which shows every
one of them as a passing message only.

#### `d8006093` — 2026-08-03 — Give the general Files page its v4 toasts (P4.29 unit 9)

_Versions: SPA 0.5.387._

Gave the general Files page confirmation and error messages for every
action: loading the list, deleting a file (both the plain and the
"delete anyway" paths), syncing the filesystem, and both orphan-cleanup
actions. All of these had shown a plain browser alert box on failure and
nothing at all on success. Also added a confirmation for downloading a
file, and for creating a folder (including trying to move it to another
project) -- an inline error banner v5 had added for the create-folder
and move-to-project dialogs is gone, matching the original.

#### `484f9d52` — 2026-08-03 — Give the project detail screen its full toast set (P4.29 unit 7)

_Versions: SPA 0.5.386._

Gave the project detail page confirmation and error messages across all
of its settings: the header save, Allow Any Character, removing a
character from the roster, agent mode, answer confirmation, the default
roleplay template, avatar generation, Lantern image announcements, the
story-background display mode, the default image profile, and removing a
chat from the project. None of these showed anything before. An inline
error banner v5 had added for four of these actions is gone, matching
the original, which shows none of them inline.

#### `30624987` — 2026-08-03 — Give the group editor its save/add/remove-member toasts (P4.29 unit 5)

_Versions: SPA 0.5.385._

Gave the group editor confirmation and error messages for saving a
group's details and for adding or removing a member, matching the
original -- which shows none of these inline, only as a passing message.
An inline error banner v5 had added for these four actions is gone.

#### `8b0943f0` — 2026-08-03 — Give template replace/restore and header toggles their toasts (P4.29 unit 4)

_Versions: SPA 0.5.384._

Gave the character detail page confirmation and error messages for
converting between character and NPC, and for the favorite/Carina/
controlled-by toggles in its header, plus messages for the Details tab's
four name-to-template and template-to-name replacement buttons.

#### `67f2b206` — 2026-08-03 — Port DescriptionsTab's physical-description toasts (P4.29 unit 3)

_Versions: SPA 0.5.383._

Gave the character editor's Appearance tab a Clear button for the physical
description, matching the original, and confirmation and error messages
for saving or clearing it — including refusing to save without a name.

#### `a98dfe4f` — 2026-08-03 — Give character save and avatar changes their v4 toasts (P4.29 unit 2)

_Versions: SPA 0.5.382._

Gave the character editor confirmation and error messages for saving a
character and for setting or clearing its avatar, matching the original.
The avatar picker's inline error banner is gone now that failures raise a
message the same way the original always did.

#### `8cb4a16e` — 2026-08-03 — Give the Characters roster its v4 notification toasts (P4.29 unit 1)

_Versions: SPA 0.5.381._

Gave the Characters page confirmation and error messages for actions that
previously finished silently: deleting a character (naming how many chats,
images, and memories went with it), toggling the favorite star, toggling
whether Carina will answer for a character, toggling who controls a
character, importing from SillyTavern, and resetting the built-in
characters. Two dialogs also lost inline error banners the original never
had, now that the same failures raise a toast instead.

#### `ceeec560` — 2026-08-03 — Plan the hard-link-groups drift + restore-remainder round (four orders)

_Docs-only change._

Planned the next porting round (documentation only): four parallel work
orders. The reference app moved one commit — making hard links between
document stores real, so a file linked into two places stays one file —
and one order absorbs all of it, including the cleanup of content rows
that older writes left behind. The other three cover the backup/restore
faults the latest hands-on walk surfaced (annotations that survive a
wipe, links and files missing from archives, jobs running mid-restore)
together with tolerance for databases carried forward from older
versions; the remaining screens still owed their notification messages;
and making chat messages render with the conversation's own template
patterns instead of the defaults.

#### `08284bba` — 2026-08-03 — Let settings writes tolerate a column the database lacks

_Versions: core 0.0.445._

Fixed a fault that could leave Quilltap unusable, showing an error on every
screen with no way to recover from inside the app. Settings are stored in a
table that, on a database carried forward from an older version, can be
missing a column this version knows about. Reading such a table already
coped; writing to it did not. Because Quilltap creates a settings row
whenever it finds none — which is exactly the state a restore leaves behind
if it could not put the old one back — the app would try that write on every
page load and fail. Writes now leave out columns the database in front of
them does not have, which is what the original does too. No database is
altered in the process.

#### `3f5dc700` — 2026-08-03 — Port v4's list-indent pre-pass, reversing the (a)-edge ruling

_Versions: SPA 0.5.380._

Stopped the editor from destroying nested lists in documents written
elsewhere. A list whose sub-items were indented a little less than the
parent's text begins — common in hand-written and Obsidian markdown, and
easy to produce under a numbered item like `20.`, whose text starts four
columns in — was read as two separate lists rather than one nested one.
Opening such a document and saving it wrote the flattened version back, so
the nesting was lost for good rather than merely displayed wrongly. In one
real case the sub-points stopped belonging to their numbered item
altogether. Indentation is now resolved from the shape of the document, as
the original does, and a document still saves at whatever indent width it
was written with. Found by scanning real documents: thirty instances across
five files.

#### `fdaa42d0` — 2026-08-03 — Drop the groups section's unused error-alert import

_Versions: SPA 0.5.379._

Removed an unused import from the Groups section of the Characters page. It
had been declaring an error-alert component it never displayed, which the
compiler flagged on every build. Nothing on the page changes: the original
also shows no error banner there, so a groups list that fails to load reads
as an empty one in both.

#### `66bc0605` — 2026-08-02 — Close the stray space before the may-write sentence's full stop (#52)

_Versions: SPA 0.5.378._

Closed a stray gap before the full stop in the run dialog's note about what a
tool may write.

#### `1bef3aad` — 2026-08-02 — Stop DOM events from impersonating output payloads (dogfood #51)

_Versions: SPA 0.5.377._

Fixed two ways the app confused a browser event for a piece of data. Running
a custom tool left its written-out parameters filled with a machine
placeholder the next time the tool was opened, because closing a field was
being mistaken for a change to what it held. The same mistake, found while
chasing the first, meant that **copying selected text out of a message
replaced what you copied with the word "undefined"** and claimed the message
had been copied — so anything you had lined up on the clipboard was lost.
Copying from a conversation now does what it says, and tool parameters keep
what you typed.

#### `8307136e` — 2026-08-02 — Restore the Project library button to the document picker (dogfood #50)

_Versions: SPA 0.5.376._

Restored the Project library button to the Open Document window. A
conversation that belongs to a project could not reach that project's own
files from the document picker at all — and turning on "Look everywhere"
did not help, because every project's store appeared there except the one
the conversation lives in. The store was being deliberately set aside for
a button that had never been built. It is there now, opens the project's
files, and stays put when the "Look everywhere" switch is toggled.

#### `bc9f51ab` — 2026-08-02 — Record dogfood finding #49: a side-effect counter cannot bootstrap

_Docs-only change._

Recorded a limitation of custom-tool side effects, found while trying them
on real data: a counter cannot start itself. An effect that adds one to a
stored value is skipped whenever that value does not exist yet, and the
only thing that would create it is the effect being skipped. The guard
forms an author reaches for are not available either — the expression
language has arithmetic and text joining, but no logical operators and no
defaulting, and a condition cannot ask whether a stored value is present.
Seed the value once in the state editor and the increment works from then
on. This matches the original app exactly, so the repair belongs there
first; it is queued with the other post-release improvements.

#### `18646d82` — 2026-08-02 — Rule the P4.D40 list-indent edge: the divergence stands, with a committed check

_Versions: SPA 0.5.375._

Ruled the one place the rewritten editor deliberately disagrees with the
original app, and committed the check that keeps the ruling honest. A list
item indented two spaces under a numbered parent is read as a sub-list by
the old editor and as a separate list by the new one, which follows the
Markdown standard its parser implements. The new behavior stands: the old
app's rule was a workaround for a since-fixed bug of its own, it never
writes that shape itself, and chasing it would mean rewriting every
document on open. Because the disagreement costs the nesting on the next
save rather than only on screen, the ruling is conditional on evidence — a
scanner now ships that finds the shape in a body of documents. It reports
nothing across the documents stored as plain files; the documents held in
the encrypted stores need the passphrase and are queued for a hands-on
pass, and anything it finds there reopens the decision with the repair
already identified.

#### `14d6655c` — 2026-08-01 — Fix the two e2e beats the merged branch broke, and the docs for the round

_No crate versions bumped._

Unified the round that catches this project up with two days of changes to
the app it is a rewrite of — ten commits, four of them landing on parts
already rewritten, absorbed across six parallel workstreams.

Custom tools can now record side effects. A tool may carry a short list of
conditional writes that run after the roll: into the scene's persistent
state — chat, project, group, or the general store, chosen by finding
where the value already lives — or onto the rolling character's own record
sheet. A written value can be a literal or a small expression (arithmetic,
joined text, references to the roll and its parameters), evaluated by a
closed parser with no identifiers and no function calls. A bad expression
is refused when the tool is loaded; one that fails at roll time skips that
single write and never sinks the roll. Tools can also carry a per-run chip
label, so a roll can name itself in the transcript, and a result whose
message opens with a list or a heading now renders properly instead of
being crushed onto one line. The workbench gained a card for authoring all
of this and shows what a trial run would write without writing it.

Announcements can now be whispered. The Insert Announcement dialog has a
"Who hears it" section; check nobody and the announcement is public as
before, check one or more characters and only they receive it in their
context. The collapsed chip says who it went to. Two related fixes came
with it: an announcement posted as a named character now tells the model
who spoke — previously it arrived anonymous, and a character in testing
attributed a private aside to entirely the wrong member of the staff — and
one high-volume class of internal whisper stopped ignoring the All
Whispers toggle. Whisper labels in all six bundled themes were too faint to
read and now meet the accessibility contrast bar in both light and dark.

Characters are dressed from all three wardrobes at the start of a chat.
Previously only a character's own wardrobe was read, so shared and
project-wide garments were invisible: default outfits living in a shared
wardrobe never got worn, an outfit assembled from shared pieces resolved to
nothing, and a character whose wardrobe was entirely shared was never even
offered to the model. Opening a chat with several characters now consults
for them at the same time rather than one after another (one stalled
provider used to hold up an entire cast), a stalled request gives up after
a minute and falls back to defaults, and "wearing nothing on purpose" is
now distinguishable from "the request failed".

The editor stopped reflowing nested lists. A document indented with four
spaces, three spaces, or tabs now comes back out the way it went in
instead of being rewritten to two on the first edit. Tab and Shift-Tab
indent and outdent list items, with matching toolbar buttons that also
work in raw-source mode, and Tab still moves focus everywhere else.

The review before merging caught a dependency block that a conflict
resolution had silently deleted — the code that needed it was landing
without it — and six recorded instructions for regenerating comparison data
that could no longer be followed, three of them pointing at scratch
directories that no longer exist. All were repaired; none was a fault in
the ported code. Gate: formatting, both lint configurations, a release
build, 409 test binaries and 1,798 tests with every one of the round's 42
comparison suites confirmed to have actually run, 3,639 unit tests across
268 files, a production bundle, and the full end-to-end suite — 172 of
172, no skips.

#### `1125cac3` — 2026-08-01 — Rebuild the fixture and prove where the writes land (P4.D35 unit 6)

_Versions: core 0.0.439, harness 0.0.377._

Rebuilt the custom-tool test instance so the side effects can actually be
measured rather than described. It now has a project alongside its conversation,
group and shared stores, and two tools that write: one whose single roll touches
all four stores plus the rolling character's own sheet, and one that keeps its
odds secret while still writing. Both test harnesses now read every store back
after a run and compare it to the reference app's, so "the write landed in the
right place" is a measurement, not a claim about a record. The Workbench's dry
run does the same in reverse: every bench case checks that nothing was written
at all, which is the only way to prove a preview stays a preview. A deliberate
break that silently skipped one store — leaving the record still claiming
success — was caught by exactly that check.

#### `051f6946` — 2026-08-01 — Wire the effects through both entrances (P4.D35 unit 5)

_Versions: core 0.0.438, harness 0.0.376._

Connected the custom-tool side effects to the two places a tool can be rolled —
a character reaching for it and the operator reaching for it from the composer.
The writes land before the result is announced, so if the announcement fails the
writes still stand; and an operator roll made as nobody edits nobody's character
sheet. The result bubble now puts the roll's name on its own line with the
message as a separate paragraph below, so an outcome that opens with a list, a
heading, a quotation or a code block renders as what its author wrote instead of
running into the heading. Messages posted before this change keep their old
one-line shape — a record is not rewritten. A tool that writes now says which
places it writes to, in the tool listing the model reads and in the panel a
person reads, without disclosing what it writes or when. Tools that keep their
odds secret keep their consequences secret too.

#### `4f68c41c` — 2026-08-01 — Port the side-effect applier (P4.D35 unit 4)

_Versions: core 0.0.437._

Added the part that actually performs a custom tool's side effects. A write
lands in the store where its key already lives — the conversation first, then
the project, then the group, then the shared store — and a brand-new key goes
to the conversation, the most local place with the least reach. Each store is
written at most once no matter how many effects touch it, in a fixed order, and
each write is caught on its own: if one store refuses, only its own effects are
dropped and the rest still land. A run nobody made writes to nobody's character
sheet. The user-only underscore keys are refused a second time here, behind the
refusal that already happens when the file is read. Nothing here can fail a
roll that already happened.

#### `5a80c6f5` — 2026-08-01 — Resolve chipLabel and effects in the execution core (P4.D35 unit 3)

_Versions: core 0.0.436, harness 0.0.375._

Taught a custom-tool roll to work out its chip label and its side effects.
The label is rendered once, after the outcome is chosen, so it can quote
anything the outcome message can — including the answer of a mid-roll consult.
The effects are worked out but not yet written anywhere: each one records
either the value it would write and where, or the reason it was passed over —
a condition that did not hold, a formula that could not be worked out, a
reference that resolved to nothing. Nothing sinks a roll. Passed-over effects
keep their place in the list rather than being dropped, so the audit trail
shows what the tool declared, not just what fired. The table audit ignores
both, as it should: an audit that wrote ten thousand times would not be an
audit. Verified by 12 new differential cases and three deliberate breakages.

#### `702a5c70` — 2026-08-01 — Port the effects + chipLabel definition schema (P4.D35 unit 2)

_Versions: core 0.0.435._

Taught the custom-tool file format to declare side effects and a per-run chip
label. A tool may now carry an `effects` list — up to sixteen conditional
writes, each naming where it writes and what — and an optional label template
for the result chip. Both are checked when the file is read, not when someone
rolls: a target that names nowhere writable, a formula that does not parse, a
reference to a parameter the tool never declared, and the underscore-guarded
state keys that belong to the user alone are all refused up front, in the
reference app's exact words. The check that walks an outcome row's subjects is
now shared with effect conditions rather than copied. Verified by growing the
format's differential corpus by 63 cases, with three deliberate breakages
proving the new key positions and the order rules are really being compared.

#### `a36e2413` — 2026-08-01 — Port the effect-expression grammar (P4.D35 unit 1)

_Versions: core 0.0.434, harness 0.0.374._

Ported the expression grammar that a custom tool's side effects will use to
compute what they write: arithmetic, string joining, parentheses, literals,
and the same `{{...}}` placeholders an outcome message already understands.
There are no identifiers and no function calls, so there is nothing to call
and nothing to reach beyond the roll's own values. A typo in a formula is
reported when the tool file is read, not when someone rolls. Every error
sentence is reproduced word for word against the reference app, including
the character positions it reports, which count the way text is counted in a
browser rather than in bytes. Verified with a new 125-case differential and
three deliberate breakages to prove it catches them.
Walked the new tool-authoring controls in a real browser: filling in a
chip label, adding a side effect, watching the editor object to a bare
phrase where a quoted one was meant, and confirming that the exact file
the editor would save carries both new pieces. Two further walks — the
test bench's dry-run display, and a labelled roll's chip in the
conversation — are written and waiting on the matching server work, each
held behind a named switch rather than a guess about what the server can
do. Also confirmed that the recorded reference results the browser is
checked against are unchanged by this release of the reference app, so
the existing checks still hold.

#### `4684ca88` — 2026-08-01 — Say what a tool may write, and label each run by its own chip label

_Versions: SPA 0.5.362._

Told the reader what a tool may change, and named each run by its own
label. The Run Tool dialog now lists the places a tool may write when it
runs — kept as a separate sentence from the list of things it quotes,
since consulting a number and changing it are different claims — and shows
that panel even for a tool that quotes nothing at all but does change
something. In the conversation, a roll's chip is now labelled by the
run's own rendered label when it has one, falling back to the tool's title
and then its name. Rosters from an older server, which say nothing about
writes, are read as writing nothing rather than as an error.

#### `f2d11272` — 2026-08-01 — Add the chip label and Side Effects card to the Workbench

_Versions: SPA 0.5.361._

Gave the tool workbench the two controls the new format needs: a chip-label
field beside the title, and a Side Effects card between the form and the
outcome table where a roll's consequences are written down. A condition too
rich for the card to draw is shown as a read-only badge rather than a
control, so it cannot be flattened by accident. The test bench's miniature
result now matches the real one — the heading stands over its own paragraph,
so an outcome that opens with a list or a quote reads properly — and it
lists what each effect would write, alongside a plain statement that the
bench computes those effects and never applies them.

#### `19c938e4` — 2026-08-01 — Carry chip labels and side effects through the draft layer

_Versions: SPA 0.5.360._

Extended the tool builder's working model to hold a chip label and a list
of side effects, and to hand them back unchanged when the file is saved
again. A condition richer than the form can draw — the kind a person writes
by hand in the raw file — is carried through untouched behind a read-only
badge rather than quietly dropped. Both new fields are audited as they are
typed: an unknown placeholder in the label is a gentle warning, while a
target that names nowhere writable, or a formula that will not parse, is a
blocking error said beside the row that caused it.

#### `7b743e85` — 2026-08-01 — Add chipLabel and effects to the browser schema twin

_Versions: SPA 0.5.359._

Taught the browser's copy of the custom-tool format about the two new
things a tool may declare: a per-run label for its result chip, and a list
of side effects — small writes a roll records once the dice have settled.
The browser now accepts, refuses, and explains exactly what the server
does for the same file, including the trap that catches every author once:
a bare phrase where a quoted one was meant. Checked against eighty-one
verdicts recorded from the reference app, compared down to the wording and
the order of the keys.

#### `b64d5745` — 2026-08-01 — Name announcement speakers in LLM context (P4.D37 unit 2)

_No crate versions bumped._

Taught the browser to read the new side-effect expression language for
itself. Custom tools can now carry small formulas — arithmetic, text
joining, parentheses, and references to the run's own numbers — and the
editor has to judge them the moment they are typed rather than waiting for
the server to refuse the file. The browser's copy of that reader reproduces
the reference app's wording exactly, down to the sentence an author sees
when they forget to quote a phrase, because the same complaint is also
raised on the server about the same file and the two must not disagree.
Verified against ninety-six results recorded from the reference app itself,
plus its own test suite ported case for case.
Announcements now tell the model who is speaking. When an announcement is
posted as a named off-scene character or under a free-text name, that name
was painted on the bubble but never reached the model, so the announcement
arrived as anonymous prose and the model guessed the speaker — badly, and
then carried the mistake into the scene. Each such announcement is now
prefixed with its speaker's name in the model's transcript only; nothing
stored, exported, or displayed changes, and an announcer whose name cannot
be resolved passes through unnamed rather than being given an invented one.
Re-running a turn does not stack the tag. The same pass also closes the
last gap where a private aside could reach a character it was not
addressed to: the single-character transcript builder applied its privacy
check to tool results alone, and now applies it to every kind of message.
Verified against the reference app's real code with a new twenty-six-case
comparison plus eleven regenerated end-to-end comparisons.

#### `3b51a0ba` — 2026-08-01 — Whisper an ad-hoc announcement to named participants (P4.D37 unit 1)

_Versions: host 0.0.55._

Manual announcements can now be whispered to specific people in a
conversation instead of always being spoken to the room. The composer's
audience is re-checked on the server against who is actually in the scene
right now, so an announcement can never be addressed to someone who has
left or to a stranger from another conversation — that is refused outright,
because the alternative is filing a note nobody could ever be shown.
Naming nobody posts publicly exactly as before. When the announcement is
rewritten in a character's own voice, the character is now told privately
who is listening and given those names in place of the room's roster, so a
private aside is not pitched like a declaration to a crowd. The rehearsal
itself stays forgiving: a stale name there simply drops out of the audience
rather than failing the preview. Verified against the reference app's real
code across fifty-six compared cases, with deliberate breakages proving
each new check can actually fail.
The Insert Announcement dialog can now whisper. A new "Who hears it" section
lists the chat's current participants with a checkbox each — leave every box
unchecked and the announcement posts publicly exactly as before, or check one
or more names and it becomes a whisper only those participants' contexts
include. The collapsed chip and the composer both say who a whisper went to,
and changing who hears it invalidates any in-character rewrite already on
screen, since a private aside reads differently than a public proclamation.

#### `9c1e5135` — 2026-08-01 — Show the whisper audience on both announcement render sites

_No crate versions bumped._

A whispered announcement's collapsed chip and its full-row Staff header now
say who it went to, the same "to Alice, Bob" tag either way, and the chip
wears the whisper's border color so a private aside is distinguishable from
a public proclamation before it's ever expanded.

#### `f4e14624` — 2026-08-01 — Port the overheard-whisper dim (tier 2)

_No crate versions bumped._

Turning on "All Whispers" now visibly dims the whispers that weren't meant
for the operator, instead of showing every one of them at full strength.
The dim keeps the whisper's border and label legible, so the operator can
still tell it's a whisper — just not one addressed to them.

#### `6ec31319` — 2026-08-01 — Re-port whisper-visibility.ts: kind-narrowing + operator-authored announcements

_No crate versions bumped._

The operator now keeps seeing their own private asides no matter who they're
signed as, and Prospero's busiest whisper — telling one character which group
shelves it may read — finally stays hidden behind the All Whispers toggle
instead of leaking into the main flow. The visibility rule that decides which
whispers the operator sees even with the toggle off is now keyed on the exact
kind of whisper (a private dice roll, a private tool run and its errors)
rather than on the sender alone, and a whisper posted through Insert
Announcement is now recognized as the operator's own writing and never
hidden or dimmed, whichever Staff member or invented name it's signed as. A
legacy row with no recorded kind keeps its old behavior rather than
disappearing from a view the operator is used to.

#### `f4ff60d4` — 2026-08-01 — Prove the tri-tier dressing on the create path, composite and all

_No crate versions bumped._

The "whispered to X" label on a whisper bubble, and the matching audience tag
on a collapsed announcement chip, were unreadable in light mode in five of six
bundled themes (Art Deco measured a near-invisible 1.07:1 contrast) and failed
the accessibility bar in two themes in dark mode as well. All six bundled
themes now clear the WCAG AA 4.5:1 contrast requirement for this label in both
light and dark mode, matching the reference app's fix.
Proved the whole thing end to end on the path that creates a chat, against the
reference app. Two characters open a project chat wearing the shared shirt, the
project's sash, and — for the one who owns clothes of her own — her jacket
layered on top in the right order. And when the model picks a shared outfit set
whose pieces also live in the shared collection, the pieces now appear: the
creation dialog lists the coat and the boots by name where before it showed an
empty outfit, because nothing had gone looking for parts the wearer doesn't own.

#### `bc742826` — 2026-08-01 — Measure the merged-pool consult and the deliberate-nudity contract

_No crate versions bumped._

Proved the outfit-choosing model now sees the shared wardrobe, against the
reference app. A character who owns no clothes is now asked at all — before,
the empty candidate list meant the model was skipped entirely and they were
dressed in nothing — and a character with a wardrobe of their own can pick a
shared garment without it being thrown away. The whole conversation sent to the
model is compared word for word, including the new paragraph asking it to say
when nakedness is the point, and the three ways an empty answer can arrive
(flagged, unflagged, and flagged with the wrong kind of value) each land where
they should.

#### `3f461207` — 2026-08-01 — Give chat-cast a shared wardrobe tier to measure against

_No crate versions bumped._

Proved the tri-tier dressing against the reference app on the add-a-character
path: a character who owns no clothes at all now joins wearing the shared
collection, shared and personal defaults layer in the order they were created,
and a character who keeps their own unmarked copy of a shared garment goes
without it. The test fixture had no shared wardrobe in it before, so none of
this was measurable there.

#### `740bf69f` — 2026-08-01 — Sort layered defaults and say what a default garment promises

_No crate versions bumped._

Layered default outfits now appear in the same order in the composer's preview
and in the chat that opens, and the "default" checkbox says what it will
actually do. Marking a garment default in a project store or in the shared
collection puts it on every character who can reach it, which is a much larger
promise than "part of this character's default outfit" — so the label now
depends on where the item is headed.

#### `08269619` — 2026-08-01 — Dress characters from all three tiers at chat start

_No crate versions bumped._

Characters are now dressed from all three wardrobe tiers when a chat opens.
Until now the two paths that dress everyone at the start — creating a chat and
adding someone to one — looked only in that character's own vault, so a
character whose clothes all live in a project's shared store or in Quilltap
General opened the scene wearing nothing, and the model that picks outfits was
never even asked. Defaults from different tiers now layer together in the same
slot, oldest first, and a character can still opt out of a shared default by
keeping their own copy of it unmarked.

Three problems from the first live run are fixed with it. Everyone's outfit is
now decided at the same time rather than one after another — a chat that took
two and a half minutes to open was waiting on a single slow reply — while the
saving still happens one character at a time, because it has to. A stalled
provider is given one minute and then the character wears their usual clothes.
And "naked on purpose" is no longer indistinguishable from "the model gave up":
the model is asked to say which it means, and only a plainly stated choice
counts.

#### `348a43fc` — 2026-08-01 — Hydrate a shared composite's components before unpacking it

_No crate versions bumped._

Fixed an outfit made of shared parts resolving to nothing. A composite garment
— a livery, a dress uniform, anything that bundles other pieces — used to be
unpacked using only the wearer's own wardrobe, so when the bundle lived in a
project store or Quilltap General its pieces were invisible and the whole
outfit quietly came out empty. The pieces are now fetched a level at a time,
one lookup per level, as deep as the unpacking itself will go. Listing a
character's wardrobe now asks the shared pool for its answer instead of
repeating the merge by hand.

#### `c72b9562` — 2026-08-01 — Add toolbar and source-mode list indent/outdent buttons

_No crate versions bumped._

Gave the wardrobe one place to answer "what can this character actually wear?".
The answer folds the shared tiers — Quilltap General plus any of the project's
own stores — underneath the character's own vault, so a character's private
copy of a house garment hides the shared one, and an archived copy simply
disappears, letting the shared item show through again. The merge deliberately
happens on the full lists and only then drops archived items, because a
character's opt-out from a shared default is itself an item, and filtering
first would throw it away before it could do its job. The equip path, which
needs archived garments so it can still name what someone is wearing, does not
use this and says so.
Added list indent/outdent buttons to the editor's formatting toolbar and its
raw-Markdown source mode (P4.D40, tier 1 unit 6, closing the sub-list
indentation feature). The two new buttons carry the reference app's exact
arrow glyphs, titles, and keyboard-shortcut hints, sit between the list/
blockquote buttons and the code-block toggle exactly where the reference
app places them, and dispatch the same sink/lift commands Tab already
reaches in rich-text mode. In source mode they read the raw text's own
indentation width — never a fixed default — and shift only the selected
list lines by it, matching the reference app's toolbar behavior line for
line. The button-sizing CSS rule (arrow glyphs at a slightly larger size to
match the text-label buttons) is copied over unchanged.

#### `3162067c` — 2026-08-01 — Add Tab/Shift-Tab list indent/outdent, confined to list items

_No crate versions bumped._

Gave the editor Tab/Shift+Tab list indent and outdent, confined to list items
(P4.D40, tier 1 unit 3) — Markdown has no way to represent an indented
paragraph, so pressing Tab anywhere else still moves focus like a normal
textbox. A ProseMirror caret always knows its full ancestor chain, so unlike
the reference app the check needed no special case for an ambiguous caret
position; it is simply true whenever either end of the selection sits inside
a list item. Tab nests the item under its previous sibling and Shift+Tab
lifts it back out, both dispatched through the standard ProseMirror list
commands, and — matching the reference app — pressing Tab on an item that
has nothing to nest under (the first item in a list) still counts as
handled rather than falling through to focus-move. Verified with real DOM
keydown events dispatched at the live editor component, including a
4-space document proving the indent still preserves that document's own
nesting width rather than the two-space default. Also fixes a latent gap
this uncovered: jsdom implements `Element.getClientRects`/
`getBoundingClientRect` but not `Range`'s, so any editor command that asks
the browser to scroll a change into view (already true of the pre-existing
Shift+Enter line break, just never previously exercised by a test) threw
under the test environment; a small shared stub answers both the way a
real browser's Range would.

#### `41933c12` — 2026-08-01 — Wire the unit-preserving post-pass into the markdown bridge

_No crate versions bumped._

Wired list-indentation unit memory into the editor's Markdown bridge
(P4.D40, tier 1 unit 2). Loading a document now records the nesting width it
was written with, and every save re-indents to that same width instead of
rewriting the whole list to a fixed two spaces — a two-space file stays
two-space, a four-space file stays four-space, a single edit no longer
reflows every nested line. The round-trip gate grew five entries covering
four-space and tab-nested bullets, nested and wide ordered lists, and
three-space bullets, plus two editor-integration specs proving the unit
survives an in-editor Tab/toolbar indent, not just a plain parse-then-save.
One divergence from the reference app is recorded and pinned rather than
silently accepted: a two-column list item nested under a numbered parent
(`1. a` / `  - b`) nests in the reference app but parses as two separate
lists here, because Markdown's own CommonMark rules require at least three
columns to continue a numbered item — the reference app's own output never
produces this shape, so only hand-written input can hit it, and a human
ruling is requested before building anything more invasive.

#### `3ccd3ebd` — 2026-08-01 — Port v4's list-indentation pure functions with a tier-1 differential

_No crate versions bumped._

Ported the reference app's list-indentation math for the editor's Markdown
bridge (P4.D40, tier 1 unit 1). The reference app resolves how deeply a
Markdown list item is nested from the document's own structure, not by
assuming every file uses two-space indentation, and re-indents its output to
match whichever spacing the file was written with instead of rewriting the
whole thing to one fixed width. New `editor/list-indentation.ts` ports the
pure detect/apply/source-line-shift functions verbatim; a new tier-1
differential drives the reference app's real module directly and byte-compares
every case against the port (24 cases, zero reimplemented). Not yet wired into
the live editor — that's the next commit.

#### `8d783e05` — 2026-08-01 — Plan the c4d4b0de drift catch-up round: six work orders (P4.D35-P4.D40)

_Docs-only change._

Planned the next porting round: the reference app moved ten commits in two
days, and six work orders now cover absorbing all of it — the Pascal
custom-tool side-effects feature (server and Workbench halves), whispered
manual announcements with speaker attribution in model context (server and
Salon halves), dressing characters from all three wardrobe tiers at chat
start, and the editor's sub-list indentation contract — plus theme
contrast corrections and two commits dispositioned as not applicable to
this codebase with evidence recorded. Planning surveys ran against both
codebases first, so each order carries verified file-and-line starting
points, the shared wire contract, and the exact oracle families it must
regenerate. Documents only; no application code changed.

#### `d65b4804` — 2026-08-01 — Close the deflake follow-up: the pgrep rule, the round record, and the docs

_Docs-only change._

Unified that deflake onto the main line. The review before merging checked
each of the change's stated reasons against the actual application code
rather than taking them on trust; all of them held, including the finding
that one race the tests now step around is a faithful copy of how the old
app behaves and so was correctly left alone rather than "fixed" into a
difference. One correction was made: a new comment described a field as
being sent as empty when the feature is off, when in fact the field is
omitted entirely and is a decimal number when on — which is why the tests
match it loosely on purpose. Left uncorrected, the next person to tidy
that match would have reintroduced the very flake this work removed.
Gate: formatting, both lint configurations, a release build, the full
workspace test run, the unit suite (3,210), a production bundle, and the
complete end-to-end suite — 168 of 168, no skips.

