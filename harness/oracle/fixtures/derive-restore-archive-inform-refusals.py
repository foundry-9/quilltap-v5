#!/usr/bin/env python3
"""P4.161 (§S.2 — the whole-row `ChatInformSchema` twin): derive
`restore-archive-inform-refusals.zip` from `restore-archive.zip`.

No committed archive but `restore-archive-informs.zip` carries
`data/chat-informs.json`, and that one varies `permanent` alone. This
derivation adds the file with NINE rows on chat c…0001's one seat
(`e1…0001`), one bad field each, beside two sound rows:

  af…01  batchId "not-a-uuid"
  af…02  contentMarkdown ABSENT        (v5 used to write it as "")
  af…03  consumedAt "now"
  af…04  recordMessageId ""            (UUIDSchema.nullable refuses "")
  af…05  recordMessageId 5
  af…06  participantId "p"
  af…07  consumedByMessageId "m"
  af…08  permanent "yes"               (refused on restore — no normalization)
  af…09  chatId ABSENT
  af…10  sound, standing
  af…11  sound, one-shot

v4's `chatInforms.create(informData, { id })` validates the WHOLE row
(`_create`), so the nine are skipped with `Failed to restore inform: <ZodError
message>`; v5's `restored_inform` used to check `permanent` alone.

The manifest gains `counts.chatInforms: 11`; `data/chat-informs.json` is
appended at the end; every other entry and byte is kept.
`restore-archive.zip` is md5-checked unchanged before and after.

Usage (from the v5 repo root):
  python3 harness/oracle/fixtures/derive-restore-archive-inform-refusals.py
"""
import hashlib
import json
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parents[3]
ARCHIVES = ROOT / "crates/quilltap-web/tests/fixtures/restore-archives"
SRC = ARCHIVES / "restore-archive.zip"
DST = ARCHIVES / "restore-archive-inform-refusals.zip"
SRC_MD5 = "6b076003bacf8db22c1bf8440fac09d6"

C1 = "c1000000-0000-4000-8000-000000000001"
SEAT1 = "e1000000-0000-4000-8000-000000000001"
BATCH = "bf000000-0000-4000-8000-000000000001"
TS = "2026-03-05T00:00:00.000Z"

ABSENT = object()
ROWS = [
    ("01", {"batchId": "not-a-uuid"}),
    ("02", {"contentMarkdown": ABSENT}),
    ("03", {"consumedAt": "now"}),
    ("04", {"recordMessageId": ""}),
    ("05", {"recordMessageId": 5}),
    ("06", {"participantId": "p"}),
    ("07", {"consumedByMessageId": "m"}),
    ("08", {"permanent": "yes"}),
    ("09", {"chatId": ABSENT}),
    ("10", {"permanent": True}),
    ("11", {}),
]


def informs():
    out = []
    for suffix, patch in ROWS:
        row = {
            "id": f"af000000-0000-4000-8000-0000000000{suffix}",
            "chatId": C1,
            "batchId": BATCH,
            "participantId": SEAT1,
            "contentMarkdown": f"Inform refusal {suffix}: the glass is falling.",
            "recordMessageId": None,
            "permanent": False,
            "createdAt": TS,
            "updatedAt": TS,
            "consumedAt": None,
            "consumedByMessageId": None,
        }
        for k, v in patch.items():
            if v is ABSENT:
                row.pop(k)
            else:
                row[k] = v
        out.append(row)
    return out


def main():
    before = hashlib.md5(SRC.read_bytes()).hexdigest()
    assert before == SRC_MD5, f"restore-archive.zip moved: {before}"
    with zipfile.ZipFile(SRC) as zin, zipfile.ZipFile(DST, "w", zipfile.ZIP_DEFLATED) as zout:
        infos = zin.infolist()
        root = infos[0].filename.rstrip("/")
        assert f"{root}/data/chat-informs.json" not in zin.namelist()
        last = infos[-1]
        for info in infos:
            data = zin.read(info.filename)
            if info.filename.endswith("/data/chats.json"):
                chats = json.loads(data)
                c1 = next(c for c in chats if c["id"] == C1)
                assert c1["participants"][0]["id"] == SEAT1
            elif info.filename.endswith("/manifest.json"):
                m = json.loads(data)
                assert "chatInforms" not in m["counts"]
                m["counts"]["chatInforms"] = len(ROWS)
                data = (json.dumps(m, indent=2) + "\n").encode()
            out = zipfile.ZipInfo(info.filename, date_time=info.date_time)
            out.external_attr = info.external_attr
            out.compress_type = zipfile.ZIP_DEFLATED
            zout.writestr(out, data)
        out = zipfile.ZipInfo(f"{root}/data/chat-informs.json", date_time=last.date_time)
        out.external_attr = last.external_attr
        out.compress_type = zipfile.ZIP_DEFLATED
        zout.writestr(out, (json.dumps(informs(), indent=2) + "\n").encode())
    after = hashlib.md5(SRC.read_bytes()).hexdigest()
    assert after == SRC_MD5, f"restore-archive.zip changed: {after}"
    print(f"wrote {DST.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
