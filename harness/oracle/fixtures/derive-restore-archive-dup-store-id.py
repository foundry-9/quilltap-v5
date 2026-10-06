#!/usr/bin/env python3
"""P4.158 item 2 (ruling R-B): derive `restore-archive-dup-store-id.zip` from
`restore-archive.zip`.

The restore's archived-store map (P4.147) is keyed by the archive's RAW
`doc_mount_points` rows; 22a then creates those rows in order. With a
DUPLICATED id the two disagreed: the map was a `collect()` (LAST row wins)
while 22a's primary key keeps the FIRST row and refuses the second. No
committed archive carries a duplicate id, so the disagreement was invisible.

So this derivation applies ONE mutation: it APPENDS a second
`doc_mount_points` row carrying Lorian's vault id `aff3114e-…` but
`storeType: "documents"` and its own name. 22a restores the FIRST (the real
`character` vault) on both sides and refuses the duplicate with SQLite's
`UNIQUE constraint failed: doc_mount_points.id`; before R-B v5's map read the
duplicate's `documents` and dropped Lorian off the preserve arm onto a fresh
vault, though the vault 22a restores is hers.

The manifest's `docMountPoints` count moves by one. Every other byte is kept
(the `derive-restore-archive-bag-nulls.py` precedent). `restore-archive.zip`
is md5-checked unchanged before and after.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-dup-store-id.py
"""
import hashlib
import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
ARCHIVES = ROOT / "crates/quilltap-web/tests/fixtures/restore-archives"
SRC = ARCHIVES / "restore-archive.zip"
DST = ARCHIVES / "restore-archive-dup-store-id.zip"
SRC_MD5 = "6b076003bacf8db22c1bf8440fac09d6"

LORIAN_VAULT = "aff3114e-ed90-4d5b-99c1-ef3fa20203fc"


def md5(path):
    return hashlib.md5(path.read_bytes()).hexdigest()


def load(zin, root, name):
    return json.loads(zin.read(f"{root}/data/{name}.json"))


def plan(zin, root):
    points = load(zin, root, "doc-mount-points")
    first = [p for p in points if p["id"] == LORIAN_VAULT]
    assert len(first) == 1 and first[0]["storeType"] == "character"
    dup = dict(first[0])
    dup["name"] = "Lorian Character Vault (duplicated id)"
    dup["storeType"] = "documents"
    points.append(dup)
    return {"doc-mount-points": points}


def main():
    before = md5(SRC)
    assert before == SRC_MD5, f"restore-archive.zip moved: {before}"
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
                elif rel == "manifest.json":
                    m = json.loads(data)
                    m["counts"]["docMountPoints"] = len(rewritten["doc-mount-points"])
                    data = (json.dumps(m, indent=2) + "\n").encode()
                out = zipfile.ZipInfo(info.filename, date_time=info.date_time)
                out.external_attr = info.external_attr
                out.compress_type = zipfile.ZIP_DEFLATED
                zout.writestr(out, data)
    after = md5(SRC)
    assert after == SRC_MD5, f"restore-archive.zip moved during the derive: {after}"
    print(f"wrote {DST.relative_to(ROOT)} (restore-archive.zip md5 {after} unchanged)")


if __name__ == "__main__":
    main()
