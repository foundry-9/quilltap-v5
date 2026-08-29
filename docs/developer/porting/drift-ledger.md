# The v4 drift ledger

**The single standard record of where v4 stands relative to the oracle
baseline, what each drift commit touches, where it intersects work this port
has already landed, and what has been done about it.** Written by
`/driftcheck` (and by `/unify` when a round moves the baseline); read by
`/setupphase`, `/carryout`, `/unify`, and `/dogfood`. Those commands do NOT
re-derive drift — they run the §2 freshness probe and then trust this file.

Ownership: this file is written only from a main-checkout session
(`/driftcheck`, `/setupphase` marking rows ORDERED, `/unify` marking rows
ABSORBED, `/dogfood` when a walk changes a row's facts). **Lane agents never
write it** — a lane that finds the probe failing STOPs and reports instead.

---

## §1 Current state

_Updated only by `/driftcheck` and `/unify`. Every field here is what the §2
probe verifies against._

- **Oracle baseline:** `b121ac77f` — "feat(cli): rebuild a lost or
  passphrase-locked .dbkey from the pepper" (v4 main, 2026-08-27), adopted
  at the P4.D131 ∥ P4.D132 ∥ P4.D133 ∥ P4.65 round unification
  (2026-08-27).
- **Checked:** 2026-08-29 (a full `/driftcheck`; the previous full check was
  2026-08-27).
- **v4 `main` HEAD at check:** `a1d88aa3a` — **SIX commits past the
  baseline** (`package.json` `4.9.0-dev.99`). Four are new since the
  2026-08-27 check.
- **v4 `bugfix` tip at check:** `3a76b17df` — **unmoved** since the
  2026-08-27 content measurement (the bare 4.8.4 fork marker; its long
  `main..bugfix` commit list is the squash-topology lie, §4 step 2 —
  nothing genuinely unabsorbed).
- **v4 `release` tip at check:** `8736d7042` ("release: 4.8.4") — verified
  an ancestor of `main`.
- **Checkout at check:** branch `main`, **tree CLEAN**.
- **Verdict: DRIFT PENDING — 6 commits.** Two are large real ports on
  heavily-ported surfaces (`65f5021c8` provider/model fallback chains —
  **PORT-NEW**, a new feature with two `connection_profiles` columns; and
  `a1d88aa3a` bugs 106/107 — **PORT**, which rewrites the cheap-LLM budget
  ceilings v5 ported twice and adds the per-profile attachment re-decision).
  Two are docs-only NO-PORT? candidates (`3c3432ae9` release notes,
  `97ebfb9fc` the bug registrations), plus the two rows carried from the
  2026-08-27 check (`1560bd43b` the Lima/WSL2 retirement — PORT;
  `7819afb1d` CI/tests — NO-PORT?).
- **⚠ A freshly-banked 💸 proof has already expired (§5.5).** The
  2026-08-27 dogfood pass closed C4 PARTIAL on the **75 s compression
  budget** and recorded that the `[CheapLLM] Task failed` warn "needs
  >75 s, never once crossed in 400 real calls". `a1d88aa3a` raises that
  ceiling to **120 s** (background) / keeps **75 s** (interactive
  cache-miss) and the shared tier **45 s → 90 s** (interactive legs stay
  45 s) — so v5's ported constants, the `cheap_llm_exec.rs` deadline
  tests, and that dogfood row are all superseded upstream. Re-measure,
  do not carry the old numbers forward.
- **Regen rule in force: PIN REQUIRED** — v4 HEAD is six commits past the
  baseline, so every oracle regen runs from a worktree pinned at
  `b121ac77f` (§5.1) until a catch-up round moves the baseline.
- **Release shape:** still no `release: 4.9.0` squash and no 4.9 bugfix
  fork; v4 develops on `main` alone (the 4.9.0 release notes moved at
  `3c3432ae9`, so the squash may be imminent). Keep probing BOTH branches.

## §2 The freshness probe

What every consuming command runs before trusting §1 — four read-only
commands, no classification, no judgment:

```bash
git -C ~/source/quilltap-server branch --show-current
git -C ~/source/quilltap-server status --short
git -C ~/source/quilltap-server log --oneline <§1 main HEAD>..main
git -C ~/source/quilltap-server log --oneline <§1 bugfix tip>..bugfix
```

Pass = the branch and tree state match §1 and both logs are **empty**. Then
§1's verdict and regen rule stand; proceed on them. Any mismatch = the ledger
is stale: `/setupphase`, `/unify`, and `/dogfood` run `/driftcheck` (or its
§4 procedure) before continuing; a `/carryout` lane **STOPs and reports**.

A dirty tree matters even when HEAD hasn't moved: uncommitted `lib/`, `app/`,
`packages/`, or `plugins/` edits poison any regen run from the checkout
(§5.1's mid-lane note). Infra-only dirt (CI files, docs) still gets recorded
here so the next probe doesn't re-alarm on it.

## §3 The drift table

Classes: **PORT** (behavior change on a ported surface), **PORT-NEW** (new v4
feature to port), **CONVERGENCE** (v4 adopting a fix this port made first —
both-directions pins will trip at the baseline move *by design*; retire them
by measurement, §5.4), **NO-PORT?** (docs/infra/tests-only candidate —
needs ratification with evidence, never by subject line alone).

Dispositions: **UNPROCESSED** → **ORDERED(order-id)** (set by `/setupphase`)
→ **ABSORBED(round)** or **NO-PORT-RATIFIED(round)** (set by `/unify` when
the baseline moves past the commit). A row leaves this table for §6 only
when absorbed/ratified.

| sha | date | subject | class | intersects (already-ported work) | disposition |
|---|---|---|---|---|---|
| `1560bd43b` | 2026-08-27 | refactor(runtime): drop Lima and WSL2 support, Docker is now the sandbox | **PORT** | **Six ported surfaces.** (1) `lib/database/backends/sqlite/instance-lock.ts` → `quilltap-host/src/env.rs` (`EnvironmentType` still carries `Lima`/`Wsl2`; `is_lima_environment()`; the lima-before-docker probe order) + `quilltap-host/src/lock.rs` (the `is_vm` matches!, the `"Lima VM"`/`"WSL2 instance"` labels at 4 sites, the stale-reason arm) + `quilltap-cli/src/db_cmd.rs` `is_vm_environment()` (P4.1 host drivers / P4.D74 lock-order round / CLI Tier R). v4 now emits ONE label (`Docker container`) and narrows the cross-host heartbeat arm to `environment === 'docker'` — a **user-visible sentence change** the Tier R lock arms compare. (2) `lib/paths.ts` + `app/api/v1/system/data-dir/route.ts` → `quilltap-core/src/services/data_dir.rs` (`is_lima_environment`, the Lima-first `getPlatform`, `LimaContainer` env capture, `is_vm`) + `api/data_dir.rs` (the `isVM` response key AND its key-order list) → harness family **`data_dir_paths_equivalence`** (its `isVM` comparand) + SPA `screens/profile/profile.api.ts`/`profile.spec.ts`. v4 DELETED the `isVM` field — a wire/key-order change. (3) `lib/host-rewrite.ts` → `quilltap-core/src/provider_manifest/rewrite.rs`: five gateway strategies → two, and **`isVMEnvironment()` changes meaning** (`isDocker \|\| QUILLTAP_HOST_IP` — a self-managed VM now opts in by env var); the `/proc/net/route`, `/etc/resolv.conf` WSL2 and `/etc/hosts` fallbacks are gone. (4) `lib/mount-index/base-path-availability.ts` → the P4.D67 bug-56 port (`isContainerized()` drops the Lima disjunct). (5) `lib/tools/almanack/{types,phase1-premises}.ts` → `quilltap-core/src/almanack/{types,phase1_premises}.rs` + `quilltap-host/src/almanack_services.rs` → family **`almanack_tier2_equivalence`** (`runtimeType` drops `'lima'` — v5 already never emits it, so this may be pure doc-comment convergence: MEASURE). (6) `lib/tools/self-inventory-tool.ts` + `handlers/self-inventory/{builders,formatters}.ts` → `quilltap-core/src/tools/self_inventory.rs`: `SelfInventoryRuntimeMode` drops `'vm'`/`'electron-vm'` and the two `RUNTIME_MODE_LABELS` rows (`"VM (Lima/WSL2)"`, `"Electron + VM"`) — **prompt-visible bytes**. Plus: `components/footer-wrapper.tsx` `BackendMode` (SPA footer/profile), `app/about/AboutView.tsx` (three prose blocks + the tech list — the SPA About mirror, m6 §1.4), `packages/quilltap/{bin/quilltap.js,lib/lock-helpers.js}` (CLI Tier R lock display + `detectEnvironmentType`), `help/{chat-settings,the-almanack}.md` (→ bank `p4.9i2`). NO-PORT remainder: `lima/`, `scripts/build-rootfs.mjs`, `Dockerfile.ci`, `.github/workflows/release.yml`, `docker/entrypoint.sh`, `docs/WINDOWS.md`, README/DEPLOYMENT/DDL/DEVELOPMENT, `packages/plugin-utils` + `plugins/dist/qtap-plugin-mcp` (unported plugin surfaces), v4's own test deletions. | UNPROCESSED |
| `7819afb1d` | 2026-08-27 | fix(ci): the restore-key suite couldn't find the SQLCipher binding on CI | **NO-PORT?** | Tests + release plumbing only: `packages/quilltap/lib/__tests__/dbkey-restore.test.js` (the jest mock factory falls back to the root `better-sqlite3` alias by absolute path), README, `docs/CHANGELOG.md`, and the `4.9.0-dev.94 → dev.95` version bumps. Zero `lib/`/`app/` hunks. It is the CI-side suite for the surface **P4.D133 ported** (`instances restore-key`, Tier R 188 → 212) — v5's Tier R arms drive v4's REAL launcher, so nothing here changes what they compare. Ratify with evidence at the catch-up round. | UNPROCESSED |
| `3c3432ae9` | 2026-08-28 | docs(release): Update to 4.9.0 release notes | **NO-PORT?** | `docs/releases/4.9.0.md` only (+52/−6), one file, zero `lib/`/`app/`/`packages/`/`plugins/` hunks. Release-notes prose for the 4.9 line. Ratify with evidence at the catch-up round. | UNPROCESSED |
| `65f5021c8` | 2026-08-29 | feat(llm): provider/model fallback chains | **PORT-NEW** | **A new feature landing on ~a dozen ported surfaces.** A connection profile can name an understudy; a failed call (rejected key, rate limit, network error, missing model, 5xx, empty response, moderation refusal) hands the turn to the next candidate. Capped at three attempts (profile → understudy → one auto-picked tier replacement), no recursion, no stickiness. **(1) Schema — D23 re-dump required.** `migrations/scripts/sqlite-initial-schema.ts` adds `"fallbackProfileId" TEXT` + `"allowTierFallback" INTEGER DEFAULT 0` to `connection_profiles`, **inserted MID-TABLE** (after `multiCharacterPrefill`, before `supportsImageUpload`) while the new `add-profile-fallback-fields-v1` migration (`introducedInVersion: '4.10.0'`) APPENDs them — the standing generateDDL-vs-migration column-order disagreement, both shapes to carry → `quilltap-core/src/services/provisioning/fresh_schema.json` (re-dump, never hand-edit) + the boot ensure + per-column read tolerance, and `qtap_export/schema-key-order.json` (a mid-table insert moves exported key order — the P4.D65 stale-key-order class). **(2) The chat spine.** `lib/services/chat-message/provider-failover.service.ts` (+455 — the empty-response recovery / uncensored reroute v5 ported at P4.D94 + P4.57), `primary-stream.service.ts` (the catch-all that used to rethrow unconditionally — v5 `services/primary_stream*`, family `primary_stream_tier3_equivalence`), `orchestrator.service.ts` + `turn-orchestrator.service.ts` (families `orchestrator_tier3_equivalence`, `turn_orchestrator_tier2_equivalence`). New SSE stage **`failing-over`** on the status channel — a wire addition the SPA reads. **(3) New engine.** `lib/llm/fallback/{engine,tier-picker,types,index}.ts` (~600 lines, pure: `classifyFallbackTrigger` returns null for token/content limits, tool-unsupported, Zod errors, unattributed 4xx so their own recovery paths keep their claim; `pickTierCandidate` prefers a different provider) — a tier-1 target with v4's own 568 test lines as the corpus shape. **(4) Cheap LLM.** `lib/memory/cheap-llm-tasks/{core-execution,fallback}.ts` + `lib/llm/cheap-llm.ts` → `quilltap-core/src/services/cheap_llm_exec.rs` (P4.D42 / P4.D127); new `allowCheapFallback` on `CheapLLMSettings` (default `false`) threads through `lib/schemas/settings.types.ts`, `lib/auth/single-user.ts`, `chat-settings.repository.ts`, `sillytavern-import-service.ts` and a new `app/api/v1/settings/chat/route.ts` boolean guard (`'allowCheapFallback must be a boolean'`) → v5's chat-settings verb + `settings_routes_equivalence`. **(5) Image description** is the fourth integration site → `lib/chat/file-attachment-fallback.ts` (+146) → `quilltap-core/src/services/file_fallback.rs` + `files/image_transport.rs` (P4.D106), family `file_attachment_tier3_equivalence`. **(6) Profiles CRUD + wire.** `app/api/v1/connection-profiles/route.ts` + `[id]/route.ts`, `lib/database/repositories/connection-profiles.repository.ts` (deleting a profile NULLs the column on every profile naming it — a cascade v5 must reproduce), `lib/schemas/profile.types.ts`, `lib/llm/connection-profile-legacy-fields.ts` (P4.D126 bug 103) → families `profile_routes_equivalence`, `connection_profiles_tier2_equivalence`, `connection_profile_legacy_fields_equivalence`. **(7) Id-rewriting.** `lib/backup/restore/uuid-remap.ts` adds `fallbackProfileId` to `remapFields` (family `backup_uuid_remap_equivalence`) and `lib/import/quilltap-import/reconcile.ts` remaps it in the reconcile pass (an understudy may appear later in the bundle) → families `qtap_import_equivalence`, `system_import_equivalence`/`system_import_state`; `public/schemas/qtap-export.schema.json` grows the two keys. **(8) SPA.** `components/settings/connection-profiles/{ProfileModal,types,index}.tsx` + `hooks/useProfileForm.ts` (the P4.D86 profile editor), `components/settings/chat-settings/CheapLLMSettings.tsx` + `types.ts`, `app/salon/[id]/hooks/useSSEStreaming.ts` (warn toast now also on `failing-over`). **(9)** `help/{chat-settings,connection-profiles}.md` → bank `p4.9i2`. NO-PORT remainder: `docs/{CHANGELOG,developer/API,developer/DDL,developer/features/complete/provider-fallback}.md`, README, `jest.setup.ts`, v4's own test files, `package*.json`. | UNPROCESSED |
| `97ebfb9fc` | 2026-08-29 | doc(bugs): register bugs 106 and 107 | **NO-PORT?** | `docs/developer/bugs.md` + the two open-bug files only. **Not a CONVERGENCE row** (§5.4): 106 and 107 were filed by v4 from its own Salon sitting, not by this port — bugs.md's provenance line and both rows confirm it. Read the two files before ordering `a1d88aa3a`: bug 106's v5-relevance column reads "**Applies.** Any port that swaps the model mid-turn without re-deciding the attachment question inherits it", and 107's reads "Not investigated — the numbers are v4's, but the shape transfers". Ratify with evidence at the catch-up round. | UNPROCESSED |
| `a1d88aa3a` | 2026-08-29 | fix(llm): the uncensored reroute and the cheap-LLM budgets (bugs 106, 107) | **PORT** | **Bug 106 — v5 is expected to HAVE it.** The uncensored reroute swaps the model and keeps the message array built against the *original* profile, so a vision-capable primary's embedded bytes reach a text-only substitute (400, chain stops, character says nothing). Three halves to port: `dangerous-content/provider-routing.service.ts` — `resolveProviderForDangerousContent` now takes the turn's attachment MIME types and **orders** (not filters) its scan by them → `quilltap-core/src/services/dangerous_content/provider_routing.rs` (P4.D94 / P4.D110), family `danger_routing_equivalence` + the dangerous-chat fixture; NEW `lib/chat/message-attachment-adapter.ts` (`adaptMessagesForProfile`, +183) re-runs `processFileAttachmentFallback` against the profile actually being called, returning the SAME array reference when nothing changes → a new v5 module on `services/file_fallback.rs`; and `needsVision` on the fallback chain now reads `collectAttachmentMimeTypes` off the array rather than what the user uploaded. Plus the **three-spellings consolidation** into one `profileCanReceiveAttachment` in `lib/llm/image-transport.ts` (+27) — v5's P4.D106 predicate pair (`files/image_transport.rs`, `files/attachment_support.rs`, `services/file_fallback.rs`) is the same three-site shape and should collapse the same way; families `file_attachment_tier3_equivalence`, `attachment_anchor_equivalence`. Touches `provider-failover.service.ts`, `orchestrator.service.ts`, `primary-stream.service.ts`, `lib/llm/fallback/engine.ts` (stacked on `65f5021c8` — **order the two together, D-stacked**). **Bug 107 — the cheap-LLM budget rewrite, directly overturning two v5 ports.** `lib/memory/cheap-llm-tasks/{core-execution,compression-tasks,memory-tasks,types,index}.ts` + `lib/chat/context/compression.ts`, `context-manager.ts`, `context-summary.ts`, `lib/memory/memory-processor.ts`, `compression-cache.service.ts` → `quilltap-core/src/services/cheap_llm_exec.rs` (`CHEAP_LLM_TASK_TIMEOUT_MS = 45_000`, the three `75_000` compression rows, `PROVIDER_BUDGET_HEADROOM_MS`) + `services/compression*.rs` + the memory pipeline. Changes: shared tier **45 s → 90 s**, compression background **75 s → 120 s**, with a NEW `CheapLLMLatencyClass` (`background` \| `interactive`) threaded from the call site so the memory recap, both `compressMemories` calls and the cache-miss inline compression keep **45 s**/**75 s** (which is also what stops the raise inverting `MEMORY_RECAP_PHASE_TIMEOUT_MS`); one retry on a fresh socket **on timeout only** and never on the interactive path; `CheapLLMTaskResult.timedOut` + `throwIfLostToTimeout` in **six** background-job handlers (`memory-extraction`, `carina-memory-extraction`, `context-summary`, `scene-state-tracking`, `story-background`, `title-update`) so `markFailed` gives a backed-off retry then DEAD — v5's job runner + families `title_update_tier3_equivalence`, `memory_processor_tier3_equivalence`, `compression_tier3_equivalence`, `compression_cache_tier3_equivalence`, `context_compression_equivalence`, and the `cheap_llm_exec.rs` deadline unit tests transcribed from v4's `cheap-llm-deadlines.test.ts`. `passesLostToTimeout` joins `TurnMemoryProcessingResult`. ⚠ **The 2026-08-27 dogfood C4 row is superseded by this commit (§5.5)** — re-measure, do not carry the 75 s numbers forward. `help/{chat-settings,dangerous-content,system-tasks-queue}.md` → bank `p4.9i2`; `docs/developer/BACKGROUND_JOBS_CHILD.md` records the all-or-nothing extraction-retry rule (buffered child writes are discarded on a throw) — relevant to v5's single-writer buffered apply. NO-PORT remainder: CHANGELOG/README/bugs docs, v4's own test files, `package*.json`. | UNPROCESSED |

## §4 How a full drift check runs (the `/driftcheck` procedure)

1. **Read §1** for the baseline and the previously recorded state. Never
   take the baseline from memory — CLAUDE.md's Status and this file must
   agree; if they don't, that disagreement is itself a finding to report.
2. **Both branches, always.** Since v4's 4.8.0/4.8.1 releases (2026-08-12)
   v4 develops on TWO branches: `main` (next-dev) and `bugfix` (patch
   maintenance). Release flow: a `bugfix: started X.Y bug branch` commit
   forks the branch; fixes land there; `release: X.Y` squashes back onto
   main. The topology is squash-like — bugfix commits are NOT ancestors of
   main — so `git log <baseline>..main` misses nothing but shows the release
   squash, while `git log <baseline>..bugfix` shows the whole historical
   lineage and looks alarmingly long. **Measure bugfix by CONTENT, never the
   commit list:** `git log main..bugfix --oneline -- lib/ app/ packages/`
   for candidates, then `git diff main bugfix -- <paths>` to confirm what is
   genuinely unabsorbed.
3. **Record the checkout's posture:** `git branch --show-current` and
   `git status --short`. A checkout sitting on `bugfix`, or dirty in
   `lib/`/`app/`/`packages/`/`plugins/`, silently poisons regens (it has
   happened — see §5.1) and flips the §1 regen rule to pin-required.
4. **Classify every new commit** — `git show --stat` for the file list,
   then the hunks. ⚠ **Never classify (or later port) from the commit
   message.** A v4 commit message describes the bug's shape as its author
   understood it; the shipped hunks are often narrower. The spec is
   `git show <sha>:<path>` (the post-commit file) plus `git show <sha> --
   <path>` (the hunks). When the message asserts a behavior change, find
   the hunk that makes it; if there is no such hunk, the claim is about the
   bug, not the fix (P4.D110/bug 96 is the canonical case). Carry a note of
   what a commit did NOT do into the row when the prose misleads.
5. **Delineate the intersection with ported work.** For each commit's
   files, name the v5 surface and the round/lane that ported it — grep
   `status-log.md` and CLAUDE.md's round bullets for the v4 path, feature
   name, or bug number. Rules of thumb: `lib/**` and `app/api/**` are fully
   ported (Phases 2–4); `components/**`/`app/**` client code maps to the
   SPA verticals; `packages/quilltap/**` is the CLI Tier R;
   `help/**` banks to `p4.9i2`; `.github/`, `docker/`, release scripts,
   and tests-only changes are NO-PORT candidates. Check v4's
   `docs/developer/bugs.md` for the bug number: a bug this port filed
   coming back fixed is a **CONVERGENCE** row (pins will trip; §5.4).
6. **Update §1 and §3** — never touch existing ORDERED/ABSORBED
   dispositions except to append newer facts. Commit per
   `.claude/commands/commit.md` (docs-only) and report: the verdict, the
   new rows, which ported surfaces are affected, and whether the regen rule
   changed.

## §5 Standing drift machinery — recipes and traps

_Moved into the repo from the session-memory notes, 2026-08-25. This is the
durable home; the memory notes now just point here._

### §5.1 The pinned-worktree regen recipe

A fresh oracle is only as pinned as the tree it imports. Whenever v4 HEAD is
past the baseline OR the checkout is dirty or on the wrong branch, regen from
a detached worktree pinned at the baseline. v4 is the human's active repo —
**never stash or checkout-switch it.**

```bash
PIN=/tmp/qt-v4-pin-<order>-<sha>       # LANE-UNIQUE path — never share pins
git -C ~/source/quilltap-server worktree add --detach "$PIN" <baseline-sha>
# THREE symlink classes, all untracked and absent from a bare worktree:
ln -sfn ~/source/quilltap-server/node_modules "$PIN/node_modules"
ln -sfn ~/source/quilltap-server/packages/quilltap/node_modules \
        "$PIN/packages/quilltap/node_modules"
for d in ~/source/quilltap-server/plugins/dist/*/; do
  [ -d "$d/node_modules" ] && ln -sfn "$d/node_modules" "$PIN/plugins/dist/$(basename "$d")/node_modules"
done
cd "$PIN"   # run EVERY tsx/jest oracle + fixture builder from here
# cleanup: git -C ~/source/quilltap-server worktree remove --force "$PIN"
```

- The `@/` alias resolves through the cwd's tsconfig, so cwd = the pinned
  worktree imports pinned lib code. The second symlink feeds the real-DB
  jest cases' `requireActual` of the cipher driver; the third feeds any
  oracle that loads a provider plugin (`provider-registry.ts`, the
  stream/envelope recorders, the manifest generator) — without it they die
  on `Cannot find module '@anthropic-ai/sdk'`.
- **The empty-file trap:** that failure is loud on stderr, but if stdout was
  redirected to the oracle file, the redirect already truncated it to ZERO
  bytes — the next diff then reads "DIFFERS against an empty file" exactly
  like real drift. Check the file is non-empty before believing a diff.
- **Verify the pin** by grepping the fresh NDJSON for a marker only one tree
  has (a sentence the drift added must be ABSENT from a baseline-pinned
  oracle).
- **Lane-unique paths:** a pin shared between lanes was deleted mid-run by
  whoever finished first (P4.d26) while v4 HEAD was past the baseline — the
  later regen would silently have used the wrong tree. Cheap guard:
  `git -C ~/source/quilltap-server worktree list` before each regen batch.
- **The checkout can go dirty MID-LANE** (P4.D105): clean at lane start,
  dirty two units later. Do not reason about whether the dirty files "could"
  have mattered — build the pin, re-run every regen the lane already did
  from it, and expect the re-run to change nothing. That is a cheap, total
  proof; the alternative is an argument.
- The pin is also what makes an end-of-lane sweep honest:
  `harness/tools/recipe_sweep.py --run-all --v4 "$PIN"` rewrites every
  recipe's `cd ~/source/quilltap-server`.

### §5.2 The silent-stale-pass trap (regen verification)

A fixture builder that ERRORS leaves the old fixture in place, and the
differential then passes **green on stale data** — the green comes from the
old fixture + old oracle agreeing with each other as they always had.
Discipline for every regen:

```bash
rm -f /tmp/qt-<x>.db                      # a failed build can't hide behind a stale file
... build ... 2>&1 | tail -1              # read it; a stack trace invalidates the run
grep -c '<new-field-or-id>' /tmp/oracle-<x>.ndjson   # MUST be > 0
```

Corollaries: a green regen is not coverage — grep every regenerated NDJSON
for the CHANGED bytes; when appending to a pinned-id corpus, check the WHOLE
id set for collisions, not the tail; anchor jest `--` filters
(`"case\.test\.ts$"`) because substring matches let a sibling case clobber
the same `QT_ORACLE_OUT`.

### §5.3 Commit prose misdescribes diffs

(Also in §4 step 4, because it bites at classification AND at porting.) Port
from the shipped hunks, never the message. Re-verify even when the work
order already flagged the trap — the order's paragraph is the same kind of
prose. Carry the finding into a v5-side code comment naming the sha and what
it did NOT do.

### §5.4 Convergence rows: measure, don't assume

When v4 adopts a fix this port made first, the both-directions divergence
pins trip at the baseline move **by design** — that tells you v4 MOVED, not
HOW. Regenerate the oracle and dump v4's ACTUAL post-fix output before
retiring a pin to a plain equality: v4's adoption is often partial or
differs in wording/details the planner never checked (P4.D51's bug-8 message
heads and bug-12 phantom-row surprises are the canonical cases). A partial
convergence splits one carve-out into a converged half + a still-divergent
half — expect to reshape pins, not just delete them. When a converging arm
reddens, instrument the harness to dump the actual rows (a `QT_DEBUG_*`
-gated `eprintln!`), and for restore/import read the archive's own manifest
as ground truth.

### §5.5 Drift heals real data — 💸 proofs expire

A banked live proof that depends on **damaged rows existing** can expire
because v4 (running daily on the real instance) lands its own fix and heals
the data — the orphan-reaper and poisoned-base-URL proofs both died this
way. When a round banks such a proof, record the measurement date and count;
before building a walk step around it, **measure the population first** (one
read-only query; a zero is a finding, not a failure); when it has expired,
retire it explicitly rather than carrying it forward. Planting the damage on
the disposable copy proves the mechanism but is a weaker claim — offer it,
don't silently swap it in.

## §6 History

Absorbed drift blocks get one line each here when `/unify` moves the
baseline; the full story lives in the round record in `status-log.md`.

- **The P4.D131 round (2026-08-27, baseline `aec86a613` → `b121ac77f`):**
  `679e450e3` ABSORBED(p4.d131 — the bug-105 CONVERGENCE retired by
  measurement per §5.4: FULL convergence, no residue; the arm — code name
  `execute_bug105_seed_abort`, the row's `import_aborts_on_non_string_
  provider` was its class description — is now a plain state-compared
  regression guard; the retirement measurably WIDENED coverage, the
  formerly-subtracted `main.image_profiles` table now discriminating),
  `0bd841394` + `1b0ce9eba` ABSORBED(p4.d132 — the Tooltip primitive +
  nine-button adoption + the ConfirmationBadge net-new + the deletion
  rider; help rows banked to `p4.9i2`; theme-storybook NO-PORT),
  `b121ac77f` ABSORBED(p4.d133 — `instances restore-key` whole, Tier R
  188 → 212; the row's ⚠ scope question resolved at ordering — the write
  proved in-sandbox via `reset_live`, the real-pepper walk banked 💸; the
  NO-PORT remainder RATIFIED: README/docs/help/package files + v4's two
  new test files, their behavior carried by the Tier R arms + unit pins).
  Round record: `status-log.md` → "The P4.D131 ∥ P4.D132 ∥ P4.D133 ∥
  P4.65 round unification".

- **The P4.D130 round (2026-08-27, baseline `8872d7efc` → `aec86a613`):**
  `aec86a613` ABSORBED(p4.d130 — the outfit pull-down + garments-only slot
  pickers, SPA-only; the composed-outfits selectors' recorded-vector corpus
  pinned at the drift commit, re-proven byte-identical after mid-lane
  drift), `b6c6d7793` NO-PORT-RATIFIED(this round — docs-only, this port's
  own bug-105 filing; file list verified `docs/developer/bugs.md` + the bug
  file, zero lib/app/packages/plugins content). Round record:
  `status-log.md` → "The P4.D130 ∥ P4.62 ∥ P4.63 ∥ P4.64 round".

- **The 4.9.0-push round (2026-08-27, baseline `f3892158d` → `8872d7efc`):**
  `914b59e13` + `805ef12bf` + `e000d6bfc` ABSORBED(p4.d126 — the full-wipe
  chokepoint, the variable-limit chunking, bug 103's legacy profile-column
  seeding; a v4 REGRESSION found in `e000d6bfc` itself — the helper sits
  outside the per-item try, one malformed profile aborts a whole v4 import —
  pinned v5-side, filed upstream), `964ffb959` + `8872d7efc` + `21f573039`
  ABSORBED(p4.d127 — bug 104, the per-task cheap-LLM budgets + failure warn,
  the coalesce-trace silence pin; the §1-predicted 💸 expiry executed: the
  Z.AI refusal-sentence proof retired, replaced by the glm-5.3 wire proof),
  `97d0b8f8e` + `57e7b1bc2` ABSORBED(p4.d128 — the qt-* utilities sweep,
  the four completion flags Tier R red-first), `8440b6391` ABSORBED(p4.d128,
  the AboutView hunk) + NO-PORT-RATIFIED(p4.d129, the docs remainder),
  `487ae57fe` `561466cfe` `7509c5cfb` `c0352fdba` NO-PORT-RATIFIED(p4.d129,
  each with evidence; `561466cfe`'s rider removed one vestigial v5 wardrobe
  twin), and `dcab791c2` NO-PORT-RATIFIED(p4.d129 — 410-family neutrality
  sweep + hunk measurements) **EXCEPT its title-cleaner second-trim collapse,
  which was measured NON-NEUTRAL (10/76 vectors) and ABSORBED at the
  unification wires** (both v5 cleaners + the `regen_title_quoted_padded_
  inside` tier-3 arm). Round record: `status-log.md` → "The 4.9.0-push
  drift catch-up round".

- **`f3892158d`-round (2026-08-26, baseline `b220999da` → `f3892158d`):**
  `664cfca84` ABSORBED(p4.d123 server ∥ p4.d125 client — the jobs/activity
  accounting whole), `f3892158d` ABSORBED(p4.d124 server ∥ p4.d125 client —
  the realtime subsystem whole, the hints riding v5's existing Event
  channel per the round's §Shared contract §B rather than a second
  WebSocket). Round record: `status-log.md` → "The `f3892158d` drift
  catch-up round".

- **`b220999d`-round (2026-08-26, baseline `8f9101370` → `b220999da`):**
  `b86bb1a58` ABSORBED(p4.d119 server ∥ p4.d121 SPA — the per-tier
  dressing instructions whole), `d25dacc1d` ABSORBED(p4.d120 ∥ p4.d121 —
  archive-instead-of-delete whole), `b220999da` ABSORBED(p4.d122 — the
  Documents-search vertical whole), `a47d3e034` + `2417cbed1`
  NO-PORT-RATIFIED(the two feature specs — docs-only, confirmed by the
  implementing lanes' hunk surveys; the search spec's defects paragraph is
  quoted in the p4.d122 order). Round record: `status-log.md` → "The
  `b220999d` drift catch-up round".

- **`8f910137`-round (2026-08-25, baseline `f6a10055d` → `8f9101370`):**
  `44a8137e9` ABSORBED(p4.d115 ∥ p4.d116 — the scenario-change feature whole),
  `8018c487a` ABSORBED(p4.d117 — bug 99, measured-then-ported),
  `309aaa97a` ABSORBED(p4.d117 — bugs 100/102 + the check-qt-classes guard),
  `6afacb187` ABSORBED(p4.d118 — bug 101, Tier R red-first),
  `8f9101370` NO-PORT-RATIFIED(p4.d118 — CI + tests-only; the +18 test lines
  absorbed by `completion_behavior.rs`). Round record: `status-log.md` →
  "The `8f910137` drift catch-up round".
- (This ledger was seeded 2026-08-25 with the baseline at `f6a10055d`. Drift
  older than that baseline is recorded in CLAUDE.md's round bullets and
  `claude-md-status-history.md`.)
