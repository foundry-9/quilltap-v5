#!/usr/bin/env python3
"""P4.158 item 2 (ruling R-B): derive `restore-archive-two-claimants.zip` from
`restore-archive.zip`.

The restore's PRESERVE arm (P4.147, dogfood #141) keeps an entity on the store
its archived pointer names. Nothing stopped TWO entities from naming the same
archived store — every committed archive gives each entity its own — so both
would be preserved onto it, cross-linking unrelated content (the very thing
v4's create refuses by always minting fresh: `characters.repository.ts:253`).

So this derivation applies ONE mutation — a SECOND claimant on an archived
store, on each kind of preserve arm:

  - Riya's `characterDocumentMountPointId` is pointed at LORIAN's vault
    `aff3114e-…` (the character arm, phase 6 — Lorian is first in the
    archive's row order, so she keeps it);
  - the group's `officialMountPointId` is pointed at the PROJECT's store
    `5c17e916-…` (the store-backed arm — the project restores at 13, before
    the group at 13a, so it keeps it).

The two stores the second claimants used to own stay in the archive, now
unreferenced by any entity pointer (so they restore at 22a beside the fresh
stores the second claimants take — on BOTH sides, since v4 always mints).

Ruling R-B: first claim wins (phase order, then archive row order); a second
claimant takes the v4-convergent fresh-store arm with a v5-only WARN.

Every other byte is kept (the `derive-restore-archive-bag-nulls.py`
precedent); the manifest's counts do not move. `restore-archive.zip` is
md5-checked unchanged before and after.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-two-claimants.py
"""
import hashlib
import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
ARCHIVES = ROOT / "crates/quilltap-web/tests/fixtures/restore-archives"
SRC = ARCHIVES / "restore-archive.zip"
DST = ARCHIVES / "restore-archive-two-claimants.zip"
SRC_MD5 = "6b076003bacf8db22c1bf8440fac09d6"

LORIAN = "a1000000-0000-4000-8000-000000000001"
RIYA = "a1000000-0000-4000-8000-000000000002"
LORIAN_VAULT = "aff3114e-ed90-4d5b-99c1-ef3fa20203fc"
RIYA_VAULT = "f6b22d51-85e5-4bee-a053-262dd965962f"
PROJECT_STORE = "5c17e916-5f79-4cca-a134-ec09c05924e9"
GROUP_STORE = "60a8194d-f8ea-4540-af77-5e13e7ed0e9b"


def md5(path):
    return hashlib.md5(path.read_bytes()).hexdigest()


def load(zin, root, name):
    return json.loads(zin.read(f"{root}/data/{name}.json"))


def plan(zin, root):
    out = {}
    characters = load(zin, root, "characters")
    assert [c["id"] for c in characters] == [LORIAN, RIYA], "Lorian must be first"
    assert characters[0]["characterDocumentMountPointId"] == LORIAN_VAULT
    assert characters[1]["characterDocumentMountPointId"] == RIYA_VAULT
    characters[1]["characterDocumentMountPointId"] = LORIAN_VAULT
    out["characters"] = characters

    projects = load(zin, root, "projects")
    assert [p["officialMountPointId"] for p in projects] == [PROJECT_STORE]
    groups = load(zin, root, "groups")
    assert [g["officialMountPointId"] for g in groups] == [GROUP_STORE]
    groups[0]["officialMountPointId"] = PROJECT_STORE
    out["groups"] = groups
    return out


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
