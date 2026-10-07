#!/usr/bin/env python3
"""P4.161 (dogfood #152 + ruling R-C): derive
`restore-archive-memory-refusals.zip` from `restore-archive.zip`.

The archive's two memories are kept byte for byte; ELEVEN more are appended on
Lorian (`a1…0001`, restored at phase 6, so the user-scoped character check
passes), one bad key each, so v4's `MemorySchema` (`memories.create` →
`_create` → `validate`) refuses each with its ZodError and the restore's
per-row catch warns `Failed to restore memory: <ZodError message>`:

  ad…03  importance 5 + kind "bogus-kind"  (the dogfood walk's C6 shape)
  ad…04  importance "0.5"                  (a string)
  ad…05  reinforcementCount 0              (min 1)
  ad…06  tags ["nope"]                     (not a uuid)
  ad…07  content ABSENT                    (required)
  ad…08  occurredAt "yesterday"            (not an ISO datetime)
  ad…09  embedding {"0": 0.25}             (JSON.stringify(Float32Array) — v4 REFUSES (no union
                                            option); v5 DECODES and lands it: the ruled
                                            INDEX_KEYED_EMBEDDING divergence, 2026-10-07)
  "not-a-uuid"  a bad `id`                 (the preserved id itself)

and THREE that land:

  ad…11  sound, every key set
  ad…12  the defaulted keys ABSENT (importance, source, kind,
         reinforcementCount, reinforcedImportance, keywords, tags, entities,
         relatedMemoryIds) — R-C: v4's schema defaults (0.5, MANUAL,
         semantic, 1, 0.5, []), where v5's restore used to fill 5.0 / AUTO /
         0 / 0
  ad…13  sound, with a number[] embedding

The manifest's `counts.memories` moves 2 → 13. Every other entry and byte is
kept; `restore-archive.zip` is md5-checked unchanged before and after.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-memory-refusals.py
"""
import copy
import hashlib
import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
ARCHIVES = ROOT / "crates/quilltap-web/tests/fixtures/restore-archives"
SRC = ARCHIVES / "restore-archive.zip"
DST = ARCHIVES / "restore-archive-memory-refusals.zip"
SRC_MD5 = "6b076003bacf8db22c1bf8440fac09d6"

LORIAN = "a1000000-0000-4000-8000-000000000001"
DEFAULTED = [
    "importance",
    "source",
    "kind",
    "reinforcementCount",
    "reinforcedImportance",
    "keywords",
    "tags",
    "entities",
    "relatedMemoryIds",
]


def rewrite_memories(memories):
    assert len(memories) == 2, len(memories)
    template = copy.deepcopy(memories[0])
    assert template["characterId"] == LORIAN

    def item(suffix, patch, drop=()):
        m = copy.deepcopy(template)
        m["id"] = f"ad000000-0000-4000-8000-0000000000{suffix}"
        m.pop("chatId", None)
        m["content"] = f"Refusal memory {suffix}."
        m["summary"] = f"refusal {suffix}"
        m.update(patch)
        for k in drop:
            m.pop(k, None)
        return m

    added = [
        item("03", {"importance": 5, "kind": "bogus-kind"}),
        item("04", {"importance": "0.5"}),
        item("05", {"reinforcementCount": 0}),
        item("06", {"tags": ["nope"]}),
        item("07", {}, ["content"]),
        item("08", {"occurredAt": "yesterday"}),
        item("09", {"embedding": {"0": 0.25}}),
        item("10", {"id": "not-a-uuid"}),
        item("11", {"witnessedContext": "manual", "occurredAt": "2026-03-02T00:00:00.000Z"}),
        item("12", {}, DEFAULTED),
        item("13", {"embedding": [0.25, -0.5]}),
    ]
    return memories + added


def main():
    before = hashlib.md5(SRC.read_bytes()).hexdigest()
    assert before == SRC_MD5, f"restore-archive.zip moved: {before}"
    with zipfile.ZipFile(SRC) as zin, zipfile.ZipFile(DST, "w", zipfile.ZIP_DEFLATED) as zout:
        for info in zin.infolist():
            data = zin.read(info.filename)
            if info.filename.endswith("/data/memories.json"):
                data = (json.dumps(rewrite_memories(json.loads(data)), indent=2) + "\n").encode()
            elif info.filename.endswith("/manifest.json"):
                m = json.loads(data)
                assert m["counts"]["memories"] == 2
                m["counts"]["memories"] = 13
                data = (json.dumps(m, indent=2) + "\n").encode()
            out = zipfile.ZipInfo(info.filename, date_time=info.date_time)
            out.external_attr = info.external_attr
            out.compress_type = zipfile.ZIP_DEFLATED
            zout.writestr(out, data)
    after = hashlib.md5(SRC.read_bytes()).hexdigest()
    assert after == SRC_MD5, f"restore-archive.zip changed: {after}"
    print(f"wrote {DST.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
