#!/usr/bin/env python3
"""P4.D351 (v4 `778b97b2b`, the Novel template): derive
`restore-archive-novel-template.zip` from `restore-archive.zip`.

v4's restore (step 11, `restore.ts:403-416`) hands each template to
`roleplayTemplates.create`, which validates the WHOLE entity through
`RoleplayTemplateSchema` before `toPersistedRow` ENCODES the narration: a
MISSING `narrationDelimiters` key takes the schema's `.default('*')`, an
explicit `null` stays `null` (unmarked prose — the four-byte TEXT `null`), a
missing `unmarkedStyle` stays NULL and a missing `markdownAsterisks` is
`true`. The committed archive's ONE template (`House Style`, written at
`4.8.0-dev.98`) carries a `"*"` narration and neither new key, so no committed
archive can see any of that. This one adds FIVE templates under fixed ids (the
original kept byte for byte):

  ad…00a1  Novel Copy          narration null, a Dialogue wrap with both flags,
                               unmarkedStyle {qt-roleplay-plain, italic},
                               markdownAsterisks false     → text `null` / the JSON / 0
  ad…00a2  Legacy No Key       NO narrationDelimiters key, neither new key
                               (a pre-pin archive's shape)  → `"*"` / NULL / 1
  ad…00a3  Bracket Explicit    ["[","]"] + unmarkedStyle: null
                               + markdownAsterisks: true    → `["[","]"]` / NULL / 1
  ad…00a4  Bad Asterisks       markdownAsterisks: "yes"     → REFUSED (the warning)
  ad…00a5  Bad Unmarked Empty  unmarkedStyle: {style: ""}   → REFUSED (the warning)

The manifest's `counts.roleplayTemplates` moves 1 → 6. Entry order, names,
`date_time` and every other byte kept (the `derive-restore-archive-voice-
legacy.py` precedent). Writes a NEW file; never reads and writes one path.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-novel-template.py
"""
import copy
import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
ARCHIVES = ROOT / "crates/quilltap-web/tests/fixtures/restore-archives"
SRC = ARCHIVES / "restore-archive.zip"
DST = ARCHIVES / "restore-archive-novel-template.zip"
assert SRC != DST

HOUSE = "a4000000-0000-4000-8000-000000000001"
DIALOGUE = {
    "kind": "wrap",
    "name": "Dialogue",
    "buttonName": "Dia",
    "style": "qt-chat-dialogue",
    "delimiters": '"',
    "matchCurlyQuotes": True,
    "closeOptional": True,
}


def clone(base, n, name, drop=(), **fields):
    row = copy.deepcopy(base)
    row["id"] = f"ad000000-0000-4000-8000-0000000000a{n}"
    row["name"] = name
    for k in drop:
        row.pop(k)
    # Rebuild in v4's schema key order: the planted keys replace / follow
    # `narrationDelimiters`, before `createdAt`.
    out = {}
    for k, v in row.items():
        if k == "createdAt":
            for fk, fv in fields.items():
                out[fk] = fv
        if k in fields:
            continue
        out[k] = v
    return out


def rewrite_templates(rows):
    assert [r["id"] for r in rows] == [HOUSE], [r["id"] for r in rows]
    base = rows[0]
    assert "unmarkedStyle" not in base and "markdownAsterisks" not in base
    return rows + [
        clone(
            base,
            1,
            "Novel Copy",
            delimiters=[DIALOGUE],
            narrationDelimiters=None,
            unmarkedStyle={"style": "qt-roleplay-plain", "addOns": {"italic": True}},
            markdownAsterisks=False,
        ),
        clone(base, 2, "Legacy No Key", drop=("narrationDelimiters",)),
        clone(
            base,
            3,
            "Bracket Explicit",
            narrationDelimiters=["[", "]"],
            unmarkedStyle=None,
            markdownAsterisks=True,
        ),
        clone(base, 4, "Bad Asterisks", markdownAsterisks="yes"),
        clone(base, 5, "Bad Unmarked Empty", unmarkedStyle={"style": ""}),
    ]


def main():
    with zipfile.ZipFile(SRC) as zin, zipfile.ZipFile(DST, "w", zipfile.ZIP_DEFLATED) as zout:
        for info in zin.infolist():
            data = zin.read(info.filename)
            if info.filename.endswith("/data/roleplay-templates.json"):
                data = (json.dumps(rewrite_templates(json.loads(data)), indent=2) + "\n").encode()
            elif info.filename.endswith("/manifest.json"):
                m = json.loads(data)
                assert m["counts"]["roleplayTemplates"] == 1
                m["counts"]["roleplayTemplates"] = 6
                data = (json.dumps(m, indent=2) + "\n").encode()
            out = zipfile.ZipInfo(info.filename, date_time=info.date_time)
            out.external_attr = info.external_attr
            out.compress_type = zipfile.ZIP_DEFLATED
            zout.writestr(out, data)
    print(f"wrote {DST.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
