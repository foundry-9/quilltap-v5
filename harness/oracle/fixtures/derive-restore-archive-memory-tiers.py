#!/usr/bin/env python3
"""P4.D273 (R-K): derive `restore-archive-memory-tiers.zip` from
`restore-archive-memory-graph.zip` — v4 `d58548051`'s memory tiers through
the restore, in miniature (the dogfood Friday copy's 365 `CONSOLIDATED`
digests the `f5e953a3f` baseline refused).

Mutations (memory ids are the source archive's):

- `ae…02` (Lorian's) becomes a DIGEST: `source: 'CONSOLIDATED'`, `tier:
  'hot'`, `consolidatedFrom: [ae…01, ad…01]`, `consolidatedAt`.
- its members `ae…01` and `ad…01` go `tier: 'cold'` with `supersededById`
  naming the digest — both listed BEFORE it, so a `new-account` remap mints
  the digest's id at a member's `supersededById` and the digest row reuses it.
- `ad…02` carries NO tier key (a pre-tier memory): v4's create prefill gives
  it `tier: 'hot'`, `consolidatedFrom: []`.
- `ae…03` a modern hot row (all four keys, nulls where they apply).
- `ae…04` `tier: 'warm'` — refused by v4's `MemoryTierEnum` (Tier 2 item 16:
  the refusal path through `parse_create_memory` on the restore).
- chat `c1…01`'s `otherExtractionWatermarkMessageId` names its own message
  `d1…02`.

No counts move. The source archive is md5-checked unchanged before and after.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-memory-tiers.py
"""
import hashlib
import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
ARCHIVES = ROOT / "crates/quilltap-web/tests/fixtures/restore-archives"
SRC = ARCHIVES / "restore-archive-memory-graph.zip"
DST = ARCHIVES / "restore-archive-memory-tiers.zip"
SRC_MD5 = "f2f6086d972c69f6d6935a3239eefe84"

DIGEST = "ae000000-0000-4000-8000-000000000002"
M1 = "ae000000-0000-4000-8000-000000000001"
M2 = "ad000000-0000-4000-8000-000000000001"
PRE_TIER = "ad000000-0000-4000-8000-000000000002"
HOT = "ae000000-0000-4000-8000-000000000003"
WARM = "ae000000-0000-4000-8000-000000000004"
CHAT = "c1000000-0000-4000-8000-000000000001"
WATERMARK = "d1000000-0000-4000-8000-000000000002"
CONSOLIDATED_AT = "2026-07-30T12:00:00.000Z"


def md5(path):
    return hashlib.md5(path.read_bytes()).hexdigest()


def load(zin, root, name):
    return json.loads(zin.read(f"{root}/data/{name}.json"))


def tiered(m, tier, superseded_by, consolidated_from, consolidated_at):
    """The four tier keys, after `reinforcedImportance` (MemorySchema order)."""
    out = {}
    for k, v in m.items():
        out[k] = v
        if k == "reinforcedImportance":
            out["tier"] = tier
            out["supersededById"] = superseded_by
            out["consolidatedFrom"] = consolidated_from
            out["consolidatedAt"] = consolidated_at
    assert "tier" in out
    return out


def plan(zin, root):
    memories = load(zin, root, "memories")
    by_id = {m["id"]: i for i, m in enumerate(memories)}
    assert by_id[M1] < by_id[DIGEST] and by_id[M2] < by_id[DIGEST]
    for i, m in enumerate(memories):
        if m["id"] == DIGEST:
            m["source"] = "CONSOLIDATED"
            memories[i] = tiered(m, "hot", None, [M1, M2], CONSOLIDATED_AT)
        elif m["id"] in (M1, M2):
            memories[i] = tiered(m, "cold", DIGEST, [], None)
        elif m["id"] == HOT:
            memories[i] = tiered(m, "hot", None, [], None)
        elif m["id"] == WARM:
            memories[i] = tiered(m, "warm", None, [], None)
        else:
            assert m["id"] == PRE_TIER and "tier" not in m
    chats = load(zin, root, "chats")
    chat = [c for c in chats if c["id"] == CHAT]
    assert len(chat) == 1 and any(msg["id"] == WATERMARK for msg in chat[0]["messages"])
    chat[0]["otherExtractionWatermarkMessageId"] = WATERMARK
    return {"memories": memories, "chats": chats}


def main():
    before = md5(SRC)
    assert before == SRC_MD5, f"restore-archive-memory-graph.zip moved: {before}"
    with zipfile.ZipFile(SRC) as zin:
        root = zin.infolist()[0].filename.rstrip("/")
        rewritten = plan(zin, root)
        with zipfile.ZipFile(DST, "w", zipfile.ZIP_DEFLATED) as zout:
            for info in zin.infolist():
                rel = info.filename[len(root) + 1:]
                data = zin.read(info.filename)
                stem = rel[len("data/"):-len(".json")] if rel.startswith("data/") else None
                if stem in rewritten:
                    data = (json.dumps(rewritten[stem], indent=2, ensure_ascii=False) + "\n").encode()
                out = zipfile.ZipInfo(info.filename, date_time=info.date_time)
                out.external_attr = info.external_attr
                out.compress_type = zipfile.ZIP_DEFLATED
                zout.writestr(out, data)
    after = md5(SRC)
    assert after == SRC_MD5, f"restore-archive-memory-graph.zip moved during the derive: {after}"
    print(f"wrote {DST.relative_to(ROOT)} (restore-archive-memory-graph.zip md5 {after} unchanged)")


if __name__ == "__main__":
    main()
