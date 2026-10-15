#!/usr/bin/env python3
"""P4.D293 (Tier 1 items 7 + 12): derive `restore-archive-cursors.zip` from
`restore-archive.zip` — v4 `ac6348c5f` + `64743ee13`'s bundled extraction
cursors through the restore (taken off the chat, written to
`memory_extraction_cursors` after the create, before the messages), and
`c2795733f`'s memory `supersededAt` carried verbatim (L-6).

Mutations:

- chat `c1…01` carries all three bundle keys, appended after its other keys
  (where a pin backup writes them, before `messages`): OTHER → its message
  `d1…01`, EPISODE → `d1…02`, TURN → `d1…ff`, a message NOT in the archive
  (still written — no message-id check).
- chat `c1…02` carries OTHER `""` and EPISODE `null` (blank values are not
  cursors) and no TURN key.
- NEW chat `c1…03` ("The Old Watermark") carries ONLY the OTHER key, in the
  old chat-column slot after `terminalMode` — the baseline-vintage shape every
  committed archive with a watermark has — naming `d1…01`.
- memories: `ad…01` superseded by `ad…02` WITH `supersededAt`; `ad…02`
  carries `supersededAt` beside a null `supersededById` (kept verbatim — v4's
  restore never stamps or clears it); NEW `ad…06` superseded by `ad…02`
  WITHOUT `supersededAt` (restores null — a pre-`c2795733f` row).

The source archive is md5-checked unchanged.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-cursors.py
"""
import copy

from restore_archive_derive import derive, load

C = lambda n: f"c1000000-0000-4000-8000-00000000000{n}"
D = lambda n: f"d1000000-0000-4000-8000-0000000000{n}"
M = lambda n: f"ad000000-0000-4000-8000-00000000000{n}"


def insert_before(row, before, key, value):
    out = {}
    for k, v in row.items():
        if k == before:
            out[key] = value
        out[k] = v
    if key not in out:
        out[key] = value
    return out


def insert_after(row, after, key, value):
    out = {}
    for k, v in row.items():
        out[k] = v
        if k == after:
            out[key] = value
    assert key in out
    return out


def plan(zin, root):
    chats = load(zin, root, "chats")
    by_id = {c["id"]: i for i, c in enumerate(chats)}
    one = chats[by_id[C(1)]]
    assert [m["id"] for m in one["messages"]] == [D("01"), D("02")]
    for key, value in [
        ("otherExtractionWatermarkMessageId", D("01")),
        ("episodeExtractionWatermarkMessageId", D("02")),
        ("turnExtractionWatermarkMessageId", D("ff")),
    ]:
        one = insert_before(one, "messages", key, value)
    chats[by_id[C(1)]] = one
    two = chats[by_id[C(2)]]
    two = insert_before(two, "messages", "otherExtractionWatermarkMessageId", "")
    two = insert_before(two, "messages", "episodeExtractionWatermarkMessageId", None)
    chats[by_id[C(2)]] = two
    three = copy.deepcopy(chats[by_id[C(2)]])
    for k in ("otherExtractionWatermarkMessageId", "episodeExtractionWatermarkMessageId"):
        three.pop(k)
    three["id"] = C(3)
    three["title"] = "The Old Watermark"
    three["participants"] = []
    three = insert_after(three, "terminalMode", "otherExtractionWatermarkMessageId", D("01"))
    chats.append(three)

    memories = load(zin, root, "memories")
    mi = {m["id"]: i for i, m in enumerate(memories)}
    a = memories[mi[M(1)]]
    a = insert_after(a, "reinforcedImportance", "supersededById", M(2))
    a = insert_after(a, "supersededById", "supersededAt", "2026-03-05T00:00:00.000Z")
    memories[mi[M(1)]] = a
    b = memories[mi[M(2)]]
    b = insert_after(b, "reinforcedImportance", "supersededById", None)
    b = insert_after(b, "supersededById", "supersededAt", "2026-03-06T00:00:00.000Z")
    memories[mi[M(2)]] = b
    c = copy.deepcopy(memories[mi[M(1)]])
    c["id"] = M(6)
    c["content"] = "Lorian remembers an older road."
    c.pop("supersededAt")
    memories.append(c)
    return {"chats": chats, "memories": memories}


if __name__ == "__main__":
    derive("restore-archive-cursors.zip", plan)
