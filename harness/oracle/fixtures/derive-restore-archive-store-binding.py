#!/usr/bin/env python3
"""P4.D273 (R-K): derive `restore-archive-store-binding.zip` from
`restore-archive.zip` — every arm of v4 `ed8b15b50`'s store claim map (bug
185) on one archive.

Mutations, each one arm:

- **untyped** — Lorian's vault row (`aff3114e-…`) loses `storeType`. An
  untyped store fits every kind, so Lorian still binds (the character arm).
- **not-carried** — Riya's `characterDocumentMountPointId` names an id the
  archive lacks (`dead0000-…0001`). She falls back to a fresh vault with v4's
  warning; her real vault (`f6b22d51-…`) restores unreferenced.
- **archived fallback** — Riya is also ARCHIVED (`archivedAt`, the
  `restore-archive-wardrobe-picture-tombstone.zip` value). On the fallback
  arm v4's create refuses an archived character's vault and v5 (ruled, R-F
  (b), "fix v5, keep archivedAt") gives her one — the one character keeps
  every other arm intact.
- **wrong-kind** — the group's `officialMountPointId` names RIYA's vault (a
  `character` store), which a group may not take.
- **bound project + the deferral** — the project stays bound and gains two
  `files` rows: a CARRIED one (`mount-blob:<its store>:bc000000-…0001`, a
  blob the archive carries — bytes planted under `files/` as a real backup
  writes them) and a LOOSE one (a legacy storage key, bytes planted under
  `files/`) that a bound project's restore defers until after its links.
- **carried metadata (P4.167, H-4)** — the CARRIED row's `mimeType` / `size`
  deliberately DISAGREE with its blob (`application/x-quilltap-chart`, the
  blob's size + 7): v4's carried arm spreads the ARCHIVE row and takes only
  the blob's `sha256` (`restore.ts:791-810` at `322a5488e`).
- **carried without bytes (P4.167, P-1)** — two more carried rows naming the
  SAME blob through other mounts' keys (Lorian's vault, Riya's), whose bytes
  are NOT planted under `files/`: v4's carried arm never reads the disk. The
  second also has NO `size` (measured: how the archive row's absent size
  decodes).
- **carried rows v4's `FileEntrySchema` refuses WHOLE (the H-4 §3 review)** —
  `chart-null-mime.bin` (`mimeType: null`) and `chart-bad-both.bin`
  (`mimeType: null` AND `size: "13"`): v4's `files.create` validates the
  whole entity, so each is skipped with ONE ZodError listing every bad field
  in schema order (`mimeType` before `size`); neither is written with a `""`
  stand-in.

The manifest's `files` count moves by six; every other byte is kept.
`restore-archive.zip` is md5-checked unchanged before and after.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-store-binding.py
"""
import hashlib
import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
ARCHIVES = ROOT / "crates/quilltap-web/tests/fixtures/restore-archives"
SRC = ARCHIVES / "restore-archive.zip"
DST = ARCHIVES / "restore-archive-store-binding.zip"
SRC_MD5 = "6b076003bacf8db22c1bf8440fac09d6"

LORIAN_VAULT = "aff3114e-ed90-4d5b-99c1-ef3fa20203fc"
RIYA = "a1000000-0000-4000-8000-000000000002"
RIYA_VAULT = "f6b22d51-85e5-4bee-a053-262dd965962f"
NOT_CARRIED = "dead0000-0000-4000-8000-000000000001"
PROJECT = "a3000000-0000-4000-8000-000000000001"
PROJECT_STORE = "5c17e916-5f79-4cca-a134-ec09c05924e9"
BLOB = "bc000000-0000-4000-8000-000000000001"
USER = "e18e05bc-63e8-4539-8a85-719b7a508850"
CARRIED_KEY = f"mount-blob:{PROJECT_STORE}:{BLOB}"
LOOSE_KEY = f"{USER}/voyage-notes.txt"
CARRIED_MIME = "application/x-quilltap-chart"
LOOSE_BYTES = b"Leg two: the wind backed north-east before dawn.\n"


def md5(path):
    return hashlib.md5(path.read_bytes()).hexdigest()


def load(zin, root, name):
    return json.loads(zin.read(f"{root}/data/{name}.json"))


def plan(zin, root):
    points = load(zin, root, "doc-mount-points")
    lorian = [p for p in points if p["id"] == LORIAN_VAULT]
    assert len(lorian) == 1 and lorian[0]["storeType"] == "character"
    del lorian[0]["storeType"]

    characters = load(zin, root, "characters")
    riya = [c for c in characters if c["id"] == RIYA]
    assert len(riya) == 1 and riya[0]["characterDocumentMountPointId"] == RIYA_VAULT
    riya[0]["characterDocumentMountPointId"] = NOT_CARRIED
    riya[0]["archivedAt"] = "2026-03-06T00:00:00.000Z"

    groups = load(zin, root, "groups")
    assert len(groups) == 1
    groups[0]["officialMountPointId"] = RIYA_VAULT

    files = load(zin, root, "files")
    assert len(files) == 1
    base = files[0]
    blob = [b for b in load(zin, root, "doc-mount-blobs") if b["id"] == BLOB]
    assert len(blob) == 1
    carried = dict(base)
    # P4.167 (H-4): the row DISAGREES with its blob — v4's carried arm keeps
    # the ROW's mime / size and only the blob's sha256 (the human's ruling of
    # 2026-10-27: converge on v4's archive row).
    assert blob[0]["storedMimeType"] != CARRIED_MIME
    carried.update(
        id="f0000002-0000-4000-8000-000000000001",
        originalFilename="chart-blank.bin",
        mimeType=CARRIED_MIME,
        size=blob[0]["sizeBytes"] + 7,
        sha256=blob[0]["sha256"],
        category="DOCUMENT",
        storageKey=CARRIED_KEY,
        projectId=PROJECT,
        folderPath="/",
    )
    for k in ("width", "height"):
        carried.pop(k, None)
    loose = dict(base)
    loose.update(
        id="f0000003-0000-4000-8000-000000000001",
        originalFilename="voyage-notes.txt",
        mimeType="text/plain",
        size=len(LOOSE_BYTES),
        sha256=hashlib.sha256(LOOSE_BYTES).hexdigest(),
        category="DOCUMENT",
        storageKey=LOOSE_KEY,
        projectId=PROJECT,
        folderPath="/",
    )
    for k in ("width", "height"):
        loose.pop(k, None)
    # P4.167 (P-1): carried rows whose bytes the archive does NOT carry under
    # `files/` (their keys name other mounts), so only a disk read misses them.
    no_bytes = dict(carried)
    no_bytes.update(
        id="f0000004-0000-4000-8000-000000000001",
        originalFilename="chart-no-bytes.bin",
        mimeType="text/plain",
        size=11,
        storageKey=f"mount-blob:{LORIAN_VAULT}:{BLOB}",
    )
    no_size = dict(carried)
    no_size.update(
        id="f0000005-0000-4000-8000-000000000001",
        originalFilename="chart-no-size.bin",
        storageKey=f"mount-blob:{RIYA_VAULT}:{BLOB}",
    )
    del no_size["size"]
    null_mime = dict(carried)
    null_mime.update(
        id="f0000006-0000-4000-8000-000000000001",
        originalFilename="chart-null-mime.bin",
        mimeType=None,
        storageKey=f"mount-blob:{RIYA_VAULT}:{BLOB}",
    )
    bad_both = dict(carried)
    bad_both.update(
        id="f0000007-0000-4000-8000-000000000001",
        originalFilename="chart-bad-both.bin",
        mimeType=None,
        size="13",
        storageKey=f"mount-blob:{LORIAN_VAULT}:{BLOB}",
    )
    files += [carried, loose, no_bytes, no_size, null_mime, bad_both]
    return {
        "doc-mount-points": points,
        "characters": characters,
        "groups": groups,
        "files": files,
    }


def main():
    before = md5(SRC)
    assert before == SRC_MD5, f"restore-archive.zip moved: {before}"
    with zipfile.ZipFile(SRC) as zin:
        root = zin.infolist()[0].filename.rstrip("/")
        rewritten = plan(zin, root)
        blob_bytes = zin.read(f"{root}/mount-blobs/{BLOB}")
        with zipfile.ZipFile(DST, "w", zipfile.ZIP_DEFLATED) as zout:
            first = zin.infolist()[0]

            def put(name, data):
                out = zipfile.ZipInfo(f"{root}/{name}", date_time=first.date_time)
                out.external_attr = 0o644 << 16
                out.compress_type = zipfile.ZIP_DEFLATED
                zout.writestr(out, data)

            for info in zin.infolist():
                rel = info.filename[len(root) + 1:]
                data = zin.read(info.filename)
                stem = rel[len("data/"):-len(".json")] if rel.startswith("data/") else None
                if stem in rewritten:
                    data = (json.dumps(rewritten[stem], indent=2, ensure_ascii=False) + "\n").encode()
                elif rel == "manifest.json":
                    m = json.loads(data)
                    m["counts"]["files"] = len(rewritten["files"])
                    data = (json.dumps(m, indent=2) + "\n").encode()
                out = zipfile.ZipInfo(info.filename, date_time=info.date_time)
                out.external_attr = info.external_attr
                out.compress_type = zipfile.ZIP_DEFLATED
                zout.writestr(out, data)
            put(f"files/{CARRIED_KEY}", blob_bytes)
            put(f"files/{LOOSE_KEY}", LOOSE_BYTES)
    after = md5(SRC)
    assert after == SRC_MD5, f"restore-archive.zip moved during the derive: {after}"
    print(f"wrote {DST.relative_to(ROOT)} (restore-archive.zip md5 {after} unchanged)")


if __name__ == "__main__":
    main()
