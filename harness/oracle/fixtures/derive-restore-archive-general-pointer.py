#!/usr/bin/env python3
"""P4.158 item 3 (ruling R-C): derive `restore-archive-general-pointer.zip`
from `restore-archive-gen2.zip`.

Step 22f-bis restores LEGACY wardrobe items (and the outfit presets folded into
them at parse time) through `wardrobe.create`, which files a SHARED archetype
(`characterId: null`) in Quilltap General — resolved through
`instance_settings.generalMountPointId`. 22o restores the archive's pointers
LAST, so on a target whose General store is not the archive's (a fresh
instance: a minted id, wiped by the `replace` delete), 22f-bis read the
TARGET's pointer and wrote the archetype into a store that no longer exists,
while the instance comes out of the restore pointing at the archive's General.

No committed archive could show it: `restore-archive-legacy.zip` carries the
one shared preset but no General store (and its pointer is JSON-quoted, naming
none); the archives that carry their own General carry no legacy wardrobe.
`restore-archive-gen2.zip` carries a bare General pointer AND the General store,
and restores convergently into a fresh target (its two files are carried by the
archive's own store rows — `restore_gen2_fresh_replace`).

So this derivation applies ONE mutation: it ADDS `data/outfit-presets.json`
carrying a single SHARED preset (`characterId: null`, the shape of
`restore-archive-legacy.zip`'s "The Empty Ensemble"), which the parse folds
into one shared wardrobe item. The manifest is untouched (v4's manifest has no
outfit-presets count; the fold happens at parse time). Every other entry's
bytes are kept. `restore-archive-gen2.zip` is md5-checked unchanged before
and after.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-general-pointer.py
"""
import hashlib
import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
ARCHIVES = ROOT / "crates/quilltap-web/tests/fixtures/restore-archives"
SRC = ARCHIVES / "restore-archive-gen2.zip"
DST = ARCHIVES / "restore-archive-general-pointer.zip"
SRC_MD5 = "c98e63dc64016a652825cb0179fbf293"

GENERAL = "7d2a92e2-04c9-4424-8363-324ad8ebd47e"
PRESETS = [
    {
        "id": "22222222-0000-4000-8000-000000000003",
        "characterId": None,
        "name": "The Shared Greatcoat",
        "description": "Hangs by the door for whoever goes out first.",
        "slots": {"top": None, "bottom": None, "footwear": None, "accessories": None},
        "createdAt": "2026-03-01T00:00:00.000Z",
        "updatedAt": "2026-03-01T00:00:00.000Z",
    }
]


def md5(path):
    return hashlib.md5(path.read_bytes()).hexdigest()


def main():
    before = md5(SRC)
    assert before == SRC_MD5, f"restore-archive-gen2.zip moved: {before}"
    with zipfile.ZipFile(SRC) as zin:
        root = zin.infolist()[0].filename.rstrip("/")
        names = zin.namelist()
        assert f"{root}/data/outfit-presets.json" not in names
        settings = json.loads(zin.read(f"{root}/data/instance-settings.json"))
        assert {"key": "generalMountPointId", "value": GENERAL} in settings
        points = json.loads(zin.read(f"{root}/data/doc-mount-points.json"))
        assert any(p["id"] == GENERAL and p["name"] == "Quilltap General" for p in points)
        assert json.loads(zin.read(f"{root}/data/wardrobe-items.json")) == []
        last_data = max(i for i, n in enumerate(names) if n.startswith(f"{root}/data/"))
        with zipfile.ZipFile(DST, "w", zipfile.ZIP_DEFLATED) as zout:
            for i, info in enumerate(zin.infolist()):
                out = zipfile.ZipInfo(info.filename, date_time=info.date_time)
                out.external_attr = info.external_attr
                out.compress_type = zipfile.ZIP_DEFLATED
                zout.writestr(out, zin.read(info.filename))
                if i == last_data:
                    added = zipfile.ZipInfo(
                        f"{root}/data/outfit-presets.json", date_time=info.date_time
                    )
                    added.external_attr = info.external_attr
                    added.compress_type = zipfile.ZIP_DEFLATED
                    zout.writestr(
                        added,
                        (json.dumps(PRESETS, indent=2, ensure_ascii=False) + "\n").encode(),
                    )
    after = md5(SRC)
    assert after == SRC_MD5, f"restore-archive-gen2.zip moved during the derive: {after}"
    print(f"wrote {DST.relative_to(ROOT)} (restore-archive-gen2.zip md5 {after} unchanged)")


if __name__ == "__main__":
    main()
