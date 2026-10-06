#!/usr/bin/env python3
"""P4.158 item 1 (ruling R-A): derive `restore-archive-damaged-store.zip` from
`restore-archive.zip`.

The restore's PRESERVE arm (P4.147, dogfood #141) keeps an entity on the store
the archive carries. No committed archive can show what happens when that store
is INCOMPLETE — every archived vault and official store carries its whole
managed-file set — so the preserve arm's "the store restores itself" premise is
never tested against a store that cannot.

So this derivation applies ONE mutation: it REMOVES `description.md` from the
three preserved stores whose entity carries a non-empty `description` — Lorian's
vault `aff3114e-…`, the project store `5c17e916-…` and the group store
`60a8194d-…` — with the links' chunks and the content rows (`doc_mount_files`,
their documents and blobs and the blob BYTES entry) that only those links
reference. The archive stays referentially whole (no #58 orphan); the archived
`characters.json` / `projects.json` / `groups.json` rows still carry the
inline `description` the stores no longer hold.

v4 never preserves (it projects every managed field into a FRESH store), so it
restores each description from the row; v5's preserve arm keeps the damaged
store and, since R-A, BACKFILLS the missing file from the same row
(`system_restore_state`'s `PRESERVE_BACKFILL`).

The manifest's counts move with every collection touched. Entry order, names
and every other byte are kept (the `derive-restore-archive-bag-nulls.py`
precedent). `restore-archive.zip` is md5-checked unchanged before and after.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-damaged-store.py
"""
import hashlib
import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
ARCHIVES = ROOT / "crates/quilltap-web/tests/fixtures/restore-archives"
SRC = ARCHIVES / "restore-archive.zip"
DST = ARCHIVES / "restore-archive-damaged-store.zip"
SRC_MD5 = "6b076003bacf8db22c1bf8440fac09d6"

LORIAN_VAULT = "aff3114e-ed90-4d5b-99c1-ef3fa20203fc"
PROJECT_STORE = "5c17e916-5f79-4cca-a134-ec09c05924e9"
GROUP_STORE = "60a8194d-f8ea-4540-af77-5e13e7ed0e9b"
DAMAGED = {LORIAN_VAULT, PROJECT_STORE, GROUP_STORE}
REMOVED_PATH = "description.md"


def md5(path):
    return hashlib.md5(path.read_bytes()).hexdigest()


def load(zin, root, name):
    return json.loads(zin.read(f"{root}/data/{name}.json"))


def plan(zin, root):
    """Every collection rewritten, keyed by its data file stem."""
    out = {}
    links = load(zin, root, "doc-mount-file-links")
    gone = [
        l for l in links
        if l["mountPointId"] in DAMAGED and l["relativePath"] == REMOVED_PATH
    ]
    assert {l["mountPointId"] for l in gone} == DAMAGED, "one description.md per store"
    assert len(gone) == 3
    gone_links = {l["id"] for l in gone}
    kept_links = [l for l in links if l["id"] not in gone_links]
    out["doc-mount-file-links"] = kept_links
    still_used = {l["fileId"] for l in kept_links}
    gone_files = {l["fileId"] for l in gone} - still_used

    chunks = load(zin, root, "doc-mount-chunks")
    out["doc-mount-chunks"] = [c for c in chunks if c["linkId"] not in gone_links]
    files = load(zin, root, "doc-mount-files")
    out["doc-mount-files"] = [f for f in files if f["id"] not in gone_files]
    docs = load(zin, root, "doc-mount-documents")
    out["doc-mount-documents"] = [d for d in docs if d["fileId"] not in gone_files]
    blobs = load(zin, root, "doc-mount-blobs")
    out["doc-mount-blobs"] = [b for b in blobs if b["fileId"] not in gone_files]
    gone_blob_ids = {b["id"] for b in blobs if b["fileId"] in gone_files}

    # The rows the stores no longer back must still carry their value.
    for stem, store_key, store in (
        ("characters", "characterDocumentMountPointId", LORIAN_VAULT),
        ("projects", "officialMountPointId", PROJECT_STORE),
        ("groups", "officialMountPointId", GROUP_STORE),
    ):
        rows = [r for r in load(zin, root, stem) if r.get(store_key) == store]
        assert len(rows) == 1 and rows[0].get("description"), (stem, rows)
    return out, gone_blob_ids


COUNT_KEYS = {
    "doc-mount-files": "docMountFiles",
    "doc-mount-file-links": "docMountFileLinks",
    "doc-mount-chunks": "docMountChunks",
    "doc-mount-documents": "docMountDocuments",
    "doc-mount-blobs": "docMountBlobs",
}


def main():
    before = md5(SRC)
    assert before == SRC_MD5, f"restore-archive.zip moved: {before}"
    with zipfile.ZipFile(SRC) as zin:
        root = zin.infolist()[0].filename.rstrip("/")
        rewritten, gone_blob_ids = plan(zin, root)
        with zipfile.ZipFile(DST, "w", zipfile.ZIP_DEFLATED) as zout:
            for info in zin.infolist():
                rel = info.filename[len(root) + 1:]
                if rel.startswith("mount-blobs/") and rel.split("/", 1)[1] in gone_blob_ids:
                    continue
                data = zin.read(info.filename)
                stem = rel[len("data/"):-len(".json")] if rel.startswith("data/") else None
                if stem in rewritten:
                    data = (json.dumps(rewritten[stem], indent=2, ensure_ascii=False) + "\n").encode()
                elif rel == "manifest.json":
                    m = json.loads(data)
                    for s, key in COUNT_KEYS.items():
                        m["counts"][key] = len(rewritten[s])
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
