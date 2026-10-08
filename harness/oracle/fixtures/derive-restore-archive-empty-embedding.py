#!/usr/bin/env python3
"""P4.D258 Tier 2 item 11 (v4 bug 181, `039f7017c`; ruling R-A): derive
`restore-archive-empty-embedding.zip` from `restore-archive.zip`.

The archive's two memories are kept byte for byte (both sound, no embedding
key); ONE more is appended on Lorian (`a1…0001`, restored at phase 6, so the
user-scoped character check passes):

  ad…21  embedding {}   (`JSON.stringify(new Float32Array(0))` — the empty
                         index-keyed object)

v4's `decodeIndexKeyedEmbedding` returns `{}` UNCHANGED (`index-keyed-
embedding.ts:17`), so `MemorySchema`'s embedding union refuses the memory
(`invalid_union`) and the restore warns `Failed to restore memory: …`; v5 used
to decode `{}` to `[]` and restore it with a NULL vector, and now converges.
Both sides land `…01` and `…02` alone: the summary's `memories` (rows WRITTEN
since `039f7017c`) is 2 against the manifest's 3.

The manifest's `counts.memories` moves 2 → 3. Every other entry and byte is
kept; `restore-archive.zip` is md5-checked unchanged before and after.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-empty-embedding.py
"""
import copy
import hashlib
import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
ARCHIVES = ROOT / "crates/quilltap-web/tests/fixtures/restore-archives"
SRC = ARCHIVES / "restore-archive.zip"
DST = ARCHIVES / "restore-archive-empty-embedding.zip"
SRC_MD5 = "6b076003bacf8db22c1bf8440fac09d6"

LORIAN = "a1000000-0000-4000-8000-000000000001"


def rewrite_memories(memories):
    assert len(memories) == 2, len(memories)
    template = copy.deepcopy(memories[0])
    assert template["characterId"] == LORIAN
    assert "embedding" not in template
    m = template
    m["id"] = "ad000000-0000-4000-8000-000000000021"
    m.pop("chatId", None)
    m["content"] = "An empty index-keyed embedding."
    m["summary"] = "empty embedding"
    m["embedding"] = {}
    return memories + [m]


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
                m["counts"]["memories"] = 3
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
