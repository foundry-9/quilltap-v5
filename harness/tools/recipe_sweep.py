#!/usr/bin/env python3
"""The harness recipe sweep driver (P4.27, rebuilt from P4.D32's lane recipe).

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
  --collisions    report /tmp paths written by more than one family's regen
                  (P4.D32 sweep hazard 2 — cross-family clobbering).

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
    r"[A-Za-z_][A-Za-z0-9_]*=\S"  # VAR=value (env prefix or assignment)
    r"|cd\s|cp\s|mv\s|rm\s|mkdir\s|ln\s|export\s|for\s|do\s|done\b|touch\s"
    r"|\$N/|\$\{?N\}?/|npx\s|node\s|cargo\s|python3?\s|bash\s|sh\s|diff\s"
    r"|brctl\s|sqlite3\s|tar\s|unzip\s|curl\s"
    r")"
)
# Prose traps that would otherwise match SHELL_START ("TZ=UTC is REQUIRED …").
PROSE_TRAP = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=\S+\s+(?:is|are|was|were|means)\b")

PLACEHOLDER_WORKTREE = re.compile(
    r"<[^<>]*(?:worktree|checkout|repo-root|this tree|v5)[^<>]*>", re.IGNORECASE
)
ANY_PLACEHOLDER = re.compile(r"<[^<>=\s][^<>]*>")
ELISION = "…"

CASE_REF = re.compile(r"(?:cases/)?([A-Za-z0-9][A-Za-z0-9_.-]*?\.(?:test\.)?tsx?)\b")
JEST_ELIDED = re.compile(r"npx jest[^\n]*?--\s+([A-Za-z0-9_.\\$-]+)")


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
        looks_shell = bool(SHELL_START.match(stripped)) and not PROSE_TRAP.match(
            stripped
        )
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
    # Scan writes over placeholder-normalized text so `<V5W>`-style brackets
    # can't be misread as redirects.
    scan_writes(r, PLACEHOLDER_WORKTREE.sub("$V5W", joined))
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
    # Policy 2: per-family scratch dirs for restored jest mirrors.
    for var in ("TMPO", "STAGE"):
        text = re.sub(
            rf"^{var}=(/tmp/\S+?)/?$",
            rf"{var}=\1-{family}",
            text,
            flags=re.M,
        )
    return "set -euo pipefail\n" + text


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
                "notes": r.notes,
                "tmp_writes": sorted(r.tmp_writes),
                "repo_writes": r.repo_writes,
            }
        )
    return rows


def cmd_list(v5w: Path, json_out: str | None) -> int:
    rows = classify(v5w)
    counts: dict[str, int] = {}
    for row in rows:
        counts[row["status"]] = counts.get(row["status"], 0) + 1
        if row["status"] in ("non_extractable", "no_recipe"):
            print(f"{row['family']}: {', '.join(row.get('problems', []))}")
    print(f"\ntotals: {counts}")
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


def cmd_run(v5w: Path, family: str) -> int:
    path = find_family(v5w, family)
    r = extract(v5w, family, path)
    if r.repo_writes:
        print(
            f"REFUSED: {family}'s recipe writes into the repo (policy 1):",
            file=sys.stderr,
        )
        for line in r.repo_writes:
            print(f"  {line}", file=sys.stderr)
        return 2
    if r.problems:
        print(f"REFUSED: {family} is non-extractable: {r.problems}", file=sys.stderr)
        return 2
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
        sys.stdout.write(proc.stdout)
        sys.stderr.write(proc.stderr)
        if proc.returncode != 0:
            print(f"FAILED: {family} {label} (exit {proc.returncode})", file=sys.stderr)
            return proc.returncode
        if label == "run" and re.search(
            r"\bSKIP\b|skipping .*differential|not set — skipping",
            proc.stdout + proc.stderr,
        ):
            print(
                f"FAILED: {family} run SKIPPED (its oracle env var never reached "
                "the cargo run — the recipe is not self-contained)",
                file=sys.stderr,
            )
            return 3
    print(f"OK: {family} recipe ran end-to-end")
    return 0


def find_family(v5w: Path, family: str) -> Path:
    for fam, path in family_files(v5w):
        if fam == family:
            return path
    sys.exit(f"unknown family: {family}")


def main() -> int:
    ap = argparse.ArgumentParser(
        description="The harness recipe sweep driver (see the module docstring)."
    )
    ap.add_argument("--v5w", type=Path, default=Path(__file__).resolve().parents[2])
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--json")
    ap.add_argument("--show")
    ap.add_argument("--run")
    ap.add_argument("--collisions", action="store_true")
    args = ap.parse_args()
    if args.list:
        return cmd_list(args.v5w, args.json)
    if args.collisions:
        return cmd_collisions(args.v5w)
    if args.show:
        return cmd_show(args.v5w, args.show)
    if args.run:
        return cmd_run(args.v5w, args.run)
    ap.error("pick one of --list / --show / --run / --collisions")


if __name__ == "__main__":
    sys.exit(main())
