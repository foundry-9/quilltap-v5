# Work order PB1 — the product version manifest (the first pre-beta item)

**Status:** ORDERED, NOT SCHEDULED (written 2026-08-18 from a human
conversation; see "The gate" below for when it runs). This is the first
**PB** ("pre-beta") order — the class of release-adjacent work D21 defers
out of Phase 4 but which must land before anyone outside this repo installs
a build.

## The gate — when to run this

**Run it when parity work is winding down and before the first build a
beta tester installs.** Not before: it touches the gate (a health-shape
assertion, the About spec, `m6-screen-parity.md`) and buys nothing while
the port is still absorbing v4 drift every other day.

**Do not run it as a "next candidate."** It is deliberately parked outside
the round-to-round candidate list. The planner's cue is the pre-beta gate
note at the tail of `phase-4.md`, not value-ordering against drift lanes.

Prerequisites: none technical. It is independent of the oracle baseline,
touches no ported behavior, and needs no v4 regen.

## Why (the three things asking for it, measured 2026-08-18)

1. **The About screen carries a recorded divergence whose whole cause is
   the missing version.** `apps/web/src/app/screens/about/about-page.ts:19`
   — "v4's badge shows the product version from a build-time
   `import packageJson`. v5 has no product version — so the badge shows the
   version the running server reports over `/health`."
2. **One build reports four different numbers.** `HostConfig::new` defaults
   `version` to quilltap-host's `CARGO_PKG_VERSION`
   (`crates/quilltap-host/src/host.rs:135`); `quilltap-web`'s binary
   overrides it with its own (`src/main.rs:225`); the Tauri shell with its
   own (`src/lib.rs:126`); `quilltap --version` prints the CLI's
   (`src/main.rs:141`). `crates/quilltap-web/tests/profile_web_routes.rs:200`
   already documents this and asserts only the *shape*, because there is no
   single right value to assert.
3. **`docs/CHANGELOG.md` has no version anchors**, because there are no
   versions to anchor to. (It was one flat ~19,400-line `## Recent Changes`
   body when this order was written; on 2026-08-19, by human ruling, it was
   restructured into per-commit H4 headers — hash + date + subject + bumped
   crate versions — split by month with older months archived under
   `docs/changelog/`. The version-anchor gap stands.)

## The human rulings this order implements (2026-08-18)

- **The first real release is `5.0.0`.** Fixed point. Everything before it
  is a prerelease of it.
- **The format is semver prerelease: `5.0.0-dev.N`**, N incrementing once
  per source commit — the same shape v4 already prints
  (`4.9.0-dev.28`), so both repos read alike. Verified: Cargo accepts a
  dotted prerelease and `env!("CARGO_PKG_VERSION")` returns it verbatim.
- **Odd/even minor numbering (the Linux 2.x scheme) was considered and
  rejected.** It encodes a stability class instead of a counter, gives no
  "which build is this" answer, was abandoned upstream in 2003, and would
  spend the *minor* field forever — colliding with semver's meaning of it
  once 5.0.0 ships.
- **One canonical string, derived projections — never a parallel
  hand-maintained number per platform.** Hand-maintained numbers drift.

## Units

### 1. The canonical version

`[workspace.package] version = "5.0.0-dev.1"` in the root `Cargo.toml`,
inherited with `version.workspace = true` by the four crates that *report*
a version: **quilltap-host, quilltap-web, quilltap-cli, quilltap-tauri**.

**Explicitly NOT inherited by:**

- **`quilltap-sqlite3mc-sys`** — its pinned `0.1.0` is what keeps the 12 MB
  amalgamation C compile cached (Cargo's build-script fingerprint includes
  the package version). Inheriting would recompile it every commit. This is
  the same rule CLAUDE.md and the crate's own manifest already state; state
  it a third time in the root manifest at the inheritance site.
- **`quilltap-core` / `quilltap-harness` / `quilltap-fixture-sanitizer`** —
  their per-commit counters are the ledger the round records cite
  ("core 0.0.581, harness 0.0.503") and core's is genuinely informative.
  Round records after this order read `product 5.0.0-dev.N, core 0.0.x,
  harness 0.0.x`.

**Named tradeoff:** host/web/cli/tauri lose their individual counters. That
is the point (they were four answers to one question), but the round-record
template in the `unify` skill must stop asking for them.

### 2. The transports agree

All four report the product version. `HealthDto.version` then means "the
product version of the build serving you" on every transport, and the Tauri
`health` command and `/health` agree by construction rather than by luck.

### 3. Fix the naive shape assertion

`crates/quilltap-web/tests/profile_web_routes.rs:209` asserts
`version.split('.').len() == 3`. `"5.0.0-dev.28".split('.')` yields **four**
segments (`5`, `0`, `0-dev`, `28`), so this test goes red on the first run.
Replace the hand-rolled parse with a real semver parse (major/minor/patch
numeric, optional prerelease). **The test is wrong, not the format** — do
not bend the version to satisfy it.

### 4. The About badge — retire the divergence

The badge becomes the product version in v4's own form. Update:
`about-page.ts` (the doc comment at :19 and `versionLabel()` at :301),
`about.spec.ts`, and the divergence row in `m6-screen-parity.md`.

### 5. The projection generator + its test

One canonical string in, four platform projections out, generated and
asserted — never hand-edited:

| Target | Projection | Note |
|---|---|---|
| GitHub tag / release | `5.0.0-dev.28` verbatim | mark `prerelease: true`; GitHub's "Latest release" pointer skips prereleases automatically |
| Docker tag | `5.0.0-dev.28` verbatim | tag charset is `[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}` — dots and hyphens legal, **`+` is not**, so never use semver's build-metadata field |
| Linux tarball | verbatim | filename only |
| Debian `.deb` | `5.0.0~dev.28` | **see the trap below** |
| Windows MSI | `ProductVersion 5.0.0` | WiX takes three numeric fields (major ≤ 255, minor ≤ 255, build ≤ 65535). Tauri exposes a WiX-specific version override for exactly this — **verify the current config key at implementation time** |
| macOS `.app` | `CFBundleShortVersionString 5.0.0`, `CFBundleVersion 28` | the dev counter drops straight into the build-number slot — which is why N must stay a plain integer |

**⚠ The Debian trap — the one that bites in the field, not in a build.** In
Debian's version grammar the hyphen is *structural*: it separates the
upstream version from the Debian revision. `5.0.0-dev.28` therefore parses
as upstream `5.0.0`, revision `dev.28`, and sorts **after** plain `5.0.0` —
the prerelease would look newer than the release it precedes and `apt`
would refuse the real 5.0.0 as a downgrade. The tilde sorts before anything
including the empty string, so the `.deb` projection is a deliberate
`-` → `~` substitution on the prerelease separator.

Also land: `crates/quilltap-tauri/tauri.conf.json` (sitting at `0.0.1`
today) synced from the canonical string, **with a test asserting the conf
and the crate version agree** rather than trusting hands.

### 6. Changelog anchoring

**Partially superseded (2026-08-19, human ruling):** the historical body
HAS now been sliced retroactively — per-commit H4 headers (date + subject
+ bumped crate versions; historical entries also carry the short hash),
monthly sections, older months archived under `docs/changelog/`. The
going-forward per-commit format lives in `.claude/commands/commit.md` §7.

What remains for this order: layer **product-version anchors** on top —
when the canonical `5.0.0-dev.N` bumps, a version heading (above the
per-commit entries it covers) so the changelog can be read release-wise.

### 7. The commit-skill amendment

`.claude/commands/commit.md` §6 gains the product-version bump alongside
the per-crate rule. Note there that §6's standing "don't initiate a
release" caveat still holds: **this order is a version manifest, not a
release process.** Signing, publishing, and the updater remain deferred
under D21.

## Deferred out of this order (name them, don't do them)

- Multi-arch Docker publishing. `Dockerfile:20` declares the image
  dev-grade — "no multi-arch, no size-golfing, and emphatically no release
  (D21) — nothing here is published, signed, or tagged." Multi-arch is a
  `buildx --platform linux/amd64,linux/arm64` manifest list and is **not a
  versioning question**; both architectures share the version string.
- The `latest` tag discipline, when publishing does land: **a prerelease
  must never move `latest`.** Push `5.0.0-dev.28` and a rolling `5.0-dev`
  pointer; leave `latest` unpushed until 5.0.0 ships.
- Windows/Linux CI. There is no `.github/workflows` in this repo yet, and
  the cross-platform build-from-one-source goal ("the very first thing we
  do when we hit production" — human, 2026-08-18) is its own order.
- Signing, notarization, the updater.

## Verification

- `cargo test --workspace` — the repaired health-shape assertion and the
  new `tauri.conf.json` sync test.
- `ng test` — the About badge specs.
- A projection unit test over the table in unit 5, including the Debian
  tilde and a `+`-rejection case for the Docker tag.
- Manual: `quilltap --version`, `/health`, the Tauri health command, and
  the About badge all print the same string.
