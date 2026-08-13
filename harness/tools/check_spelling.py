#!/usr/bin/env python3
"""Repo-wide guard against the "quilt"-based misspelling of "Quilltap".

The v5 analog of v4's `scripts/check-quilltap-spelling.mjs` (added there in
4.8.1, `85fd8744`): the spelling rule is standing in both repos, but v5 had no
mechanical enforcement at all — nothing here parses prose. This sweep scans
every tracked (and new-but-not-ignored) text file and fails on any
case-insensitive hit of the misspelling outside an explicit allowlist. It is
wired into `cargo test --workspace` via the `quilltap-harness`
`spelling_guard` test, the same way the recipe sweep's `--self-test` guards
its extractor.

Two escape hatches, both deliberate and greppable (v4's exact design):
 - ALLOWED_PATHS below — files that must quote the wrong spelling (the rule
   statements, this script), or frozen records that would be falsified by
   "correcting" them.
 - A line containing the marker `quilltap-spelling-exception` is skipped, for
   prose that needs to name the misspelling in passing.

Run standalone:  python3 harness/tools/check_spelling.py
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]

# v4 `quilltap-spelling.js` MISSPELLING — one pattern, shared reasoning: the
# misspelling doubles the t (quilt + tap) where the name doubles the l
# (quill + tap).
MISSPELLING = re.compile(r"quilttap(?!ap)", re.IGNORECASE)

# Marker that exempts a single line. Spelled correctly, so it can't self-trip.
LINE_EXCEPTION = "quilltap-spelling-exception"

# Paths that may contain the misspelling, with the reason each one earns it.
# Repo-relative and exact — a new file is covered by default, and adding one
# here should take an argument (v4's rule).
ALLOWED_PATHS = {
    # The enforcer has to spell the word to match it.
    "harness/tools/check_spelling.py": "this script",
    # Documents that state the spelling rule, and so must quote the wrong
    # spelling.
    "CLAUDE.md": "states the spelling rule",
    ".claude/commands/commit.md": "states the spelling rule",
    "docs/developer/porting/work-orders/p4.6g-characters-spa.md": "states the spelling rule",
    "docs/developer/porting/work-orders/p4.6v-mount-index-file-ops-server.md": "states the spelling rule",
    "docs/developer/porting/work-orders/p4.6w-document-mode-server.md": "states the spelling rule",
    "docs/developer/porting/work-orders/p4.6x-document-mode-spa.md": "states the spelling rule",
    "docs/developer/porting/work-orders/p4.6y-mount-file-ops-remainder.md": "states the spelling rule",
    # The work order that commissioned this checker quotes the misspelling to
    # describe what the checker must catch.
    "docs/developer/porting/work-orders/p4.d68-dbkey-onefile-seed-lock-drift.md": "names the misspelling the checker catches",
}

# Directories mirrored verbatim from v4 (reference content, not v5 prose).
# v4's own checker governs the originals; "correcting" a mirror would make it
# diverge from what it mirrors.
ALLOWED_PREFIXES = ("docs/v4/",)

# Extensions with no text worth scanning (v4's list, plus Rust build leavings).
BINARY_EXTENSIONS = {
    ".png", ".jpg", ".jpeg", ".gif", ".webp", ".ico", ".icns", ".bmp", ".avif",
    ".woff", ".woff2", ".ttf", ".otf", ".eot",
    ".zip", ".gz", ".tgz", ".bz2", ".xz", ".7z", ".tar",
    ".pdf", ".mp3", ".mp4", ".wav", ".m4a", ".mov", ".webm",
    ".node", ".wasm", ".dylib", ".so", ".dll", ".exe",
    ".db", ".sqlite", ".sqlite3", ".msgpack", ".pack", ".idx",
    ".rlib", ".rmeta", ".o", ".a", ".qtap",
}

# Files this large are generated or vendored; scanning them is not worth the
# read (v4's cap).
MAX_BYTES = 5 * 1024 * 1024


def tracked_files() -> list[str]:
    """Tracked files plus new files that aren't gitignored, so a brand-new doc
    is checked before it's ever staged (v4's `--cached --others
    --exclude-standard`)."""
    out = subprocess.run(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
        cwd=REPO_ROOT,
        capture_output=True,
        check=True,
    )
    return [p for p in out.stdout.decode("utf-8", "replace").split("\0") if p]


def scan(rel_path: str) -> list[tuple[int, int, str, str]]:
    abs_path = REPO_ROOT / rel_path
    try:
        if not abs_path.is_file() or abs_path.is_symlink():
            return []
        if abs_path.stat().st_size > MAX_BYTES:
            return []
        raw = abs_path.read_bytes()
    except OSError:
        return []  # raced with a delete, or a dangling symlink
    if b"\0" in raw:
        return []  # binary that dodged the extension list
    hits = []
    for index, line in enumerate(raw.decode("utf-8", "replace").split("\n")):
        if LINE_EXCEPTION in line:
            continue
        for match in MISSPELLING.finditer(line):
            hits.append((index + 1, match.start() + 1, line.strip(), match.group(0)))
    return hits


def main() -> int:
    failures = []
    for rel_path in tracked_files():
        if rel_path in ALLOWED_PATHS:
            continue
        if rel_path.startswith(ALLOWED_PREFIXES):
            continue
        if Path(rel_path).suffix.lower() in BINARY_EXTENSIONS:
            continue
        for line, col, text, found in scan(rel_path):
            failures.append((rel_path, line, col, text, found))

    if not failures:
        return 0

    print(f'\nMisspelled "Quilltap" found in {len(failures)} place(s):\n', file=sys.stderr)
    for rel_path, line, col, text, found in failures:
        print(f"  {rel_path}:{line}:{col}  {found}", file=sys.stderr)
        print(f"      {text[:140]}", file=sys.stderr)
    print(
        '\nThe project is "Quilltap" (quill + tap) — never the quilt-based spelling.\n'
        "Fix the text, or (with an argument) add the file to ALLOWED_PATHS in\n"
        f"harness/tools/check_spelling.py, or mark the one line with `{LINE_EXCEPTION}`.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
