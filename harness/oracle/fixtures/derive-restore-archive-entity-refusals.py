#!/usr/bin/env python3
"""P4.161 (P4.155's R-B): derive `restore-archive-entity-refusals.zip` from
`restore-archive.zip`.

v4's restore hands each project / group to the store-backed `create`, whose
`_create` validates the WHOLE `ProjectSchema` / `GroupSchema` — the row keys
as well as the property bag. v5's restore validated the BAG alone, so a row
with a 101-code-point name restored where v4 skips it. This derivation adds,
AHEAD of the archive's own project / group (so a refused row that wrongly
claimed a store would steal it):

  projects
    a3…0e1  name of 101 code points, `officialMountPointId` = THE VOYAGE's
            archived store `5c17e916-…` — v4 refuses it; it must claim
            nothing, so The Voyage keeps its own store (v5's preserve arm)
    a3…0e2  description of 2001 characters (no store pointer)
    a3…0e3  a sound twin (no store pointer — the fresh-store arm)
  groups
    a2…0e1  name of 101 characters (no store pointer)
    a2…0e2  a sound twin (no store pointer)

Each refusal is skipped with `Failed to restore {project|group} "<name>":
<ZodError message>` and v4's three repository ERRORs. The manifest's counts
move projects 1 → 4, groups 1 → 3. Every other byte is kept;
`restore-archive.zip` is md5-checked unchanged before and after.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-entity-refusals.py
"""
import copy
import hashlib
import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
ARCHIVES = ROOT / "crates/quilltap-web/tests/fixtures/restore-archives"
SRC = ARCHIVES / "restore-archive.zip"
DST = ARCHIVES / "restore-archive-entity-refusals.zip"
SRC_MD5 = "6b076003bacf8db22c1bf8440fac09d6"

VOYAGE_STORE = "5c17e916-5f79-4cca-a134-ec09c05924e9"


def rewrite_projects(projects):
    assert len(projects) == 1 and projects[0]["officialMountPointId"] == VOYAGE_STORE
    base = projects[0]

    def item(suffix, **patch):
        p = copy.deepcopy(base)
        p["id"] = f"a3000000-0000-4000-8000-0000000000{suffix}"
        p.pop("officialMountPointId")
        p["characterRoster"] = []
        p.update(patch)
        return p

    long_name = "The Over-Long Voyage ".ljust(101, "v")
    assert len(long_name) == 101
    return [
        item("e1", name=long_name, officialMountPointId=VOYAGE_STORE),
        item("e2", name="The Verbose Voyage", description="d" * 2001),
        item("e3", name="The Sound Voyage", description="A sound twin."),
    ] + projects


def rewrite_groups(groups):
    assert len(groups) == 1
    base = groups[0]

    def item(suffix, **patch):
        g = copy.deepcopy(base)
        g["id"] = f"a2000000-0000-4000-8000-0000000000{suffix}"
        g.pop("officialMountPointId")
        g.update(patch)
        return g

    return [
        item("e1", name="The Over-Long Crew ".ljust(101, "c")),
        item("e2", name="The Sound Crew", description="A sound twin."),
    ] + groups


def main():
    before = hashlib.md5(SRC.read_bytes()).hexdigest()
    assert before == SRC_MD5, f"restore-archive.zip moved: {before}"
    with zipfile.ZipFile(SRC) as zin, zipfile.ZipFile(DST, "w", zipfile.ZIP_DEFLATED) as zout:
        for info in zin.infolist():
            data = zin.read(info.filename)
            if info.filename.endswith("/data/projects.json"):
                data = (json.dumps(rewrite_projects(json.loads(data)), indent=2) + "\n").encode()
            elif info.filename.endswith("/data/groups.json"):
                data = (json.dumps(rewrite_groups(json.loads(data)), indent=2) + "\n").encode()
            elif info.filename.endswith("/manifest.json"):
                m = json.loads(data)
                assert m["counts"]["projects"] == 1 and m["counts"]["groups"] == 1
                m["counts"]["projects"] = 4
                m["counts"]["groups"] = 3
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
