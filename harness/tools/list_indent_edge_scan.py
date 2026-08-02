#!/usr/bin/env python3
"""Scan markdown for the P4.D40 (a)-edge shape, the one place v5's editor
deliberately disagrees with v4.

THE SHAPE. A list child indented DEEPER than its parent's indent but SHORT of
that parent's content column:

    1. a
      - b          <- 2 columns; `1. `'s content column is 3

v4's Lexical bridge resolves depth from structure alone (any deeper line opens
a level), so it reads that as a sub-list. v5 parses with markdown-it, which
implements CommonMark, where a child must reach the parent item's content
column to stay inside the item — so v5 reads two SIBLING lists and, on the next
save, writes the flattened form back. The nesting intent is lost permanently.

WHY IT IS NARROW. v4 cannot author these bytes itself: its export rule is
`width = parentWidth + max(step, markerWidth(parentMarker))`, and
`markerWidth('1.')` is 3, so a child of a numbered parent is always emitted at
3+ columns even at indent unit 2. Only hand-written, other-tool, or
model-written markdown can reach the shape.

WHY IT IS SCANNED RATHER THAN ARGUED. The ruling (2026-08-02, human) is to KEEP
v5's CommonMark behavior and let evidence decide whether that stands: if real
documents carry the shape, the destructive-on-save consequence makes adopting
v4's structural pre-pass (`normalizeListIndentForLexical`, deliberately NOT
ported) the right repair. This driver is that evidence, committed so the check
is reproducible instead of re-derived. See
`work-orders/p4.d40-editor-sublist-indentation.md` and the D17 gate's pinned
divergence block in `apps/web/src/app/editor/markdown-round-trip.spec.ts`.

USAGE

Disk-backed markdown (no pepper needed):

    find ~/qt-dogfood-friday -type f -name '*.md' -not -path '*/node_modules/*' \\
      -print0 | xargs -0 python3 harness/tools/list_indent_edge_scan.py

Store-backed documents (the ones that matter most — needs the real pepper, so
this half belongs to a human dogfood pass): export each store's documents to a
directory with the `quilltap` CLI, then scan that directory the same way.

Exit status is 0 whether or not hits are found — this reports, it does not gate.
"""

from __future__ import annotations

import pathlib
import re
import sys

# A list-item line. Whitespace before the content is required so `---` (a
# thematic break) and `*emphasis*` are not mistaken for markers — v4's own
# LIST_ITEM_RE makes the same demand for the same reason.
LIST_ITEM = re.compile(r"^(\s*)([-*+]|\d{1,9}[.)])([ \t]+\S.*)?$")
FENCE = re.compile(r"^\s*(`{3,}|~{3,})")


def content_column(indent: str, marker: str) -> int:
    """The column a child must reach to continue this item (CommonMark)."""
    return len(indent) + len(marker) + 1


def scan(path: pathlib.Path) -> list[tuple[int, str, str]]:
    try:
        lines = path.read_text(errors="replace").split("\n")
    except OSError:
        return []

    hits: list[tuple[int, str, str]] = []
    stack: list[tuple[int, int, str]] = []  # (indent width, content column, marker)
    in_fence = False

    for number, line in enumerate(lines, 1):
        if FENCE.match(line):
            in_fence = not in_fence
            continue
        if in_fence:
            continue

        match = LIST_ITEM.match(line)
        if not match:
            # A non-blank line at column 0 closes every open list level.
            if line.strip() and not line.startswith((" ", "\t")):
                stack = []
            continue

        indent = match.group(1).expandtabs(4)
        marker = match.group(2)
        width = len(indent)

        for parent_width, parent_content_col, parent_marker in reversed(stack):
            if parent_width < width < parent_content_col:
                hits.append((number, parent_marker, line.rstrip()))
                break
            if width > parent_width:
                break

        stack = [level for level in stack if level[0] < width]
        stack.append((width, content_column(indent, marker), marker))

    return hits


def main(argv: list[str]) -> int:
    paths = [pathlib.Path(a) for a in argv[1:]]
    if not paths:
        print(__doc__)
        return 0

    total = 0
    for path in paths:
        for number, parent_marker, text in scan(path):
            total += 1
            print(f"  {path}:{number}  under {parent_marker!r}  ->  {text!r}")

    print(f"scanned {len(paths)} file(s); {total} risky pair(s)")
    if total:
        print(
            "\nHits mean the divergence is reachable on real documents. Re-read the "
            "ruling in work-orders/p4.d40-editor-sublist-indentation.md before acting."
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
