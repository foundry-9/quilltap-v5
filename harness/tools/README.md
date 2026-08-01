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
python3 harness/tools/recipe_sweep.py --collisions
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
- `--run` executes ONE family end-to-end in its own clean invocation: it
  deletes the family's oracle NDJSON outputs first (a stale oracle can never
  pass silently), runs the regen stage(s), then the `cargo test` run stage
  with `CARGO_INCREMENTAL=0`. This is the "recipe executed verbatim" proof
  the work orders ask for.

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
- Elided jest stages (`… npx jest -- <case>`) in `.rs` headers are tolerated
  ONLY when the named oracle case's own header carries the complete recipe —
  the driver restores from there. If neither side is complete, the family is
  `non_extractable` and the header must be fixed.
