# harness/tools — the recipe sweep driver

`recipe_sweep.py` makes the standing rule `harness-recipes-are-runnable`
mechanically checkable: every differential family's test file carries its own
oracle-regeneration recipe in its leading doc comment, and this driver
extracts, validates, and executes those recipes. It was rebuilt for P4.27 from
P4.D32's lane recipe (whose scratch scripts did not survive) and committed so
the next broad sweep runs it instead of re-deriving it.

## Usage

```bash
python3 harness/tools/recipe_sweep.py --list [--json report.json]
python3 harness/tools/recipe_sweep.py --show  <family>
python3 harness/tools/recipe_sweep.py --run   <family>
python3 harness/tools/recipe_sweep.py --run-all [--families a,b] [--exclude c,d] \
    --results harness/tools/sweep-results/<date>-<v4-baseline>-<label>.json
python3 harness/tools/recipe_sweep.py --collisions
python3 harness/tools/recipe_sweep.py --self-test

# during a drift round, pin v4 (see "The v4 pin" below)
python3 harness/tools/recipe_sweep.py --v4 /tmp/qt-v4-pin-<order>-<sha> --run <family>
```

`<family>` is a test-file stem, e.g. `characters_read_equivalence`. The scan
covers `crates/{quilltap-harness,quilltap-web,quilltap-cli}/tests/*.rs`.

- `--list` classifies every family: `ok` (recipe extracts clean),
  `ok_restored` (the `.rs` header elides the jest stage and the full recipe
  was restored from the oracle case's own `/** … */` header under
  `harness/oracle/cases/` — P4.D32's "anchored restoration"),
  `committed_corpus` (the oracle is a committed recorded corpus; nothing to
  regenerate mechanically — the header names the deliberate regen script),
  `no_oracle` (integration tests with no oracle at all — the quilltap-web
  envelope arms, the CLI Tier R driver), `exempt` (compile-time wire/seam
  pins, no oracle by design), and `non_extractable` (a broken header — fix
  it; the P4.27 lane drove this bucket to zero).
  `--list` also reports two WARNING classes that do not change a family's
  status but do change whether/where its recipe runs:
  `unstaged_jest_roots` (see "The venue rule") and `external_tmp_input`
  (a /tmp path the recipe reads but never writes — it leans on another
  recipe's staging).
- `--run` executes ONE family end-to-end in its own clean invocation: it
  deletes the family's oracle NDJSON outputs first (a stale oracle can never
  pass silently), runs the regen stage(s), then the `cargo test` run stage
  with `CARGO_INCREMENTAL=0`. This is the "recipe executed verbatim" proof
  the work orders ask for.
- `--run-all` does the same over many families and **writes its results
  artifact after every one**, so a batch that dies mid-run still leaves its
  classification behind. P4.D32's and P4.D42's per-family results both died
  in /tmp (`/tmp/d32-rest-final.json`, `/tmp/p4d42-sweep-logs/`), and P4.34
  had to recover D42's from a session transcript. Committed results live in
  `harness/tools/sweep-results/`; each row is
  `family → status (ok / regen_failed / run_failed / skipped /
  refused_*) → cause`.
- `--self-test` runs the driver's own classifier and detector assertions
  (the two real F3 prose-leak lines, the F5 `_SKIP` false positive, the F6
  pin paths, the venue and external-/tmp classes, the policy-2 suffix). Run
  it after ANY change to the extraction machinery.

## The venue rule (P4.34's F1)

v4's `jest.config.ts` puts `/\.claude/` in BOTH `testPathIgnorePatterns` and
`modulePathIgnorePatterns` whenever jest itself runs outside an agent
worktree — which is always, since the oracle runs from
`~/source/quilltap-server`. So a recipe that hands jest a `--roots` under
the **v5** checkout finds ZERO tests when the sweep runs from a
`.claude/worktrees/…` lane checkout, and fails for a reason that has
nothing to do with the recipe. P4.D42's sweep ran from a worktree and
counted ten such families as "regen rot"; eight of them were green from the
main checkout.

`--list` flags the shape as `unstaged_jest_roots`, and `--run` refuses such
a family from a `/.claude/` venue (override with `--force`). The repair is
the staged-mirror convention below, after which the recipe runs from any
venue. Until a family is repaired, run its recipe with
`--v5w ~/source/quilltap-v5` from the main checkout.

**P4.40 corrected the detector.** It used to fire on ANY root ending in
`harness/oracle/cases` — which the correct staged mirror also ends in, since
the case reads its spec via `join(here,'..','fixtures',…)` and the mirror
must keep `cases/` and `fixtures/` as siblings. That warned 16 already-correct
families and made `--run` refuse them from a lane worktree, which is most of
why the venue rule read as a large standing debt. The check now expands the
root's leading variable and skips anything resolving under `/tmp`. If you
change it, keep both self-test false-positive assertions.

## The v4 pin (`--v4`)

Every recipe reaches v4 through a literal `cd ~/source/quilltap-server`, and
headers are forbidden from naming a `/tmp` pin (a detached pin does not
survive the round that made it — the `stale_v4_pin_path` refusal). So a lane
whose baseline is BEHIND v4 HEAD — the normal state during a drift round, and
v4 ships daily — must pass its pin on the command line:

```bash
git -C ~/source/quilltap-server worktree add --detach /tmp/qt-v4-pin-<order>-<sha> <sha>
ln -sfn ~/source/quilltap-server/node_modules /tmp/qt-v4-pin-<order>-<sha>/node_modules
ln -sfn ~/source/quilltap-server/packages/quilltap/node_modules \
        /tmp/qt-v4-pin-<order>-<sha>/packages/quilltap/node_modules
python3 harness/tools/recipe_sweep.py --v4 /tmp/qt-v4-pin-<order>-<sha> --run-all …
```

Without it a sweep regenerates every oracle against whatever v4 has shipped
since, silently baking an unabsorbed drift into the comparands. The default is
a byte-for-byte no-op, and `--self-test` asserts both that and the redirect.

## The two P4.D32 sweep hazards, enforced as policy

1. **No recipe writes into the repo.** `--run` refuses any recipe whose
   `cp`/`mv`/redirect destination — or whose fixture-builder invocation —
   lands inside the checkout. Regenerating an oracle for a committed fixture
   family means copying the committed `.db` to /tmp and pointing the oracle
   at the COPY; a rebuild mints fresh UUIDs and silently invalidates every
   other family reading that fixture. As a second layer, `--run` rewrites any
   `QT_FIXTURE*` env value that points at a repo `.db` to a per-family /tmp
   copy before executing (some v4 oracle drivers mutate the fixture they are
   pointed at). The named exception list (`DELIBERATE_REPO_WRITERS`) carries
   the families whose recipe writes a committed artifact BY DESIGN with a
   safeguard that makes it sound (today: the hash-pinned uuid-remap corpus);
   `--run` still refuses those — regenerating a committed artifact is a lane
   decision, never a sweep's.
2. **No cross-family /tmp clobbering.** `--run` is atomic per family
   (regen + run in one invocation — the D32 reds came from batch-regen then
   batch-run), and recipe-local scratch dirs (`TMPO=`, `STAGE=`) are suffixed
   with the family name at run time. `--collisions` reports the /tmp paths
   still shared between families' headers; shared mirror DIRS are benign
   under atomic per-family runs, but new headers should pick per-family
   names (`/tmp/qt-<family>-oracle`), never the old shared
   `/tmp/qt-oracle-stage`.

## Header conventions

- `N=~/.nvm/versions/node/v24.13.1/bin` for the Node 24 bin dir; the driver
  prepends it when a recipe uses `$N` without assigning it.
- `V5W=${V5W:-$HOME/source/quilltap-v5}` for the v5 checkout — copy-paste
  safe, and the driver overrides it to `--v5w` (default: the repo containing
  the script) so a worktree sweep tests the worktree's own cases.
- TZ pins (`TZ=UTC`, `America/Chicago` legs) are load-bearing since P4.d26 —
  the driver never adds or strips environment words; preserve them verbatim
  when editing headers.
- **Stage the case into a per-family /tmp mirror; never point jest at
  `$V5W/harness/oracle/cases`** (the venue rule above):

  ```text
  TMPO=/tmp/qt-<family>-oracle
  rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
  cp "$V5W/harness/oracle/cases/<case>.test.ts"  "$TMPO/cases/"
  cp "$V5W/harness/oracle/fixtures/<spec>.json"  "$TMPO/fixtures/"
  cd ~/source/quilltap-server
  … $N/npx jest --silent --watchman=false --roots "$PWD" --roots "$TMPO/cases" -- <pattern>
  ```

  The `mkdir` is load-bearing in its own right: a recipe that assigns
  `TMPO=` and never creates it leans on whatever an earlier recipe left
  behind (`external_tmp_input`).
- **Never open a header sentence with a command word.** The extractor keeps
  shell-looking lines, and a sentence starting `diff the written …` is
  indistinguishable from a `diff` invocation — the recipe then runs the doc
  sentence and dies on a bash syntax error (the F3 prose-leak class; P4.40
  found two more, in `danger_gatekeeper_tier3` and `state_sql_tools`).
  Rewrite as "the written … are compared".
- **One assignment per line.** `WT=… STAGE=/tmp/qt-oracle-stage` on a single
  line defeats the per-family scratch suffix (policy 2 anchors on `^VAR=`),
  so the family silently shares a mirror with every other family that used
  the same name.
- **Never name a `/tmp` v4 pin worktree in a header.** A detached pin does
  not survive the round that made it, so the recipe is dead on arrival
  (`stale_v4_pin_path`). Regenerate from `~/source/quilltap-server`; if the
  family needs an older v4 vintage, say so in prose and let the reader make
  the pin.
- **The `cargo test` run line carries its own `QT_ORACLE_*` env prefix.**
  A run line without it inherits nothing and the family SKIPs — which
  `--run` reports as `skipped`, not as a pass.
- Elided jest stages (`… npx jest -- <case>`) in `.rs` headers are tolerated
  ONLY when the named oracle case's own header carries the complete recipe —
  the driver restores from there. If neither side is complete, the family is
  `non_extractable` and the header must be fixed.
