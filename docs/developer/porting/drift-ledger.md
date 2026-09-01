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

- **Oracle baseline:** `4622411fd` — "docs(release): Update to 4.9.0 release
  notes" (v4 main, 2026-08-31), adopted at the round-2 drift catch-up
  unification (P4.D138 ∥ P4.D139 ∥ P4.D140 ∥ P4.D141 ∥ P4.D142 ∥ P4.66,
  2026-09-01). The round absorbed five of the eight rows past `7fb668263`
  whole and the three-commit LoRA train PARTIALLY (see below).
- **Checked:** 2026-09-01 (at the round-2 `/unify` — the §2 probe at survey
  and again at the baseline move; the previous full `/driftcheck` was
  2026-08-31).
- **v4 `main` HEAD at check:** `4622411fd` — **ZERO commits past the new
  baseline.**
- **v4 `bugfix` tip at check:** `3a76b17df` — **unmoved** since the
  2026-08-27 content measurement (the bare 4.8.4 fork marker; its long
  `main..bugfix` commit list is the squash-topology lie, §4 step 2 —
  nothing genuinely unabsorbed).
- **v4 `release` tip at check:** `8736d7042` ("release: 4.8.4") — same tip
  as the prior checks, ancestry previously verified.
- **Checkout at check:** branch `main`, **tree CLEAN**.
- **Verdict: DRIFT CLEARED — 0 commits past the baseline. THREE §3 rows
  remain as PARTIAL** (the LoRA train `84f33ce94` → `648d5c8aa` →
  `2ece98c90`): their server units 5–7 — bugs 110/111, the `list-models`
  `loraSupport` read side + `options-schema` + the NanoGPT detailed-catalog
  cache, and the HuggingFace `lora-metadata` lookup — are tracked by
  P4.D138's status header and resume list (`status-log.md` → "P4.D138 —
  lane status: OPEN at units 5–7"), NOT by this ledger: the baseline has
  moved past them, so `/driftcheck` will not re-surface them. Two standing
  consequences at the new baseline: (1) the committed `image-dialects`
  corpus was deliberately recorded at the commit-1 pin `84f33ce94` (bug 110
  PRE-fix, by name) — a regen of that family at the baseline goes RED by
  design until unit 5 lands and re-records at the tip; (2)
  `image_profiles_routes_equivalence`'s four `list_models` success arms
  strip v4's `loraSupport` key behind `LORA_SUPPORT_PENDING_P4D138_UNIT6`
  after MEASURING the shape — it reddens the moment v5 answers the key.
  (The superseded round-2 verdict paragraph follows for history.)
- _Superseded (the round-1 verdict): DRIFT PENDING — 8 commits (round 2 of
  the pre-planned pair)._
  Two substantial ports: **`735d9408c` (bug 112 — `lastMessageAt`
  redefined to character-authored activity: a new chokepoint predicate,
  both write sites, every chat-list sort, and a recompute migration — it
  lands on the P4.64/P4.65 home + Salon-list surfaces this port just
  rebuilt)** and **`60e3c4a0a` (the Concierge four-state per-chat
  control — `conciergeOverride` widens to `'OFF' | 'UNCENSORED'`, the
  chat-override/manual-flip/resolver family reshapes across ~14 ported
  call sites, new Concierge notice sentences)**. The THREE-commit
  D-stacked LoRA train: `84f33ce94` (LoRA adapters + per-model image
  options — PORT-NEW, with the five-call-site image-params consolidation
  that fixes drift v5 likely inherited) → `648d5c8aa` (bugs 110/111) →
  `2ece98c90` (the HuggingFace LoRA lookup — PORT-NEW). One smaller port:
  `5f56f7a7d` (the `qt-range` class). Two docs-only NO-PORT? candidates:
  `e41fcb12e` (banks help hunks to `p4.9i2`) and `4622411fd`.
- **Regen rule in force: NO PIN REQUIRED** — v4 HEAD IS the baseline and
  the checkout is on `main`, clean. Re-run the §2 probe before every regen
  batch all the same; the moment v4 moves, §5.1's pinned worktree at
  `4622411fd` is the rule again. Exceptions that stay pinned regardless:
  `image_dialects_equivalence`'s committed corpus is pinned at `84f33ce94`
  (above) until P4.D138 unit 5 re-records it at `2ece98c90`+.
- **Release shape:** still no `release: 4.9.0` squash and no 4.9 bugfix
  fork; v4 develops on `main` alone (the 4.9.0 release-notes file moved
  again at `4622411fd` on 2026-08-31, so the squash may be imminent).
  Keep probing BOTH branches.

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
| `84f33ce94` | 2026-08-29 | feat(images): LoRA adapters + per-model image options | **PORT-NEW** | **A new feature plus a five-call-site consolidation that fixes long-standing drift v5 very likely inherited.** **(1) LoRA storage — no DDL change.** The existing `parameters` bag gains a reserved `loras` key (`{ source, scale?, triggerPhrase?, label? }`); a provider opts in via a new `loraSupport` declaration, resolved per model exactly like orientation (exact id → longest-prefix family → provider → none) through the same `matchModel` — NEW `lib/plugins/model-matchers.ts` + `lib/image-gen/{lora-support,lora-validation}.ts` → `quilltap-core/src/image_gen.rs` (which already holds v5's `match_model`/orientation) + `model/image_dialects.rs`. Malformed `parameters.loras` is a **400 before any write**; an over-cap list is **kept and flagged, never deleted** (narrow the model and widen it again and nothing is lost), with capping at request time and every dropped adapter named in the log → `api/image_profiles.rs` + `db/image_profiles.rs`, families `image_profiles_routes_equivalence` / `image_profiles_tier2_equivalence`. **(2) NanoGPT is the first consumer** — three wire dialects (indexed `lora_url_N`/`lora_scale_N`, single `lora_weights`/`lora_scale`, `lora_url`/`lora_strength`) chosen from a **static** family table, because NanoGPT's catalog tags a model `lora` but leaves `allowed_passthrough_parameters` empty; an unlisted LoRA-tagged model gets the capability with a "family unknown" warning rather than a guessed body. `hf_api_token` and `lora_preset` are kept OFF the general passthrough allow-list and attached only where the dialect proves they belong (a credential must not be broadcast to whatever model a profile names) → v5 reimplements NanoGPT natively (P4.D100/D101), so this is manifest + builder work, not plugin work. **(3) The consolidation — measure v5 for the same drift.** Five call sites built image params independently and **three read only `quality` off the profile**, so profile settings worked for `generate_image` and vanished for avatars, story backgrounds, `POST /api/v1/images` and the wardrobe preview. All five now share NEW `lib/image-gen/params-builder.ts` (+298), preserving the previous merge semantics key for key; those four paths **gain** the profile's `negativePrompt`/`seed`/`guidanceScale`/`steps`, and the wardrobe preview resolves portrait through the provider's own mechanism instead of a hardcoded 1024×1792 only OpenAI ever accepted → v5's five twins: `tools/generate_image.rs`, `services/character_avatar_job.rs`, `services/story_background_job.rs`, `api/image_profiles.rs`/the images route, and the wardrobe preview-avatar path (`app/api/v1/wardrobe/preview-avatar/route.ts`). **(4) `ProviderOptionField.appliesToModels` is no longer reserved** — the shared renderer honours it (exact id, `*` glob, family prefix), which gates fields on the LLM side too → v5's P4.D84 schema-driven options panel (`apps/web/src/app/screens/settings/providers/provider-options-schema.ts:80` declares the field today and nothing honours it) + `components/settings/connection-profiles/ProviderOptionsPanel.tsx`. **(5) SPA.** NEW `components/image-profiles/LoraListEditor.tsx` (+191), `ImageProfileForm.tsx` (+123) now asking the plugin what to render for the selected model instead of a hand-written per-provider switch, `ImageProfileParameters.tsx`. **(6)** `public/schemas/qtap-export.schema.json` + `lib/schemas/profile.types.ts`. `help/image-generation-profiles.md` → bank `p4.9i2`. NO-PORT remainder: `packages/plugin-types` (2.5.8 → 2.6.0 — v5 has no plugin SDK), the `plugins/dist/qtap-plugin-nanogpt` sources (v5 ports behaviour natively), `playwright.config.ts` (v4's own harness — a fresh `mkdtemp` data dir had no `.dbkey` so every route answered 503), docs, v4's own tests. v4 records "Phase 6 dogfood proofs against a live NanoGPT key remain outstanding" — v5's live proof is owed the same way. | **PARTIAL(round-2 unify, 2026-09-01)** — p4.d139 (the whole client half) + p4.d138 units 1–4 ABSORBED (matchers/support, the `loras` write guard + Zod envelope, the params builder + five-site consolidation, the NanoGPT dialects at THIS commit's pin); the `list-models` `loraSupport` read side + `options-schema` + the detailed-catalog cache = p4.d138 **unit 6, OPEN** (tracked by the order, the baseline has moved past this row) |
| `648d5c8aa` | 2026-08-30 | fix(nanogpt): the discarded LoRA preset and the unlogged image request (bugs 110, 111) | **PORT** (stacked on `84f33ce94`) | Both v4-filed from one sitting; **order with the LoRA commit, D-stacked.** **Bug 110:** `applyLoras` opened with an early return on an empty `loras` list, and `lora_preset`'s attachment lives BELOW that inside the url-dialect branch — while the other road is closed on purpose (the preset is in `NANOGPT_LORA_SCOPED_KEYS` rather than the general passthrough list, so it reaches only the family that understands it). Two correct decisions with nothing covering the seam. The real mistake underneath is a **conflation**: a preset names a style the host already keeps and stands alone, while `hf_api_token` authorises the fetch of caller-supplied weights and has no errand without them — they look alike in the options panel and the code applied one rule to both. `applyLoras` now resolves the family FIRST and applies each scoped key on its own terms (preset whenever the family is `url`, credential only alongside weights), with the asymmetry commented so it is not "consistency"-fixed back; the unknown-family refusal is untouched; reported dialect no longer collapses "nothing was configured" into "nothing could be spelled". **The failure mode was a success** — no error, no dropped entry, a completed and billed job returning a stock image — and **v4's suite asserted this exact call as correct**, passing `undefined` for `profileParameters` so the preset was never in frame: a corpus blind spot to reproduce deliberately on the v5 side. **Bug 111:** NanoGPT answers a rejected adapter, an unreachable repo, a bad resolution and a filtered prompt with **one generic 400**, so the composed body is the only thing separating those causes — and it was logged at `debug`, which no packaged instance keeps. The generate call is now wrapped and logs model, size, dialect, wire keys, drops and passthrough keys **at error** before rethrowing unchanged; **key names only, never values**, which keeps `hf_api_token` out of the log while still recording that it was sent. → v5's native NanoGPT image path (`model/image_dialects.rs`, `services/image_job_common.rs`) and the P4.49 file logging; a log-only fix is invisible to a differential (the standing note) so it wants a capturing-layer pin. `help/image-generation-profiles.md` → bank `p4.9i2`. NO-PORT remainder: the plugin sources + manifest/package bumps (1.2.0 → 1.2.1), CHANGELOG, bugs docs, v4's own tests. | **PARTIAL(round-2 unify)** — NOT ported: p4.d138 **unit 5, OPEN** (bug 110's family-first `apply_loras` + bug 111's error-level request log; the committed `image-dialects` corpus deliberately carries the PRE-fix bodies from the `84f33ce94` pin, by name) |
| `2ece98c90` | 2026-08-30 | feat(images): query a LoRA source against HuggingFace from the profile editor | **PORT-NEW** (stacked on `84f33ce94` — the LoRA train's third commit) | **The repo's second mocked non-LLM external HTTP provider after Serper (P4.42's `mock-serper.ts` is the precedent).** NEW `lib/image-gen/huggingface-lookup.ts` (+263): asks `https://huggingface.co/api/models` what it knows about a LoRA source — resolves/gated, weights files, `base_model`, and `cardData.instance_prompt` (the trigger phrase, the reason the feature earns a button) — **deliberately renders NO compatibility verdict** (the module doc says a false "this will not work" is worse than silence); 10 s timeout; host-side only so egress stays in one place and a gated-weights token never reaches the page. NEW `huggingface-repo-id.ts` (+56, `extractHuggingFaceRepoId`/`huggingFaceCardUrl`). **Wire:** `POST /api/v1/image-profiles?action=lora-metadata` (POST not GET **because `hfToken` is a credential and doesn't belong in a query string** — carry that comment); hand-rolled body guards (`'A JSON body with a `source` is required'`, non-object/array refused) → v5 `api/image_profiles.rs` + family `image_profiles_routes_equivalence` (P4.9H2B / `84f33ce94`'s row). **SPA:** NEW `LoraQueryResult.tsx` (+187), `LoraListEditor.tsx` (+120 — the query button + trigger-phrase adoption), `ImageProfileForm.tsx` → the v5 LoRA editor from the `84f33ce94` order. `help/image-generation-profiles.md` → bank `p4.9i2`. NO-PORT remainder: README, CHANGELOG, API/feature docs, v4's own 327-line test file (its fetch-mock shapes are the differential's corpus shape), `package*.json`. | **PARTIAL(round-2 unify)** — p4.d139's client half ABSORBED (the repo-id twin, the query panel over v4's REAL lookup shapes, the editor's Query button); the server lookup + `POST ?action=lora-metadata` = p4.d138 **unit 7, OPEN** |

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

- **The round-2 drift catch-up (2026-09-01, baseline `7fb668263` →
  `4622411fd`):** `5f56f7a7d` ABSORBED(p4.d142 — `.qt-range` + tokens
  byte-identical, all twelve v5 range hosts adopted, dogfood #107's
  `qt-markdown-field` rule, the host-class guard at the ordered NARROW
  scope + the `--self-test` landed at unification), `735d9408c`
  ABSORBED(p4.d140 — bug 112 whole: the `chat_activity` chokepoint with
  its tier-1 family, both write sites red-first, the six readers, the
  restore re-derive, the ai-import twin NO-COUNTERPART, the boot recompute
  heal in the P4.D97 ledger shape with its own family, the four SPA
  display flips, the e2e seed landmine repaired; plus the out-of-mandate
  `allowCheapFallback` P4.D135 remainder), `e41fcb12e`
  NO-PORT-RATIFIED(p4.d138 — CHANGELOG + two help files, +34, zero
  lib/app/packages/plugins; help hunks banked to `p4.9i2`), `4622411fd`
  NO-PORT-RATIFIED(the unify — `docs/releases/4.9.0.md` alone, +70/−6),
  `60e3c4a0a` ABSORBED(p4.d141 — the Concierge four-state whole: the
  predicate family reshape at every call site, the resolver's operator
  arms, the flips + writer sentences byte-exact, the `conciergeState` PUT
  arm closing v5's long-named deferral, the classifier-gate corpora that
  can finally see the gate, the SPA control + single-pill badge + client
  twin, the four-state walk live; the §3 review's sidebar-latch fix and
  the broken `post_office_writers_tier3` family repaired at unification).
  The LoRA train (`84f33ce94` → `648d5c8aa` → `2ece98c90`) is PARTIAL —
  the whole client half (p4.d139) + p4.d138 units 1–4; units 5–7 stay in
  §3 as PARTIAL rows and in the order's resume list. The §3 unification
  review (five parallel readers over six lanes) fixed six finding groups
  on the unify branch — headline: the Concierge select's permanent
  optimistic latch, the optimistic-bubble echo scoped across two clocks,
  and a harness family the kind rename had silently broken. Round record:
  `status-log.md` → "Round record — the drift catch-up round 2 of 2
  unification".

- **The round-1 drift catch-up (2026-09-01, baseline `b121ac77f` →
  `7fb668263`):** `1560bd43b` ABSORBED(p4.d134 — the Lima/WSL2 retirement
  whole: env/lock/CLI, the data-dir `isVM` wire deletion with two renamed
  deletion pins, the host-rewrite two-strategy collapse, self-inventory/
  almanack retirements, the SPA About/footer/profile mirrors; the grep
  census; the unported host gateway resolver named as a follow-up order),
  `7819afb1d` + `3c3432ae9` NO-PORT-RATIFIED(p4.d134, file lists in the
  lane record), `65f5021c8` ABSORBED(p4.d135 — provider/model fallback
  chains whole: the D23 re-dump with the generateDDL-position correction,
  the pure engine tier-1 at 155→158 cases, the Salon spine both
  entrances, cheap-LLM + image-description sites, both id-remap paths,
  the delete-nulls cascade with the updatedAt stamp, the SPA mirrors +
  the live understudy round-trip beat), `97ebfb9fc`
  NO-PORT-RATIFIED(p4.d136 — both bug files' v5-relevance columns quoted
  in the lane record), `a1d88aa3a` ABSORBED(p4.d136 — bug 106 proven
  red-first in both halves + the three-spellings consolidation; bug 107's
  budget rewrite with the latency class threaded from 45 call sites, the
  timeout-only retry, five of six handler guards [scene-state deferred
  loud], the C4 dogfood row superseded per §5.5; the outfit-consult
  bound inversion measured as v4's own and reproduced), `487ae16b1`
  ABSORBED(p4.d137 — bugs 108/109 red-first: the argument guards
  byte-exact, the fold module + rebuilt per-UTF-16-unit diacritics map,
  the 5/25 replay split executable), `7fb668263` ABSORBED(p4.d134's About
  rider; README/CHANGELOG_V4 remainder NO-PORT). The §3 unification
  review fixed four finding groups on the unify branch — headline: the
  `[CheapLLM] Task failed` warn fired AFTER the chain instead of before
  it (v4 warns first; the counter bug 107 was measured from would have
  under-counted). Round record: `status-log.md` → "The drift catch-up
  round 1 of 2 unification".

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
