#!/usr/bin/env python3
"""P4.D273 (R-K): derive `restore-archive-store-row-fails.zip` from
`restore-archive.zip` — v4 `ed8b15b50`'s 22a-i (bug 185): an entity BOUND to
its archived store whose store ROW then fails to restore at 22a.

Mutations:

- the `name` of Lorian's vault, the project's store and the group's store set
  to `null`. 22a's free-name pick calls `desired.trim()` on it, so each row
  is refused with V8's `Cannot read properties of null (reading 'trim')` —
  the same refusal on both engines (P4.D273 R-H). The three entities were
  already bound (6 / 13 / 13a), so 22a-i gives each a FRESH store populated
  from its backup row, with v4's warning.
- Riya ARCHIVED (`archivedAt`, the `restore-archive-wardrobe-picture-
  tombstone.zip` value) and her vault's `name` nulled too: a bound tombstone
  is never re-provisioned (`The archived character "Riya" could not be
  restored readable, because its vault could not be restored`).

No counts move. `restore-archive.zip` is md5-checked unchanged before and
after.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-store-row-fails.py
"""
import hashlib
import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
ARCHIVES = ROOT / "crates/quilltap-web/tests/fixtures/restore-archives"
SRC = ARCHIVES / "restore-archive.zip"
DST = ARCHIVES / "restore-archive-store-row-fails.zip"
SRC_MD5 = "6b076003bacf8db22c1bf8440fac09d6"

NULLED = {
    "aff3114e-ed90-4d5b-99c1-ef3fa20203fc",  # Lorian's vault
    "f6b22d51-85e5-4bee-a053-262dd965962f",  # Riya's vault
    "5c17e916-5f79-4cca-a134-ec09c05924e9",  # the project's store
    "60a8194d-f8ea-4540-af77-5e13e7ed0e9b",  # the group's store
}
RIYA = "a1000000-0000-4000-8000-000000000002"


def md5(path):
    return hashlib.md5(path.read_bytes()).hexdigest()


def load(zin, root, name):
    return json.loads(zin.read(f"{root}/data/{name}.json"))


def plan(zin, root):
    points = load(zin, root, "doc-mount-points")
    hit = 0
    for p in points:
        if p["id"] in NULLED:
            p["name"] = None
            hit += 1
    assert hit == len(NULLED)
    characters = load(zin, root, "characters")
    riya = [c for c in characters if c["id"] == RIYA]
    assert len(riya) == 1
    riya[0]["archivedAt"] = "2026-03-06T00:00:00.000Z"
    return {"doc-mount-points": points, "characters": characters}


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
                out = zipfile.ZipInfo(info.filename, date_time=info.date_time)
                out.external_attr = info.external_attr
                out.compress_type = zipfile.ZIP_DEFLATED
                zout.writestr(out, data)
    after = md5(SRC)
    assert after == SRC_MD5, f"restore-archive.zip moved during the derive: {after}"
    print(f"wrote {DST.relative_to(ROOT)} (restore-archive.zip md5 {after} unchanged)")


if __name__ == "__main__":
    main()
