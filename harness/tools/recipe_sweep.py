#!/usr/bin/env python3
r"""The harness recipe sweep driver (P4.27, rebuilt from P4.D32's lane recipe).

Every differential family's test file carries its own oracle-regeneration
recipe in the leading `//!` doc comment (the standing rule:
`harness-recipes-are-runnable`). This driver makes that rule mechanically
checkable, so a broad sweep (a P4.D32-style neutrality sweep, or a baseline
move) never has to re-derive the extraction machinery again:

  --list          extract + statically validate every family's recipe;
                  report each family as ok / ok_restored / committed_corpus /
                  non_extractable / no_recipe / exempt, with reasons.
                  `--json PATH` writes the full report.
  --show FAMILY   print the extracted, normalized stages for one family.
  --run FAMILY    execute one family's recipe end-to-end in its OWN clean
                  invocation: remove the oracle outputs first, run the regen
                  stage(s), then the `cargo test` run stage. This is the
                  "recipe executed verbatim" proof the work orders ask for.
  --run-all       the same, over MANY families, writing a durable results
                  artifact after EVERY family (P4.34's F7 — two sweeps' worth
                  of per-family classification died in /tmp because the driver
                  had no batch mode and no artifact). `--families a,b,c` /
                  `--exclude a,b,c` / `--results PATH` shape the batch.
  --collisions    report /tmp paths written by more than one family's regen
                  (P4.D32 sweep hazard 2 — cross-family clobbering).
  --self-test     run the driver's own classifier/detector assertions.

Policies enforced (both are P4.D32's sweep hazards, made rules by P4.27):

  1. REPO-WRITE REFUSAL. A recipe must never write inside the repository:
     no `cp`/`mv` destination and no `>` redirect may land under the v5
     checkout. Regenerating an oracle for a COMMITTED fixture family means
     copying the committed `.db` to /tmp and pointing the oracle at the
     COPY (a rebuild mints fresh UUIDs and silently invalidates every other
     family reading that fixture). `--run` refuses such recipes outright;
     `--list` flags them. As a belt-and-braces shield, `--run` also rewrites
     any `QT_FIXTURE*` env value that points at a repo-committed `.db` to a
     per-family /tmp COPY before executing — some v4 oracle drivers mutate
     the fixture they are pointed at (the `episodic-recall-*` class).
  2. UNIQUE /tmp PATHS. Two different families' regens must not write the
     same /tmp path, or one sweep's families clobber each other (five of
     P4.D32's twelve gate reds). `--collisions` finds offenders; `--run`
     additionally suffixes recipe-local scratch-dir assignments (`TMPO=`,
     `STAGE=`) with the family name so restored jest mirrors cannot collide.

Anchored restoration (P4.D32's rule for elided invocations): many `.rs`
headers elide the jest stage as `… QT_ORACLE_OUT=… npx jest -- <case>` and
point at "the .ts header" for the real steps. The authoritative recipe DOES
exist — in the oracle case file's own `/** … */` header — so the driver
resolves the named case under `harness/oracle/cases/` and extracts THAT
header's shell block as the regen stage. A family is only `non_extractable`
when even that fails.

Variable conventions the driver understands (and headers should use):

  N     the Node 24 bin dir (`~/.nvm/versions/node/v24.13.1/bin`). If a
        recipe uses `$N` without assigning it, the driver prepends the
        canonical assignment.
  V5W   the v5 checkout (headers default it with
        `V5W=${V5W:-$HOME/source/quilltap-v5}` so they are copy-paste-safe;
        the driver overrides it to `--v5w`, default: the repo containing
        this script, so a worktree sweep tests the worktree's own cases).
  Legacy single-letter aliases (`V5`, `W`) and the literal
  `~/source/quilltap-v5` are rewritten to the driver's `--v5w` at run time.

TZ pins are load-bearing (the P4.d26 rule): the driver never strips or adds
environment words — `TZ=UTC` / `TZ=America/Chicago` run exactly as written.

THE VENUE RULE (P4.34's F1 — the confound that poisoned P4.D42's numbers).
v4's `jest.config.ts` puts `/\.claude/` in BOTH `testPathIgnorePatterns` and
`modulePathIgnorePatterns` whenever jest itself runs outside an agent worktree
— which is always, since the oracle runs from `~/source/quilltap-server`. So a
recipe that hands jest a `--roots` under the v5 checkout finds ZERO tests when
the sweep runs from a `.claude/worktrees/…` checkout, and exits 1 for a reason
that has nothing to do with the recipe. The driver therefore (a) flags that
recipe shape statically (`unstaged_jest_roots`) so it can be repaired to the
staged-mirror convention, and (b) refuses to `--run` an unstaged family from a
`.claude/` venue unless `--force` is given. A repaired recipe stages its case
into its own `/tmp` mirror and runs correctly from any venue.

Exempt by design (compile-time pins, no oracle): see EXEMPT_FAMILIES.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

# Families that are compile-time pins with no oracle by design (P4.D32's six).
EXEMPT_FAMILIES = {
    "p4_6ao_wire_contract",
    "p4_6ar_wire_contract",
    "p4_9g1_wire_contract",
    "p4_d10_wire_contract",
    "p4_9g6_seam_contract",
    "settings_wire_actions",
}

NODE_BIN_DEFAULT = "~/.nvm/versions/node/v24.13.1/bin"

# Families whose recipe writes into the repo BY DESIGN, with the safeguard that
# makes it sound. These are not policy-1 violations; `--run` still refuses them
# (regenerating a committed artifact is a lane decision, never a sweep's).
DELIBERATE_REPO_WRITERS = {
    "backup_uuid_remap_equivalence": (
        "hash-pinned corpus regen: the oracle authors the committed "
        "uuid-remap-corpus.json and its NDJSON atomically, and the family "
        "recomputes the sha256 from the committed file — a stale corpus can "
        "never pass (P4.9G6)."
    ),
}

# A line that STARTS a shell command (vs prose). Continuations of a line
# ending in `\` are shell regardless.
SHELL_START = re.compile(
    r"^(?:"
    # F8 (P4.34): an ELIDED command (`… QT_ORACLE_OUT=… npx jest -- <case>`).
    # Dropping it as prose hid the elision from `extract`'s own `…` check, so
    # anchored restoration never fired and the family ran the truncated tail
    # instead — `chat_regenerate_title_tier3`'s recipe degenerated to a bare
    # `npx jest -- <case>` with no `cd`, and jest died looking for a config.
    r"…\s*\S"
    r"|[A-Za-z_][A-Za-z0-9_]*=\S"  # VAR=value (env prefix or assignment)
    r"|cd\s|cp\s|mv\s|rm\s|mkdir\s|ln\s|export\s|for\s|do\s|done\b|touch\s"
    r"|\$N/|\$\{?N\}?/|npx\s|node\s|cargo\s|python3?\s|bash\s|sh\s|diff\s"
    r"|brctl\s|sqlite3\s|tar\s|unzip\s|curl\s"
    r")"
)
# Prose traps that would otherwise match SHELL_START ("TZ=UTC is REQUIRED …").
PROSE_TRAP = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=\S+\s+(?:is|are|was|were|means)\b")
# F3 (P4.34): the same trap for KEYWORD-led prose — a doc sentence that happens
# to open with a shell word ("touch is the preserve closure, a no-op here), …",
# "cargo run fine from the worktree.)"). Two such lines leaked a stray `)` into
# the generated script and made `tool_build` / `text_tool_loop_tier3` exit 2
# with `bash: -c: syntax error near unexpected token ')'` — a driver defect
# that P4.D42 counted as recipe rot.
COPULA_PROSE = re.compile(
    r"^[A-Za-z_][A-Za-z0-9_]*(?:=\S+)?\s+"
    r"(?:is|are|was|were|means|must|should|can|will|would|does|has|have|"
    r"needs?|only|also|still|never|always)\b"
)
_BRACKET_PAIRS = (("(", ")"), ("[", "]"), ("{", "}"))

PLACEHOLDER_WORKTREE = re.compile(
    r"<[^<>]*(?:worktree|checkout|repo-root|this tree|v5)[^<>]*>", re.IGNORECASE
)
ANY_PLACEHOLDER = re.compile(r"<[^<>=\s][^<>]*>")
ELISION = "…"

CASE_REF = re.compile(r"(?:cases/)?([A-Za-z0-9][A-Za-z0-9_.-]*?\.(?:test\.)?tsx?)\b")
JEST_ELIDED = re.compile(r"npx jest[^\n]*?--\s+([A-Za-z0-9_.\\$-]+)")

# F6 (P4.34): a detached v4 pin worktree lives in /tmp and NEVER survives the
# round that made it (`/tmp`-pins-die-between-rounds — it has bitten at least
# seven recipes across three rounds). A recipe naming one is dead on arrival.
STALE_V4_PIN = re.compile(r"/(?:private/)?tmp/qt-v4-pin[^\s\"';]*")

# F2 (P4.34): a jest root under the v5 checkout — the venue rule above.
JEST_ROOTS_ARG = re.compile(r"--(?:roots|rootDir)[= ]\"?([^\s\"]+)\"?")
V5_REF = re.compile(r"\$\{?(?:V5W|V5|W|WT)\}?/|~/source/quilltap-v5/")

# The harness's own skip notice, anchored (F5): `eprintln!("SKIP: …")` and its
# `SKIP <family>: …` variants always start the line. The old detector grepped a
# bare `\bSKIP\b` over the whole output, so a recipe echoing an env var NAME
# that ends in `_SKIP` (`QT_ORACLE_SALON_SKIP`) reported a phantom skip.
SKIP_LINE = re.compile(r"^\s*SKIP\b")
SKIP_PROSE = re.compile(r"skipping\b[^\n]*differential|not set — skipping")


def rs_doc_header(path: Path) -> list[str]:
    """The file's leading `//!` block, comment markers stripped."""
    out = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.startswith("//!"):
            out.append(line[3:])
        elif out:
            break
    return out


def ts_doc_header(path: Path) -> list[str]:
    """The leading `/** … */` block of an oracle case, ` * ` markers stripped."""
    out = []
    inside = False
    for line in path.read_text(encoding="utf-8").splitlines():
        s = line.strip()
        if not inside:
            if s.startswith("/**"):
                inside = True
            continue
        if s.startswith("*/"):
            break
        if s.startswith("*"):
            out.append(" " + s[1:])
    return out


def is_prose(stripped: str) -> bool:
    """True when a line that OPENS like a shell command is really a sentence."""
    if PROSE_TRAP.match(stripped) or COPULA_PROSE.match(stripped):
        return True
    # An unbalanced closer can only come from prose whose parenthesis opened on
    # an earlier (prose, hence dropped) line. A shell command must balance its
    # own grouping to run at all, so this never fires on a real command.
    for opener, closer in _BRACKET_PAIRS:
        if stripped.count(closer) > stripped.count(opener):
            return True
    return False


def shell_lines(doc: list[str]) -> list[str]:
    """Classify doc lines into an ordered shell script (prose dropped)."""
    lines: list[str] = []
    in_continuation = False
    for raw in doc:
        stripped = raw.strip()
        if not stripped:
            in_continuation = False
            continue
        indented = raw.startswith("  ")
        looks_shell = bool(SHELL_START.match(stripped)) and not is_prose(stripped)
        if (in_continuation and indented) or looks_shell:
            lines.append(stripped)
            in_continuation = stripped.endswith("\\")
        else:
            in_continuation = False
    return lines


class Recipe:
    def __init__(self, family: str, path: Path):
        self.family = family
        self.path = path
        self.regen: list[str] = []  # shell lines before the run line(s)
        self.run: list[str] = []  # the cargo-test run line(s)
        self.problems: list[str] = []
        self.notes: list[str] = []
        # Defects that do NOT make a recipe statically unextractable but change
        # whether/where it can run (P4.34's F2 + tier-2 external-/tmp class).
        self.warnings: list[str] = []
        self.tmp_writes: set[str] = set()
        self.repo_writes: list[str] = []


def extract(v5w: Path, family: str, path: Path) -> Recipe:
    r = Recipe(family, path)
    doc = rs_doc_header(path)
    doc_text = "\n".join(doc)
    lines = shell_lines(doc)

    # Split on LOGICAL commands (continuation lines joined), so the env-prefix
    # lines that precede a `cargo test` continuation stay with the run stage.
    logical: list[list[str]] = []
    for line in lines:
        if logical and logical[-1][-1].endswith("\\"):
            logical[-1].append(line)
        else:
            logical.append([line])
    run_seen = False
    for cmd in logical:
        if run_seen or any(re.search(r"\bcargo test\b", l) for l in cmd):
            r.run.extend(cmd)
            run_seen = True
        else:
            r.regen.extend(cmd)

    # A family "consumes an oracle" when its run needs an env-pointed NDJSON;
    # committed recorded corpora (`*.recorded.ndjson` in-repo) do not count.
    consumes_oracle = bool(
        re.search(r"QT_ORACLE[A-Z0-9_]*=", doc_text) or "/tmp/oracle" in doc_text
    )
    committed_corpus = bool(
        re.search(r"committed", doc_text, re.I)
        and re.search(
            r"regenerat|runs in every plain|no env var", doc_text, re.I
        )
    )
    regen_text = "\n".join(r.regen)
    has_regen_cmd = bool(re.search(r"npx|\$N/|tsx\b|jest\b|\.sh\b|\.mjs\b", regen_text))

    if not consumes_oracle and committed_corpus:
        r.notes.append("committed_corpus")
        scan_writes(r, PLACEHOLDER_WORKTREE.sub("$V5W", "\n".join(lines)))
        return r
    if not lines and not consumes_oracle:
        if ".ndjson" not in doc_text and "QT_ORACLE" not in doc_text:
            # An integration test with no oracle at all (the quilltap-web
            # envelope arms, the CLI Tier R driver) — nothing to regenerate.
            r.notes.append("no_oracle")
        else:
            r.problems.append("no_recipe")
        return r

    if ELISION in regen_text or (consumes_oracle and not has_regen_cmd):
        # Anchored restoration: pull the regen from the named oracle case's
        # own header.
        case = resolve_case(v5w, doc_text, family)
        restored = ts_case_recipe(case) if case is not None else []
        if restored and case is not None:
            r.regen = restored
            r.notes.append(f"restored_from {case.relative_to(v5w)}")
        else:
            r.problems.append(
                "elided_or_missing_regen (no oracle-case header to restore from)"
            )

    if not r.run and consumes_oracle:
        r.problems.append("no_cargo_test_run_line")

    joined = "\n".join(r.regen + r.run)
    if ELISION in joined:
        r.problems.append("elided_command (…)")
    if re.search(r"=\.\.\.(?:\s|\\|$)", joined):
        r.problems.append("elided_env_value (=...)")
    for m in ANY_PLACEHOLDER.finditer(joined):
        token = m.group(0)
        if PLACEHOLDER_WORKTREE.match(token):
            continue  # normalized at run time
        if "/" in token or " " in token:
            r.problems.append(f"unresolved_placeholder {token}")
    if ".claude/worktrees" in joined:
        r.problems.append("stale_worktree_path")
    for m in STALE_V4_PIN.finditer(joined):
        problem = f"stale_v4_pin_path {m.group(0)}"
        if problem not in r.problems:
            r.problems.append(problem)
    # Scan writes over placeholder-normalized text so `<V5W>`-style brackets
    # can't be misread as redirects.
    scan_writes(r, PLACEHOLDER_WORKTREE.sub("$V5W", joined))
    scan_venue(r, joined)  # after scan_writes — it consumes r.tmp_writes
    # Policy 1's second form: a REGEN stage whose oracle is pointed straight at
    # a repo-committed `.db` that the v4 case then MUTATES IN PLACE (the
    # `embedding-generate-*` / `episodic-recall-*` class, P4.D32). Cases that
    # copy the pointed fixture into a scratch first (`copyFileSync`) are pure
    # reads — the standing committed-fixture rule REQUIRES pointing those at
    # the committed DBs, so they are not flagged. The RUN stage reading
    # committed fixtures is the Rust side's canonical read and is always fine.
    regen_norm = PLACEHOLDER_WORKTREE.sub("$V5W", "\n".join(r.regen))
    # Group continuation lines into logical commands so env prefixes are seen
    # together with the command they precede.
    commands = re.sub(r"\\\n", " ", regen_norm).splitlines()
    for cmd in commands:
        vals = [
            m.group(1)
            for m in re.finditer(r"QT_[A-Z0-9_]*=(\S+\.db)\b", cmd)
            if re.search(r"\$\{?(?:V5W|V5|W|WT)\}?/", m.group(1))
            or "crates/" in m.group(1)
        ]
        if not vals:
            continue
        if re.search(r"build-[A-Za-z0-9-]*fixture", cmd):
            # A fixture BUILDER invoked with env values inside the repo writes
            # the committed fixture in place (the `episodic-recall-*` overwrite
            # P4.D32 observed) — always a policy-1 violation.
            r.repo_writes.append(cmd.strip())
            if "repo_write (policy 1)" not in r.problems:
                r.problems.append("repo_write (policy 1)")
            continue
        case = resolve_case(v5w, doc_text, family)
        case_copies = case is not None and bool(
            re.search(
                r"copyFileSync|cpSync|fs\.copyFile", case.read_text(encoding="utf-8")
            )
        )
        if not case_copies:
            for val in vals:
                r.problems.append(f"oracle_points_at_committed_db {val}")
    return r


def resolve_case(v5w: Path, doc_text: str, family: str) -> Path | None:
    cases = v5w / "harness" / "oracle" / "cases"
    candidates: list[str] = []
    for m in CASE_REF.finditer(doc_text):
        candidates.append(m.group(1))
    for m in JEST_ELIDED.finditer(doc_text):
        pat = m.group(1).replace("\\.test", ".test").replace("$", "")
        candidates.append(f"{pat}.test.ts")
        candidates.append(f"{pat}.ts")
    # Fall back on the family name itself (family foo_bar_equivalence →
    # foo-bar.test.ts / foo-bar.ts), with and without a -tier2/-tier3 suffix.
    stem = family.removesuffix("_equivalence").replace("_", "-")
    bare = re.sub(r"-tier\d$", "", stem)
    for s in dict.fromkeys([stem, bare]):
        candidates += [f"{s}.test.ts", f"{s}.ts"]
    for name in candidates:
        p = cases / name
        if p.is_file():
            return p
    return None


def ts_case_recipe(case: Path) -> list[str]:
    lines = shell_lines(ts_doc_header(case))
    # The case header's recipe is regen-only (jest/tsx); drop any cargo-test
    # tail it may carry (the .rs run line is authoritative).
    return [l for l in lines if "cargo test" not in l]


def scan_writes(r: Recipe, joined: str) -> None:
    """Collect /tmp write targets and flag repo-write destinations."""
    assigns: dict[str, str] = {}
    for m in re.finditer(r"\b([A-Za-z_][A-Za-z0-9_]*)=(/(?:private/)?tmp/\S+)", joined):
        assigns.setdefault(m.group(1), m.group(2).strip('"').rstrip(";"))

    def resolve(tok: str) -> str:
        tok = tok.strip('"').strip("'")
        for var, val in assigns.items():
            tok = tok.replace(f"${{{var}}}", val).replace(f"${var}", val)
        return os.path.expanduser(tok)

    def record(dest: str, line: str) -> None:
        if dest.startswith(("/tmp", "/private/tmp")):
            r.tmp_writes.add(dest.rstrip("/"))
        elif "$" in dest and not re.search(r"\$\{?(?:V5W|V5|W|WT)\}?\b", dest):
            pass  # an unresolved recipe-local var — unknowable, don't flag
        elif is_repo_path(dest):
            r.repo_writes.append(line)

    for line in joined.splitlines():
        m = re.match(r"^(?:cp|mv)\s+(?:-[a-zA-Z]+\s+)*(.+)$", line.rstrip("\\").strip())
        if m:
            args = m.group(1).split()
            if len(args) >= 2:
                record(resolve(args[-1]), line)
        for rm_ in re.finditer(r"(?:^|\s)>\s*(\S+)", line):
            record(resolve(rm_.group(1)), line)
        for rm_ in re.finditer(r"(?:QT_[A-Z0-9_]*_|QT_)(?:OUT|OUTPUT)[A-Z0-9_]*=(\S+)", line):
            record(resolve(rm_.group(1)), line)
    if r.repo_writes:
        if r.family in DELIBERATE_REPO_WRITERS:
            r.notes.append(f"deliberate_repo_write: {DELIBERATE_REPO_WRITERS[r.family]}")
        else:
            r.problems.append("repo_write (policy 1)")


def scan_venue(r: Recipe, joined: str) -> None:
    """F2 + the tier-2 external-/tmp class, both venue/ordering hazards.

    `unstaged_jest_roots`: a jest `--roots` under the v5 checkout. Correct from
    the main checkout, ZERO tests found from a `.claude/worktrees/…` one (see
    THE VENUE RULE). The repair is the staged-mirror convention.

    `external_tmp_input`: a /tmp path the recipe READS but no line of it
    WRITES — i.e. it leans on another recipe's staging (or on a pin worktree
    from a round that is over). Warning, not a problem: the paths a recipe
    produces are only mechanically knowable up to the driver's write scan.
    """
    text = PLACEHOLDER_WORKTREE.sub("$V5W", joined)
    for line in re.sub(r"\\\n", " ", text).splitlines():
        if "jest" not in line:
            continue
        for m in JEST_ROOTS_ARG.finditer(line):
            root = m.group(1)
            if V5_REF.search(root) or root.rstrip("/").endswith("harness/oracle/cases"):
                warning = f"unstaged_jest_roots {root}"
                if warning not in r.warnings:
                    r.warnings.append(warning)

    produced = set(r.tmp_writes)
    # An ASSIGNMENT is not a creation. `TMPO=/tmp/qt-oracle-run` followed by
    # `--roots "$TMPO/cases"` and no mkdir is precisely the "leans on another
    # recipe's staging" defect (compression_cache_tier3, found by P4.34's phase
    # 1: `Directory /tmp/qt-oracle-run/cases in the roots[1] option was not
    # found`). So assignments feed variable EXPANSION only.
    assigns: dict[str, str] = {}
    for m in re.finditer(r"\b([A-Za-z_][A-Za-z0-9_]*)=(/(?:private/)?tmp/\S+)", text):
        assigns.setdefault(m.group(1), m.group(2).strip('"').rstrip(";").rstrip("/"))
    for m in re.finditer(r"\bmkdir\s+(?:-[a-zA-Z]+\s+)*(.+)$", text, re.M):
        for tok in m.group(1).split():
            produced.add(expand_tmp(tok, assigns))
    produced = {p for p in produced if p.startswith(("/tmp", "/private/tmp"))}

    # Only genuine READ inputs count — a jest root, a `cp` source, a script path
    # handed to tsx/node. Every other /tmp literal is an output the recipe is
    # declaring, and scanning those made the class 73-families noisy.
    consumed: set[str] = set()
    for line in re.sub(r"\\\n", " ", text).splitlines():
        for m in JEST_ROOTS_ARG.finditer(line):
            consumed.add(expand_tmp(m.group(1), assigns))
        m = re.match(r"^(?:cp|mv)\s+(?:-[a-zA-Z]+\s+)*(.+)$", line.strip())
        if m:
            args = m.group(1).split()
            consumed.update(expand_tmp(a, assigns) for a in args[:-1])
        for m2 in re.finditer(r"\b(?:tsx|node|bash|sh|python3?)\s+(\S+)", line):
            consumed.add(expand_tmp(m2.group(1), assigns))
    consumed = {c for c in consumed if c.startswith(("/tmp", "/private/tmp"))}
    made = {p.replace("/private/tmp/", "/tmp/", 1) for p in produced}
    for tok in sorted(consumed):
        norm = tok.replace("/private/tmp/", "/tmp/", 1)
        # Either direction counts: the recipe writes AT the path, or it writes
        # BELOW it (a `mkdir -p "$TMPO/cases"` creates `$TMPO` on the way).
        if any(
            norm == p or norm.startswith(p + "/") or p.startswith(norm + "/")
            for p in made
        ):
            continue
        warning = f"external_tmp_input {tok}"
        if warning not in r.warnings:
            r.warnings.append(warning)


def expand_tmp(tok: str, assigns: dict[str, str]) -> str:
    tok = tok.strip('"').strip("'")
    for var, val in assigns.items():
        tok = tok.replace(f"${{{var}}}", val).replace(f"${var}", val)
    return tok.rstrip("/")


def is_repo_path(dest: str) -> bool:
    if dest.startswith(("/tmp", "/private/tmp", "/dev/")):
        return False
    d = re.sub(r"^\$\{?(?:V5W|V5|W)\}?/", "REPO/", dest)
    d = d.replace("~/source/quilltap-v5", "REPO")
    return (
        d.startswith("REPO")
        or "crates/" in d
        or "harness/oracle/fixtures" in d
        or "tests/fixtures" in d
    )


def normalize(script_lines: list[str], v5w: Path, family: str) -> str:
    """Rewrite worktree placeholders/aliases to the driver's --v5w, suffix
    recipe-local scratch dirs with the family name (policy 2), and make the
    script standalone-runnable. Never touches TZ pins or other env words."""
    text = "\n".join(script_lines)
    text = PLACEHOLDER_WORKTREE.sub(str(v5w), text)
    text = text.replace("~/source/quilltap-v5", str(v5w))
    if re.search(r"\$N\b|\$\{N\}", text) and not re.search(r"^N=", text, re.M):
        text = f"N={NODE_BIN_DEFAULT}\n" + text
    header = "\n".join(
        f'{var}="{v5w}"'
        for var in ("V5W", "V5", "W")
        if re.search(rf"\${var}\b|\$\{{{var}\}}", text)
    )
    if header:
        text = header + "\n" + text
    # Policy 2: per-family scratch dirs for restored jest mirrors. The path ends
    # at the first `;`/whitespace, not at end-of-line — headers idiomatically
    # write `TMPO=/tmp/qt-oracle-x; mkdir -p $TMPO/cases`, and anchoring on `$`
    # silently skipped every one of them (so those mirrors could still collide).
    for var in ("TMPO", "STAGE"):
        text = re.sub(
            rf"^{var}=([^\s;]+?)/?(?=$|;|\s)",
            rf"{var}=\1-{family}",
            text,
            flags=re.M,
        )
    return "set -euo pipefail\n" + text


def detect_skip(output: str) -> str | None:
    """The harness's skip notice, anchored (F5). Returns the offending line."""
    for line in output.splitlines():
        if SKIP_LINE.match(line) or SKIP_PROSE.search(line):
            return line.strip()
    return None


def shield_fixture_envs(script: str, v5w: Path, family: str) -> str:
    """Policy 1 shield: any QT_FIXTURE* env value pointing at a repo `.db` is
    rewritten to a per-family /tmp COPY (some v4 oracle drivers mutate the
    fixture they are pointed at)."""
    shield_dir = Path(f"/tmp/qt-recipe-shield-{family}")
    copies: dict[str, str] = {}

    def sub(m: re.Match) -> str:
        var, val = m.group(1), m.group(2)
        expanded = val.replace("$V5W", str(v5w)).replace("$V5", str(v5w)).replace(
            "$W", str(v5w)
        )
        expanded = os.path.expanduser(expanded)
        if not expanded.endswith(".db") or expanded.startswith(("/tmp", "/private/tmp")):
            return m.group(0)
        src = Path(expanded)
        if not src.is_file():
            return m.group(0)
        shield_dir.mkdir(parents=True, exist_ok=True)
        dst = shield_dir / src.name
        if str(src) not in copies:
            shutil.copyfile(src, dst)
            copies[str(src)] = str(dst)
            print(f"shielded {src} -> {dst}")
        return f"{var}={copies[str(src)]}"

    return re.sub(r"(QT_FIXTURE[A-Z_]*)=(\S+)", sub, script)


def family_files(v5w: Path) -> list[tuple[str, Path]]:
    out = []
    for crate in ("quilltap-harness", "quilltap-web", "quilltap-cli"):
        d = v5w / "crates" / crate / "tests"
        if d.is_dir():
            for f in sorted(d.glob("*.rs")):
                out.append((f.stem, f))
    return out


def classify(v5w: Path) -> list[dict]:
    rows = []
    for family, path in family_files(v5w):
        if family in EXEMPT_FAMILIES:
            rows.append({"family": family, "file": str(path.relative_to(v5w)), "status": "exempt"})
            continue
        r = extract(v5w, family, path)
        if "committed_corpus" in r.notes:
            status = "committed_corpus"
        elif "no_oracle" in r.notes:
            status = "no_oracle"
        elif r.problems == ["no_recipe"]:
            status = "no_recipe"
        elif r.problems:
            status = "non_extractable"
        elif any(n.startswith("restored_from") for n in r.notes):
            status = "ok_restored"
        else:
            status = "ok"
        rows.append(
            {
                "family": family,
                "file": str(path.relative_to(v5w)),
                "status": status,
                "problems": r.problems,
                "warnings": r.warnings,
                "notes": r.notes,
                "tmp_writes": sorted(r.tmp_writes),
                "repo_writes": r.repo_writes,
            }
        )
    return rows


def cmd_list(v5w: Path, json_out: str | None) -> int:
    rows = classify(v5w)
    counts: dict[str, int] = {}
    warned: dict[str, int] = {}
    for row in rows:
        counts[row["status"]] = counts.get(row["status"], 0) + 1
        if row["status"] in ("non_extractable", "no_recipe"):
            print(f"{row['family']}: {', '.join(row.get('problems', []))}")
        for w in row.get("warnings", []):
            kind = w.split()[0]
            warned[kind] = warned.get(kind, 0) + 1
    for kind in sorted(warned):
        fams = [r["family"] for r in rows if any(w.startswith(kind) for w in r.get("warnings", []))]
        print(f"\n{kind} ({len(fams)}): {' '.join(fams)}")
    print(f"\ntotals: {counts}")
    if warned:
        print(f"warnings: {warned}")
    if json_out:
        Path(json_out).write_text(json.dumps(rows, indent=2))
        print(f"wrote {json_out}")
    return 0


def cmd_collisions(v5w: Path) -> int:
    writers: dict[str, list[str]] = {}
    for row in classify(v5w):
        for p in row.get("tmp_writes", []):
            writers.setdefault(p, []).append(row["family"])
    bad = {p: fams for p, fams in writers.items() if len(set(fams)) > 1}
    for p, fams in sorted(bad.items()):
        print(f"{p}: {sorted(set(fams))}")
    print(f"\n{len(bad)} colliding /tmp paths")
    return 1 if bad else 0


def cmd_show(v5w: Path, family: str) -> int:
    path = find_family(v5w, family)
    r = extract(v5w, family, path)
    print(f"# ---- regen ({'; '.join(r.notes) or 'from the .rs header'})")
    print(normalize(r.regen, v5w, family))
    print("# ---- run")
    print(normalize(r.run, v5w, family))
    if r.problems:
        print(f"# problems: {r.problems}", file=sys.stderr)
    return 0


def venue_is_worktree(v5w: Path) -> bool:
    return "/.claude/" in str(v5w.resolve()) + "/"


def run_family(v5w: Path, family: str, force: bool, quiet: bool = False) -> dict:
    """Execute one family's recipe end-to-end. Returns a results record:
    status ∈ ok / refused_repo_write / refused_non_extractable / refused_venue /
    regen_failed / run_failed / skipped."""
    rec: dict = {"family": family, "status": "ok", "cause": None, "exit": 0}
    path = find_family(v5w, family)
    r = extract(v5w, family, path)
    rec["warnings"] = list(r.warnings)
    unstaged = [w for w in r.warnings if w.startswith("unstaged_jest_roots")]
    if r.repo_writes:
        rec.update(
            status="refused_repo_write",
            cause="recipe writes into the repo (policy 1): " + "; ".join(r.repo_writes),
            exit=2,
        )
        return rec
    if r.problems:
        rec.update(
            status="refused_non_extractable",
            cause="; ".join(r.problems),
            exit=2,
        )
        return rec
    if unstaged and venue_is_worktree(v5w) and not force:
        rec.update(
            status="refused_venue",
            cause=(
                "the recipe hands jest a root under the v5 checkout and this "
                f"checkout is under /.claude/ ({v5w}) — v4 jest ignores those "
                "paths, so the run would find ZERO tests and fail for a reason "
                "that is not the recipe's. Re-run from the main checkout "
                f"(--v5w ~/source/quilltap-v5), repair to a staged mirror, or "
                f"pass --force. [{'; '.join(unstaged)}]"
            ),
            exit=4,
        )
        return rec
    # Clean invocation: remove this family's oracle outputs (redirect targets)
    # so a stale NDJSON can never pass silently (`oracle-regen-silent-stale-pass`).
    joined = "\n".join(r.regen + r.run)
    for m in re.finditer(r">\s*(/tmp/\S+\.ndjson)", joined):
        try:
            os.remove(m.group(1))
            print(f"removed stale {m.group(1)}")
        except FileNotFoundError:
            pass
    env = dict(os.environ, CARGO_INCREMENTAL="0")
    for label, stage in (("regen", r.regen), ("run", r.run)):
        if not stage:
            continue
        script = shield_fixture_envs(normalize(stage, v5w, family), v5w, family)
        if label == "run" and "--nocapture" not in script:
            # Make skip notices observable so a family that silently SKIPs
            # (missing env var) cannot masquerade as a green proof.
            script = re.sub(
                r"(cargo test[^\n]*)$", r"\1 -- --nocapture", script, count=1, flags=re.M
            )
        print(f"==== {family} {label} ====")
        print(script)
        proc = subprocess.run(
            ["bash", "-c", script],
            cwd=v5w,
            env=env,
            text=True,
            capture_output=True,
        )
        if not quiet:
            sys.stdout.write(proc.stdout)
            sys.stderr.write(proc.stderr)
        if proc.returncode != 0:
            rec.update(
                status=f"{label}_failed",
                cause=tail(proc.stdout + proc.stderr),
                exit=proc.returncode,
            )
            return rec
        if label == "run":
            skipped = detect_skip(proc.stdout + proc.stderr)
            if skipped:
                rec.update(
                    status="skipped",
                    cause=(
                        "its oracle env var never reached the cargo run — the "
                        f"recipe is not self-contained: {skipped}"
                    ),
                    exit=3,
                )
                return rec
    return rec


def tail(text: str, lines: int = 12) -> str:
    kept = [l for l in text.splitlines() if l.strip()][-lines:]
    return "\n".join(kept)


def cmd_run(v5w: Path, family: str, force: bool) -> int:
    rec = run_family(v5w, family, force)
    if rec["status"] == "ok":
        print(f"OK: {family} recipe ran end-to-end")
        return 0
    print(f"FAILED [{rec['status']}]: {family}: {rec['cause']}", file=sys.stderr)
    return rec["exit"]


def cmd_run_all(
    v5w: Path,
    families: list[str] | None,
    exclude: set[str],
    results_path: str | None,
    force: bool,
    label: str | None,
) -> int:
    """The durable batch (F7). Writes the results artifact after EVERY family,
    so a batch that dies mid-run still leaves its classification behind — the
    exact failure that cost P4.D32 and P4.D42 their per-family numbers."""
    if families is None:
        families = [
            fam
            for fam, _ in family_files(v5w)
            if fam not in EXEMPT_FAMILIES
            and classify_one(v5w, fam) not in ("committed_corpus", "no_oracle")
        ]
    families = [f for f in families if f not in exclude]
    out = Path(results_path) if results_path else None
    if out:
        out.parent.mkdir(parents=True, exist_ok=True)
    report = {
        "label": label,
        "v5w": str(v5w),
        "venue_is_worktree": venue_is_worktree(v5w),
        "requested": families,
        "excluded": sorted(exclude),
        "families": [],
    }

    def flush() -> None:
        if out:
            out.write_text(json.dumps(report, indent=2) + "\n")

    flush()
    for i, family in enumerate(families, 1):
        print(f"\n######## [{i}/{len(families)}] {family} ########", flush=True)
        rec = run_family(v5w, family, force)
        report["families"].append(rec)
        counts: dict[str, int] = {}
        for row in report["families"]:
            counts[row["status"]] = counts.get(row["status"], 0) + 1
        report["totals"] = counts
        flush()
        print(f"---- {family}: {rec['status']}", flush=True)
    print(f"\ntotals: {report.get('totals', {})}")
    if out:
        print(f"wrote {out}")
    return 0 if all(r["status"] == "ok" for r in report["families"]) else 1


def classify_one(v5w: Path, family: str) -> str:
    r = extract(v5w, family, find_family(v5w, family))
    if "committed_corpus" in r.notes:
        return "committed_corpus"
    if "no_oracle" in r.notes:
        return "no_oracle"
    return "runnable"


def find_family(v5w: Path, family: str) -> Path:
    for fam, path in family_files(v5w):
        if fam == family:
            return path
    sys.exit(f"unknown family: {family}")


SELF_TEST_PROSE = [
    # F3's two real leak sites (tool_build_equivalence.rs, and
    # text_tool_loop_tier3_equivalence.rs), verbatim.
    "cargo run fine from the worktree.)",
    "touch is the preserve closure, a no-op here), so there is no fixture — just the",
    # The pre-existing PROSE_TRAP class, kept covered.
    "TZ=UTC is REQUIRED (the P4.d26 rule).",
    "N=... must point at Node 24.",
]
SELF_TEST_SHELL = [
    "N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5",
    "TMPO=/tmp/qt-oracle-w41g; mkdir -p $TMPO/cases $TMPO/fixtures",
    "cp $V5/harness/oracle/cases/tool-build.test.ts $TMPO/cases/",
    "cd ~/source/quilltap-server",
    "QT_ORACLE_OUT=/tmp/oracle-tool-build.ndjson \\",
    "$N/npx jest --silent --watchman=false --roots \"$PWD\" --roots \"$TMPO/cases\" -- x",
    "cargo test -p quilltap-harness --test tool_build_equivalence",
    "rm -f /tmp/oracle-x.ndjson",
    "touch /tmp/qt-marker",
    "for f in a b c; do echo $f; done",
    # F8: an elided command must survive classification, or `extract`'s own
    # `…` check never fires and anchored restoration is skipped.
    "… QT_ORACLE_OUT=/tmp/oracle-x.ndjson \\",
]


def cmd_self_test() -> int:
    failures: list[str] = []

    def check(cond: bool, msg: str) -> None:
        if not cond:
            failures.append(msg)

    for line in SELF_TEST_PROSE:
        check(is_prose(line) or not SHELL_START.match(line), f"prose leaked: {line!r}")
    for line in SELF_TEST_SHELL:
        check(bool(SHELL_START.match(line)), f"not recognized as shell: {line!r}")
        check(not is_prose(line), f"shell misread as prose: {line!r}")

    # F3 end-to-end: the two leak lines must not survive shell_lines().
    doc = [
        " Generate the oracle:",
        " cargo run fine from the worktree.)",
        "   N=~/.nvm/versions/node/v24.13.1/bin",
        "   cd ~/source/quilltap-server",
    ]
    check(
        shell_lines(doc)
        == ["N=~/.nvm/versions/node/v24.13.1/bin", "cd ~/source/quilltap-server"],
        f"shell_lines leaked prose: {shell_lines(doc)}",
    )

    # F8 end-to-end: the elision marker reaches the extracted script, so the
    # `…` check in extract() can trigger anchored restoration.
    elided = [" Generate the oracle (see the .ts header):", "   … npx jest -- foo"]
    check(ELISION in "\n".join(shell_lines(elided)), "F8: elision dropped as prose")

    # F5: the SKIP detector must fire on the harness's notice and NOT on an env
    # var name that merely ends in _SKIP (the `salon_skip` false positive).
    check(detect_skip("SKIP: set QT_ORACLE_X (see test header).") is not None, "F5 miss")
    check(detect_skip("   SKIP canonicalize spot-check: set X.") is not None, "F5 miss2")
    check(
        detect_skip("QT_ORACLE_SALON_SKIP=/tmp/oracle-salon-skip.ndjson \\") is None,
        "F5 false positive on QT_ORACLE_SALON_SKIP",
    )
    check(
        detect_skip("test salon_skip_matches_oracle ... ok\ntest result: ok. 1 passed")
        is None,
        "F5 false positive on a green cargo run",
    )

    # F6: a /tmp v4 pin path is a problem class; a normal /tmp path is not.
    check(
        bool(STALE_V4_PIN.search("cd /private/tmp/qt-v4-pin-b8b12695")),
        "F6 miss (private/tmp)",
    )
    check(bool(STALE_V4_PIN.search("V4=/tmp/qt-v4-pin-616930db")), "F6 miss (tmp)")
    check(not STALE_V4_PIN.search("/tmp/qt-oracle-w41g"), "F6 false positive")

    # Policy 2: the per-family scratch suffix must survive a `;`-tailed
    # assignment (the form every staged-mirror header actually uses).
    norm = normalize(["TMPO=/tmp/qt-oracle-w41g; mkdir -p $TMPO/cases"], Path("/v5"), "fam")
    check("TMPO=/tmp/qt-oracle-w41g-fam;" in norm, f"policy-2 suffix missed: {norm}")

    # F2: a jest root under the v5 checkout is flagged; the v4 checkout is not.
    r = Recipe("x", Path("x"))
    scan_venue(r, 'npx jest --roots "$PWD" --roots "$V5W/harness/oracle/cases" -- x')
    check(
        any(w.startswith("unstaged_jest_roots") for w in r.warnings),
        f"F2 miss: {r.warnings}",
    )
    r2 = Recipe("x", Path("x"))
    r2.tmp_writes.add("/tmp/qt-oracle-fam")
    scan_venue(r2, 'npx jest --roots "$PWD" --roots "/tmp/qt-oracle-fam/cases" -- x')
    check(
        not any(w.startswith("unstaged_jest_roots") for w in r2.warnings),
        f"F2 false positive: {r2.warnings}",
    )
    check(
        not any(w.startswith("external_tmp_input") for w in r2.warnings),
        f"tier-2 tmp false positive: {r2.warnings}",
    )
    r3 = Recipe("x", Path("x"))
    scan_venue(r3, "cp /tmp/qt-someone-elses-stage/x.db /tmp/mine.db")
    check(
        any("external_tmp_input /tmp/qt-someone-elses-stage" in w for w in r3.warnings),
        f"tier-2 tmp miss: {r3.warnings}",
    )
    # An assignment is not a creation: the compression_cache_tier3 shape.
    r4 = Recipe("x", Path("x"))
    scan_venue(r4, 'TMPO=/tmp/qt-oracle-run\nnpx jest --roots "$TMPO/cases" -- x')
    check(
        any("external_tmp_input /tmp/qt-oracle-run" in w for w in r4.warnings),
        f"assignment wrongly counted as a producer: {r4.warnings}",
    )
    r5 = Recipe("x", Path("x"))
    scan_venue(r5, 'TMPO=/tmp/qt-ok\nmkdir -p "$TMPO/cases"\nnpx jest --roots "$TMPO/cases" -- x')
    check(
        not any(w.startswith("external_tmp_input") for w in r5.warnings),
        f"mkdir producer missed: {r5.warnings}",
    )

    for f in failures:
        print(f"SELF-TEST FAIL: {f}", file=sys.stderr)
    print(f"self-test: {len(failures)} failure(s)")
    return 1 if failures else 0


def main() -> int:
    ap = argparse.ArgumentParser(
        description="The harness recipe sweep driver (see the module docstring)."
    )
    ap.add_argument("--v5w", type=Path, default=Path(__file__).resolve().parents[2])
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--json")
    ap.add_argument("--show")
    ap.add_argument("--run")
    ap.add_argument("--run-all", action="store_true")
    ap.add_argument("--families", help="comma-separated family list for --run-all")
    ap.add_argument("--exclude", default="", help="comma-separated families to skip")
    ap.add_argument("--results", help="path for the --run-all results artifact")
    ap.add_argument("--label", help="a note recorded in the results artifact")
    ap.add_argument(
        "--force",
        action="store_true",
        help="run even from a /.claude/ venue (see THE VENUE RULE)",
    )
    ap.add_argument("--collisions", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return cmd_self_test()
    if args.list:
        return cmd_list(args.v5w, args.json)
    if args.collisions:
        return cmd_collisions(args.v5w)
    if args.show:
        return cmd_show(args.v5w, args.show)
    if args.run:
        return cmd_run(args.v5w, args.run, args.force)
    if args.run_all:
        return cmd_run_all(
            args.v5w,
            [f for f in (args.families or "").split(",") if f] or None,
            {f for f in args.exclude.split(",") if f},
            args.results,
            args.force,
            args.label,
        )
    ap.error("pick one of --list / --show / --run / --run-all / --collisions / --self-test")


if __name__ == "__main__":
    sys.exit(main())
