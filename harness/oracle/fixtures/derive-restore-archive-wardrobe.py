#!/usr/bin/env python3
"""P4.D264: derive the three wardrobe-carrier restore archives from
`restore-archive.zip` (v4 `3ee3b1342` #81 + `7c8572869` #82).

No committed archive carries `data/wardrobe-wear.json` (every one predates the
ledger) nor a wardrobe item whose frontmatter names a picture, so neither
restore can be seen running 22n-bis or 22f-ter. Rebuilding the archive family
would add the file to EVERY derived archive (P4.D264 Tier 3 item 16), so these
are DERIVATIONS — every other entry and byte is kept.

1. `restore-archive-wardrobe-wear.zip` — appends `data/wardrobe-wear.json`
   (LAST, as v4 writes it) with four ledger rows and `counts.wardrobeWear: 4`
   (LAST):
     3e…01  Travelling Coat × Lorian, last worn in chat c…01   (attributed)
     3e…02  Travelling Coat × unattributed, no chat            (a departed wearer)
     3e…03  an item the archive carries NO document for × Riya (itemId passes
            through the new-account remap UNCHANGED)
     3e…04  Travelling Coat × a wearer the archive does not carry (new-account
            mints a stranger id that names nothing — v4's comment)

2. `restore-archive-wardrobe-picture.zip` — the coat's frontmatter gains
   `imageFileId: f0000001-…0001` (the archive's own `portrait.png` IMAGE row),
   whose `files` row is linked to the coat (`linkedTo: [coat, chat c…01]`,
   `tags: [coat]`); the document's `contentSha256` and its `doc_mount_files`
   row's `sha256` / `fileSizeBytes` follow the new text. `data/wardrobe-items.json`
   gains ONE legacy (pre-cutover) row on RIYA carrying `imageFileId` — 22f-bis
   writes it into her vault, the new-account remap moves it with the file. Riya,
   not Lorian: her vault holds no garment, so creating the legacy item there
   re-projects no OTHER document — on Lorian's, v5's preserved archive vault
   (the ruled #141 `FRESH_STORE_RESIDUAL`) would re-write the coat beside it
   while v4's fresh store never sees the coat.

3. `restore-archive-wardrobe-picture-tombstone.zip` — (2) with Lorian ARCHIVED
   (`archivedAt`): his vault is a tombstone, so 22f-ter skips the coat's fix
   (both mount ids are checked) and the pointer keeps its source id. Compared
   on the 22f-ter facts only (`system_restore_state.rs`'s
   `wardrobe_picture_tombstone_is_skipped`): v4 refuses to restore an archived
   character's store while v5 restores the character un-archived — an
   unruled pre-existing divergence this archive surfaced (P4.D264 lane record).

`restore-archive.zip` is md5-checked unchanged before and after.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-wardrobe.py
"""
import hashlib
import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
ARCHIVES = ROOT / "crates/quilltap-web/tests/fixtures/restore-archives"
SRC = ARCHIVES / "restore-archive.zip"
SRC_MD5 = "6b076003bacf8db22c1bf8440fac09d6"

COAT = "ac000000-0000-4000-8000-000000000001"
LORIAN = "a1000000-0000-4000-8000-000000000001"
RIYA = "a1000000-0000-4000-8000-000000000002"
C1 = "c1000000-0000-4000-8000-000000000001"
C2 = "c1000000-0000-4000-8000-000000000002"
PORTRAIT = "f0000001-0000-4000-8000-000000000001"
COAT_FILE = "523f4062-7cc0-4c2d-ab82-ea98fd03749d"
LEGACY_ITEM = "ab000000-0000-4000-8000-000000000001"


def wear_row(suffix, item, wearer, count, first, last, chat):
    return {
        "id": f"3e000000-0000-4000-8000-0000000000{suffix}",
        "itemId": item,
        "wearerCharacterId": wearer,
        "wearCount": count,
        "firstWornAt": first,
        "lastWornAt": last,
        "lastWornChatId": chat,
        "createdAt": first,
        "updatedAt": last,
    }


WEAR = [
    wear_row("01", COAT, LORIAN, 3, "2026-03-02T00:00:00.000Z", "2026-03-05T00:00:00.000Z", C1),
    wear_row("02", COAT, None, 2, "2026-03-01T00:00:00.000Z", "2026-03-03T00:00:00.000Z", None),
    wear_row("03", "ae000000-0000-4000-8000-000000000099", RIYA, 1,
             "2026-03-04T00:00:00.000Z", "2026-03-04T00:00:00.000Z", C2),
    wear_row("04", COAT, "a1000000-0000-4000-8000-000000000099", 5,
             "2026-02-01T00:00:00.000Z", "2026-02-20T00:00:00.000Z", None),
]

COAT_CONTENT_OLD = (
    "---\nid: ac000000-0000-4000-8000-000000000001\ntitle: Travelling Coat\ntypes:\n  - top\n"
    "imagePrompt: brown coat\ncreatedAt: 2026-03-01T00:00:00.000Z\n"
    "updatedAt: 2026-03-01T00:00:00.000Z\n---\n\nA weathered coat."
)
COAT_CONTENT_NEW = (
    "---\nid: ac000000-0000-4000-8000-000000000001\ntitle: Travelling Coat\ntypes:\n  - top\n"
    "imagePrompt: brown coat\nimageFileId: f0000001-0000-4000-8000-000000000001\n"
    "createdAt: 2026-03-01T00:00:00.000Z\nupdatedAt: 2026-03-01T00:00:00.000Z\n---\n\n"
    "A weathered coat."
)

LEGACY = {
    "id": LEGACY_ITEM,
    "characterId": RIYA,
    "title": "Opera Cloak",
    "description": "A cloak for the second act.",
    "types": ["top"],
    "componentItemIds": [],
    "appropriateness": None,
    "isDefault": False,
    "replace": False,
    "imageFileId": PORTRAIT,
    "archivedAt": None,
    "createdAt": "2026-03-01T00:00:00.000Z",
    "updatedAt": "2026-03-01T00:00:00.000Z",
}


def dump(value):
    return (json.dumps(value, indent=2) + "\n").encode()


def picture_edits(name, data, tombstone):
    """The picture archive's per-file rewrites (None = keep the bytes)."""
    if name.endswith("/data/doc-mount-documents.json"):
        docs = json.loads(data)
        hits = [d for d in docs if d.get("fileId") == COAT_FILE]
        assert len(hits) == 1 and hits[0]["content"] == COAT_CONTENT_OLD, hits
        hits[0]["content"] = COAT_CONTENT_NEW
        if "contentSha256" in hits[0]:
            hits[0]["contentSha256"] = hashlib.sha256(COAT_CONTENT_NEW.encode()).hexdigest()
        return dump(docs)
    if name.endswith("/data/doc-mount-files.json"):
        files = json.loads(data)
        hits = [f for f in files if f.get("id") == COAT_FILE]
        assert len(hits) == 1, hits
        hits[0]["sha256"] = hashlib.sha256(COAT_CONTENT_NEW.encode()).hexdigest()
        if "fileSizeBytes" in hits[0]:
            hits[0]["fileSizeBytes"] = len(COAT_CONTENT_NEW.encode())
        return dump(files)
    if name.endswith("/data/files.json"):
        files = json.loads(data)
        assert [f["id"] for f in files] == [PORTRAIT], files
        files[0]["linkedTo"] = [COAT, C1]
        files[0]["tags"] = [COAT]
        return dump(files)
    if name.endswith("/data/wardrobe-items.json"):
        assert json.loads(data) == []
        return dump([LEGACY])
    if name.endswith("/data/characters.json") and tombstone:
        chars = json.loads(data)
        lorian = [c for c in chars if c["id"] == LORIAN]
        assert len(lorian) == 1 and lorian[0].get("archivedAt") is None
        lorian[0]["archivedAt"] = "2026-03-06T00:00:00.000Z"
        return dump(chars)
    if name.endswith("/manifest.json"):
        m = json.loads(data)
        assert m["counts"]["wardrobeItems"] == 0
        m["counts"]["wardrobeItems"] = 1
        return dump(m)
    return None


def derive(dst, edit, append=None):
    with zipfile.ZipFile(SRC) as zin, zipfile.ZipFile(dst, "w", zipfile.ZIP_DEFLATED) as zout:
        infos = zin.infolist()
        root = infos[0].filename.rstrip("/").split("/")[0]
        last = infos[-1]
        for info in infos:
            data = zin.read(info.filename)
            new = edit(info.filename, data)
            out = zipfile.ZipInfo(info.filename, date_time=info.date_time)
            out.external_attr = info.external_attr
            out.compress_type = zipfile.ZIP_DEFLATED
            zout.writestr(out, data if new is None else new)
        if append:
            path, payload = append
            assert f"{root}/{path}" not in zin.namelist()
            out = zipfile.ZipInfo(f"{root}/{path}", date_time=last.date_time)
            out.external_attr = last.external_attr
            out.compress_type = zipfile.ZIP_DEFLATED
            zout.writestr(out, payload)
    print(f"wrote {dst.relative_to(ROOT)}")


def wear_edit(name, data):
    if name.endswith("/manifest.json"):
        m = json.loads(data)
        assert "wardrobeWear" not in m["counts"]
        m["counts"]["wardrobeWear"] = len(WEAR)
        return dump(m)
    return None


def main():
    before = hashlib.md5(SRC.read_bytes()).hexdigest()
    assert before == SRC_MD5, f"restore-archive.zip moved: {before}"
    derive(ARCHIVES / "restore-archive-wardrobe-wear.zip", wear_edit,
           ("data/wardrobe-wear.json", dump(WEAR)))
    derive(ARCHIVES / "restore-archive-wardrobe-picture.zip",
           lambda n, d: picture_edits(n, d, False))
    derive(ARCHIVES / "restore-archive-wardrobe-picture-tombstone.zip",
           lambda n, d: picture_edits(n, d, True))
    after = hashlib.md5(SRC.read_bytes()).hexdigest()
    assert after == SRC_MD5, f"restore-archive.zip moved during the derive: {after}"
    print(f"restore-archive.zip md5 {after} unchanged")


if __name__ == "__main__":
    main()
