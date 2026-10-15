#!/usr/bin/env python3
"""P4.D293 (Tier 1 item 6, L-5): derive `restore-archive-legacy-vectors.zip`
from `restore-archive.zip` — v4 `af48f6cc2`'s legacy-vector ADOPTION through
the restore: an old archive's mirror rows (`data/vector-entries.json`) fill a
memory that arrived WITHOUT an embedding.

Mutations (memory ids under Lorian, `a1…01`, chat `c1…01`):

- `ad…01` (the source's, no embedding) — the mirror names it → ADOPTS.
- `ad…02` (the source's, no embedding) — no mirror row → stays null.
- NEW `ad…03` with its OWN embedding — the mirror names it → keeps its own.
- NEW `ad…04` whose embedding is `{}` (an un-decoded index-keyed object, "no
  embedding" to v4's check) — the mirror names it → ADOPTS.
- NEW `ad…05` (no embedding) — TWO mirror rows name it → the LAST adopts.
- the orphan `bb…01` row kept — it names no memory → creates nothing.

`legacyVectorsAdopted` = 3. The source archive is md5-checked unchanged.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-legacy-vectors.py
"""
import copy

from restore_archive_derive import derive, load

LORIAN = "a1000000-0000-4000-8000-000000000001"
M = lambda n: f"ad000000-0000-4000-8000-00000000000{n}"
V = lambda *xs: list(xs)


def plan(zin, root):
    memories = load(zin, root, "memories")
    base = next(m for m in memories if m["id"] == M(1))
    assert "embedding" not in base or base["embedding"] is None
    for n, content, embedding in [
        (3, "Lorian keeps his own vector.", V(0.25, 0.5, 0.75, 1.0)),
        (4, "Lorian's vector was written index-keyed.", {}),
        (5, "Lorian's mirror row appears twice.", None),
    ]:
        m = copy.deepcopy(base)
        m["id"] = M(n)
        m["content"] = content
        if embedding is not None:
            m["embedding"] = embedding
        memories.append(m)
    entries = load(zin, root, "vector-entries")
    assert [e["id"] for e in entries] == ["bb000000-0000-4000-8000-000000000001"]
    row = lambda mid, vec: {"id": mid, "characterId": LORIAN, "embedding": vec,
                            "createdAt": "2026-07-24T20:50:57.212Z"}
    entries = [
        row(M(1), V(0.5, 0.25, 0.125, 0.0625)),
        row(M(3), V(-1.0, -1.0, -1.0, -1.0)),
        row(M(4), V(0.0, 0.5, 0.0, 0.5)),
        row(M(5), V(9.0, 9.0, 9.0, 9.0)),
        entries[0],
        row(M(5), V(0.75, 0.5, 0.25, 0.125)),
    ]
    return {"memories": memories, "vector-entries": entries}


if __name__ == "__main__":
    derive("restore-archive-legacy-vectors.zip", plan)
