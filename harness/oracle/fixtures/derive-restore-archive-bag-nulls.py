#!/usr/bin/env python3
"""P4.147 item 8 (P4.146 item 13): derive `restore-archive-bag-nulls.zip` from
`restore-archive.zip`.

The restore's FALLBACK arm — a project or group whose official store the
archive does NOT carry — provisions a fresh store and writes the archived
property bag into it. No committed archive can see that arm's bag: every one
either carries the project/group stores (so the store's own `properties.json`
restores at 22e and the bag is never written) or carries no projects at all,
and every archived project holds exactly the six keys v5's old
`project_properties` copied.

So this derivation:

  1. REMOVES the project store `5c17e916-…` and the group store `60a8194d-…`
     with everything keyed by them — their folders, file links, the links'
     chunks, the `project_/group_doc_mount_links` rows, the content rows
     (`doc_mount_files`, with their documents and blobs and the blob BYTES
     entry) that only those links reference — so the archive stays
     referentially whole (no #58 orphan) and the two entities take the
     fallback arm;
  2. WIDENS the project row to all sixteen `ProjectPropertiesSchema` keys:
     explicit `null`s on five nullable keys, real values on `icon`,
     `defaultImageProfileId` (the archive's own image profile) and
     `answerConfirmationOverride`, and real booleans on two more;
  3. gives the group `color: null` and `icon: "⚙"`.

The manifest's counts move with every collection touched. Entry order, names
and every other byte are kept (the `derive-restore-archive-concierge-bogus.py`
precedent). `restore-archive.zip` is md5-checked unchanged before and after.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-bag-nulls.py
"""
import hashlib
import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
ARCHIVES = ROOT / "crates/quilltap-web/tests/fixtures/restore-archives"
SRC = ARCHIVES / "restore-archive.zip"
DST = ARCHIVES / "restore-archive-bag-nulls.zip"
SRC_MD5 = "6b076003bacf8db22c1bf8440fac09d6"

PROJECT = "a3000000-0000-4000-8000-000000000001"
GROUP = "a2000000-0000-4000-8000-000000000001"
PROJECT_STORE = "5c17e916-5f79-4cca-a134-ec09c05924e9"
GROUP_STORE = "60a8194d-f8ea-4540-af77-5e13e7ed0e9b"
GONE = {PROJECT_STORE, GROUP_STORE}
IMAGE_PROFILE = "a6000000-0000-4000-8000-000000000001"


def md5(path):
    return hashlib.md5(path.read_bytes()).hexdigest()


def load(zin, root, name):
    return json.loads(zin.read(f"{root}/data/{name}.json"))


def plan(zin, root):
    """Every collection rewritten, keyed by its data file stem."""
    out = {}
    points = load(zin, root, "doc-mount-points")
    assert GONE <= {p["id"] for p in points}, "the two stores must be in the source"
    out["doc-mount-points"] = [p for p in points if p["id"] not in GONE]

    folders = load(zin, root, "doc-mount-folders")
    out["doc-mount-folders"] = [f for f in folders if f["mountPointId"] not in GONE]

    links = load(zin, root, "doc-mount-file-links")
    gone_links = {l["id"] for l in links if l["mountPointId"] in GONE}
    kept_links = [l for l in links if l["mountPointId"] not in GONE]
    out["doc-mount-file-links"] = kept_links
    still_used = {l["fileId"] for l in kept_links}
    gone_files = {l["fileId"] for l in links if l["id"] in gone_links} - still_used

    chunks = load(zin, root, "doc-mount-chunks")
    out["doc-mount-chunks"] = [
        c for c in chunks if c["linkId"] not in gone_links and c["mountPointId"] not in GONE
    ]
    files = load(zin, root, "doc-mount-files")
    out["doc-mount-files"] = [f for f in files if f["id"] not in gone_files]
    docs = load(zin, root, "doc-mount-documents")
    out["doc-mount-documents"] = [d for d in docs if d["fileId"] not in gone_files]
    blobs = load(zin, root, "doc-mount-blobs")
    out["doc-mount-blobs"] = [b for b in blobs if b["fileId"] not in gone_files]
    gone_blob_ids = {b["id"] for b in blobs if b["fileId"] in gone_files}

    out["project-doc-mount-links"] = [
        l for l in load(zin, root, "project-doc-mount-links") if l["mountPointId"] not in GONE
    ]
    out["group-doc-mount-links"] = [
        l for l in load(zin, root, "group-doc-mount-links") if l["mountPointId"] not in GONE
    ]
    assert out["project-doc-mount-links"] == [] and out["group-doc-mount-links"] == []

    projects = load(zin, root, "projects")
    assert [p["id"] for p in projects] == [PROJECT]
    p = projects[0]
    assert p["officialMountPointId"] == PROJECT_STORE
    p.update(
        icon="⛵",
        defaultAgentModeEnabled=None,
        defaultAvatarGenerationEnabled=True,
        defaultImageProfileId=IMAGE_PROFILE,
        defaultRoleplayTemplateId=None,
        defaultAlertCharactersOfLanternImages=None,
        answerConfirmationOverride="ON",
        storyBackgroundsEnabled=False,
        staticBackgroundImageId=None,
        storyBackgroundImageId=None,
    )
    out["projects"] = projects

    groups = load(zin, root, "groups")
    assert [g["id"] for g in groups] == [GROUP]
    assert groups[0]["officialMountPointId"] == GROUP_STORE
    groups[0].update(color=None, icon="⚙")
    out["groups"] = groups
    return out, gone_blob_ids


COUNT_KEYS = {
    "doc-mount-points": "docMountPoints",
    "doc-mount-folders": "docMountFolders",
    "doc-mount-files": "docMountFiles",
    "doc-mount-file-links": "docMountFileLinks",
    "doc-mount-chunks": "docMountChunks",
    "doc-mount-documents": "docMountDocuments",
    "doc-mount-blobs": "docMountBlobs",
    "project-doc-mount-links": "projectDocMountLinks",
    "group-doc-mount-links": "groupDocMountLinks",
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
