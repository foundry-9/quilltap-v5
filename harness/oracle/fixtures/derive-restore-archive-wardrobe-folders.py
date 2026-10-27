#!/usr/bin/env python3
"""P4.167 (P4.D284 banked Tier 2): derive `restore-archive-wardrobe-folders.zip`
from `restore-archive.zip` — the restore's 22f-bis `folder` arms.

v4's backup writes `wardrobeItems: []` since the vault cutover, so only a
crafted (pre-cutover-shaped) archive reaches 22f-bis with a `folder`. The
derive's `data/wardrobe-items.json` carries six LEGACY rows on RIYA (her vault
holds no garment, so creating them re-projects no other document — the
`derive-restore-archive-wardrobe.py` reasoning), one per `folder` arm of
`carried_wardrobe_folder` / v4's writer `safeFolder`:

  ab…11  "Formal"     a safe folder — filed under `Wardrobe/Formal/`
  ab…12  "../x"       an unsafe string — the writer normalises it
  ab…13  ""           falsy — the root
  ab…14  5            a truthy non-string — `path.includes is not a function`
  ab…15  ["Formal"]   an array — `path.split is not a function`
  ab…16  (absent)     the root

`counts.wardrobeItems` becomes 6; every other byte is kept.
`restore-archive.zip` is md5-checked unchanged before and after.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-wardrobe-folders.py
"""
import hashlib
import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
ARCHIVES = ROOT / "crates/quilltap-web/tests/fixtures/restore-archives"
SRC = ARCHIVES / "restore-archive.zip"
DST = ARCHIVES / "restore-archive-wardrobe-folders.zip"
SRC_MD5 = "6b076003bacf8db22c1bf8440fac09d6"

RIYA = "a1000000-0000-4000-8000-000000000002"
MISSING = object()
FOLDERS = [
    ("11", "Formal Gloves", "Formal"),
    ("12", "Escaped Hat", "../x"),
    ("13", "Root Scarf", ""),
    ("14", "Numbered Boots", 5),
    ("15", "Listed Sash", ["Formal"]),
    ("16", "Unfiled Belt", MISSING),
]


def legacy(suffix, title, folder):
    row = {
        "id": f"ab000000-0000-4000-8000-0000000000{suffix}",
        "characterId": RIYA,
        "title": title,
        "description": f"{title}, carried by a pre-cutover backup.",
        "types": ["accessories"],
        "componentItemIds": [],
        "appropriateness": None,
        "isDefault": False,
        "replace": False,
        "imageFileId": None,
        "archivedAt": None,
        "createdAt": "2026-03-01T00:00:00.000Z",
        "updatedAt": "2026-03-01T00:00:00.000Z",
    }
    if folder is not MISSING:
        row["folder"] = folder
    return row


def dump(value):
    return (json.dumps(value, indent=2) + "\n").encode()


def edit(name, data):
    if name.endswith("/data/wardrobe-items.json"):
        assert json.loads(data) == []
        return dump([legacy(*f) for f in FOLDERS])
    if name.endswith("/manifest.json"):
        m = json.loads(data)
        assert m["counts"]["wardrobeItems"] == 0
        m["counts"]["wardrobeItems"] = len(FOLDERS)
        return dump(m)
    return None


def main():
    before = hashlib.md5(SRC.read_bytes()).hexdigest()
    assert before == SRC_MD5, f"restore-archive.zip moved: {before}"
    with zipfile.ZipFile(SRC) as zin, zipfile.ZipFile(DST, "w", zipfile.ZIP_DEFLATED) as zout:
        for info in zin.infolist():
            data = zin.read(info.filename)
            new = edit(info.filename, data)
            out = zipfile.ZipInfo(info.filename, date_time=info.date_time)
            out.external_attr = info.external_attr
            out.compress_type = zipfile.ZIP_DEFLATED
            zout.writestr(out, data if new is None else new)
    after = hashlib.md5(SRC.read_bytes()).hexdigest()
    assert after == SRC_MD5, f"restore-archive.zip moved during the derive: {after}"
    print(f"wrote {DST.relative_to(ROOT)} (restore-archive.zip md5 {after} unchanged)")


if __name__ == "__main__":
    main()
