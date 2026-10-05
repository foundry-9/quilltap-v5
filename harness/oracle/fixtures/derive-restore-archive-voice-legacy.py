#!/usr/bin/env python3
"""P4.D251 (v4 `07b8f0209`): derive `restore-archive-voice-legacy.zip` from
`restore-archive.zip`.

v4's restore chains `withImpersonationVoiceModeFromLegacy` AFTER the Concierge
translation (`restore.ts:393-413`): a backup taken while the impersonated-line
rehearsal was still the on/off `impersonationVoiceRewrite` boolean restores
with the mode derived from it (`true`/`1` → `'ask'`, else `'off'`), an
explicit mode winning over the old key, and a current record left alone. The
committed archive's ONE settings row (written at `4.8.0-dev.98`) carries
NEITHER key, so no committed archive can see any of that. This one gives the
instance FIVE settings rows — the original kept byte for byte, plus four
clones under their own ids and user ids (every id a fixed UUID that appears
in the archive, so the state diff compares it rather than minting it):

  ab…0001  (the original)            neither key           → 'off' (the Zod default)
  ab…0002  impersonationVoiceRewrite: true                  → 'ask'
  ab…0003  impersonationVoiceRewrite: false                 → 'off'
  ab…0004  impersonationVoiceRewrite: 1 (the INTEGER cell)  → 'ask'
  ab…0005  impersonationVoiceRewrite: true
           + impersonationVoiceMode: 'always'               → 'always' (the explicit mode wins)
  ab…0006  impersonationVoiceMode: 'ask', no old key        → 'ask' (a current record, untouched)

The manifest's `counts.chatSettings` moves 1 → 6. Entry order, names,
`date_time` and every other byte kept (the `derive-restore-archive-chat-serde-
arm.py` precedent). Writes a NEW file; never reads and writes one path.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-voice-legacy.py
"""
import copy
import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
ARCHIVES = ROOT / "crates/quilltap-web/tests/fixtures/restore-archives"
SRC = ARCHIVES / "restore-archive.zip"
DST = ARCHIVES / "restore-archive-voice-legacy.zip"
assert SRC != DST

S1 = "ab000000-0000-4000-8000-000000000001"


def clone(base, n, **fields):
    row = copy.deepcopy(base)
    row["id"] = f"ab000000-0000-4000-8000-00000000000{n}"
    row["userId"] = f"e18e05bc-63e8-4539-8a85-719b7a50885{n}"
    # Rebuild the row so the planted keys sit where v4's writer put the
    # boolean (after composerSpellcheck's block, before textReplacementsEnabled)
    # — the position is immaterial to the restore, kept for a reader's sanity.
    out = {}
    for k, v in row.items():
        if k == "textReplacementsEnabled":
            for fk, fv in fields.items():
                out[fk] = fv
        out[k] = v
    return out


def rewrite_settings(rows):
    assert [r["id"] for r in rows] == [S1], [r["id"] for r in rows]
    base = rows[0]
    assert "impersonationVoiceRewrite" not in base and "impersonationVoiceMode" not in base
    return rows + [
        clone(base, 2, impersonationVoiceRewrite=True),
        clone(base, 3, impersonationVoiceRewrite=False),
        clone(base, 4, impersonationVoiceRewrite=1),
        clone(base, 5, impersonationVoiceRewrite=True, impersonationVoiceMode="always"),
        clone(base, 6, impersonationVoiceMode="ask"),
    ]


def main():
    with zipfile.ZipFile(SRC) as zin, zipfile.ZipFile(DST, "w", zipfile.ZIP_DEFLATED) as zout:
        for info in zin.infolist():
            data = zin.read(info.filename)
            if info.filename.endswith("/data/chat-settings.json"):
                data = (json.dumps(rewrite_settings(json.loads(data)), indent=2) + "\n").encode()
            elif info.filename.endswith("/manifest.json"):
                m = json.loads(data)
                assert m["counts"]["chatSettings"] == 1
                m["counts"]["chatSettings"] = 6
                data = (json.dumps(m, indent=2) + "\n").encode()
            out = zipfile.ZipInfo(info.filename, date_time=info.date_time)
            out.external_attr = info.external_attr
            out.compress_type = zipfile.ZIP_DEFLATED
            zout.writestr(out, data)
    print(f"wrote {DST.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
