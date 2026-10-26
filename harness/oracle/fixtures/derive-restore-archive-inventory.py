#!/usr/bin/env python3
"""P4.D327: derive the two Inventory-carrier restore archives from
`restore-archive.zip` (v4 `4fbdad186` / `e1872c94f`, pin `322a5488e`).

No committed archive carries an `Inventory/` object, an `equippedInventory`
cell or a `canManageInventory` flag, so no restore can be seen running
22f-quater or the remap's verbatim-object-id rule. Rebuilding the archive
family would move EVERY archive (the P4.D264 DERIVE rule), so these are
DERIVATIONS of the base — every other entry and byte is kept.

The objects go in RIYA's vault (`f6b22d51-…`): it holds no garment and no
`Inventory/` (measured), so the re-projection 22f-quater triggers touches
only the documents planted here — the P4.D264 reason for choosing her.

1. `restore-archive-inventory.zip` — in Riya's vault, five object files
   (each a `doc_mount_files` + `doc_mount_documents` + `doc_mount_file_links`
   row with consistent `sha256` / `fileSizeBytes` / `contentSha256` /
   `plainTextLength`, and the `Inventory` + `Inventory/Weapons` folder rows):
     1e…01  `Inventory/Cutlass.json` — an ARCHETYPE, written whole (every
            defaulted key present, so a re-projection re-writes it unchanged)
     1e…02  `Inventory/Weapons/Old Cutlass.json` — an INSTANCE of 1e…01,
            SPARSE (v4 never defaults an instance), whose `imageFileId` is the
            archive's own `portrait.png` IMAGE row `f0000001-…0001`: the one
            pointer the new-account restore plans (planned=1)
     1e…03  `Inventory/Boarding Kit.json` — a KIT naming the instance
     1e…04  `Inventory/rope.json` — a SPARSE STANDALONE (`id`, `name`, the
            timestamps only): v4 validates a standalone whole, so when 22f-
            quater re-projects the mount it is re-written WITH its defaults
            (R-6 — compared, never masked)
     1e…05  `Inventory/Lantern.json` — whole, its `imageFileId` naming a file
            the archive does NOT carry (`f0000001-…0099`): planned nothing
   `data/files.json`: the portrait's `linkedTo` = [instance, chat c…02] and
   `tags` = [instance] (the instance id must travel verbatim through the
   new-account remap). `data/chats.json`: chat c…02 (Riya) carries
   `equippedInventory` = {Riya: [instance], a character the archive does not
   carry: [instance]} (the new-account re-key DROPS the second); chat c…01
   carries {Lorian: "not-a-list"} (a non-array entry → `[]`).
   `data/characters.json`: Riya `canManageInventory: false`, Lorian `true`
   (appended last — the archive is a v4 backup, which spreads the row).
   `manifest.counts` gains the three doc-mount rows' counts only (v4's backup
   counts nothing Inventory-specific — no `backup-service.ts` hunk).

2. `restore-archive-inventory-tombstone.zip` — (1) with Riya ARCHIVED
   (`archivedAt`): her vault is a tombstone, so 22f-quater skips the
   instance's fix (both mount ids checked) while the INFO still logs
   `planned=1 fixed=0`. Compared on the 22f-quater facts only (v4 refuses to
   restore an archived character's store while v5 restores it un-archived —
   the P4.D264 recorded pre-existing divergence, re-cited, not re-ruled).

`restore-archive.zip` is md5-checked unchanged before and after.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-inventory.py
"""
import hashlib
import json
import pathlib
import sys
import zipfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from restore_archive_derive import ARCHIVES, derive, load  # noqa: E402

LORIAN = "a1000000-0000-4000-8000-000000000001"
RIYA = "a1000000-0000-4000-8000-000000000002"
STRANGER = "a1000000-0000-4000-8000-000000000099"
C1 = "c1000000-0000-4000-8000-000000000001"
C2 = "c1000000-0000-4000-8000-000000000002"
RIYA_VAULT = "f6b22d51-85e5-4bee-a053-262dd965962f"
PORTRAIT = "f0000001-0000-4000-8000-000000000001"
MISSING_FILE = "f0000001-0000-4000-8000-000000000099"

ARCH = "1e000000-0000-4000-8000-000000000001"
INST = "1e000000-0000-4000-8000-000000000002"
KIT = "1e000000-0000-4000-8000-000000000003"
ROPE = "1e000000-0000-4000-8000-000000000004"
LANTERN = "1e000000-0000-4000-8000-000000000005"

FOLDER_INV = "1f000000-0000-4000-8000-000000000001"
FOLDER_WEAPONS = "1f000000-0000-4000-8000-000000000002"

T0 = "2026-03-01T00:00:00.000Z"
T1 = "2026-03-02T00:00:00.000Z"


def whole(id_, kind, name, title, extra=None):
    """A standalone / archetype / kit file as v4's parse-then-serialise
    writes it: every defaulted key present, INVENTORY_FILE_KEYS order."""
    rec = {"id": id_, "kind": kind, "name": name, "title": title}
    rec.update(
        {
            "equippedByDefault": False,
            "maxEquipped": 1,
            "canCopy": False,
            "isArchetype": False,
            "parentId": None,
            "itemIds": [],
        }
    )
    for k, v in (extra or {}).items():
        rec[k] = v
    rec["metadata"] = {}
    rec["createdAt"] = T0
    rec["updatedAt"] = T0
    return rec


def ordered(rec, keys):
    return {k: rec[k] for k in keys if k in rec}


FILE_KEYS = [
    "id", "kind", "name", "title", "description", "imageDescription",
    "equippedByDefault", "maxEquipped", "canCopy", "isArchetype", "parentId",
    "itemIds", "emoji", "svg", "imageFileId", "metadata", "archivedAt",
    "createdAt", "updatedAt",
]

OBJECTS = [
    # (file id, link id, folder id, relative path, record)
    ("1d000000-0000-4000-8000-000000000001", "1c000000-0000-4000-8000-000000000001", FOLDER_INV,
     "Inventory/Cutlass.json",
     ordered(dict(whole(ARCH, "item", "cutlass", "Cutlass"), isArchetype=True,
                  description="A curved blade."), FILE_KEYS)),
    ("1d000000-0000-4000-8000-000000000002", "1c000000-0000-4000-8000-000000000002", FOLDER_WEAPONS,
     "Inventory/Weapons/Old Cutlass.json",
     {"id": INST, "title": "Old Cutlass", "parentId": ARCH, "imageFileId": PORTRAIT,
      "createdAt": T0, "updatedAt": T1}),
    ("1d000000-0000-4000-8000-000000000003", "1c000000-0000-4000-8000-000000000003", FOLDER_INV,
     "Inventory/Boarding Kit.json",
     ordered(dict(whole(KIT, "kit", "boarding kit", "Boarding Kit"), itemIds=[INST]), FILE_KEYS)),
    ("1d000000-0000-4000-8000-000000000004", "1c000000-0000-4000-8000-000000000004", FOLDER_INV,
     "Inventory/rope.json",
     {"id": ROPE, "name": "rope", "createdAt": T0, "updatedAt": T0}),
    ("1d000000-0000-4000-8000-000000000005", "1c000000-0000-4000-8000-000000000005", FOLDER_INV,
     "Inventory/Lantern.json",
     ordered(dict(whole(LANTERN, "item", "lantern", "Lantern"), imageFileId=MISSING_FILE), FILE_KEYS)),
]


def js_pretty(value):
    """`JSON.stringify(value, null, 2) + '\\n'` (ASCII content only)."""
    return json.dumps(value, indent=2, ensure_ascii=False) + "\n"


def plan(tombstone):
    def go(zin, root):
        links = load(zin, root, "doc-mount-file-links")
        files = load(zin, root, "doc-mount-files")
        docs = load(zin, root, "doc-mount-documents")
        folders = load(zin, root, "doc-mount-folders")
        assert not any(l["relativePath"].lower().startswith("inventory/") for l in links), \
            "the base archive already carries an Inventory/ document"
        assert not any(l["mountPointId"] == RIYA_VAULT and l["relativePath"].startswith("Wardrobe/")
                       for l in links), "Riya's vault must hold no garment"
        folders = folders + [
            {"id": FOLDER_INV, "mountPointId": RIYA_VAULT, "parentId": None, "name": "Inventory",
             "path": "Inventory", "createdAt": T0, "updatedAt": T0},
            {"id": FOLDER_WEAPONS, "mountPointId": RIYA_VAULT, "parentId": FOLDER_INV, "name": "Weapons",
             "path": "Inventory/Weapons", "createdAt": T0, "updatedAt": T0},
        ]
        for file_id, link_id, folder_id, rel, rec in OBJECTS:
            content = js_pretty(rec)
            raw = content.encode()
            sha = hashlib.sha256(raw).hexdigest()
            files.append({"id": file_id, "sha256": sha, "fileSizeBytes": len(raw), "fileType": "json",
                          "source": "database", "createdAt": T0, "updatedAt": T0})
            docs.append({"id": "1b" + file_id[2:], "fileId": file_id, "content": content,
                         "contentSha256": sha, "plainTextLength": len(content),
                         "createdAt": T0, "updatedAt": T0})
            links.append({
                "id": link_id, "fileId": file_id, "mountPointId": RIYA_VAULT, "relativePath": rel,
                "fileName": rel.rsplit("/", 1)[-1], "folderId": folder_id, "originalFileName": None,
                "originalMimeType": None, "description": "", "descriptionUpdatedAt": None,
                "conversionStatus": "converted", "conversionError": None,
                "plainTextLength": len(content), "extractedText": None, "extractedTextSha256": None,
                "extractionStatus": "none", "extractionError": None, "chunkCount": 0, "allowEmbed": 1,
                "allowCharacterRead": 1, "allowCharacterWrite": 1, "lastModified": T0,
                "createdAt": T0, "updatedAt": T0,
            })

        pictures = load(zin, root, "files")
        assert [f["id"] for f in pictures] == [PORTRAIT], pictures
        pictures[0]["linkedTo"] = [INST, C2]
        pictures[0]["tags"] = [INST]

        chats = load(zin, root, "chats")
        by_id = {c["id"]: c for c in chats}
        assert "equippedInventory" not in by_id[C1] and "equippedInventory" not in by_id[C2]
        by_id[C2]["equippedInventory"] = {RIYA: [INST], STRANGER: [INST]}
        by_id[C1]["equippedInventory"] = {LORIAN: "not-a-list"}

        chars = load(zin, root, "characters")
        flags = {LORIAN: True, RIYA: False}
        for c in chars:
            assert "canManageInventory" not in c
            c["canManageInventory"] = flags[c["id"]]
            if tombstone and c["id"] == RIYA:
                assert c.get("archivedAt") is None
                c["archivedAt"] = "2026-03-06T00:00:00.000Z"

        return {
            "doc-mount-folders": folders,
            "doc-mount-files": files,
            "doc-mount-documents": docs,
            "doc-mount-file-links": links,
            "files": pictures,
            "chats": chats,
            "characters": chars,
        }

    return go


COUNT_KEYS = {
    "doc-mount-folders": "docMountFolders",
    "doc-mount-files": "docMountFiles",
    "doc-mount-documents": "docMountDocuments",
    "doc-mount-file-links": "docMountFileLinks",
}


def recount(dst_name):
    """`derive` rewrites `data/*.json` only; bring `manifest.counts` for the
    four grown doc-mount files in step (v4's backup counts each row), every
    other byte of the derived archive kept."""
    dst = ARCHIVES / dst_name
    with zipfile.ZipFile(dst) as zin:
        infos = zin.infolist()
        blobs = [(i, zin.read(i.filename)) for i in infos]
    root = infos[0].filename.rstrip("/")
    rows = {stem: json.loads(dict((i.filename, b) for i, b in blobs)[f"{root}/data/{stem}.json"])
            for stem in COUNT_KEYS}
    with zipfile.ZipFile(dst, "w", zipfile.ZIP_DEFLATED) as zout:
        for info, data in blobs:
            if info.filename == f"{root}/manifest.json":
                m = json.loads(data)
                for stem, key in COUNT_KEYS.items():
                    assert m["counts"][key] + (5 if stem != "doc-mount-folders" else 2) == len(rows[stem])
                    m["counts"][key] = len(rows[stem])
                data = (json.dumps(m, indent=2) + ("\n" if data.endswith(b"\n") else "")).encode()
            out = zipfile.ZipInfo(info.filename, date_time=info.date_time)
            out.external_attr = info.external_attr
            out.compress_type = zipfile.ZIP_DEFLATED
            zout.writestr(out, data)
    print(f"recounted {dst_name}: {hashlib.md5(dst.read_bytes()).hexdigest()}")


def main():
    for name, tombstone in (("restore-archive-inventory.zip", False),
                            ("restore-archive-inventory-tombstone.zip", True)):
        derive(name, plan(tombstone))
        recount(name)


if __name__ == "__main__":
    main()
