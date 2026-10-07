#!/usr/bin/env python3
"""P4.161 Tier 2: derive `restore-archive-kind-refusals.zip` from
`restore-archive.zip` — ONE archive carrying a refusing row for every Tier 2
kind whose whole-row parse P4.161 landed on the restore, each beside a sound
twin (the archive's own rows stay byte for byte).

  prompt templates (`PromptTemplateSchema`, `promptTemplates.create` →
  `_create`; v4 `restore.ts:268-283`, id NOT preserved, userId retargeted):
    b2…0e1  name of 101 characters
    b2…0e2  content ""
    b2…0e3  description of 501 characters
    b2…0e4  tags ["nope"]
    b2…0e5  a sound twin

  library folders (`FolderSchema`, `folders.create(..., {id})` → `_create`;
  v4 `restore.ts:425-446`, id preserved, userId retargeted):
    a9…0e1  projectId "nope"
    a9…0e2  name 5             (the warning renders it `"5"`)
    a9…0e3  path 5
    a9…0e4  a sound twin

  tags (`TagSchema`; v4 `restore.ts:108-116` derives `nameLower` itself
  — `tagData.nameLower || tagData.name.toLowerCase()` — then `tags.create`):
    a5…0e1  visualStyle {"foregroundColor": "red"}
    a5…0e2  name 5, no nameLower   (the restorer's TypeError, no repo line)
    a5…0e3  nameLower 5            (`create`'s TypeError — the wrap only)
    a5…0e4  quickHide "yes"        (normalized false — LANDS)
    a5…0e5  a sound twin

Each refusal is skipped with `Failed to restore <kind> "<name>": <ZodError
message>`. The manifest's counts move with every collection
touched; every other byte is kept. `restore-archive.zip` is md5-checked
unchanged before and after.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-kind-refusals.py
"""
import copy
import hashlib
import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
ARCHIVES = ROOT / "crates/quilltap-web/tests/fixtures/restore-archives"
SRC = ARCHIVES / "restore-archive.zip"
DST = ARCHIVES / "restore-archive-kind-refusals.zip"
SRC_MD5 = "6b076003bacf8db22c1bf8440fac09d6"


def rewrite_prompt_templates(rows):
    assert len(rows) == 1, len(rows)
    base = rows[0]

    def item(suffix, name, **patch):
        r = copy.deepcopy(base)
        r["id"] = f"b2000000-0000-4000-8000-0000000000{suffix}"
        r["name"] = name
        r.update(patch)
        return r

    return rows + [
        item("e1", "Refused Long Name ".ljust(101, "n")),
        item("e2", "Refused Empty Content", content=""),
        item("e3", "Refused Long Description", description="d" * 501),
        item("e4", "Refused Bad Tag", tags=["nope"]),
        item("e5", "Sound Template Twin"),
    ]


def rewrite_folders(rows):
    assert len(rows) == 1, len(rows)
    base = rows[0]

    def item(suffix, **patch):
        r = copy.deepcopy(base)
        r["id"] = f"a9000000-0000-4000-8000-0000000000{suffix}"
        r.update(patch)
        return r

    return rows + [
        item("e1", path="/refused-project", name="Refused Project", projectId="nope"),
        item("e2", path="/refused-name", name=5),
        item("e3", path=5, name="Refused Path"),
        item("e4", path="/sound-twin", name="Sound Twin"),
    ]


def rewrite_tags(rows):
    base = rows[0]

    def item(suffix, **patch):
        r = copy.deepcopy(base)
        r["id"] = f"a5000000-0000-4000-8000-0000000000{suffix}"
        r.pop("nameLower", None)
        r.update(patch)
        return r

    return rows + [
        item("e1", name="Red Style", visualStyle={"foregroundColor": "red"}),
        item("e2", name=5),
        item("e3", name="Numeric Lower", nameLower=5),
        item("e4", name="Quick Hide Yes", quickHide="yes"),
        item("e5", name="Sound Tag Twin"),
    ]


REWRITES = {
    # data file stem → (rewrite, manifest count key)
    "prompt-templates": (rewrite_prompt_templates, "promptTemplates"),
    "folders": (rewrite_folders, "folders"),
    "tags": (rewrite_tags, "tags"),
}


def main():
    before = hashlib.md5(SRC.read_bytes()).hexdigest()
    assert before == SRC_MD5, f"restore-archive.zip moved: {before}"
    counts = {}
    with zipfile.ZipFile(SRC) as zin, zipfile.ZipFile(DST, "w", zipfile.ZIP_DEFLATED) as zout:
        root = zin.infolist()[0].filename.rstrip("/")
        for stem, (rewrite, key) in REWRITES.items():
            counts[key] = len(rewrite(json.loads(zin.read(f"{root}/data/{stem}.json"))))
        for info in zin.infolist():
            data = zin.read(info.filename)
            rel = info.filename[len(root) + 1:]
            stem = rel[len("data/"):-len(".json")] if rel.startswith("data/") else None
            if stem in REWRITES:
                rows = REWRITES[stem][0](json.loads(data))
                data = (json.dumps(rows, indent=2, ensure_ascii=False) + "\n").encode()
            elif rel == "manifest.json":
                m = json.loads(data)
                m["counts"].update(counts)
                data = (json.dumps(m, indent=2) + "\n").encode()
            out = zipfile.ZipInfo(info.filename, date_time=info.date_time)
            out.external_attr = info.external_attr
            out.compress_type = zipfile.ZIP_DEFLATED
            zout.writestr(out, data)
    after = hashlib.md5(SRC.read_bytes()).hexdigest()
    assert after == SRC_MD5, f"restore-archive.zip changed: {after}"
    print(f"wrote {DST.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
